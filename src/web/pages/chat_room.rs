//! chat_room.rs — Grup Chat Room (SSR + WASM WebSocket).
//!
//! SSR: room info + message history via Resource/Suspense.
//! WASM: WebSocket real-time connection in #[cfg(target_arch = "wasm32")] Effect.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_chat_history, get_chat_room_detail};
use crate::web::app::AuthResource;
use crate::web::models::ChatMessage;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_time_ms(ms: u64) -> String {
    let secs = ms / 1000 + 7 * 3600; // WIB offset
    let hours = (secs / 3600) % 24;
    let mins  = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}

// ── Component ─────────────────────────────────────────────────────────────────

/// Gulir daftar pesan ke dasar SETELAH DOM digambar.
///
/// ── KENAPA HARUS DITUNDA ────────────────────────────────────────────────
/// `RwSignal::update` hanya MENANDAI sinyalnya kotor; Leptos merender di
/// microtask berikutnya. Membaca `scroll_height()` tepat sesudahnya karena itu
/// mengukur daftar yang BELUM memuat pesan baru — hasilnya menggulir ke dasar
/// yang LAMA, dan pesan yang baru tiba mendarat tepat di bawah lipatan layar.
/// Dari sisi pengguna itu terlihat seperti pesannya tak pernah masuk.
///
/// DUA rAF, bukan satu: yang pertama menunggu Leptos menerapkan perubahannya,
/// yang kedua menunggu peramban selesai menata ulang. Dengan satu rAF saja,
/// pesan yang tingginya membungkus ke dua baris masih terukur setinggi satu.
///
/// `once_into_js`, bukan `once(..) + forget()`: `forget()` adalah `mem::forget`,
/// jadi pembungkus closure dan slot tabel fungsi wasm-nya tak pernah dibebaskan
/// — satu kebocoran kecil per pesan masuk, tanpa batas.
/// Bawa tampilan ke pesan terbaru, apa pun yang sebenarnya menggulir.
///
/// ── KENAPA TIDAK LANGSUNG `set_scroll_top` SAJA ──────────────────────────
/// Elemen yang menggulir ditentukan oleh CSS, dan itu pernah salah tanpa
/// menimbulkan galat apa pun: saat `.chat-page` masih `min-height` alih-alih
/// tinggi tetap, daftar pesannya tak pernah meluap sehingga yang tergulir
/// adalah HALAMAN. `set_scroll_top` pada wadah yang tak bisa menggulir tidak
/// melempar apa-apa — ia hanya diam, dan seluruh perilaku gulir mati tanpa
/// satu pun petunjuk di mana pun.
///
/// Jadi tanyakan dulu, jangan berasumsi: kalau wadahnya memang meluap, gulir
/// wadahnya; kalau tidak, yang meluap pasti halamannya.
#[cfg(target_arch = "wasm32")]
fn ke_pesan_terbaru(list_ref: NodeRef<leptos::html::Div>) {
    let Some(el) = list_ref.get_untracked() else { return };
    if el.scroll_height() > el.client_height() {
        el.set_scroll_top(el.scroll_height());
        return;
    }
    let Some(win) = web_sys::window() else { return };
    let tinggi = win
        .document()
        .and_then(|d| d.document_element())
        .map(|d| d.scroll_height())
        .unwrap_or(0);
    win.scroll_to_with_x_and_y(0.0, tinggi as f64);
}

