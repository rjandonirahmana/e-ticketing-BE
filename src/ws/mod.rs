pub mod handler;
pub mod manager;
pub mod proto;
pub mod routes;

use axum::extract::ws::WebSocketUpgrade;

use crate::utils::capacity::{
    WS_MAX_MESSAGE_CHAT, WS_MAX_MESSAGE_SIGNAL, WS_READ_BUFFER, WS_WRITE_BUFFER,
};

/// Pasang batas buffer pada socket CHAT sebelum `on_upgrade`.
///
/// ── KENAPA ADA FUNGSI, BUKAN EMPAT PEMANGGILAN DI TIAP TEMPAT ─────────────
/// Batas ini bukan penyetelan per-endpoint; ia bagian dari anggaran RAM
/// seluruh proses. `utils::capacity::PER_WS_BYTES` meramal biaya satu koneksi
/// DARI angka yang sama, dan plafon `MAX_CONNECTIONS` diturunkan dari ramalan
/// itu. Satu endpoint yang lupa memasangnya menagih 128 KiB — bawaan
/// `tungstenite` — dan diam-diam membatalkan perhitungan untuk semua yang lain.
///
/// Satu fungsi membuat "lupa memasang" terlihat sebagai `on_upgrade` telanjang,
/// bukan sebagai angka yang kebetulan berbeda.
pub fn siapkan_chat(ws: WebSocketUpgrade) -> WebSocketUpgrade {
    ws.read_buffer_size(WS_READ_BUFFER)
        .write_buffer_size(WS_WRITE_BUFFER)
        .max_message_size(WS_MAX_MESSAGE_CHAT)
        .max_frame_size(WS_MAX_MESSAGE_CHAT)
}

/// Sama, untuk socket SINYAL (live publish/subscribe, meet). Bedanya hanya
/// plafon pesan: muatannya SDP, bukan teks chat.
pub fn siapkan_sinyal(ws: WebSocketUpgrade) -> WebSocketUpgrade {
    ws.read_buffer_size(WS_READ_BUFFER)
        .write_buffer_size(WS_WRITE_BUFFER)
        .max_message_size(WS_MAX_MESSAGE_SIGNAL)
        .max_frame_size(WS_MAX_MESSAGE_SIGNAL)
}
