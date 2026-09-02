use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen::closure::Closure;
use std::rc::Rc;
use std::cell::Cell;

/// Semua wasm-bindgen `Closure` milik satu sesi tontonan. DIPEGANG (bukan
/// `.forget()`) supaya bisa di-DROP saat disconnect/unmount → tak bocor tiap
/// kali penonton membuka stream. Penting di feed `/lives` yang bisa membuka
/// banyak kartu berturut-turut. Cerminan pola publisher di `merchant_live.rs`.
struct ViewerRtcClosures {
    _on_track: Closure<dyn FnMut(web_sys::RtcTrackEvent)>,
    _on_msg: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _on_err: Closure<dyn FnMut(web_sys::Event)>,
    _on_ice: Closure<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>,
}

// Hanya field yang dipakai UI viewer; field lain di respons diabaikan serde.
#[derive(Debug, Clone, Deserialize)]
struct RoomInfo {
    /// Dibutuhkan sejak hitungan penonton datang lewat `/ws/lives`: snapshot
    /// itu memuat SELURUH room, jadi harus ada cara memilih yang ini.
    ///
    /// Hanya DIBACA di klien — pemilihannya ada di dalam blok yang dipagari
    /// wasm. Bidangnya tetap harus ada di kedua target supaya bentuk yang
    /// diurai serde tak berbeda antara SSR dan klien.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    #[serde(default)]
    room_id: String,
    merchant_name: String,
    viewer_count: usize,
    /// Pemilik siaran — dipakai mengambil rincian produk lewat
    /// `get_merchant_public_products` yang sudah ada.
    #[serde(default)]
    merchant_id: String,
    /// Id produk yang sedang dijual di siaran ini, urut pilihan merchant.
    #[serde(default)]
    product_ids: Vec<String>,
}

async fn api_get_room(room_id: &str) -> Result<RoomInfo, String> {
    let url = format!("/api/live/rooms/{}", room_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())
}

/// Bangun URL WebSocket dari path relatif.
/// Otomatis menggunakan wss:// bila halaman di-serve via HTTPS.
use crate::web::utils::build_ws_url;

/// Bangun daftar ICE server sebagai array berisi objek JS biasa.
/// `serde_wasm_bindgen` (default) menserialisasi map jadi `Map` JS — bukan
/// object — sehingga `RTCIceServer.urls` tak terbaca dan konstruktor
/// RTCPeerConnection menolak ("urls is required"). Maka dibangun manual.
/// Helper: add a recvonly transceiver using raw JS
/// web-sys 0.3.99 does NOT have `add_transceiver_with_str_and_init`,
/// so we call the JS method directly via Reflect.
fn add_recvonly_transceiver(
    pc: &web_sys::RtcPeerConnection,
    kind: &str,
) -> Result<(), JsValue> {
    let init = js_sys::Object::new();
    js_sys::Reflect::set(&init, &"direction".into(), &"recvonly".into())?;
    let _ = js_sys::Reflect::get(pc.as_ref(), &"addTransceiver".into())?
        .dyn_into::<js_sys::Function>()?
        .call2(pc.as_ref(), &kind.into(), &init)?;
    Ok(())
}

