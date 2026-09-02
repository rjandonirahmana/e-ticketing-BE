//! ws_chat.rs — Langganan WebSocket chat untuk halaman selain ruang obrolan.
//!
//! ── KENAPA ADA ────────────────────────────────────────────────────────────
//! Daftar `/pulse` dirender dari sebuah `Resource`: ia bertanya SEKALI saat
//! halaman dibuka, lalu diam. Artinya lencana "belum dibaca" pada dasarnya
//! adalah foto keadaan pada detik halaman dimuat — pesan yang tiba semenit
//! kemudian tidak mengubah apa pun sampai orangnya menyegarkan halaman. Yang
//! ganjil: daftar percakapan justru satu-satunya tempat orang MENUNGGU kabar,
//! dan justru itu yang paling tidak hidup.
//!
//! Server sudah mengirimkan pesan SEMUA room milik pengguna ke koneksi mana
//! pun yang ia buka (`register_rooms` saat Hello) — jadi tidak ada yang perlu
//! ditambahkan di sisi server. Yang kurang cuma telinganya.
//!
//! Peristiwa diserahkan mentah sebagai `serde_json::Value`: tiap pemanggil
//! peduli pada bidang yang berbeda (daftar butuh `room_id` + `sender_id`,
//! ruangan butuh seluruh pesan), dan memaksakan satu bentuk untuk keduanya
//! hanya memindahkan penguraian ke tempat yang salah.

use leptos::prelude::*;


/// Pegangan ke koneksi yang sedang hidup.
///
/// `Copy` supaya bebas ditangkap closure mana pun tanpa ritual clone —
/// seluruh isinya sudah `Copy` (`StoredValue` dan `RwSignal` cuma indeks ke
/// arena reaktif, bukan datanya sendiri).
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub struct KoneksiChat {
    ws: StoredValue<Option<web_sys::WebSocket>>,
    /// Terbuka dan siap dipakai. Yang dipajang sebagai "● LIVE".
    pub siap: RwSignal<bool>,
}

