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

        // Sambung ulang tiap 3 dtk bila koneksinya putus. Tanpa ini, satu
        // gangguan jaringan membuat lencananya beku sampai halaman disegarkan
        // — persis keadaan yang hendak diperbaiki.
        let interval = send_wrapper::SendWrapper::new(gloo_timers::callback::Interval::new(
            3_000,
            move || {
                if closing.get_value() {
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