#[component]
pub fn LiveStreamViewer(
    room_id: String,
    /// Bila true, langsung menyambung tanpa menunggu klik (dipakai di feed lives).
    #[prop(optional)]
    autoplay: bool,
    /// Mode pratinjau untuk kartu di daftar `/lives`: hanya videonya, tanpa
    /// perabot apa pun (header, tombol, hitungan penonton, overlay unmute).
    ///
    /// Bukan cuma soal tampilan — mode ini memangkas dua biaya yang jadi serius
    /// begitu beberapa kartu tayang serentak:
    ///   * TAK ada polling hitungan penonton (biasanya satu permintaan HTTP tiap
    ///     5 dtk PER kartu — dengan 4 kartu itu 48 permintaan semenit hanya
    ///     untuk angka yang sudah dikirim WS `/ws/lives` secara cuma-cuma).
    ///   * TAK meminta track audio. Pratinjau selalu bisu, jadi menegosiasi
    ///     audio berarti menyuruh SFU meneruskan aliran yang dijamin dibuang.
    #[prop(optional)]
    preview: bool,
) -> AnyView {
    // StoredValue (Copy) supaya bisa dipakai di beberapa closure `move`
    // (polling effect, connect, disconnect, on_cleanup) tanpa konflik move.
    let room_id = StoredValue::new(room_id);
    // Identitas penonton (jika login) dikirim ke server saat subscribe agar
    // merchant bisa melihat siapa saja yang join.
    let auth = crate::web::hooks::use_auth();
    let is_playing = RwSignal::new(false);
    // Viewer mulai muted (syarat autoplay browser). Tombol kustom mengubahnya.
    let is_muted = RwSignal::new(true);
    let viewer_count = RwSignal::new(0u32);
    let merchant_name = RwSignal::new(String::new());
    // ── Keranjang kuning ────────────────────────────────────────────────────
    // Id produk datang bersama snapshot room (lihat `live/room.rs`), jadi ia
    // ikut berubah SEKETIKA saat merchant menambah/mencabut produk di tengah
    // siaran — tanpa penonton memuat ulang apa pun.
    let produk_ids = RwSignal::new(Vec::<String>::new());
    let merchant_id = RwSignal::new(String::new());
    let keranjang_buka = RwSignal::new(false);
    let produk_live = RwSignal::new(Vec::<crate::web::models::Product>::new());
    let produk_loading = RwSignal::new(false);
    let error_msg = RwSignal::new(None::<String>);
    let pc: RwSignal<Option<SendWrapper<web_sys::RtcPeerConnection>>> = RwSignal::new(None);
    // Stream remote yang dirakit dari track yang masuk (audio + video).
    let remote_stream: RwSignal<Option<SendWrapper<web_sys::MediaStream>>> = RwSignal::new(None);
    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();
    // Koneksi WS signaling yang sedang aktif.
    // Menutup WS ini secara otomatis memanggil remove_subscriber di server —
    // tidak perlu lagi HTTP DELETE /subscribe/{id} saat keluar.
    let sig_ws: RwSignal<Option<SendWrapper<web_sys::WebSocket>>> = RwSignal::new(None);
    // Penampung closure RTC/WS sesi ini — dipegang agar bisa di-drop saat
    // disconnect/unmount (bukan `.forget()` yang bocor permanen). SendWrapper
    // memenuhi bound Send StoredValue (single-thread WASM; no-op native).
    let rtc_closures: StoredValue<Option<SendWrapper<ViewerRtcClosures>>> = StoredValue::new(None);

    // ── Hitungan penonton & daftar produk lewat WS ────────────────────────
    // Dulu di sini ada polling `GET /api/live/rooms/:id` tiap 5 detik. Data
    // yang sama SUDAH didorong server lewat `/ws/lives` tiap ada perubahan —
    // jadi yang dilakukan polling itu hanya menanyakan ulang sesuatu yang
    // sudah dikirim cuma-cuma.
    //
    // Ongkosnya nyata: sepuluh ribu penonton × sekali per lima detik = dua ribu
    // permintaan per detik ke basis data yang sama dengan yang melayani
    // halaman, hanya untuk satu angka di pojok layar. Itu persis bentuk beban
    // yang sudah pernah menjatuhkan situs ini.
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        let ws_store: StoredValue<Option<web_sys::WebSocket>> = StoredValue::new(None);
        let cb_msg: StoredValue<Option<JsValue>> = StoredValue::new(None);

        Effect::new(move |_| {
            // Pratinjau tak menampilkan angkanya, jadi tak perlu mendengarkan.
            if preview {
                return;
            }
            let rid = room_id.get_value();

            let proto = if web_sys::window()
                .map(|w| w.location().protocol().unwrap_or_default() == "https:")
                .unwrap_or(false)
            {
                "wss"
            } else {
                "ws"
            };
            let host = web_sys::window()
                .and_then(|w| w.location().host().ok())
                .unwrap_or_default();
            let Ok(ws) = web_sys::WebSocket::new(&format!("{proto}://{host}/ws/lives")) else {
                return;
            };

            let vc = viewer_count;
            let pids = produk_ids;
            let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |e: web_sys::MessageEvent| {
                    let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() else {
                        return;
                    };
                    let s: String = txt.into();
                    let Ok(list) = serde_json::from_str::<Vec<RoomInfo>>(&s) else {
                        return;
                    };
                    // Snapshot memuat SELURUH room; yang kita perlukan satu.
                    let Some(room) = list.into_iter().find(|r| r.room_id == rid) else {
                        return;
                    };
                    vc.set(room.viewer_count as u32);
                    // Daftar produk ikut di sini. Merchant kerap menambah produk
                    // DI TENGAH siaran karena ada yang menanyakannya; tanpa ini
                    // penonton baru melihatnya setelah memuat ulang halaman —
                    // yang berarti keluar dari siaran.
                    //
                    // Dibandingkan dulu: menyetel ulang daftar yang isinya sama
                    // membuat Leptos merender ulang keranjangnya tiap ada
                    // penonton masuk atau keluar.
                    if pids.with_untracked(|v: &Vec<String>| *v != room.product_ids) {
                        pids.set(room.product_ids);
                    }
                },
            );
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            cb_msg.set_value(Some(onmessage.into_js_value()));
            ws_store.set_value(Some(ws));

            on_cleanup(move || {
                ws_store.with_value(|opt| {
                    if let Some(ws) = opt {
                        ws.set_onmessage(None);
                        let _ = ws.close();
                    }
                });
                ws_store.set_value(None);
                cb_msg.set_value(None);
            });
        });
    }

    let connect = Action::new_local(move |_: &()| {
        let room_id = room_id.get_value();
        let is_playing = is_playing;
        let viewer_count = viewer_count;
        let merchant_name = merchant_name;
        let error_msg = error_msg;
        let pc = pc;
        // video_ref tidak dipakai di sini: srcObject + play() dipasang oleh
        // Effect reaktif level-komponen saat `remote_stream` berubah.
        let sig_ws = sig_ws;
        let rtc_closures = rtc_closures;
        let profile = auth.user.get_untracked();

        async move {
            error_msg.set(None);

            let room = match api_get_room(&room_id).await {
                Ok(r) => r,
                Err(e) => {
                    error_msg.set(Some(format!("Stream not found: {e}")));
                    return;
                }
            };

            viewer_count.set(room.viewer_count as u32);
            merchant_name.set(room.merchant_name);
            merchant_id.set(room.merchant_id);
            produk_ids.set(room.product_ids);

            let config = web_sys::RtcConfiguration::new();
            config.set_ice_servers(crate::web::rtc::fetch_ice_servers().await.as_ref());

            let peer_connection = web_sys::RtcPeerConnection::new_with_configuration(&config)
                .map_err(|e| format!("RTCPeerConnection failed: {:?}", e))
                .ok();

            let peer_connection = match peer_connection {
                Some(p) => p,
                None => {
                    error_msg.set(Some("WebRTC not supported".to_string()));
                    return;
                }
            };

            // ── Add recvonly transceivers BEFORE createOffer ──────────────────
            if let Err(e) = add_recvonly_transceiver(&peer_connection, "video") {
                error_msg.set(Some(format!("addTransceiver(video) failed: {:?}", e)));
                return;
            }
            // Pratinjau bisu permanen → jangan negosiasi audio sama sekali.
            // `forward_to_subscribers` di SFU menulis per-kind ke tiap
            // subscriber; tanpa mid audio, kartu pratinjau berhenti menjadi
            // tujuan penerusan audio dan beban SFU per kartu turun separuh.
            if !preview {
                if let Err(e) = add_recvonly_transceiver(&peer_connection, "audio") {
                    error_msg.set(Some(format!("addTransceiver(audio) failed: {:?}", e)));
                    return;
                }
            }

            // ── ontrack: rakit track masuk → update signal saja ─────────────
            // Jangan manipulasi DOM dari sini (Closure JS, di luar sistem
            // reaktif Leptos) karena video_ref.get_untracked() bisa None.
            // Reactive Effect di bawah yang akan memasang srcObject + play().
            let on_track = {
                Closure::<dyn FnMut(web_sys::RtcTrackEvent)>::new(move |product: web_sys::RtcTrackEvent| {
                    // Selalu rakit SEMUA track masuk (audio + video) ke SATU
                    // MediaStream. JANGAN pakai product.streams()[0]: str0m memberi
                    // msid berbeda per track, jadi memakai streams[0] akan MENIMPA
                    // stream tiap kali ontrack terpicu → hanya track terakhir
                    // (audio) yang tersisa, track video hilang → video 0x0/hitam.
                    let stream = match remote_stream.get_untracked() {
                        Some(s) => (*s).clone(),
                        None => match web_sys::MediaStream::new() {
                            Ok(s) => s,
                            Err(_) => return,
                        },
                    };
                    // add_track idempoten (spec): aman walau ontrack terpicu ulang.
                    stream.add_track(&product.track());
                    // Signal update → reactive Effect merespons dan set srcObject.
                    remote_stream.set(Some(SendWrapper::new(stream)));
                })
            };
            peer_connection.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
            // JANGAN forget: on_track dipegang & disimpan di rtc_closures (bawah).

            // ── Buka WS signaling SEBELUM createOffer ────────────────────────
            // Alasannya: setLocalDescription memulai ICE gathering. Kandidat ICE
            // pertama bisa datang sangat cepat. Dengan WS sudah terbuka, kandidat
            // langsung terkirim (trickle ICE sejati) — tidak perlu wait polling.
            let ws_url = build_ws_url(&format!("/ws/live/subscribe/{}", room_id));
            let ws = match web_sys::WebSocket::new(&ws_url) {
                Ok(w) => w,
                Err(e) => {
                    error_msg.set(Some(format!("WS gagal dibuka: {:?}", e)));
                    return;
                }
            };

            // Simpan WS segera agar on_cleanup bisa menutupnya walau
            // koneksi masih dalam proses (navigasi pergi sebelum selesai).
            sig_ws.set(Some(SendWrapper::new(ws.clone())));

            // ── Siapkan channel untuk menerima answer dari server ─────────────
            // Pola Rc<Cell<Option<Sender>>> aman di WASM (single-thread).
            // Cell::take() bekerja untuk Option<T> karena Option: Default.
            let (answer_tx, answer_rx) = futures::channel::oneshot::channel::<
                Result<serde_json::Value, String>,
            >();
            let answer_tx = Rc::new(Cell::new(Some(answer_tx)));

            let on_msg = {
                let tx = answer_tx.clone();
                let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                    move |e: web_sys::MessageEvent| {
                        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                            let s: String = txt.into();
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                                // Publisher mematikan siaran → server kirim
                                // "stream_ended". Keluarkan penonton: hentikan
                                // koneksi, bersihkan video, tampilkan info.
                                if v.get("type").and_then(|t| t.as_str()) == Some("stream_ended") {
                                    if let Some(conn) = pc.get_untracked() {
                                        let _ = conn.close();
                                    }
                                    pc.set(None);
                                    remote_stream.set(None);
                                    is_playing.set(false);
                                    is_muted.set(true);
                                    error_msg.set(Some("Siaran telah berakhir".to_string()));
                                    return;
                                }
                                if let Some(tx) = tx.take() {
                                    let _ = tx.send(Ok(v));
                                }
                            }
                        }
                    },
                );
                ws.set_onmessage(Some(cb.as_ref().unchecked_ref()));
                cb
            };
            let on_err = {
                let tx = answer_tx.clone();
                let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Err("WS error sebelum answer diterima".to_string()));
                    }
                });
                ws.set_onerror(Some(cb.as_ref().unchecked_ref()));
                cb
            };

            // ── onicecandidate: kirim kandidat langsung via WS (trickle ICE) ─
            // Tidak lagi menunggu ICE gathering selesai (wait_ice_gathering_complete
            // dihapus). Ini mempersingkat setup koneksi 300–500 ms.
            let on_ice = {
                let ws_ref = ws.clone();
                let cb = Closure::<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>::new(
                    move |product: web_sys::RtcPeerConnectionIceEvent| {
                        if let Some(candidate) = product.candidate() {
                            let msg = serde_json::json!({
                                "type": "candidate",
                                "candidate": candidate.candidate(),
                                "sdp_mid": candidate.sdp_mid().unwrap_or_default(),
                                "sdp_mline_index": candidate.sdp_m_line_index().unwrap_or(0),
                            });
                            let _ = ws_ref.send_with_str(&msg.to_string());
                        }
                    },
                );
                peer_connection.set_onicecandidate(Some(cb.as_ref().unchecked_ref()));
                cb
            };

            // ── Tunggu WS terbuka (maks 5 detik) ─────────────────────────────
            {
                use std::time::Duration;
                for _ in 0..50u8 {
                    match ws.ready_state() {
                        1 => break,                // OPEN
                        2 | 3 => {                 // CLOSING / CLOSED
                            error_msg.set(Some("WS ditutup sebelum terbuka".to_string()));
                            return;
                        }
                        _ => gloo_timers::future::sleep(Duration::from_millis(100)).await,
                    }
                }
                if ws.ready_state() != 1 {
                    error_msg.set(Some("WS koneksi timeout".to_string()));
                    return;
                }
            }

            // ── createOffer → setLocalDescription (ICE gathering dimulai) ────
            let offer_promise = peer_connection.create_offer_with_rtc_offer_options(
                &web_sys::RtcOfferOptions::new(),
            );
            let offer = match wasm_bindgen_futures::JsFuture::from(offer_promise).await {
                Ok(o) => o,
                Err(e) => {
                    error_msg.set(Some(format!("createOffer gagal: {:?}", e)));
                    return;
                }
            };

            let sdp_str = js_sys::Reflect::get(&offer, &wasm_bindgen::JsValue::from_str("sdp"))
                .unwrap()
                .as_string()
                .unwrap_or_default();

            let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
            desc.set_sdp(&sdp_str);

            if let Err(e) = wasm_bindgen_futures::JsFuture::from(
                peer_connection.set_local_description(&desc),
            )
            .await
            {
                error_msg.set(Some(format!("setLocalDescription gagal: {:?}", e)));
                return;
            }

            // Ambil SDP lokal saat ini (kandidat yang sudah terkumpul sebelum offer dikirim).
            let offer_sdp = peer_connection
                .local_description()
                .map(|d| d.sdp())
                .filter(|s| !s.is_empty())
                .unwrap_or(sdp_str);

            // ── Kirim offer via WS SINKRON (sebelum await berikutnya) ─────────
            // JavaScript single-thread: onicecandidate tidak bisa menyela sebelum kita await.
            // Dengan mengirim offer sinkron di sini, kita jamin server menerima offer
            // lebih dulu daripada kandidat ICE apapun.
            // Penonton yang belum login TETAP dikirimi identitas — identitas
            // TAMU yang tersimpan di peramban, bukan `None`.
            //
            // Dengan `None`, server menerbitkan UUID baru pada SETIAP subscribe
            // (lihat `live/api.rs`), sehingga satu tamu yang me-refresh atau
            // pindah dari halaman produk ke `/lives` terhitung sebagai penonton
            // baru berkali-kali. Angka penonton yang dipakai merchant untuk
            // menilai siarannya jadi menggelembung tanpa ada orang tambahan,
            // dan semua tamu tampil sebagai "Anonim" yang tak terbedakan.
            //
            // Lihat `web::utils::identitas_tamu`.
            let (viewer_id, viewer_name) = match &profile {
                Some(p) => (Some(p.id.clone()), Some(p.name.clone())),
                None => {
                    let (id, nama) = crate::web::utils::identitas_tamu();
                    // Kosong = padanan sisi server (lihat `utils::identitas_tamu`)
                    // atau localStorage yang menolak dipakai. Dikembalikan ke
                    // `None` supaya server memakai jalur lamanya — penonton tetap
                    // bisa menonton, hanya tanpa penanda yang bertahan. Mengirim
                    // string kosong justru lebih buruk: SEMUA tamu akan berbagi
                    // satu identitas yang sama.
                    if id.is_empty() || nama.is_empty() {
                        (None, None)
                    } else {
                        (Some(id), Some(nama))
                    }
                }
            };
            let offer_msg = serde_json::json!({
                "type": "subscribe_offer",
                "sdp": offer_sdp,
                "viewer_id": viewer_id,
                "viewer_name": viewer_name,
            });
            if ws.send_with_str(&offer_msg.to_string()).is_err() {
                error_msg.set(Some("Gagal mengirim offer ke server".to_string()));
                return;
            }

            // ── Tunggu answer dari server (maks 15 detik) ────────────────────
            // answer_rx: Receiver<Result<Value, String>>
            // Awaiting: Result<Result<Value, String>, Canceled> — dua lapisan Result.
            let answer_json: serde_json::Value = match futures::future::select(
                Box::pin(answer_rx),
                Box::pin(gloo_timers::future::TimeoutFuture::new(15_000)),
            )
            .await
            {
                // Outer Ok = channel tidak dropped; inner Ok = server kirim answer JSON.
                futures::future::Either::Left((Ok(Ok(v)), _)) => v,
                // Outer Ok, inner Err = server kirim pesan error (misalnya "No active stream").
                futures::future::Either::Left((Ok(Err(e)), _)) => {
                    error_msg.set(Some(format!("Gagal terhubung: {e}")));
                    return;
                }
                // Outer Err = channel dropped (WS ditutup sebelum answer diterima).
                futures::future::Either::Left((Err(_), _)) => {
                    error_msg.set(Some("WS ditutup sebelum answer diterima".to_string()));
                    return;
                }
                futures::future::Either::Right(_) => {
                    error_msg.set(Some("Server tidak merespons (timeout 15 s)".to_string()));
                    return;
                }
            };

            // Periksa apakah server mengirim error
            if answer_json.get("type").and_then(|t| t.as_str()) == Some("error") {
                let msg = answer_json["message"].as_str().unwrap_or("Unknown error");
                error_msg.set(Some(format!("Gagal terhubung: {msg}")));
                return;
            }

            let answer_sdp = match answer_json["sdp"].as_str() {
                Some(s) => s.to_string(),
                None => {
                    error_msg.set(Some("Tidak ada SDP dalam answer".to_string()));
                    return;
                }
            };

            // ── setRemoteDescription dengan answer SDP ────────────────────────
            let answer_desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
            answer_desc.set_sdp(&answer_sdp);

            if let Err(e) = wasm_bindgen_futures::JsFuture::from(
                peer_connection.set_remote_description(&answer_desc),
            )
            .await
            {
                error_msg.set(Some(format!("setRemoteDescription gagal: {:?}", e)));
                return;
            }

            // Pegang keempat closure selama sesi. Set_value me-REPLACE grup lama
            // (bila reconnect) → closure sesi sebelumnya otomatis di-drop.
            rtc_closures.set_value(Some(SendWrapper::new(ViewerRtcClosures {
                _on_track: on_track,
                _on_msg: on_msg,
                _on_err: on_err,
                _on_ice: on_ice,
            })));
            pc.set(Some(SendWrapper::new(peer_connection)));
            is_playing.set(true);
        }
    });

    // Auto-join sekali saat dipasang (feed lives): langsung connect tanpa tap.
    if autoplay {
        Effect::new(move |prev: Option<()>| {
            if prev.is_none() {
                connect.dispatch(());
            }
        });
    }

    let disconnect = Action::new_local(move |_: &()| {
        let pc = pc;
        let is_playing = is_playing;
        let sig_ws = sig_ws;
        let rtc_closures = rtc_closures;

        async move {
            if let Some(conn) = pc.get_untracked() {
                // Lepas handler SEBELUM drop closure (cegah "closure invoked
                // after drop"); close() juga menghentikan product lanjutan.
                conn.set_ontrack(None);
                conn.set_onicecandidate(None);
                let _ = conn.close();
            }
            pc.set(None);
            // Tutup WS → server memanggil remove_subscriber secara otomatis.
            // Tidak perlu lagi HTTP DELETE /subscribe/{id}.
            if let Some(ws) = sig_ws.get_untracked() {
                ws.set_onmessage(None);
                ws.set_onerror(None);
                let _ = ws.close();
            }
            sig_ws.set(None);
            // Drop keempat closure sesi ini → tak bocor.
            rtc_closures.set_value(None);
            is_playing.set(false);
            is_muted.set(true);
        }
    });

    on_cleanup(move || {
        if let Some(conn) = pc.get_untracked() {
            conn.set_ontrack(None);
            conn.set_onicecandidate(None);
            let _ = conn.close();
        }
        // Menutup WS secara otomatis memanggil remove_subscriber di server
        // (live_subscribe_ws_loop mendeteksi disconnect dan memanggil remove_subscriber).
        // Ini menangani: navigasi keluar, tutup tab, refresh saat masih menonton.
        if let Some(ws) = sig_ws.get_untracked() {
            ws.set_onmessage(None);
            ws.set_onerror(None);
            let _ = ws.close();
        }
        // Drop closure RTC/WS → tak bocor saat komponen unmount.
        rtc_closures.set_value(None);
    });

    // ── Reactive srcObject Effect ────────────────────────────────────────────
    // Diletakkan di level komponen (bukan di dalam Action) agar:
    //   1. video_ref.get() selalu bekerja — Effect berjalan di dalam sistem
    //      reaktif Leptos sehingga NodeRef terlacak dan selalu valid.
    //   2. play() dijadwalkan ulang tiap kali stream berubah (reconnect) tanpa
    //      perlu menyentuh DOM secara manual dari dalam wasm_bindgen Closure.
    //   3. Promise rejection dari play() ditangani via spawn_local (tidak
    //      dibuang diam-diam), sehingga konsol tidak penuh "Uncaught in promise".
    //
    // Alur: ontrack Closure → remote_stream.set() → Effect ini berjalan →
    //       set_src_object → play().
    Effect::new(move |_| {
        let Some(video) = video_ref.get() else { return };
        match remote_stream.get() {
            Some(stream) => {
                // set_src_object mungkin mengembalikan error jika element
                // sedang di-garbage-collect — abaikan saja.
                let _ = video.set_src_object(Some(&*stream));

                // Set property `muted` secara eksplisit (atribut `muted` tidak
                // selalu ter-refleksi ke property). Tanpa ini, autoplay media
                // ber-audio ditolak browser → video tetap hitam/pause.
                video.set_muted(true);

                // play() robust dengan retry. Safari sering MENOLAK play() saat
                // dipanggil tepat setelah set_src_object (metadata belum siap)
                // → video tetap pause/hitam walau frame sudah mengalir. Coba
                // berulang dengan jeda pendek sampai benar-benar berjalan; berhenti
                // begitu elemen tidak lagi `paused` (mis. user menekan kontrol).
                let video = video.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    use std::time::Duration;
                    for _ in 0..25u8 {
                        if !video.paused() {
                            break;
                        }
                        // Pastikan tetap muted tiap percobaan (syarat autoplay).
                        video.set_muted(true);
                        if let Ok(promise) = video.play() {
                            if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
                                break;
                            }
                        }
                        gloo_timers::future::sleep(Duration::from_millis(200)).await;
                    }
                });
            }
            None => {
                // Stream dihentikan (disconnect) — bersihkan srcObject agar
                // elemen video tidak menahan referensi lama (memory leak).
                video.set_src_object(None);
            }
        }
    });

    // ── Pratinjau: hanya video ───────────────────────────────────────────────
    // Dikembalikan LEBIH AWAL, sebagai pohon yang benar-benar terpisah — bukan
    // markup penuh yang bagiannya disembunyikan CSS. Kartu di `/lives` adalah
    // sebuah `<button>`, dan menyisipkan tombol (unmute, keluar, tonton) ke
    // dalam tombol lain adalah HTML tak sah yang membuat ketukan kartu berhenti
    // bekerja di sebagian peramban. Yang tak dirender tak bisa melakukan itu.
    if preview {
        return view! {
            <div class="live-preview">
                <video
                    node_ref=video_ref
                    class=move || {
                        if is_playing.get() {
                            "live-preview-video live-preview-video--on"
                        } else {
                            "live-preview-video"
                        }
                    }
                    autoplay=true
                    muted=true
                    playsinline=true
                />
                // Sampai frame pertama tiba, kartunya tetap butuh sesuatu untuk
                // ditampilkan — kalau tidak, yang terlihat adalah kotak hitam
                // yang tak bisa dibedakan dari siaran yang gagal dimuat.
                {move || {
                    (!is_playing.get())
                        .then(|| {
                            view! {
                                <span class="live-preview-spinner" aria-hidden="true"></span>
                            }
                        })
                }}
            </div>
        }
        .into_any();
    }

    view! {
        <div class="live-viewer">
            <div class="live-viewer-header">
                {move || {
                    if !merchant_name.get().is_empty() {
                        view! {
                            <span class="live-viewer-merchant">{move || merchant_name.get()}</span>
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
                {move || {
                    if is_playing.get() {
                        view! {
                            <span class="live-viewer-badge">
                                <span class="live-viewer-dot"></span>
                                "LIVE"
                            </span>
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>

            <div class="live-viewer-video-wrap">
                <video
                    node_ref=video_ref
                    class="live-viewer-video"
                    autoplay=true
                    // muted WAJIB: browser memblokir autoplay media ber-audio tanpa
                    // gesture. Mulai muted agar video langsung jalan; user unmute
                    // lewat tombol kustom di bawah (gesture → suara dijamin nyala).
                    muted=true
                    playsinline=true
                    poster="/live-poster.svg"
                />
                {move || {
                    if is_playing.get() {
                        // Saat masih muted, tampilkan tombol kustom "ketuk untuk
                        // suara". Klik = gesture user → set_muted(false) + play()
                        // dijamin diizinkan browser (termasuk Safari yang ketat).
                        if is_muted.get() {
                            view! {
                                // Seluruh area video bisa diketuk untuk menyalakan
                                // suara. Klik = gesture user → set_muted(false) +
                                // play() (dijamin diizinkan browser, termasuk Safari).
                                <button
                                    class="live-viewer-unmute-overlay"
                                    on:click=move |_| {
                                        if let Some(v) = video_ref.get_untracked() {
                                            v.set_muted(false);
                                            if let Ok(p) = v.play() {
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                                                });
                                            }
                                        }
                                        is_muted.set(false);
                                    }
                                >
                                    <span class="live-viewer-unmute-pill">
                                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                             stroke-linejoin="round">
                                            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
                                            <line x1="23" y1="9" x2="17" y2="15"/>
                                            <line x1="17" y1="9" x2="23" y2="15"/>
                                        </svg>
                                        "Ketuk untuk suara"
                                    </span>
                                </button>
                            }
                                .into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }
                    } else if connect.pending().get() {
                        view! {
                            <div class="live-viewer-overlay">
                                <span class="live-viewer-spinner"></span>
                                <p>"Menghubungkan..."</p>
                            </div>
                        }
                            .into_any()
                    } else {
                        // Seluruh overlay bisa diklik untuk bergabung.
                        view! {
                            <button
                                class="live-viewer-overlay live-viewer-overlay--btn"
                                on:click=move |_| { connect.dispatch(()); }
                            >
                                <span class="live-viewer-play">
                                    <svg width="22" height="22" viewBox="0 0 24 24"
                                         fill="currentColor">
                                        <polygon points="6 4 20 12 6 20 6 4" />
                                    </svg>
                                </span>
                                <p>"Ketuk untuk menonton siaran langsung"</p>
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            <div class="live-viewer-controls">
                <span class="live-viewer-viewers">
                    <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <path d="M23 21v-2a4 4 0 00-3-3.87" />
                        <path d="M16 3.13a4 4 0 010 7.75" />
                    </svg>
                    {move || format!("{}", viewer_count.get())}
                </span>

                {move || {
                    if is_playing.get() {
                        view! {
                            <button
                                class="live-viewer-btn live-viewer-btn--leave"
                                on:click=move |_| {
                                    disconnect.dispatch(());
                                }
                            >
                                "Keluar"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <button
                                class="live-viewer-btn live-viewer-btn--join"
                                prop:disabled=move || connect.pending().get()
                                on:click=move |_| {
                                    connect.dispatch(());
                                }
                            >
                                <svg
                                    width="14"
                                    height="14"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2.5"
                                    stroke-linecap="round"
                                >
                                    <polygon points="5 3 19 12 5 21 5 3" />
                                </svg>
                                {move || if connect.pending().get() { "Menghubungkan" } else { "Tonton" }}
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            {move || error_msg.get().map(|e| view! { <div class="live-viewer-error">{e}</div> })}

            // ── Keranjang kuning ────────────────────────────────────────────
            // Hanya muncul bila merchant memang memilih produk. Tombol keranjang
            // yang membuka daftar kosong lebih buruk daripada tak ada tombol:
            // ia menjanjikan sesuatu untuk dibeli lalu tak memberi apa pun.
            {move || {
                let n = produk_ids.with(|v| v.len());
                (n > 0)
                    .then(|| {
                        view! {
                            <button
                                class="live-bag"
                                aria-label=format!("Lihat {n} produk yang dijual di siaran ini")
                                on:click=move |_| {
                                    let buka = !keranjang_buka.get_untracked();
                                    keranjang_buka.set(buka);
                                    // Rincian diambil SAAT DIBUKA, bukan saat
                                    // bergabung. Sebagian besar penonton tak
                                    // pernah menyentuh keranjangnya, dan
                                    // mengambilnya lebih awal berarti satu
                                    // permintaan per penonton untuk data yang
                                    // tak dilihat siapa pun.
                                    if buka && produk_live.get_untracked().is_empty() {
                                        let mid = merchant_id.get_untracked();
                                        if mid.is_empty() {
                                            return;
                                        }
                                        produk_loading.set(true);
                                        leptos::task::spawn_local(async move {
                                            if let Ok(pg) =
                                                crate::web::api::get_merchant_public_products(
                                                    mid, Some(1), None, None,
                                                )
                                                .await
                                            {
                                                produk_live.set(pg.data);
                                            }
                                            produk_loading.set(false);
                                        });
                                    }
                                }
                            >
                                <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2"
                                     stroke-linecap="round" stroke-linejoin="round">
                                    <path d="M6 2L3 6v14a2 2 0 002 2h14a2 2 0 002-2V6l-3-4z"/>
                                    <line x1="3" y1="6" x2="21" y2="6"/>
                                    <path d="M16 10a4 4 0 01-8 0"/>
                                </svg>
                                <span class="live-bag-count">{n}</span>
                            </button>
                        }
                    })
            }}

            {move || {
                keranjang_buka
                    .get()
                    .then(|| {
                        let ids = produk_ids.get();
                        // Disaring di klien: `produk_live` berisi produk toko
                        // ini, dan yang dijual di siaran hanyalah sebagiannya.
                        // Urutannya mengikuti pilihan merchant, bukan urutan
                        // datangnya dari server.
                        let list: Vec<_> = ids
                            .iter()
                            .filter_map(|id| {
                                produk_live.with(|v| v.iter().find(|p| &p.id == id).cloned())
                            })
                            .collect();
                        view! {
                            <div class="live-bag-sheet">
                                <div class="live-bag-head">
                                    <span class="live-bag-title">"Dijual di siaran ini"</span>
                                    <button
                                        class="live-bag-x"
                                        aria-label="Tutup"
                                        on:click=move |_| keranjang_buka.set(false)
                                    >
                                        "✕"
                                    </button>
                                </div>
                                {if produk_loading.get() {
                                    view! { <p class="live-bag-info">"Memuat…"</p> }.into_any()
                                } else if list.is_empty() {
                                    view! {
                                        <p class="live-bag-info">
                                            "Produknya sedang tidak tersedia."
                                        </p>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="live-bag-list">
                                            {list
                                                .into_iter()
                                                .map(|p| {
                                                    let href = format!("/products/{}", p.slug);
                                                    let harga = crate::web::utils::format_idr(
                                                        p.display_price as i64,
                                                    );
                                                    view! {
                                                        <a class="live-bag-item" href=href>
                                                            <img
                                                                class="live-bag-img"
                                                                src=p.cover_url.clone()
                                                                alt=""
                                                                loading="lazy"
                                                            />
                                                            <div class="live-bag-body">
                                                                <span class="live-bag-name">
                                                                    {p.name.clone()}
                                                                </span>
                                                                <span class="live-bag-price">{harga}</span>
                                                            </div>
                                                            <span class="live-bag-go">"Beli"</span>
                                                        </a>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    }
                                        .into_any()
                                }}
                            </div>
                        }
                    })
            }}
        </div>
    }
    .into_any()
}