#[cfg(target_arch = "wasm32")]
impl KoneksiChat {
    /// Kirim satu bingkai teks. Pesan galatnya sudah berbahasa manusia dan
    /// siap ditampilkan apa adanya — pemanggil tak perlu menerjemahkan keadaan
    /// soket menjadi kalimat, dan dengan begitu semua halaman mengucapkan
    /// kegagalan yang sama dengan kalimat yang sama.
    pub fn kirim(&self, muatan: &str) -> Result<(), &'static str> {
        self.ws.with_value(|opt| match opt {
            None => Err("Tidak terhubung."),
            Some(ws) if ws.ready_state() != web_sys::WebSocket::OPEN => {
                Err("Koneksi terputus, coba lagi.")
            }
            Some(ws) => ws.send_with_str(muatan).map_err(|_| "Gagal kirim pesan."),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
pub struct KoneksiChat {
    pub siap: RwSignal<bool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl KoneksiChat {
    pub fn kirim(&self, _muatan: &str) -> Result<(), &'static str> {
        Err("Tidak terhubung.")
    }
}

/// Buka koneksi chat dan panggil `on_event` untuk tiap peristiwa yang masuk.
///
/// Menutup koneksi dan menghentikan watchdog saat komponen dilepas. Tidak
/// melakukan apa-apa bila belum masuk (server akan menolak, dan watchdog akan
/// mencoba selamanya untuk koneksi yang memang tak berhak).
#[cfg(target_arch = "wasm32")]
pub fn langgan_chat<F>(on_event: F) -> KoneksiChat
where
    F: FnMut(serde_json::Value) + 'static,
{
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use web_sys::WebSocket;

    let auth = use_context::<crate::web::app::AuthResource>();

    // Dibagi karena `connect` merakit ULANG closure `onmessage` tiap kali
    // watchdog menyambung ulang — penanganannya tak boleh ikut hangus bersama
    // koneksi yang putus.
    let handler: Rc<RefCell<F>> = Rc::new(RefCell::new(on_event));

    let ws_store: StoredValue<Option<WebSocket>> = StoredValue::new(None);
    let siap = RwSignal::new(false);
    let cb_pesan: StoredValue<Option<wasm_bindgen::JsValue>> = StoredValue::new(None);
    let cb_buka: StoredValue<Option<wasm_bindgen::JsValue>> = StoredValue::new(None);
    let cb_tutup: StoredValue<Option<wasm_bindgen::JsValue>> = StoredValue::new(None);
    // Ditandai saat halaman ditinggalkan → watchdog berhenti menyambung ulang
    // koneksi yang sengaja ditutup (cegah reconnect zombie).
    let closing: StoredValue<bool> = StoredValue::new(false);

    // Menyerah SEMENTARA: sesi diambil alih tab lain, atau server menolak kita.
    //
    // ── KENAPA HARUS ADA ─────────────────────────────────────────────────
    // Server menyimpan satu sesi per pengguna: koneksi baru MENGGANTIKAN yang
    // lama, lalu mengirim galat `REPLACED` ke yang lama supaya ia berhenti.
    // Tanpa mendengarkan galat itu, kedua tab akan saling merebut sesi lewat
    // watchdog masing-masing — buka, tergusur, sambung ulang, gusur balik —
    // selamanya. Di log server pola itu tampak sebagai "WS opened" dan
    // "WS closed" bergantian tiap tiga detik, dua deret sekaligus.
    let menyerah: StoredValue<bool> = StoredValue::new(false);

    let connect = move || {
        let masuk = auth
            .and_then(|a| a.get_untracked())
            .and_then(|r| r.ok())
            .flatten()
            .is_some();
        if !masuk {
            return;
        }

        let Some(win) = web_sys::window() else { return };
        let proto = if win.location().protocol().unwrap_or_default() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let Ok(host) = win.location().host() else { return };
        // Cookie `pulse_token` (HttpOnly) ikut terkirim otomatis saat upgrade
        // same-origin — token tak perlu ditempel di URL, tempat ia akan bocor
        // ke log akses.
        let url = format!("{}://{}/api/ws/chat", proto, host);

        let Ok(ws) = WebSocket::new(&url) else { return };

        let h = handler.clone();
        let onmessage =
            wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |e: web_sys::MessageEvent| {
                    let Ok(txt) = e.data().dyn_into::<web_sys::js_sys::JsString>() else {
                        return;
                    };
                    let s: String = txt.into();
                    let Ok(evt) = serde_json::from_str::<serde_json::Value>(&s) else {
                        return;
                    };
                    if evt.get("type").and_then(|t| t.as_str()) == Some("error") {
                        match evt.get("code").and_then(|c| c.as_str()) {
                            // Tab lain mengambil alih. Berhenti — tab yang
                            // sedang dilihat orangnya berhak menang.
                            Some("REPLACED") => {
                                menyerah.set_value(true);
                                return;
                            }
                            // Menyambung ulang selamanya ke pintu yang menolak
                            // kita adalah badai yang sama dengan sebab berbeda.
                            Some("UNAUTHORIZED") => {
                                menyerah.set_value(true);
                                return;
                            }
                            _ => {}
                        }
                    }
                    // `try_borrow_mut`, bukan `borrow_mut`: kalau penanganannya
                    // sampai memicu peristiwa lain secara sinkron, panik karena
                    // pinjaman ganda akan menggugurkan SELURUH aplikasi wasm.
                    if let Ok(mut f) = h.try_borrow_mut() {
                        f(evt);
                    }
                },
            );
        let onopen = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || siap.set(true));
        // Satu closure dipakai untuk `close` DAN `error`: keduanya berarti hal
        // yang sama bagi pemakai — sambungannya sedang tidak bisa dipakai.
        let onclose =
            wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                siap.set(false)
            });

        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        ws.set_onerror(Some(onclose.as_ref().unchecked_ref()));

