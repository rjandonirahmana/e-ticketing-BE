#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use leptos::prelude::*;

#[cfg(feature = "ssr")]
pub(super) async fn app_state() -> Result<std::sync::Arc<crate::state::AppState>, ServerFnError> {
    use axum::Extension;
    leptos_axum::extract::<Extension<std::sync::Arc<crate::state::AppState>>>()
        .await
        .map(|ext| ext.0)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(format!("AppState unavailable: {e}")) })
}

#[cfg(feature = "ssr")]
pub(super) async fn auth_claims() -> Result<crate::models::auth::Claims, ServerFnError> {
    let token = super::session::get_auth_token().await.ok_or_else(|| -> ServerFnError {
        ServerFnError::ServerError("Tidak terautentikasi".into())
    })?;
    let state = app_state().await?;
    state
        .jwt
        .verify(&token)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })
}

/// Verify the caller is authenticated AND holds exactly `required` role.
///
/// SECURITY: server functions are the real authorization boundary — the
/// client-side route guards (AuthGuard/AdminGuard/MerchantGuard in app.rs)
/// only hide UI and can be bypassed by calling the `/api-fn` endpoint
/// directly. Every privileged server fn MUST gate on role here, not just
/// `auth_claims()` (which only proves the caller is logged in).
#[cfg(feature = "ssr")]
pub(super) async fn require_role(
    required: &str,
) -> Result<crate::models::auth::Claims, ServerFnError> {
    let claims = auth_claims().await?;
    if claims.role != required {
        return Err(ServerFnError::ServerError(format!(
            "Akses ditolak: endpoint memerlukan peran '{required}'"
        )));
    }
    Ok(claims)
}

/// Verify the caller is authenticated AND holds one of `roles`.
#[cfg(feature = "ssr")]
pub(super) async fn require_roles(
    roles: &[&str],
) -> Result<crate::models::auth::Claims, ServerFnError> {
    let claims = auth_claims().await?;
    if !roles.iter().any(|r| *r == claims.role) {
        return Err(ServerFnError::ServerError(format!(
            "Akses ditolak: endpoint memerlukan salah satu peran {roles:?}"
        )));
    }
    Ok(claims)
}

#[cfg(feature = "ssr")]
pub(super) fn map_app_error(e: crate::utils::error::AppError) -> ServerFnError {
    ServerFnError::ServerError(e.to_string())
}

