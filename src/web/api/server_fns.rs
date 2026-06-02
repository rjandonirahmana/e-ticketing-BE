//! server_fns.rs — Leptos server functions.
//! Auth via HttpOnly cookie `pulse_token` — tidak ada localStorage.

use crate::web::models::*;
use leptos::prelude::*;

// ═══════════════════════════════════════════════════════════════════════════════
//  Shared resources (SSR only)
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "ssr")]
static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("reqwest client should build")
});

#[cfg(feature = "ssr")]
static BACKEND_URL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into())
});

#[cfg(feature = "ssr")]
fn client() -> &'static reqwest::Client {
    &HTTP_CLIENT
}

#[cfg(feature = "ssr")]
fn base_url() -> &'static str {
    &BACKEND_URL
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Internal app token (X-App-Token)
// ═══════════════════════════════════════════════════════════════════════════════
//
// Semua route `/api/*` di backend dilindungi middleware `require_internal_jwt`
// yang mewajibkan header `X-App-Token` berupa JWT HS256 dengan:
//   - iss = "kinetic-fe"
//   - exp valid (belum kedaluwarsa)
//   - di-sign dengan INTERNAL_JWT_SECRET yang SAMA dengan backend.
//
// Karena server function ini berjalan di proses yang sama dengan backend,
// secret di-resolve dengan cara yang IDENTIK dengan `config::config::AppConfig`
// (runtime `INTERNAL_JWT_SECRET`, dengan fallback dev yang sama persis) sehingga
// token yang kita sign selalu lolos verifikasi self-call.

#[cfg(feature = "ssr")]
static INTERNAL_JWT_SECRET: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("INTERNAL_JWT_SECRET")
        .unwrap_or_else(|_| "kinetic-internal-dev-secret-changeme-in-production".into())
});

