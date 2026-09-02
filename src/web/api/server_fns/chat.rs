use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetChatRooms, "/api-fn")]
pub async fn get_chat_rooms() -> Result<Vec<ChatRoom>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let rooms = state
        .group_chat_svc
        .get_user_rooms(&claims.user_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    return Ok(rooms.into_iter().map(srv_group_room_to_web).collect());
}

#[server(GetChatHistory, "/api-fn")]
pub async fn get_chat_history(room_id: String) -> Result<Vec<ChatMessage>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let (messages, _has_more) = state
        .group_chat_svc
        .get_history(&room_id, &claims.user_id, 100, None)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    return Ok(messages.into_iter().map(srv_group_message_to_web).collect());
}

/// Tandai satu percakapan sudah dibaca sampai sekarang.
///
/// Dipanggil saat halaman percakapan dibuka. Tanpa ini lencana "pesan baru"
/// tak pernah turun — ia akan terus tumbuh meski percakapannya dibaca.
#[server(MarkChatRead, "/api-fn")]
pub async fn mark_chat_read(room_id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .group_chat_svc
        .mark_read(&room_id, &claims.user_id)
        .await
        .map_err(|e| ServerFnError::ServerError(e.to_string()))
}

#[server(GetChatRoomDetail, "/api-fn")]
pub async fn get_chat_room_detail(room_id: String) -> Result<ChatRoom, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let rooms = state
        .group_chat_svc
        .get_user_rooms(&claims.user_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    return rooms
        .into_iter()
        .find(|r| r.id == room_id)
        .map(srv_group_room_to_web)
        .ok_or_else(|| -> ServerFnError {
            ServerFnError::ServerError("Room not found".into())
        });
}

// `join_chat_room` DIBUANG bersama grup produk (migrasi 029). Percakapan berdua
// sudah memuat kedua pesertanya sejak baris `chats` lahir — tak ada keadaan di
// mana seseorang perlu "bergabung" ke percakapannya sendiri.

/// Kirim pesan PERTAMA ke sebuah toko: room dibuat di sini, lalu pesannya
/// disimpan. Mengembalikan `room_id` supaya halaman bisa berpindah ke jalur
/// WebSocket seperti percakapan biasa.
///
/// Room lahir dari pesan, bukan dari klik. Konsekuensinya: percakapan yang ada
/// di inbox dijamin berisi setidaknya satu pesan — tak ada lagi baris kosong
/// yang tak pernah bisa dijelaskan asalnya.
///
/// Idempotensi tetap terjaga: `ensure_dm` mengembalikan room yang sudah ada
/// bila pembeli menekan kirim dua kali dari dua tab.
#[server(SendFirstChatMessage, "/api-fn")]
pub async fn send_first_chat_message(
    merchant_id: String,
    content: String,
) -> Result<String, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    if content.trim().is_empty() {
        return Err(ServerFnError::ServerError("Pesan tidak boleh kosong".into()));
    }

    // Pembatas laju yang SAMA dengan jalur WebSocket (30 pesan / 10 detik
    // per user), bukan pembatas kedua yang ditulis khusus di sini.
    //
    // Ini penting sejak plafon "satu pesan per percakapan" dibuang dari
    // `authorize_and_save`: plafon itu — betapapun kelirunya — adalah
    // satu-satunya hal yang menahan jalur INI. Jalur WebSocket sudah lama
    // dijaga `dispatch`, tetapi server function tidak melewatinya sama sekali,
    // sehingga tanpa baris ini pengiriman pesan pertama menjadi endpoint tanpa
    // batas: tiap panggilan membuat percakapan baru ke toko mana pun.
    //
    // Memakai registry yang sama berarti jatah seorang pengguna tetap satu,
    // dari jalur mana pun ia mengirim — dua pembatas terpisah akan menjumlahkan
    // jatahnya tanpa ada yang bermaksud begitu.
    if !state.ws_mgr.check_rate_limit(&claims.user_id) {
        return Err(ServerFnError::ServerError(
            "Terlalu banyak pesan, coba lagi sebentar.".into(),
        ));
    }

    // Pastikan tokonya ada sebelum apa pun dibuat — tanpa ini, id sembarangan
    // melahirkan percakapan dengan lawan bicara yang tak pernah ada.
    let _ = state
        .merchant_svc
        .public_profile(&merchant_id)
        .await
        .map_err(map_app_error)?;

    let room = state
        .group_chat_svc
        .ensure_dm(&claims.user_id, &merchant_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    state
        .group_chat_svc
        .send_text(&room.id, &claims.user_id, &claims.name, &content, None)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    return Ok(room.id);
}
