use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetNotifications, "/api-fn")]
pub async fn get_notifications() -> Result<Vec<NotificationItem>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let notifs = state
        .notification_store_svc
        .list(&claims.user_id, 1, 100)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    return Ok(notifs.into_iter().map(srv_notification_to_web).collect());
}

#[server(GetNotificationDetail, "/api-fn")]
pub async fn get_notification_detail(id: String) -> Result<NotificationItem, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    // Ambil DULU, tandai terbaca SESUDAHNYA.
    //
    // Urutan lama menandai terbaca lebih dulu, lalu bisa gagal menemukan
    // isinya — meninggalkan notifikasi yang tercatat sudah dibaca padahal tak
    // pernah terbaca. Menandai hanya setelah isinya benar-benar di tangan
    // membuat kedua hal itu tak bisa lagi berselisih.
    //
    // `find_detail` sudah men-scope barisnya ke pemiliknya (`AND user_id`),
    // jadi id milik orang lain tetap menjawab "tidak ditemukan".
    let notif = state
        .notification_store_svc
        .detail(&id, &claims.user_id)
        .await
        .map_err(map_app_error)?;

    let _ = state
        .notification_store_svc
        .mark_read(&id, &claims.user_id)
        .await;

    return Ok(srv_notification_to_web(notif));
}

#[server(MarkNotificationRead, "/api-fn")]
pub async fn mark_notification_read(id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    return state
        .notification_store_svc
        .mark_read(&id, &claims.user_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) });
}

#[server(GetNotifUnreadCount, "/api-fn")]
pub async fn get_notif_unread_count() -> Result<i64, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    return state
        .notification_store_svc
        .unread_count(&claims.user_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) });
}

#[server(MarkAllNotificationsRead, "/api-fn")]
pub async fn mark_all_notifications_read() -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    return state
        .notification_store_svc
        .mark_all_read(&claims.user_id)
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) });
}