        cb_pesan.set_value(Some(onmessage.into_js_value()));
        cb_buka.set_value(Some(onopen.into_js_value()));
        cb_tutup.set_value(Some(onclose.into_js_value()));
        ws_store.set_value(Some(ws));
    };

    Effect::new(move |_| {
        connect();

        // Disalin, tidak dipindahkan. `connect` menangkap `Rc` — berbeda dengan
        // versi di ruang obrolan yang hanya menangkap `StoredValue` (Copy) —
        // jadi menyerahkannya ke watchdog akan MENGHABISKANNYA, dan closure
        // `Effect` yang menghabiskan tangkapannya cuma bisa dijalankan sekali.
        // Salinannya murah: seluruh isinya `Copy` kecuali satu hitungan Rc.
        let sambung_lagi = connect.clone();

        // ── Tab yang dilihat orangnya menang ─────────────────────────────
        // Menyerah pada `REPLACED` menghentikan badainya, tapi kalau berhenti
        // di situ saja, tab yang tergusur jadi tuli SELAMANYA — orangnya
        // kembali ke tab itu, mengetik, dan tak pernah melihat balasan sampai
        // ia memuat ulang halaman tanpa tahu kenapa.
        //
        // Maka: yang menyerah adalah tab yang sedang di LATAR. Begitu sebuah
        // tab kembali terlihat, ia mengambil sesinya kembali. Aturannya jadi
        // sederhana dan sesuai dengan yang orangnya harapkan — yang sedang
        // ditatap adalah yang hidup — dan karena hanya SATU tab yang bisa
        // terlihat pada satu waktu, perebutannya berhenti dengan sendirinya.
        let rebut_kembali = connect.clone();
        let cb_tampak = web_sys::window().and_then(|w| w.document()).map(|doc| {
            let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                    return;
                };
                if doc.hidden() || closing.get_value() {
                    return;
                }
                if menyerah.get_value() {
                    menyerah.set_value(false);
                    rebut_kembali();
                }
            });
            let _ = doc.add_event_listener_with_callback(
                "visibilitychange",
                cb.as_ref().unchecked_ref(),
            );
            (doc, cb.into_js_value())
        });

        // Sambung ulang tiap 3 dtk bila koneksinya putus. Tanpa ini, satu
        // gangguan jaringan membuat lencananya beku sampai halaman disegarkan
        // — persis keadaan yang hendak diperbaiki.
        let interval = send_wrapper::SendWrapper::new(gloo_timers::callback::Interval::new(
            3_000,
            move || {
                if closing.get_value() || menyerah.get_value() {
                    return;
                }
                let perlu = ws_store.with_value(|opt| match opt {
                    None => true,
                    Some(ws) => ws.ready_state() == WebSocket::CLOSED,
                });
                if perlu {
                    sambung_lagi();
                }
            },
        ));

        on_cleanup(move || {
            closing.set_value(true);
            drop(interval);
            if let Some((doc, cb)) = cb_tampak {
                let _ = doc.remove_event_listener_with_callback(
                    "visibilitychange",
                    cb.unchecked_ref(),
                );
            }
            ws_store.with_value(|opt| {
                if let Some(ws) = opt {
                    ws.set_onmessage(None);
                    ws.set_onopen(None);
                    ws.set_onclose(None);
                    ws.set_onerror(None);
                    let _ = ws.close();
                }
            });
            ws_store.set_value(None);
            cb_pesan.set_value(None);
            cb_buka.set_value(None);
            cb_tutup.set_value(None);
            siap.set(false);
        });
    });

    KoneksiChat { ws: ws_store, siap }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn langgan_chat<F>(_on_event: F) -> KoneksiChat
where
    F: FnMut(serde_json::Value) + 'static,
{
    KoneksiChat {
        siap: RwSignal::new(false),
    }
}

// ── Bus chat tingkat aplikasi ─────────────────────────────────────────────────

/// Satu koneksi untuk SELURUH aplikasi, disediakan di root.
///
/// ── KENAPA HARUS SATU ─────────────────────────────────────────────────────
/// `WsManager` di server menyimpan sesi berdasarkan `user_id` — SATU koneksi
/// per pengguna. Koneksi kedua tidak berdampingan dengan yang pertama, ia
/// MENGGANTIKANNYA. Jadi kalau lencana di navbar membuka koneksinya sendiri
/// sementara halaman `/pulse` sudah punya, keduanya di tab yang sama akan
/// saling merebut sesi dan salah satunya berhenti menerima apa pun — dengan
/// gejala yang mustahil ditebak: satu bagian layar hidup, bagian lain beku.
///
/// Maka koneksinya dibuka sekali di root, dan setiap peminat menumpang.
#[derive(Clone, Copy)]
pub struct ChatBus {
    koneksi: KoneksiChat,
    /// Belum dibaca per room: patokan dari server + susulan yang tiba lewat WS.
    pub belum: RwSignal<Option<std::collections::HashMap<String, i32>>>,
    /// Peristiwa terakhir, untuk halaman yang butuh isi pesannya (ruang obrolan).
    ///
    /// Satu slot, bukan antrean: tiap bingkai WebSocket tiba sebagai tugas JS
    /// tersendiri, jadi tak ada dua peristiwa yang menimpa satu sama lain dalam
    /// satu putaran — pembacanya selalu sempat berjalan di antara keduanya.
    pub peristiwa: RwSignal<Option<serde_json::Value>>,
}

impl ChatBus {
    /// Jumlah seluruh pesan belum dibaca. `None` selama patokan dari server
    /// belum tiba — dibedakan dari nol supaya lencananya tidak berkedip muncul
    /// dari angka yang belum tentu benar.
    pub fn total(&self) -> Option<i32> {
        self.belum
            .get()
            .map(|m| m.values().copied().filter(|n| *n > 0).sum())
    }

