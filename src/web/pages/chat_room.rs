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

use crate::web::utils::waktu::jam_dari_millis as fmt_time_ms;

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

/// Batas gambar chat. Kembar dengan `MAKS_GAMBAR_CHAT` di
/// `web/api/upload.rs` — yang di sana yang MENEGAKKAN, yang di sini menahan
/// unggahan sia-sia lewat data seluler sebelum berangkat. Ubah keduanya.
/// Hanya dipakai di klien — di SSR tak ada berkas yang dipilih siapa pun.
#[cfg(target_arch = "wasm32")]
const MAKS_GAMBAR_CHAT: usize = 300 * 1024;

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

    // Pesan yang sedang dibalas. Disimpan UTUH, bukan cuma id-nya: pratinjau di
    // atas kolom ketik harus menampilkan nama dan cuplikannya seketika, dan
    // mencarinya ulang di riwayat berarti gagal untuk pesan yang belum dimuat.
    let membalas: RwSignal<Option<crate::web::models::KutipanChat>> = RwSignal::new(None);
    // Naik tiap ada pengumuman baru. Timer penutup yang dijadwalkan pengumuman
    // LAMA tidak boleh menutup pengumuman yang lebih muda — tanpa penanda ini,
    // dua pesan yang datang beriringan membuat kabar kedua lenyap seketika saat
    // timer pertama jatuh tempo.
    #[cfg(target_arch = "wasm32")]
    let umur_umum: StoredValue<u32> = StoredValue::new(0);


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
                    // Pesan dari room LAIN disaring di sini dan dibuang
                    // diam-diam — SENGAJA.
                    //
                    // Dulu tempat ini memunculkan toast. Sejak `<KabarChat/>`
                    // di root melakukannya untuk seluruh aplikasi, keduanya
                    // menyala bersamaan: berada di room A saat pesan room B
                    // tiba menghasilkan DUA pemberitahuan yang sama persis.
                    //
                    // Yang di root yang bertahan, karena ia bekerja di halaman
                    // mana pun; yang di sini hanya bekerja bila kebetulan sedang
                    // membuka sebuah percakapan.
                    if m.room_id.trim() != room_id_untracked().trim() {
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
                        // Ruangan ini SEDANG dibuka, jadi pesannya terbaca saat
                        // itu juga. Lencana navbar dinolkan seketika, dan
                        // penanda baca di server ikut dimajukan — kalau hanya
                        // yang pertama, memuat ulang halaman akan menghidupkan
                        // kembali lencana untuk pesan yang sudah dilihat.
                        if let Some(b) = bus {
                            b.tandai_dibaca(&room_id_untracked());
                        }
                        let rid_baca = room_id_untracked();
                        leptos::task::spawn_local(async move {
                            let _ = crate::web::api::mark_chat_read(rid_baca).await;
                        });

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

    // Berkas sedang naik. Menahan tombolnya agar satu ketukan gugup tak
    // menghasilkan tiga unggahan berturut-turut untuk berkas yang sama.
    let mengunggah = RwSignal::new(false);
    let berkas_ref = NodeRef::<leptos::html::Input>::new();

    let kirim_gambar = move |_ev: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let Some(input) = berkas_ref.get_untracked() else { return };
            let Some(berkas) = input.files().and_then(|f| f.get(0)) else { return };
            // Kosongkan SEKARANG, bukan nanti: tanpa ini, memilih berkas yang
            // SAMA dua kali berturut-turut tak memicu `change` sama sekali —
            // nilainya tak berubah — dan orangnya menyimpulkan aplikasinya
            // rusak, padahal ia hanya diam.
            input.set_value("");

            // Diperiksa di sini juga, bukan hanya di server. Menunggu server
            // menolaknya berarti mengunggah 4 MB lebih dulu lewat data seluler
            // hanya untuk diberi tahu bahwa itu terlalu besar.
            if berkas.size() as usize > MAKS_GAMBAR_CHAT {
                error_msg.set(format!(
                    "Gambar maksimal {} KB — punyamu {} KB. Perkecil dulu ya.",
                    MAKS_GAMBAR_CHAT / 1024,
                    berkas.size() as usize / 1024
                ));
                return;
            }

            let form = web_sys::FormData::new().unwrap();
            let _ = form.append_with_blob("file", &berkas);
            mengunggah.set(true);
            let rid = room_id();
            let balas_id = membalas.get_untracked().map(|k| k.id);
            membalas.set(None);

            leptos::task::spawn_local(async move {
                let hasil = async {
                    let opts = web_sys::RequestInit::new();
                    opts.set_method("POST");
                    opts.set_body(&form);
                    let req = web_sys::Request::new_with_str_and_init(
                        "/upload/chat-image",
                        &opts,
                    )
                    .ok()?;
                    let win = web_sys::window()?;
                    let resp =
                        wasm_bindgen_futures::JsFuture::from(win.fetch_with_request(&req))
                            .await
                            .ok()?;
                    let resp: web_sys::Response = resp.dyn_into().ok()?;
                    let teks = wasm_bindgen_futures::JsFuture::from(resp.text().ok()?)
                        .await
                        .ok()?
                        .as_string()?;
                    let nilai: serde_json::Value = serde_json::from_str(&teks).ok()?;
                    if !resp.ok() {
                        // Pesan galat server sudah berbahasa manusia dan
                        // menyebut ukuran sebenarnya — tampilkan apa adanya,
                        // jangan ditukar dengan kalimat sendiri yang lebih
                        // umum dan karena itu kurang menolong.
                        return Some(Err(nilai
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Gagal mengunggah gambar.")
                            .to_string()));
                    }
                    Some(Ok(nilai.get("url")?.as_str()?.to_string()))
                }
                .await;

                mengunggah.set(false);
                match hasil {
                    Some(Ok(url)) => {
                        let muatan = serde_json::json!({
                            "type": "send_image",
                            "room_id": rid,
                            "media_url": url,
                            "client_id": format!("_opt_{}", js_sys_now()),
                            "reply_to": balas_id,
                        })
                        .to_string();
                        if let Some(b) = bus {
                            if let Err(pesan) = b.kirim(&muatan) {
                                error_msg.set(pesan.into());
                            }
                        }
                    }
                    Some(Err(pesan)) => error_msg.set(pesan),
                    None => error_msg.set("Gagal mengunggah gambar.".into()),
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = _ev;
    };

    let do_send = move || {
        let content = text_input.get_untracked().trim().to_string();
        if content.is_empty() { return; }
        let client_id = format!("_opt_{}", js_sys_now());
        text_input.set(String::new());
        let me_id = current_user_id().unwrap_or_default();
        // Diambil SEBELUM dinolkan di bawah — pesan optimistis harus sudah
        // membawa kutipannya, kalau tidak ia berkedip tanpa kutipan lalu
        // kutipannya muncul saat gemanya tiba dari server.
        let balasan = membalas.get_untracked();
        membalas.set(None);
        let msg = ChatMessage {
            id: client_id.clone(),
            room_id: room_id(),
            sender_id: me_id,
            sender_name: "You".into(),
            content: content.clone(),
            sent_at: 0,
            message_type: "text".into(),
            media_url: None,
            reply_to: balasan.clone(),
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
                "reply_to": balasan.as_ref().map(|k| k.id.clone()),
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
                                    view! { {pemisah} {message_bubble(msg, &me, false, membalas)} }
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
                        message_bubble(msg, &me, kena, membalas)
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
            // Pratinjau balasan, TEPAT di atas kolom ketik — bukan melayang di
            // tempat lain. Ia harus berada di jalur pandang yang sama dengan
            // kalimat yang sedang diketik; kalau tidak, orang mengetik balasan
            // panjang tanpa sadar ia sedang membalas pesan yang keliru.
            {move || membalas.get().map(|k| {
                let isi = if k.is_image && k.content.is_empty() {
                    "Foto".to_string()
                } else {
                    k.content.clone()
                };
                view! {
                    <div class="chat-balas-pratinjau">
                        <div class="chat-balas-teks">
                            <span class="chat-kutip-nama">{k.sender_name.clone()}</span>
                            <span class="chat-kutip-isi">{isi}</span>
                        </div>
                        <button
                            class="chat-balas-batal"
                            aria-label="Batalkan balasan"
                            on:click=move |_| membalas.set(None)
                        >
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <line x1="18" y1="6" x2="6" y2="18"/>
                                <line x1="6" y1="6" x2="18" y2="18"/>
                            </svg>
                        </button>
                    </div>
                }
            })}
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
                    type="file"
                    accept="image/*"
                    node_ref=berkas_ref
                    class="chat-file-input"
                    on:change=kirim_gambar
                />
                <button
                    class="chat-input-icon-btn"
                    aria-label="Kirim gambar"
                    disabled=move || mengunggah.get()
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(el) = berkas_ref.get_untracked() {
                            el.click();
                        }
                    }
                >
                    {move || if mengunggah.get() {
                        view! { <span class="chat-unggah-putar"></span> }.into_any()
                    } else {
                        view! {
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <rect x="3" y="3" width="18" height="18" rx="2"/>
                                <circle cx="8.5" cy="8.5" r="1.5"/>
                                <polyline points="21 15 16 10 5 21"/>
                            </svg>
                        }.into_any()
                    }}
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

// ── Rujukan produk di dalam pesan ─────────────────────────────────────────────

/// Pesan pertama dari halaman produk dirakit di `chat_new.rs` sebagai
/// `[Judul] /products/slug\npertanyaan`. Konteksnya memang perlu ikut — merchant
/// yang menjual puluhan barang tak bisa menebak pertanyaan ini tentang yang mana
/// — tapi bentuk mentahnya menempatkan beban itu pada MATA pembacanya: baris
/// alamat sepanjang dua puluh karakter acak yang harus dilompati untuk sampai ke
/// pertanyaan yang sebenarnya.
///
/// Mengurai di sisi pembaca, bukan mengubah yang dikirim: pesan lama sudah
/// telanjur tersimpan dengan bentuk ini, dan ribuan di antaranya tak bisa
/// ditulis ulang. Penguraian membuat yang lama ikut tampil rapi.
///
/// Mengembalikan `(judul, slug, sisa pesan)`.
fn pisah_rujukan_produk(teks: &str) -> Option<(String, String, String)> {
    let sisa = teks.strip_prefix('[')?;

    // Alamatnya dicari LEBIH DULU, lalu mundur ke `]` terdekat sebelumnya.
    // Memotong di `]` pertama akan mematahkan judul yang memuat kurung siku di
    // dalamnya — "Tiket [VIP] Malam" — dan judul semacam itu bukan hal aneh.
    let mulai_alamat = sisa.find("/products/")?;
    let tutup = sisa[..mulai_alamat].rfind(']')?;

    let judul = sisa[..tutup].trim().to_string();
    if judul.is_empty() {
        return None;
    }
    // Di antara `]` dan alamatnya hanya boleh ada spasi putih. Tanpa syarat ini,
    // kalimat biasa yang kebetulan menyebut sebuah tautan produk akan ikut
    // terurai jadi kartu.
    if !sisa[tutup + 1..mulai_alamat].trim().is_empty() {
        return None;
    }

    let alamat = &sisa[mulai_alamat + "/products/".len()..];

    // Slug berakhir di spasi putih PERTAMA. Pertanyaannya menyusul sesudah itu —
    // biasanya dipisah baris baru, tapi jangan bergantung pada baris baru:
    // sebagian klien memampatkannya jadi spasi biasa dalam perjalanan.
    let batas = alamat
        .find(char::is_whitespace)
        .unwrap_or(alamat.len());
    let slug = alamat[..batas].to_string();
    if slug.is_empty() {
        return None;
    }

    Some((judul, slug, alamat[batas..].trim().to_string()))
}

#[cfg(test)]
mod tests_rujukan {
    use super::pisah_rujukan_produk;

    #[test]
    fn bentuk_baku() {
        let (j, s, sisa) =
            pisah_rujukan_produk("[Neon Night Rave] /products/ea2472c40573\nmasih ada?").unwrap();
        assert_eq!(j, "Neon Night Rave");
        assert_eq!(s, "ea2472c40573");
        assert_eq!(sisa, "masih ada?");
    }

    /// Sebagian klien memampatkan baris baru jadi spasi dalam perjalanan.
    #[test]
    fn dipisah_spasi_bukan_baris_baru() {
        let (_, s, sisa) =
            pisah_rujukan_produk("[Neon Night] /products/ddd88ea24633 teta").unwrap();
        assert_eq!(s, "ddd88ea24633");
        assert_eq!(sisa, "teta");
    }

    #[test]
    fn tanpa_pertanyaan_menyusul() {
        let (_, s, sisa) = pisah_rujukan_produk("[Neon] /products/abc123").unwrap();
        assert_eq!(s, "abc123");
        assert_eq!(sisa, "");
    }

    /// Judul yang MEMUAT kurung siku tak boleh memotong slug-nya.
    #[test]
    fn judul_berkurung_siku_di_dalamnya() {
        let (j, s, _) =
            pisah_rujukan_produk("[Tiket [VIP] Malam] /products/xyz").unwrap();
        assert_eq!(j, "Tiket [VIP] Malam");
        assert_eq!(s, "xyz");
    }

    #[test]
    fn pesan_biasa_bukan_rujukan() {
        assert!(pisah_rujukan_produk("halo kak").is_none());
        assert!(pisah_rujukan_produk("[Neon Night] halo").is_none());
        assert!(pisah_rujukan_produk("[] /products/abc").is_none());
        assert!(pisah_rujukan_produk("[Neon] /products/").is_none());
    }

    /// Kurung siku yang dipakai orang untuk hal lain tak boleh ikut terurai.
    #[test]
    fn kurung_siku_tanpa_alamat_produk() {
        assert!(pisah_rujukan_produk("[penting] tolong dibalas").is_none());
        // Kalimat yang kebetulan menyebut tautan, bukan rujukan yang dirakit.
        assert!(pisah_rujukan_produk("[catatan] lihat /products/abc ya").is_none());
    }
}

/// Kartu produk di dalam gelembung pesan.
///
/// Judulnya sudah ada di dalam pesan, jadi kartunya bisa langsung tampil tanpa
/// menunggu apa pun — gambar dan harga menyusul begitu tiba. Urutan itu penting:
/// meminta orang menatap kotak kosong demi sampul yang mungkin gagal dimuat
/// adalah menukar sesuatu yang sudah pasti dengan sesuatu yang belum tentu.
#[component]
fn KartuProduk(judul: String, slug: String) -> impl IntoView {
    let s = slug.clone();
    let rinci = Resource::new(
        move || s.clone(),
        |slug| async move {
            if slug.is_empty() {
                return None;
            }
            crate::web::api::get_product_detail(slug).await.ok()
        },
    );

    let href = format!("/products/{slug}");
    let judul_awal = judul.clone();

    view! {
        <A href=href attr:class="chat-produk">
            <div class="chat-produk-gambar">
                {move || rinci.get().flatten().and_then(|p| p.cover_url).map(|url| view! {
                    <img src=url alt="" loading="lazy" decoding="async"
                         on:error=crate::web::components::gambar_cadangan />
                })}
            </div>
            <div class="chat-produk-teks">
                // Nama dari pesan dulu, lalu ditimpa nama dari basis data begitu
                // tiba — produk yang sudah berganti nama tak terus tampil dengan
                // nama lamanya selamanya.
                <span class="chat-produk-nama">
                    {move || rinci
                        .get()
                        .flatten()
                        .map(|p| p.name)
                        .unwrap_or_else(|| judul_awal.clone())}
                </span>
                {move || rinci.get().flatten().map(|p| view! {
                    <span class="chat-produk-harga">
                        {crate::web::utils::rupiah_atau_gratis(p.display_price as i64)}
                    </span>
                })}
            </div>
            <svg class="chat-produk-panah" width="16" height="16" viewBox="0 0 24 24"
                 fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                <polyline points="9 18 15 12 9 6"/>
            </svg>
        </A>
    }
}

// ── Message bubble renderer ───────────────────────────────────────────────────

fn message_bubble(
    msg: ChatMessage,
    my_id: &str,
    disorot: bool,
    membalas: RwSignal<Option<crate::web::models::KutipanChat>>,
) -> impl IntoView {
    let is_me   = msg.sender_id == my_id;
    let name    = msg.sender_name.clone();
    let text    = msg.content.clone();
    // Kosong diperlakukan sebagai tak ada: baris tanpa berkas kadang tersimpan
    // dengan string kosong alih-alih NULL, dan `<img src="">` memuat ULANG
    // halaman ini sebagai gambar sebelum menyerah.
    let gambar  = msg.media_url.clone().filter(|u| !u.is_empty());
    let time    = if msg.sent_at > 0 { fmt_time_ms(msg.sent_at) } else { String::new() };
    let initial = name.chars().next().unwrap_or('?').to_uppercase().next().unwrap_or('?').to_string();

    let row_cls    = if is_me { "chat-row chat-row--self" } else { "chat-row chat-row--other" };
    let wrap_cls   = if is_me { "chat-bubble-wrap chat-bubble-wrap--self" } else { "chat-bubble-wrap" };
    let kutipan = msg.reply_to.clone();
    // Bahan untuk pratinjau saat gelembung INI yang dibalas. Disiapkan sekarang
    // karena `msg` akan berpindah ke dalam view di bawah.
    let umpan = crate::web::models::KutipanChat {
        id: msg.id.clone(),
        sender_name: if is_me { "Kamu".to_string() } else { msg.sender_name.clone() },
        content: if gambar.is_some() && msg.content.is_empty() {
            String::new()
        } else {
            msg.content.chars().take(120).collect()
        },
        is_image: gambar.is_some(),
    };
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
                // Kutipan di ATAS isi balasannya — urutan yang sama dengan cara
                // orang membacanya: dulu ada yang bilang begini, lalu ini
                // jawabannya.
                {kutipan.map(|k| {
                    let isi = if k.is_image && k.content.is_empty() {
                        "Foto".to_string()
                    } else {
                        k.content.clone()
                    };
                    view! {
                        <div class="chat-kutip">
                            <span class="chat-kutip-nama">{k.sender_name.clone()}</span>
                            <span class="chat-kutip-isi">{isi}</span>
                        </div>
                    }
                })}
                {match gambar {
                    // Gelembung gambar: tanpa lapisan gelembung di belakangnya,
                    // dan tanpa padding. Bingkai berwarna di sekeliling foto
                    // hanya menambah dua sisi yang saling berebut menjadi
                    // batas bentuknya.
                    Some(url) => {
                        // Disalin lebih dulu: `view!` memindahkan tangkapannya
                        // ke dalam closure atribut, jadi `url` tak bisa dipakai
                        // lagi sesudahnya di dalam blok yang sama.
                        let tautan = url.clone();
                        view! {
                        <a class="chat-gambar" href=tautan target="_blank" rel="noopener">
                            // `loading=lazy`: percakapan panjang bisa memuat
                            // puluhan foto sekaligus saat riwayatnya dibuka.
                            // `decoding=async` menjaga guliran tetap mulus
                            // sementara keduanya dikerjakan di luar jalur utama.
                            <img src=url alt="Gambar" loading="lazy" decoding="async" />
                            // Keterangan foto menempati `content`, kolom yang
                            // sama dengan pesan teks — jadi ia mungkin kosong.
                            {(!text.is_empty()).then(|| view! {
                                <span class="chat-gambar-teks">{text.clone()}</span>
                            })}
                        </a>
                        }.into_any()
                    }
                    None => match pisah_rujukan_produk(&text) {
                        Some((judul, slug, sisa)) => view! {
                            <div class=bubble_cls>
                                <KartuProduk judul=judul slug=slug />
                                // Pertanyaannya di BAWAH kartu, bukan di atas:
                                // kartu menjawab "tentang apa", dan itu yang
                                // dibaca lebih dulu oleh merchant yang membuka
                                // percakapan baru.
                                {(!sisa.is_empty()).then(|| view! {
                                    <span class="chat-produk-tanya">{sisa}</span>
                                })}
                            </div>
                        }.into_any(),
                        None => view! { <div class=bubble_cls>{text}</div> }.into_any(),
                    },
                }}
                <div class="chat-msg-meta">
                    // Tombol, bukan geser: geser lebih halus tapi tak terlihat
                    // sama sekali sampai seseorang kebetulan menemukannya, dan
                    // fitur yang harus ditemukan sendiri sama saja dengan tidak
                    // ada bagi kebanyakan orang.
                    <button
                        class="chat-balas-btn"
                        aria-label="Balas pesan ini"
                        on:click=move |_| membalas.set(Some(umpan.clone()))
                    >
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                             stroke-linejoin="round">
                            <polyline points="9 17 4 12 9 7"/>
                            <path d="M20 18v-2a4 4 0 0 0-4-4H4"/>
                        </svg>
                    </button>
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