/// Gulir ke dasar saat ruangan BARU dibuka, dan bertahan sampai tata letaknya
/// benar-benar diam.
///
/// ── KENAPA SATU KALI TIDAK CUKUP ─────────────────────────────────────────
/// `gulir_ke_dasar` menunggu dua bingkai — cukup untuk pesan yang menyusul satu
/// per satu, karena saat itu tata letaknya sudah mapan. Muat AWAL berbeda:
/// tingginya masih terus berubah sesudah bingkai kedua. Suspense menukar
/// shimmer dengan isi sungguhan, fon memuat lalu semua gelembung dihitung ulang
/// dan sebagian membungkus ke baris kedua. Tiap perubahan itu menambah tinggi
/// SESUDAH kita menggulir, dan hasilnya berhenti menggantung di tengah — yang
/// terlihat oleh orangnya sebagai "harus menggulir sendiri jauh ke bawah".
///
/// Jadi diulang beberapa kali selama sedetik pertama. Percobaan berhenti begitu
/// orangnya menyentuh gulirannya sendiri: menyeret balik ke dasar seseorang
/// yang sedang membaca ke atas jauh lebih buruk daripada mendarat kurang pas.
#[cfg(target_arch = "wasm32")]
fn gulir_awal(list_ref: NodeRef<leptos::html::Div>, dibatalkan: StoredValue<bool>) {
    use wasm_bindgen::JsCast;
    // 0 dan 60ms menangkap kasus lazim; 200/450/900 menangkap fon dan gambar
    // yang datang belakangan. Sesudah itu tata letaknya boleh dianggap diam.
    for tunda in [0, 60, 200, 450, 900, 1500] {
        let Some(win) = web_sys::window() else { return };
        let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
            if dibatalkan.get_value() {
                return;
            }
            ke_pesan_terbaru(list_ref);
        });
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            cb.unchecked_ref(),
            tunda,
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn gulir_ke_dasar(list_ref: NodeRef<leptos::html::Div>) {
    let Some(win) = web_sys::window() else { return };
    let w2 = win.clone();
    let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
        let dalam = wasm_bindgen::closure::Closure::once_into_js(move || {
            ke_pesan_terbaru(list_ref);
        });
        let _ = w2.request_animation_frame(dalam.unchecked_ref());
    });
    use wasm_bindgen::JsCast;
    let _ = win.request_animation_frame(cb.unchecked_ref());
}

