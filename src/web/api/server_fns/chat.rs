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

#[server(JoinChatRoom, "/api-fn")]
pub async fn join_chat_room(room_id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    return state
        .group_chat_svc
        .join_room(&room_id, &claims.user_id, &claims.name)
        .await
        .map(|_| ())
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) });
}