#[cfg(feature = "ssr")]
pub(super) fn srv_user_to_web(u: crate::models::users::UserResponse) -> crate::web::models::UserResponse {
    crate::web::models::UserResponse {
        id: u.id,
        email: u.email,
        name: u.name,
        phone: u.phone,
        role: u.role,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_product_to_web(e: crate::models::products::Product) -> crate::web::models::Product {
    crate::web::models::Product {
        id: e.id,
        merchant_id: e.merchant_id,
        name: e.name,
        slug: e.slug,
        description: e.description,
        cover_url: e.cover_url,
        price: e.price,
        sale_price: e.sale_price,
        display_price: e.display_price,
        venue: e.venue,
        city: e.city,
        latitude: e.latitude,
        longitude: e.longitude,
        category: e.category,
        event_date: e.event_date,
        start_time: e.start_time,
        end_time: e.end_time,
        status: e.status,
        total_sold: e.total_sold,
        total_quota: e.total_quota,
        merchant_name: e.merchant_name,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_product_variant_to_web(
    v: crate::models::product_variants::ProductVariantResponse,
) -> crate::web::models::ProductVariant {
    crate::web::models::ProductVariant {
        id: v.id,
        event_id: v.event_id,
        name: v.name,
        description: v.description,
        price: v.price,
        sale_price: v.sale_price,
        display_price: v.effective_price,
        quota: v.quota,
        remaining: v.available,
        max_per_order: v.max_per_order,
        is_active: v.is_active,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_product_with_variants_to_web(
    e: crate::models::products::ProductWithVariants,
) -> crate::web::models::ProductWithVariants {
    crate::web::models::ProductWithVariants {
        id: e.id,
        merchant_id: e.merchant_id,
        name: e.name,
        slug: e.slug,
        description: e.description,
        cover_url: e.cover_url,
        cover_focus: e.cover_focus,
        venue: e.venue,
        city: e.city,
        latitude: e.latitude,
        longitude: e.longitude,
        category: e.category,
        event_date: e.event_date,
        start_time: e.start_time,
        end_time: e.end_time,
        status: e.status,
        price: e.price,
        sale_price: e.sale_price,
        display_price: e.display_price,
        total_sold: e.total_sold,
        total_quota: e.total_quota,
        merchant_name: e.merchant_name,
        merchant: e.merchant.map(|m| crate::web::models::ProductMerchantInfo {
            logo_url: m.logo_url,
            header_url: m.header_url,
            description: m.description,
            verified: m.verified,
            followers: m.followers,
            products_count: m.products_count,
            rating_avg: m.rating_avg,
            rating_count: m.rating_count,
        }),
        product_variants: e.product_variants.into_iter().map(srv_product_variant_to_web).collect(),
        detail_images: e
            .detail_images
            .into_iter()
            .map(|d| crate::web::models::WebDetailImage {
                url: d.url,
                image_type: d.image_type,
                caption: d.caption,
                focus: d.focus,
            })
            .collect(),
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_paginated_products_to_web(
    p: crate::models::products::PaginatedProducts,
) -> crate::web::models::PaginatedProducts {
    crate::web::models::PaginatedProducts {
        data: p.data.into_iter().map(srv_product_to_web).collect(),
        total: p.total,
        page: p.page,
        per_page: p.per_page,
        total_pages: p.total_pages,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_banner_to_web(b: crate::models::banners::Banner) -> crate::web::models::Banner {
    crate::web::models::Banner {
        id: b.id,
        image_url: b.image_url,
        link_url: b.click_url,
        title: None,
        sort_order: 0,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_ticket_to_web(t: crate::models::tickets::TicketResponse) -> crate::web::models::TicketResponse {
    crate::web::models::TicketResponse {
        id: t.id,
        ticket_code: t.ticket_code,
        status: t.status,
        used_at: t.used_at,
        created_at: t.created_at,
        order_id: t.order_id,
        order_code: t.order_code,
        event_id: t.event_id,
        event_name: t.event_name,
        event_slug: t.event_slug,
        event_date: t.event_date,
        event_venue: t.event_venue,
        event_city: t.event_city,
        variant_id: t.variant_id,
        variant_name: t.variant_name,
        unit_price: t.unit_price,
        cover_url: t.cover_url,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_order_list_item_to_web(
    o: crate::models::orders::OrderListItem,
) -> crate::web::models::OrderListItem {
    use rust_decimal::prelude::ToPrimitive;
    crate::web::models::OrderListItem {
        id: o.id,
        order_code: o.order_code,
        status: o.status,
        total_amount: o.total_amount.to_f64().unwrap_or(0.0),
        event_name: o.event_name,
        event_date: o.event_date,
        venue: o.venue,
        cover_url: o.cover_url,
        created_at: o.created_at,
        expired_at: o.expired_at,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_order_detail_to_web(
    o: crate::models::orders::OrderDetailResponse,
) -> crate::web::models::OrderDetail {
    use rust_decimal::prelude::ToPrimitive;
    let items = o
        .items
        .into_iter()
        .map(|i| crate::web::models::OrderItem {
            event_name: i.event_name,
            variant_name: i.variant_name,
            quantity: i.quantity,
            subtotal: i.subtotal.to_f64().unwrap_or(0.0),
        })
        .collect();
    crate::web::models::OrderDetail {
        id: o.id,
        order_code: o.order_code,
        status: o.status,
        total_amount: o.total_amount.to_f64().unwrap_or(0.0),
        payment_method: o.payment_method.clone(),
        paid_at: o.paid_at,
        expired_at: o.expired_at,
        created_at: Some(o.created_at),
        items,
        subtotal_amount: o.subtotal_amount.to_f64().unwrap_or(0.0),
        discount_amount: o.discount_amount.to_f64().unwrap_or(0.0),
        promo_code: o.promo_code,
        payment_code: o.payment_code.or(o.payment_method),
        payment_name: o.payment_name,
        payment_charge: o.payment_charge.to_f64().unwrap_or(0.0),
        payment_reference: o.payment_reference,
        payment_instruction: o.payment_instruction,
        payment_expired_at: o.payment_expired_at,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_notification_to_web(n: crate::models::notification::Notification) -> crate::web::models::NotificationItem {
    crate::web::models::NotificationItem {
        id: n.id,
        kind: n.kind,
        title: n.title,
        body: n.body,
        is_read: n.is_read,
        target_id: n.target_id,
        created_at: Some(n.created_at),
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_story_groups_to_web(
    groups: Vec<crate::models::stories::StoryGroupResponse>,
) -> Vec<crate::web::state::stories::StoryGroup> {
    use crate::web::state::stories::{
        OverlayType, StoryGroup, StoryItem, StoryMediaType, StoryOverlay,
    };

    fn map_media(s: &str) -> StoryMediaType {
        if s.eq_ignore_ascii_case("video") {
            StoryMediaType::Video
        } else {
            StoryMediaType::Image
        }
    }

    fn map_overlay(v: serde_json::Value) -> Option<StoryOverlay> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiOverlay {
            #[serde(default)]
            id: String,
            #[serde(default, alias = "type", alias = "overlayType")]
            overlay_type: String,
            #[serde(default)]
            x: f64,
            #[serde(default)]
            y: f64,
            #[serde(default)]
            content: Option<String>,
            #[serde(default)]
            color: Option<String>,
            #[serde(default)]
            font_size: Option<i32>,
            #[serde(default)]
            rotation: Option<f64>,
            #[serde(default)]
            emoji: Option<String>,
            #[serde(default)]
            scale: Option<f64>,
            #[serde(default)]
            z_index: i32,
            #[serde(default)]
            text_style: Option<String>,
            #[serde(default)]
            text_align: Option<String>,
        }
        let o: ApiOverlay = serde_json::from_value(v).ok()?;
        let overlay_type = if o.overlay_type.eq_ignore_ascii_case("sticker") {
            OverlayType::Sticker
        } else {
            OverlayType::Text
        };
        Some(StoryOverlay {
            id: o.id,
            overlay_type,
            x: o.x,
            y: o.y,
            content: o.content,
            color: o.color,
            font_size: o.font_size,
            rotation: o.rotation,
            emoji: o.emoji,
            scale: o.scale,
            z_index: o.z_index,
            text_style: o.text_style,
            text_align: o.text_align,
        })
    }

    groups
        .into_iter()
        .map(|g| {
            let stories: Vec<StoryItem> = g
                .stories
                .into_iter()
                .map(|s| StoryItem {
                    id: s.id,
                    user_id: s.user_id,
                    username: s.username,
                    avatar_url: s.avatar_url,
                    media_url: s.media_url,
                    media_type: map_media(&s.media_type),
                    filter: s.filter,
                    overlays: s.overlays.into_iter().filter_map(map_overlay).collect(),
                    created_at: s.created_at,
                    expires_at: s.expires_at,
                    viewed: s.viewed,
                    event_id: s.event_id,
                    event_slug: s.event_slug,
                    event_title: s.event_title,
                })
                .collect();
            let all_viewed = !stories.is_empty() && stories.iter().all(|s| s.viewed);
            StoryGroup {
                user_id: g.user_id,
                username: g.username,
                avatar_url: g.avatar_url,
                all_viewed,
                stories,
            }
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub(super) fn srv_group_room_to_web(r: crate::models::group_chat::GroupRoom) -> crate::web::models::ChatRoom {
    crate::web::models::ChatRoom {
        id: r.id,
        event_id: r.event_id,
        name: r.name,
        member_count: r.member_count as i32,
        last_message: None,
        // Dulu ditulis mati `0` — kolom `unread_count` di model web memang ada
        // sejak awal, tetapi tak pernah ada yang mengisinya, sehingga lencana
        // "pesan baru" mustahil muncul betapapun banyak pesan yang masuk.
        unread_count: r.unread_count as i32,
        cover_url: r.cover_url,
    }
}

#[cfg(feature = "ssr")]
pub(super) fn srv_group_message_to_web(m: crate::models::group_chat::GroupMessage) -> crate::web::models::ChatMessage {
    crate::web::models::ChatMessage {
        id: m.id,
        room_id: m.room_id,
        sender_id: m.sender_id,
        sender_name: m.sender_name,
        content: m.content,
        sent_at: m.sent_at.timestamp_millis() as u64,
        message_type: m.msg_type.as_str().to_string(),
    }
}