#[component]
pub fn ChatRoomPage() -> impl IntoView {
    let params  = use_params_map();
    let room_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();
    let current_user_id = move || {
        auth.get().and_then(|r| r.ok()).flatten().map(|u| u.id)
    };
    // Pasangan untracked untuk dipakai DI DALAM Effect pendengar bus — lihat
    // alasannya di sana. Hanya ada di wasm; di SSR tak ada peristiwa WS sama
    // sekali, jadi tanpa pagar ini keduanya jadi kode mati yang berisik.
    #[cfg(target_arch = "wasm32")]
    let current_user_id_untracked = move || {
        auth.get_untracked().and_then(|r| r.ok()).flatten().map(|u| u.id)
    };
    #[cfg(target_arch = "wasm32")]
    let room_id_untracked = move || {
        params.get_untracked().get("id").unwrap_or_default()
    };

    let room = Resource::new(
        move || (room_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_chat_room_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    let history = Resource::new(
        move || (room_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_chat_history(id).await
            } else {
                Ok(vec![])
            }
        },
    );

    // Satu-satunya koneksi WS aplikasi, disediakan di root. Dideklarasikan
    // sebelum Effect mana pun di bawah — beberapa di antaranya memakainya.
    let bus = crate::web::components::use_chat_bus();

    let text_input  = RwSignal::new(String::new());
    let error_msg   = RwSignal::new(String::new());
    let live_msgs: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![]);

    let msg_list_ref = NodeRef::<leptos::html::Div>::new();

    // Ditandai saat orangnya menggulir sendiri. Percobaan gulir awal berhenti
    // seketika sesudahnya — lihat `gulir_awal`.
    #[cfg(target_arch = "wasm32")]
    let gulir_manual: StoredValue<bool> = StoredValue::new(false);

    // ── Pemisah "pesan baru" ────────────────────────────────────────────────
    // Diambil SEKALI saat halaman dibuka, lalu dibekukan. Kalau ia mengikuti
    // hitungan yang hidup, garisnya akan melompat tiap ada pesan masuk — dan
    // yang dicari orang justru "sampai mana tadi saya membaca", yaitu titik
    // yang TIDAK boleh bergerak selama ia masih membacanya.
    let batas_belum = RwSignal::new(0usize);
    // Sudah di dasar daftar? Menentukan tombol gulir tampil atau tidak.
    let di_dasar = RwSignal::new(true);
    // Pesan yang TIBA saat layar tidak berada di dasar — yaitu pesan yang
    // benar-benar belum terlihat mata. Berbeda dari `batas_belum`, yang dibaca
    // sekali dari server saat halaman dibuka: yang ini hidup selama halaman
    // terbuka dan kembali nol begitu orang menggulir turun.
    let baru_masuk = RwSignal::new(0usize);

    // Pengumuman DALAM ruangan. Menambahkan gelembung di dasar daftar bukan
    // pemberitahuan: kalau layar sudah di dasar, pesannya cuma muncul begitu
    // saja dan mata yang sedang tidak menatap ke sana tak punya apa pun yang
    // menarik perhatiannya. Yang hilang selama ini bukan pesannya — melainkan
    // kabar bahwa ada pesan. Isi: (nama pengirim, sepenggal isi).
    let pengumuman: RwSignal<Option<(String, String)>> = RwSignal::new(None);
    // Id pesan yang barusan tiba, untuk menyorot gelembungnya sekejap. Sorotan
    // menjawab pertanyaan lanjutan yang selalu datang sesudah "ada pesan baru":
    // yang mana.
    let sorot: RwSignal<Option<String>> = RwSignal::new(None);
    // Berapa pesan orang lain yang masuk sejak terakhir kali kita menanggapi.
    // Pil kabar di atas padam sesudah beberapa detik — kalau saat itu mata
    // sedang tidak di layar, kabarnya lewat. Hitungan ini yang tinggal, dan ia
    // TIDAK padam sendiri.
    //
    // Berbeda dari `batas_belum`, yang dibaca SEKALI dari server saat halaman
    // dibuka dan hanya berlaku untuk riwayat.
    let jml_baru = RwSignal::new(0usize);
    // Naik tiap ada pengumuman baru. Timer penutup yang dijadwalkan pengumuman
    // LAMA tidak boleh menutup pengumuman yang lebih muda — tanpa penanda ini,
    // dua pesan yang datang beriringan membuat kabar kedua lenyap seketika saat
    // timer pertama jatuh tempo.
    #[cfg(target_arch = "wasm32")]
    let umur_umum: StoredValue<u32> = StoredValue::new(0);

    // Toast global — untuk pesan dari room LAIN. Koneksi ini menerima pesan
    // semua room milik pengguna, jadi pesan room lain tetap sampai ke sini.
    #[cfg(target_arch = "wasm32")]
    let toast = crate::web::components::use_toast();

    // Ambil hitungan belum-dibaca SEBELUM menandainya dibaca, lalu tandai.
    // Urutannya penting: kebalikannya selalu menghasilkan nol, dan pemisahnya
    // tak akan pernah muncul untuk siapa pun.
    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        let id = room_id();
        if id.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            if let Ok(rooms) = crate::web::api::get_chat_rooms().await {
                if let Some(r) = rooms.iter().find(|r| r.id == id) {
                    batas_belum.set(r.unread_count.max(0) as usize);
                }
            }
            let _ = crate::web::api::mark_chat_read(id.clone()).await;
            // Lencana navbar ikut turun seketika. Tanpa ini ia tetap menyala
            // sampai kunjungan berikutnya ke `/pulse` — memberi tahu ada pesan
            // menunggu, padahal pesannya sedang dibaca saat itu juga.
            if let Some(b) = bus {
                b.tandai_dibaca(&id);
            }
        });
    });


    // ── WebSocket ─────────────────────────────────────────────────────────────
    // Memakai langganan yang SAMA dengan daftar `/pulse`. Dulu halaman ini
    // merakit koneksinya sendiri — lengkap dengan salinan watchdog sambung-ulang
    // dan bersih-bersihnya — sehingga dua tempat yang harus bersikap identik
    // punya dua peluang berbeda untuk salah, dan hanya satu yang pernah
    // diperbaiki tiap kali.
    // Menumpang koneksi milik bus, tidak membuka sendiri: server menyimpan sesi
    // per `user_id`, jadi koneksi kedua dari tab yang sama akan MENGGANTIKAN
    // yang pertama — lencana navbar dan ruangan ini akan saling mematikan.
    //
    // Semua bacaan sinyal di dalam Effect ini WAJIB untracked. Effect hanya
    // boleh bangun oleh `peristiwa`; kalau `auth` (atau apa pun) ikut terlacak,
    // perubahannya akan menjalankan ulang Effect ini dengan peristiwa LAMA yang
    // masih tersimpan di slot — dan pesan yang sama diproses dua kali.
    Effect::new(move |_| {
        let Some(evt) = bus.and_then(|b| b.peristiwa.get()) else { return };
        #[cfg(target_arch = "wasm32")]
        {
            match evt.get("type").and_then(|t| t.as_str()) {
                Some("new_message") => {
                    // Bidangnya rata di tingkat atas — `#[serde(tag="type")]`
                    // melarutkan varian newtype. `message_type` punya alias
                    // `msg_type` agar cocok dengan nama di server.
                    let Ok(m) = serde_json::from_value::<ChatMessage>(evt.clone()) else {
                        return;
                    };
                    let my_id = current_user_id_untracked().unwrap_or_default();
                    // Koneksi ini menerima pesan SELURUH room milik pengguna
                    // (server mendaftarkan semuanya saat Hello), jadi yang dari
                    // room lain harus disaring — tapi jangan dibuang diam-diam:
                    // munculkan toast yang menuju ke sana.
                    if m.room_id.trim() != room_id_untracked().trim() {
                        if m.sender_id != my_id {
                            let preview: String = m.content.chars().take(60).collect();
                            toast.notify(
                                crate::web::components::ToastKind::Info,
                                format!("Pesan dari {}", m.sender_name),
                                Some(preview),
                                Some(format!("/pulse/{}", m.room_id)),
                            );
                        }
                        return;
                    }
                    let m_sendiri = m.sender_id == my_id;

                    // Diukur DARI DOM, di sini, sebelum pesannya ditambahkan —
                    // bukan dibaca dari sinyal `di_dasar`.
                    //
                    // Sinyal itu hanya diperbarui oleh peristiwa `scroll`,
                    // padahal wadahnya berubah tinggi tanpa ada gulir sama
                    // sekali: shimmer berganti isi sungguhan, avatar selesai
                    // dimuat, gelembung membungkus ke baris kedua. Sesudah
                    // salah satu dari itu, catatannya basi — biasanya basi
                    // `false`, dan penerima berhenti disusulkan ke dasar
                    // selamanya sampai ia menggulir manual.
                    //
                    // Diukur SEBELUM `push`: sesudahnya `scroll_height` sudah
                    // termasuk pesan barunya, dan jawabannya selalu "tidak di
                    // dasar" — yaitu tepat kesalahan yang hendak dihindari.
                    let sudah_di_dasar = msg_list_ref
                        .get_untracked()
                        .map(|el| {
                            // Ambang 40px, bukan nol: gulir mulus jarang
                            // berhenti tepat di dasar.
                            el.scroll_height() - el.scroll_top() - el.client_height() <= 40
                        })
                        // Belum terpasang = belum ada yang bisa tergulir lewat.
                        .unwrap_or(true);
                    di_dasar.set(sudah_di_dasar);

                    // Diambil SEBELUM `m` dipindahkan ke dalam Vec.
                    let nama_kirim = m.sender_name.clone();
                    let cuplikan: String = m.content.chars().take(70).collect();
                    let id_baru = m.id.clone();
                    live_msgs.update(|v| {
                        // Server sudah mengesahkan id ini.
                        if v.iter().any(|x| x.id == m.id) {
                            return;
                        }
                        // Pesan sendiri: gabungkan dengan entri optimistis yang
                        // cocok, jangan dorong entri baru. Server mengirim DUA
                        // hal ke pengirim — siaran `new_message` ke semua anggota
                        // dan `ack` — dan tanpa penggabungan ini yang mana pun
                        // yang tiba lebih dulu akan menggandakan pesannya.
                        if m.sender_id == my_id {
                            if let Some(opt) = v
                                .iter_mut()
                                .find(|x| x.id.starts_with("_opt_") && x.content == m.content)
                            {
                                opt.id = m.id.clone();
                                opt.sent_at = m.sent_at;
                                return;
                            }
                        }
                        v.push(m);
                    });

                    // Kabar masuk — tampil TERLEPAS dari posisi gulir. Ini
                    // bedanya dengan `baru_masuk` di bawah, yang hanya bicara
                    // pada orang yang sedang tersesat di tengah riwayat.
                    if !m_sendiri {
                        jml_baru.update(|n| *n += 1);
                        sorot.set(Some(id_baru));
                        pengumuman.set(Some((nama_kirim, cuplikan)));
                        umur_umum.update_value(|n| *n += 1);
                        let generasi = umur_umum.get_value();
                        set_timeout(
                            move || {
                                if umur_umum.get_value() == generasi {
                                    pengumuman.set(None);
                                    sorot.set(None);
                                }
                            },
                            std::time::Duration::from_millis(4500),
                        );
                    }

                    // Hanya menyusul ke dasar bila memang SEDANG di dasar.
                    // Menyeret orang yang sengaja menggulir ke atas membaca
                    // riwayat adalah perilaku paling menjengkelkan di aplikasi
                    // obrolan mana pun — dan tombol "ke pesan terbaru" sudah
                    // tampil untuk mereka, jadi jalan kembalinya tetap ada.
                    //
                    // Pesan SENDIRI selalu menggulir: menekan kirim adalah
                    // pernyataan bahwa yang ingin dilihat adalah yang barusan
                    // dikirim.
                    if sudah_di_dasar || m_sendiri {
                        gulir_ke_dasar(msg_list_ref);
                        di_dasar.set(true);
                        baru_masuk.set(0);
                    } else {
                        // Tidak di dasar: JANGAN diseret, tapi beri tahu. Tanpa
                        // hitungan ini yang tampil cuma panah polos — ia memberi
                        // tahu "ada bawah", bukan "ada yang baru", dan keduanya
                        // pertanyaan yang berbeda.
                        baru_masuk.update(|n| *n += 1);
                    }
                }
                Some("ack") => {
                    // Tukar id optimistis (client_id) dengan msg_id sungguhan.
                    if let (Some(msg_id), Some(client_id)) = (
                        evt.get("msg_id").and_then(|v| v.as_str()),
                        evt.get("client_id").and_then(|v| v.as_str()),
                    ) {
                        let msg_id = msg_id.to_string();
                        let client_id = client_id.to_string();
                        live_msgs.update(|v| {
                            if let Some(m) = v.iter_mut().find(|m| m.id == client_id) {
                                m.id = msg_id;
                            }
                        });
                    }
                }
                _ => {}
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = evt;
    });
    let ws_ready = bus
        .map(|b| b.siap())
        .unwrap_or_else(|| RwSignal::new(false));

    #[cfg(target_arch = "wasm32")]
    {
        // Gulir ke dasar begitu riwayat selesai dimuat.
        //
        // Memakai helper yang SAMA dengan jalur pesan masuk. Dulu blok ini punya
        // salinan rAF-nya sendiri — satu rAF, bukan dua — sehingga muat awal
        // kadang berhenti beberapa piksel di atas dasar pada percakapan yang
        // pesannya membungkus ke dua baris. Satu tempat, satu perilaku.
        Effect::new(move |_| {
            if history.get().is_some() {
                gulir_awal(msg_list_ref, gulir_manual);
            }
        });
    }

    let do_send = move || {
        let content = text_input.get_untracked().trim().to_string();
        if content.is_empty() { return; }
        let client_id = format!("_opt_{}", js_sys_now());
        text_input.set(String::new());
        let me_id = current_user_id().unwrap_or_default();
        let msg = ChatMessage {
            id: client_id.clone(),
            room_id: room_id(),
            sender_id: me_id,
            sender_name: "You".into(),
            content: content.clone(),
            sent_at: 0,
            message_type: "text".into(),
        };
        live_msgs.update(|v| v.push(msg));
        // Membalas adalah bukti paling kuat bahwa yang di atas sudah terbaca —
        // lebih kuat daripada gulir, yang bisa saja tak sengaja.
        jml_baru.set(0);
        // Pesan sendiri SELALU menggulir: menekan kirim adalah pernyataan bahwa
        // yang ingin dilihat adalah yang barusan dikirim. Tanpa ini, mengirim
        // dari tengah riwayat membuat pesan sendiri lenyap ke bawah lipatan.
        #[cfg(target_arch = "wasm32")]
        {
            gulir_ke_dasar(msg_list_ref);
            di_dasar.set(true);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let payload = serde_json::json!({
                "type": "send_text",
                "room_id": room_id(),
                "content": content,
                "client_id": client_id,
            }).to_string();
            match bus {
                Some(b) => {
                    if let Err(pesan) = b.kirim(&payload) {
                        error_msg.set(pesan.into());
                    }
                }
                None => error_msg.set("Tidak terhubung.".into()),
            }
        }
    };

    view! {
        <div class="chat-page">

            // ── Sticky header ──────────────────────────────────────────────────
            <Suspense fallback=|| view! {
                <header class="chat-header">
                    <div class="shim chat-shimmer-avatar" style="width:36px;height:36px;border-radius:50%"></div>
                    <div style="flex:1;display:flex;flex-direction:column;gap:4px">
                        <div class="shim" style="width:120px;height:14px;border-radius:4px"></div>
                        <div class="shim" style="width:80px;height:10px;border-radius:4px"></div>
                    </div>
                </header>
            }>
                {move || room.get().map(|res| {
                    let (name, cover, count) = match res {
                        Ok(r) => (r.name, r.cover_url, r.member_count),
                        _ => (String::new(), None, 0),
                    };
                    view! {
                        <header class="chat-header">
                            <A href="/pulse" attr:class="chat-back-btn" attr:aria-label="Kembali">
                                <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <polyline points="15 18 9 12 15 6"/>
                                </svg>
                            </A>
                            <div class="chat-header-avatar-wrap">
                                {match cover {
                                    Some(url) => view! { <img src=url class="chat-header-avatar" alt=name.clone()/> }.into_any(),
                                    None => view! { <div class="chat-header-avatar-placeholder">"🎪"</div> }.into_any(),
                                }}
                            </div>
                            <div class="chat-header-info">
                                <span class="chat-header-name">{name}</span>
                                <span class="chat-header-sub">
                                    {format!("{count} PULSING  ·  ")}
                                    {move || if ws_ready.get() {
                                        view! { <span class="chat-status-live">"● LIVE"</span> }.into_any()
                                    } else {
                                        view! { <span class="chat-status-connecting">"○ CONNECTING"</span> }.into_any()
                                    }}
                                    // Bersebelahan dengan status koneksi: dua
                                    // hal yang sama-sama menjawab "apa yang
                                    // sedang terjadi di ruangan ini", jadi mata
                                    // cukup singgah di satu tempat. Sebagai
                                    // penanda melayang ia justru bersaing
                                    // dengan pil kabar yang tampil bersamaan.
                                    {move || {
                                        let n = jml_baru.get();
                                        (n > 0).then(|| view! {
                                            <span class="chat-tanda-baru">
                                                {format!("{n} pesan baru")}
                                            </span>
                                        })
                                    }}
                                </span>
                            </div>
                            <div class="chat-header-actions">
                                <button class="chat-icon-btn" aria-label="Search">
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <circle cx="11" cy="11" r="8"/>
                                        <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                                    </svg>
                                </button>
                            </div>
                        </header>
                    }.into_any()
                }).unwrap_or_else(|| view! { <header class="chat-header"/> }.into_any())}
            </Suspense>

            // ── Messages ───────────────────────────────────────────────────────
            <div
                class="chat-messages"
                node_ref=msg_list_ref
                // Isyarat "orangnya menggulir sendiri" HARUS datang dari
                // gerakan tangan, bukan dari `on:scroll`.
                //
                // `set_scroll_top` juga memicu `scroll`, dan peristiwa itu tiba
                // SESUDAH tata letak berubah — jadi percobaan gulir awal yang
                // pertama (saat isinya belum selesai ditata) menghasilkan
                // peristiwa dengan sisa yang besar, yang terbaca sebagai "orang
                // ini menggulir ke atas". Penandanya menyala dan seluruh
                // percobaan berikutnya membatalkan diri: mekanisme retry-nya
                // mematikan dirinya sendiri pada langkah pertama.
                //
                // `wheel` dan `touchstart` tak bisa dipalsukan oleh penggulir
                // mana pun. Keduanya hanya lahir dari jari dan roda tetikus.
                on:wheel=move |_| {
                    #[cfg(target_arch = "wasm32")]
                    gulir_manual.set_value(true);
                }
                on:touchstart=move |_| {
                    #[cfg(target_arch = "wasm32")]
                    gulir_manual.set_value(true);
                }
                on:scroll=move |ev| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        let Some(el) = ev
                            .target()
                            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        else { return };
                        // Ambang 40px, bukan nol: gulir mulus jarang berhenti
                        // tepat di dasar, dan tanpa toleransi ini tombolnya
                        // berkedip muncul-hilang di akhir tiap guliran.
                        let sisa = el.scroll_height() - el.scroll_top() - el.client_height();
                        let bawah = sisa <= 40;
                        if di_dasar.get_untracked() != bawah {
                            di_dasar.set(bawah);
                        }
                        // Sampai di dasar = semuanya sudah terlihat.
                        if bawah && baru_masuk.get_untracked() != 0 {
                            baru_masuk.set(0);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = ev;
                }
            >
                // Masa simpan. Diletakkan DI DALAM daftar, di atas pesan
                // tertua — bukan sebagai spanduk menetap di puncak layar.
                // Aturan yang selalu terpampang berhenti dibaca dalam dua hari;
                // yang ini muncul tepat di tempat orangnya menggulir untuk
                // mencari pesan lama, yaitu satu-satunya saat pertanyaannya
                // benar-benar timbul: "yang dulu itu ke mana?"
                <div class="chat-retensi">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round"
                         stroke-linejoin="round">
                        <circle cx="12" cy="12" r="9" />
                        <polyline points="12 7 12 12 15 14" />
                    </svg>
                    <span>
                        "Pesan disimpan 30 hari. Setelah itu pesan dan gambarnya dihapus permanen dan tidak bisa dipulihkan."
                    </span>
                </div>

                <Suspense fallback=|| view! {
                    <div class="chat-shimmer-wrap">
                        <div class="chat-shimmer-row chat-shimmer-row--other">
                            <div class="shim chat-shimmer-avatar"></div>
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--sm"></div>
                        </div>
                        <div class="chat-shimmer-row chat-shimmer-row--self">
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--md"></div>
                        </div>
                        <div class="chat-shimmer-row chat-shimmer-row--other">
                            <div class="shim chat-shimmer-avatar"></div>
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--lg"></div>
                        </div>
                    </div>
                }>
                    {move || history.get().map(|res| match res {
                        Ok(hist) if hist.is_empty() => view! {
                            <div class="chat-empty-state">
                                <span class="chat-empty-icon">"💬"</span>
                                <p class="chat-empty-title">"BELUM ADA PESAN"</p>
                                <p class="chat-empty-body">"Jadilah yang pertama memulai percakapan!"</p>
                            </div>
                        }.into_any(),
                        Ok(hist) => {
                            let me = current_user_id().unwrap_or_default();
                            // Pemisah diletakkan SEBELUM pesan belum-dibaca yang
                            // PERTAMA, yaitu `n - belum` dari awal daftar —
                            // riwayat datang terurut lama→baru. Dihitung dari
                            // ekor, bukan dari kepala, supaya benar meski
                            // riwayatnya terpotong batas halaman.
                            let n = hist.len();
                            let belum = batas_belum.get_untracked().min(n);
                            let mulai = n - belum;
                            hist.into_iter()
                                .enumerate()
                                .map(|(i, msg)| {
                                    let pemisah = (belum > 0 && i == mulai).then(|| {
                                        view! {
                                            <div class="chat-pemisah-baru">
                                                <span>
                                                    {format!("{belum} pesan baru")}
                                                </span>
                                            </div>
                                        }
                                    });
                                    view! { {pemisah} {message_bubble(msg, &me, false)} }
                                })
                                .collect_view()
                                .into_any()
                        }
                        _ => view! { <div/> }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())}
                </Suspense>

                // Live WS messages appended client-side
                {move || {
                    let me = current_user_id().unwrap_or_default();
                    let disorot = sorot.get();
                    live_msgs.get().into_iter().map(|msg| {
                        let kena = disorot.as_deref() == Some(msg.id.as_str());
                        message_bubble(msg, &me, kena)
                    }).collect_view()
                }}
            </div>

            // ── Kabar di puncak ruangan ───────────────────────────────────────
            // Ditumpuk dalam SATU wadah, bukan dua elemen yang masing-masing
            // berlabuh sendiri: keduanya mengincar tempat yang sama persis di
            // bawah header, dan yang berlabuh sendiri-sendiri akan saling
            // menindih begitu keduanya tampil bersamaan.
            //
            // Wadahnya tembus klik; hanya anaknya yang menangkap tekanan —
            // kalau tidak, pita tak terlihat selebar layar ini akan memakan
            // setiap sentuhan pada pesan yang kebetulan lewat di bawahnya.
            <div class="chat-kabar-atas">
                // Kabar sekilas: siapa, dan sepenggal isinya. Ditekan =
                // melompat ke pesan itu.
                {move || pengumuman.get().map(|(nama, isi)| view! {
                    <button
                        class="chat-masuk"
                        on:click=move |_| {
                            #[cfg(target_arch = "wasm32")]
                            gulir_ke_dasar(msg_list_ref);
                            di_dasar.set(true);
                            baru_masuk.set(0);
                            pengumuman.set(None);
                        }
                    >
                        <span class="chat-masuk-titik"></span>
                        <span class="chat-masuk-teks">
                            <span class="chat-masuk-nama">{nama}</span>
                            <span class="chat-masuk-isi">{isi}</span>
                        </span>
                    </button>
                })}
            </div>

            // ── Tombol gulir ke pesan terbaru ──────────────────────────────────
            // Muncul HANYA saat tidak di dasar. Percakapan panjang mudah
            // membuat orang tersesat di tengah riwayat — terutama sesudah
            // pemisah "pesan baru" melompatkan mereka ke atas — dan tanpa ini
            // satu-satunya jalan kembali adalah menggulir manual sampai habis.
            {move || {
                let n = baru_masuk.get();
                // Tampil bila ada yang belum terlihat, ATAU sekadar tersesat di
                // tengah riwayat. Dua alasan berbeda, satu tombol — tapi
                // bentuknya berbeda supaya alasannya terbaca.
                (n > 0 || !di_dasar.get()).then(|| {
                    let ada_baru = n > 0;
                    let kelas = if ada_baru {
                        "chat-ke-bawah chat-ke-bawah--baru"
                    } else {
                        "chat-ke-bawah"
                    };
                    view! {
                        <button
                            class=kelas
                            aria-label=if ada_baru {
                                "Lihat pesan baru"
                            } else {
                                "Ke pesan terbaru"
                            }
                            on:click=move |_| {
                                if let Some(el) = msg_list_ref.get_untracked() {
                                    el.set_scroll_top(el.scroll_height());
                                    di_dasar.set(true);
                                    baru_masuk.set(0);
                                }
                            }
                        >
                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <line x1="12" y1="5" x2="12" y2="19" />
                                <polyline points="19 12 12 19 5 12" />
                            </svg>
                            {ada_baru.then(|| view! {
                                <span class="chat-ke-bawah-label">
                                    {format!("{n} pesan baru")}
                                </span>
                            })}
                        </button>
                    }
                })
            }}

            // ── Error toast ────────────────────────────────────────────────────
            {move || (!error_msg.get().is_empty()).then(|| view! {
                <div class="chat-error-toast">{error_msg.get()}</div>
            })}

            // ── Input bar ──────────────────────────────────────────────────────
            <div class="chat-input-bar">
                <button class="chat-input-icon-btn" aria-label="Emoji">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <circle cx="12" cy="12" r="10"/>
                        <path d="M8 14s1.5 2 4 2 4-2 4-2"/>
                        <line x1="9" y1="9" x2="9.01" y2="9"/>
                        <line x1="15" y1="9" x2="15.01" y2="9"/>
                    </svg>
                </button>
                <input
                    type="text"
                    class="chat-input"
                    placeholder="Pulse your message..."
                    prop:value=move || text_input.get()
                    on:input=move |e| text_input.set(event_target_value(&e))
                    on:keydown=move |e| {
                        if e.key() == "Enter" { e.prevent_default(); do_send(); }
                    }
                />
                <button
                    class="chat-send-btn"
                    disabled=move || text_input.get().trim().is_empty()
                    on:click=move |_| do_send()
                >
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="22" y1="2" x2="11" y2="13"/>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"/>
                    </svg>
                </button>
            </div>
        </div>
    }
}

// ── Message bubble renderer ───────────────────────────────────────────────────

fn message_bubble(msg: ChatMessage, my_id: &str, disorot: bool) -> impl IntoView {
    let is_me   = msg.sender_id == my_id;
    let name    = msg.sender_name.clone();
    let text    = msg.content.clone();
    let time    = if msg.sent_at > 0 { fmt_time_ms(msg.sent_at) } else { String::new() };
    let initial = name.chars().next().unwrap_or('?').to_uppercase().next().unwrap_or('?').to_string();

    let row_cls    = if is_me { "chat-row chat-row--self" } else { "chat-row chat-row--other" };
    let wrap_cls   = if is_me { "chat-bubble-wrap chat-bubble-wrap--self" } else { "chat-bubble-wrap" };
    let bubble_cls = match (is_me, disorot) {
        (true, _)      => "chat-bubble chat-bubble--self",
        (false, false) => "chat-bubble chat-bubble--other",
        // Sorotan hanya untuk pesan orang lain: menyorot pesan sendiri berarti
        // memberi tahu seseorang tentang kalimat yang baru saja ia ketik.
        (false, true)  => "chat-bubble chat-bubble--other chat-bubble--sorot",
    };

    view! {
        <div class=row_cls>
            {(!is_me).then(|| view! {
                <div class="chat-other-avatar-wrap">
                    <div class="chat-other-avatar">{initial}</div>
                </div>
            })}
            <div class=wrap_cls>
                {(!is_me).then(|| view! {
                    <span class="chat-sender-name">{name}</span>
                })}
                <div class=bubble_cls>{text}</div>
                <div class="chat-msg-meta">
                    <span class="chat-msg-time">{time}</span>
                    {is_me.then(|| view! {
                        <span class="chat-msg-sent-icon">
                            <svg width="14" height="14" viewBox="0 0 24 24"
                                 fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <polyline points="20 6 9 17 4 12"/>
                                <polyline points="20 6 9 17 14 17"/>
                            </svg>
                        </span>
                    })}
                </div>
            </div>
        </div>
    }
}

// ── Shim for non-WASM targets ─────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn js_sys_now() -> u64 { 0 }

#[cfg(target_arch = "wasm32")]
fn js_sys_now() -> u64 { web_sys::js_sys::Date::now() as u64 }