/// Bangun JWT internal HS256 (`X-App-Token`) untuk satu request ke backend.
/// Token berlaku 5 menit — cukup untuk satu round-trip request/response.
#[cfg(feature = "ssr")]
fn app_token() -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    #[derive(serde::Serialize)]
    struct InternalClaims {
        iss: &'static str,
        iat: i64,
        exp: i64,
    }

    let now = chrono::Utc::now().timestamp();
    let claims = InternalClaims {
        iss: "kinetic-fe",
        iat: now,
        exp: now + 300, // 5 menit
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(INTERNAL_JWT_SECRET.as_bytes()),
    )
    .unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Cookie helpers
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "ssr")]
pub async fn get_auth_token() -> Option<String> {
    use axum::http::{header::COOKIE, HeaderMap};
    use leptos_axum::extract;

    let headers: HeaderMap = extract().await.ok()?;
    let cookie_hdr = headers.get(COOKIE)?.to_str().ok()?;

    cookie_hdr.split(';').map(|p| p.trim()).find_map(|part| {
        part.strip_prefix("pulse_token=")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

#[cfg(feature = "ssr")]
pub fn set_auth_cookie(token: &str) {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos_axum::ResponseOptions;

    let resp = expect_context::<ResponseOptions>();
    let value = format!("pulse_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800");
    if let Ok(hv) = HeaderValue::from_str(&value) {
        resp.append_header(SET_COOKIE, hv);
    }
}

#[cfg(feature = "ssr")]
pub fn clear_auth_cookie() {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos_axum::ResponseOptions;

    let resp = expect_context::<ResponseOptions>();
    let value = "pulse_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; \
                 Expires=Thu, 01 Jan 1970 00:00:00 GMT";
    if let Ok(hv) = HeaderValue::from_str(value) {
        resp.append_header(SET_COOKIE, hv);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  HTTP helpers
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "ssr")]
async fn api_get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ServerFnError> {
    let url = format!("{}{}", base_url(), path);
    let resp = client()
        .get(&url)
        .header("X-App-Token", app_token())
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[cfg(feature = "ssr")]
async fn api_get_auth<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let url = format!("{}{}", base_url(), path);
    let resp = client()
        .get(&url)
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[cfg(feature = "ssr")]
async fn api_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ServerFnError> {
    let url = format!("{}{}", base_url(), path);
    let resp = client()
        .post(&url)
        .header("X-App-Token", app_token())
        .json(body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[cfg(feature = "ssr")]
async fn api_post_auth<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let url = format!("{}{}", base_url(), path);
    let resp = client()
        .post(&url)
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[cfg(feature = "ssr")]
async fn api_put_auth<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    path: &str,
    body: &B,
) -> Result<T, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let url = format!("{}{}", base_url(), path);
    let resp = client()
        .put(&url)
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Server Functions
// ═══════════════════════════════════════════════════════════════════════════════

// ── Session ───────────────────────────────────────────────────────────────────

#[server(GetSession, "/api-fn")]
pub async fn get_session() -> Result<Option<UserResponse>, ServerFnError> {
    let Some(token) = get_auth_token().await else {
        return Ok(None);
    };
    let resp = client()
        .get(format!("{}/api/auth/me", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(Some(resp.json::<UserResponse>().await.map_err(
            |e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) },
        )?))
    } else {
        clear_auth_cookie();
        Ok(None)
    }
}

// ── Auth ──────────────────────────────────────────────────────────────────────

#[server(LoginAction, "/api-fn")]
pub async fn login_action(phone: String, password: String) -> Result<UserResponse, ServerFnError> {
    let resp = client()
        .post(format!("{}/api/auth/login", base_url()))
        .header("X-App-Token", app_token())
        .json(&serde_json::json!({ "phone": phone, "password": password }))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let auth: AuthResponse = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        set_auth_cookie(&auth.token);
        Ok(auth.user)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Login gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(RegisterAction, "/api-fn")]
pub async fn register_action(
    name: String,
    phone: String,
    role: String,
) -> Result<(), ServerFnError> {
    api_post(
        "/api/auth/register",
        &serde_json::json!({ "name": name, "phone": phone, "role": role }),
    )
    .await
}

#[server(VerifyOtpAction, "/api-fn")]
pub async fn verify_otp_action(phone: String, otp: String) -> Result<UserResponse, ServerFnError> {
    let resp = client()
        .post(format!("{}/api/auth/verify", base_url()))
        .header("X-App-Token", app_token())
        .json(&serde_json::json!({ "phone": phone, "otp": otp }))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let auth: AuthResponse = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        set_auth_cookie(&auth.token);
        Ok(auth.user)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Verifikasi gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(ResendOtpAction, "/api-fn")]
pub async fn resend_otp_action(name: String, phone: String) -> Result<(), ServerFnError> {
    let _: serde_json::Value = api_post(
        "/api/auth/register",
        &serde_json::json!({ "name": name, "phone": phone, "role": "customer" }),
    )
    .await
    .unwrap_or(serde_json::Value::Null);
    Ok(())
}

#[server(LogoutAction, "/api-fn")]
pub async fn logout_action() -> Result<(), ServerFnError> {
    clear_auth_cookie();
    Ok(())
}

// ── Events (public) ───────────────────────────────────────────────────────────

#[server(GetEvents, "/api-fn")]
pub async fn get_events(
    page: Option<i64>,
    city: Option<String>,
    category: Option<String>,
    search: Option<String>,
) -> Result<PaginatedEvents, ServerFnError> {
    use reqwest::Url;

    let base = format!("{}/api/events", base_url());
    let mut url = Url::parse(&base)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("per_page", "12");
        if let Some(p) = page {
            q.append_pair("page", &p.to_string());
        }
        if let Some(c) = city.filter(|s| !s.is_empty()) {
            q.append_pair("city", &c);
        }
        if let Some(c) = category.filter(|s| !s.is_empty()) {
            q.append_pair("category", &c);
        }
        if let Some(s) = search.filter(|s| !s.is_empty()) {
            q.append_pair("search", &s);
        }
    }
    let resp = client()
        .get(url)
        .header("X-App-Token", app_token())
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        Ok(resp
            .json::<PaginatedEvents>()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Request gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(GetEventDetail, "/api-fn")]
pub async fn get_event_detail(slug: String) -> Result<EventWithVariants, ServerFnError> {
    api_get(&format!("/api/events/{slug}")).await
}

#[server(GetCategories, "/api-fn")]
pub async fn get_categories() -> Result<Vec<String>, ServerFnError> {
    let v: serde_json::Value = api_get("/api/events/categories").await?;
    Ok(v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

// ── Banners (public) ──────────────────────────────────────────────────────────

#[server(GetBanners, "/api-fn")]
pub async fn get_banners() -> Result<Vec<Banner>, ServerFnError> {
    api_get("/api/banners").await
}

// ── Auth: Protected User ──────────────────────────────────────────────────────

#[server(GetMe, "/api-fn")]
pub async fn get_me() -> Result<UserResponse, ServerFnError> {
    api_get_auth("/api/auth/me").await
}

// ── Tickets ───────────────────────────────────────────────────────────────────

#[server(GetMyTickets, "/api-fn")]
pub async fn get_my_tickets() -> Result<Vec<TicketResponse>, ServerFnError> {
    api_get_auth("/api/tickets").await
}

#[server(GetTicketDetail, "/api-fn")]
pub async fn get_ticket_detail(id: String) -> Result<TicketResponse, ServerFnError> {
    api_get_auth(&format!("/api/tickets/{id}")).await
}

// ── Orders ────────────────────────────────────────────────────────────────────

#[server(GetMyOrders, "/api-fn")]
pub async fn get_my_orders() -> Result<Vec<OrderListItem>, ServerFnError> {
    api_get_auth("/api/orders").await
}

#[server(GetOrderDetail, "/api-fn")]
pub async fn get_order_detail(id: String) -> Result<OrderDetail, ServerFnError> {
    api_get_auth(&format!("/api/orders/{id}")).await
}

#[server(GetOrderTickets, "/api-fn")]
pub async fn get_order_tickets(order_id: String) -> Result<Vec<TicketResponse>, ServerFnError> {
    api_get_auth(&format!("/api/orders/{order_id}/tickets")).await
}

#[server(CreateOrder, "/api-fn")]
pub async fn create_order(variant_id: String, quantity: i32) -> Result<String, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let resp = client()
        .post(format!("{}/api/orders", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "items": [{ "ticket_variant_id": variant_id, "quantity": quantity }]
        }))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data["id"].as_str().unwrap_or("").to_string())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Order gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(CreateOrderMulti, "/api-fn")]
pub async fn create_order_multi(
    variant_id: String,
    quantity: i32,
    payment_method: String,
) -> Result<String, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let resp = client()
        .post(format!("{}/api/orders", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "items": [{ "ticket_variant_id": variant_id, "quantity": quantity }],
            "payment_method": payment_method
        }))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data["id"].as_str().unwrap_or("").to_string())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Order gagal".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

// ── Notifications ─────────────────────────────────────────────────────────────

#[server(GetNotifications, "/api-fn")]
pub async fn get_notifications() -> Result<Vec<NotificationItem>, ServerFnError> {
    api_get_auth("/api/notifications").await
}

#[server(GetNotificationDetail, "/api-fn")]
pub async fn get_notification_detail(id: String) -> Result<NotificationItem, ServerFnError> {
    api_get_auth(&format!("/api/notifications/{id}")).await
}

#[server(MarkNotificationRead, "/api-fn")]
pub async fn mark_notification_read(id: String) -> Result<(), ServerFnError> {
    let _: serde_json::Value = api_post_auth(
        &format!("/api/notifications/{id}/read"),
        &serde_json::json!({}),
    )
    .await
    .unwrap_or(serde_json::Value::Null);
    Ok(())
}

// ── Merchant ──────────────────────────────────────────────────────────────────

#[server(GetMerchantEvents, "/api-fn")]
pub async fn get_merchant_events(page: Option<i64>) -> Result<PaginatedEvents, ServerFnError> {
    let p = page.unwrap_or(1);
    api_get_auth(&format!("/api/merchant/events?page={p}&per_page=20")).await
}

#[server(GetMerchantEventDetail, "/api-fn")]
pub async fn get_merchant_event_detail(slug: String) -> Result<EventWithVariants, ServerFnError> {
    api_get_auth(&format!("/api/merchant/events/{slug}")).await
}

#[server(CreateMerchantEvent, "/api-fn")]
pub async fn create_merchant_event(
    name: String,
    description: String,
    venue: String,
    city: String,
    event_date: String,
    start_time: String,
    categories: String,
) -> Result<String, ServerFnError> {
    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let body = serde_json::json!({
        "name": name,
        "description": description,
        "venue": venue,
        "city": city,
        "event_date": event_date,
        "start_time": start_time,
        "category": cats,
    });

    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let resp = client()
        .post(format!("{}/api/merchant/events", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data["slug"].as_str().unwrap_or("").to_string())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_else(|_| "Gagal membuat event".into());
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(UpdateMerchantEvent, "/api-fn")]
pub async fn update_merchant_event(
    slug: String,
    name: String,
    description: String,
    venue: String,
    city: String,
    event_date: String,
    start_time: String,
    categories: String,
) -> Result<(), ServerFnError> {
    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let body = serde_json::json!({
        "name": name,
        "description": description,
        "venue": venue,
        "city": city,
        "event_date": event_date,
        "start_time": start_time,
        "category": cats,
    });

    let _: serde_json::Value = api_put_auth(&format!("/api/merchant/events/{slug}"), &body).await?;
    Ok(())
}

// ── Scan ──────────────────────────────────────────────────────────────────────

#[server(ScanTicket, "/api-fn")]
pub async fn scan_ticket(ticket_code: String) -> Result<String, ServerFnError> {
    let body = serde_json::json!({ "ticket_code": ticket_code });
    let result: serde_json::Value =
        api_post_auth("/api/tickets/scan", &body).await?;
    Ok(result["message"].as_str().unwrap_or("OK").to_string())
}

// ── Chat / Pulse ──────────────────────────────────────────────────────────────

#[server(GetChatRooms, "/api-fn")]
pub async fn get_chat_rooms() -> Result<Vec<ChatRoom>, ServerFnError> {
    api_get_auth("/api/chat/rooms").await
}

#[server(GetChatHistory, "/api-fn")]
pub async fn get_chat_history(room_id: String) -> Result<Vec<ChatMessage>, ServerFnError> {
    api_get_auth(&format!("/api/chat/rooms/{room_id}/history")).await
}

#[server(GetChatRoomDetail, "/api-fn")]
pub async fn get_chat_room_detail(room_id: String) -> Result<ChatRoom, ServerFnError> {
    api_get_auth(&format!("/api/chat/rooms/{room_id}")).await
}

#[server(JoinChatRoom, "/api-fn")]
pub async fn join_chat_room(room_id: String) -> Result<(), ServerFnError> {
    let _: serde_json::Value = api_post_auth(
        &format!("/api/chat/rooms/{room_id}/join"),
        &serde_json::json!({}),
    )
    .await
    .unwrap_or(serde_json::Value::Null);
    Ok(())
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[server(GetAdminStats, "/api-fn")]
pub async fn get_admin_stats() -> Result<AdminStats, ServerFnError> {
    api_get_auth("/api/admin/stats").await
}

#[server(GetAdminUsers, "/api-fn")]
pub async fn get_admin_users(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let p = page.unwrap_or(1);
    api_get_auth(&format!("/api/admin/users?page={p}&per_page=20")).await
}

#[server(GetAdminOrders, "/api-fn")]
pub async fn get_admin_orders(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let p = page.unwrap_or(1);
    api_get_auth(&format!("/api/admin/orders?page={p}&per_page=20")).await
}

// ── Subscription ─────────────────────────────────────────────────────────────

#[server(GetPremiumStatus, "/api-fn")]
pub async fn get_premium_status() -> Result<serde_json::Value, ServerFnError> {
    api_get_auth("/api/subscriptions/me").await
}

#[server(CreateSubscriptionOrder, "/api-fn")]
pub async fn create_subscription_order(plan: String) -> Result<String, ServerFnError> {
    let body = serde_json::json!({ "plan": plan });
    let result: serde_json::Value = api_post_auth("/api/subscriptions", &body).await?;
    Ok(result["order_id"].as_str().unwrap_or("").to_string())
}

// ── Venue / Location ──────────────────────────────────────────────────────────

#[server(GetEventLocation, "/api-fn")]
pub async fn get_event_location(slug: String) -> Result<serde_json::Value, ServerFnError> {
    api_get(&format!("/api/events/{slug}/location")).await
}

// ── Checkout / Cart ───────────────────────────────────────────────────────────

#[server(CreateOrderCart, "/api-fn")]
pub async fn create_order_cart(
    items_json: String,   // JSON: Vec<{tier_id, quantity}>
    payment_method: String,
    promo_code: Option<String>,
) -> Result<crate::web::models::CreateOrderResponse, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let items: serde_json::Value =
        serde_json::from_str(&items_json).unwrap_or(serde_json::json!([]));
    let mut body = serde_json::json!({
        "items": items,
        "payment_method": payment_method,
    });
    if let Some(promo) = promo_code {
        body["promo_code"] = serde_json::Value::String(promo);
    }
    let resp = client()
        .post(format!("{}/api/orders", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: crate::web::models::CreateOrderResponse = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}

#[server(ValidatePromo, "/api-fn")]
pub async fn validate_promo(
    promo_code: String,
    subtotal: i64,
) -> Result<crate::web::models::ValidatePromoResponse, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let body = serde_json::json!({ "promo_code": promo_code, "subtotal": subtotal });
    let resp = client()
        .post(format!("{}/api/promos/validate", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: crate::web::models::ValidatePromoResponse = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data)
    } else {
        Ok(crate::web::models::ValidatePromoResponse {
            valid: false,
            discount_idr: 0,
            message: "Promo tidak valid".into(),
        })
    }
}

#[server(ConfirmOrderPayment, "/api-fn")]
pub async fn confirm_order_payment(
    order_id: String,
) -> Result<crate::web::models::OrderRef, ServerFnError> {
    let token = get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let resp = client()
        .post(format!("{}/api/orders/{order_id}/pay", base_url()))
        .header("X-App-Token", app_token())
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "payment_token": "qris" }))
        .send()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
    if resp.status().is_success() {
        let data: crate::web::models::OrderRef = resp
            .json()
            .await
            .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;
        Ok(data)
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(ServerFnError::ServerError(format!("HTTP {status}: {body}")))
    }
}
