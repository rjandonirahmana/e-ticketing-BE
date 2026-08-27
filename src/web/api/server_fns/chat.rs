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


/// Cari percakapan dengan sebuah toko — TANPA membuatnya.
///
/// Menggantikan `open_chat_with_merchant` yang dulu langsung membuat room saat
/// tombol chat ditekan. Membuat lebih awal berarti setiap orang yang sekadar
/// menekan ikonnya lalu pergi meninggalkan percakapan kosong di inbox merchant
/// — dan makin ramai produknya, makin banyak baris kosong yang harus disaring
/// merchant untuk menemukan pertanyaan sungguhan.
///
/// `None` = belum pernah ada percakapan. Halaman chat menampilkannya sebagai
/// percakapan kosong yang siap diketik, bukan sebagai galat.
#[server(FindChatWithMerchant, "/api-fn")]
pub async fn find_chat_with_merchant(
    merchant_id: String,
) -> Result<Option<String>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let room = state
        .group_chat_svc
        .find_dm(&claims.user_id, &merchant_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    return Ok(room.map(|r| r.id));
}

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
        .send_text(&room.id, &claims.user_id, &claims.name, &claims.role, &content)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    return Ok(room.id);
}