    /// Nolkan hitungan satu room — dipanggil saat ruangannya dibuka.
    pub fn tandai_dibaca(&self, room_id: &str) {
        self.belum.update(|opt| {
            if let Some(m) = opt {
                m.insert(room_id.to_string(), 0);
            }
        });
    }

    pub fn kirim(&self, muatan: &str) -> Result<(), &'static str> {
        self.koneksi.kirim(muatan)
    }

    pub fn siap(&self) -> RwSignal<bool> {
        self.koneksi.siap
    }
}

/// Dipanggil SEKALI di root aplikasi.
pub fn provide_chat_bus() {
    let belum: RwSignal<Option<std::collections::HashMap<String, i32>>> = RwSignal::new(None);
    let peristiwa: RwSignal<Option<serde_json::Value>> = RwSignal::new(None);

    let koneksi = langgan_chat(move |evt| {
        if evt.get("type").and_then(|t| t.as_str()) == Some("new_message") {
            if let Some(rid) = evt.get("room_id").and_then(|v| v.as_str()) {
                belum.update(|opt| {
                    // Sebelum patokan server tiba, jangan mengarang peta dari
                    // nol — hitungannya akan lebih kecil dari yang sebenarnya,
                    // dan lencana yang salah lebih buruk daripada lencana yang
                    // terlambat sedetik.
                    if let Some(m) = opt {
                        *m.entry(rid.to_string()).or_insert(0) += 1;
                    }
                });
            }
        }
        peristiwa.set(Some(evt));
    });

    provide_context(ChatBus {
        koneksi,
        belum,
        peristiwa,
    });

    // Patokan awal dari server. Dijalankan ulang tiap status auth berubah:
    // masuk → ambil hitungannya, keluar → kosongkan supaya lencana milik akun
    // sebelumnya tidak tertinggal di layar.
    // ── HANYA DI KLIEN ────────────────────────────────────────────────────
    // Pagar ini bukan optimasi. Tanpa `#[cfg]`, Effect di bawah ikut berjalan
    // saat SSR — pada SETIAP permintaan halaman — dan tiap kali ia melepas satu
    // kueri basis data yang tak seorang pun tunggu hasilnya.
    //
    // Yang membuatnya fatal: `auth` memakai `Resource::new_blocking`, jadi
    // render SSR MENUNGGU koneksi basis data sebelum satu bita pun header
    // terkirim. Begitu kolam koneksi terkuras oleh kueri-kueri lepas itu,
    // render berhenti di tempat — bukan galat, bukan panik, hanya diam. Di
    // proxy gejalanya muncul sebagai `Upstream ReadTimedout while reading
    // response headers`, dan di peramban sebagai halaman putih kosong,
    // sementara aset statis dari proses yang sama tetap terlayani normal
    // dalam puluhan milidetik — sehingga tampak seperti masalah jaringan.
    //
    // Seluruh Effect lain di `web/app/providers.rs` dipagari dengan cara yang
    // sama, dan komentar di berkas itu menyatakan aturannya: TIDAK ADA
    // `spawn_local` di provider. Lencana belum-dibaca memang tak ada gunanya
    // di server — tak ada yang melihatnya sebelum hydration.
    #[cfg(target_arch = "wasm32")]
    {
    let auth = use_context::<crate::web::app::AuthResource>();
    Effect::new(move |_| {
        let masuk = auth
            .and_then(|a| a.get())
            .and_then(|r| r.ok())
            .flatten()
            .is_some();
        if !masuk {
            belum.set(None);
            return;
        }
        leptos::task::spawn_local(async move {
            if let Ok(rooms) = crate::web::api::get_chat_rooms().await {
                let peta = rooms
                    .into_iter()
                    .map(|r| (r.id, r.unread_count.max(0)))
                    .collect();
                // Susulan yang keburu tiba selama permintaan ini berjalan
                // dipertahankan: `belum` masih `None` saat itu jadi ia terbuang,
                // tapi peristiwanya sudah lewat ke `peristiwa` — halaman yang
                // terbuka tetap melihatnya. Yang bisa meleset cuma lencananya,
                // dan hanya sampai kunjungan berikutnya ke daftar.
                belum.set(Some(peta));
            }
        });
    });
    }
}

/// Ambil bus dari context. `None` bila dipanggil di luar pohon App.
pub fn use_chat_bus() -> Option<ChatBus> {
    use_context::<ChatBus>()
}
