use crate::web::models::*;
use leptos::prelude::*;
use super::helpers::*;

#[server(GetMerchantEvents, "/api-fn")]
pub async fn get_merchant_events(page: Option<i64>) -> Result<PaginatedEvents, ServerFnError> {
    use crate::models::events::EventListQuery;
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;
    let q = EventListQuery {
        page,
        per_page: Some(20),
        city: None,
        category: None,
        search: None,
        status: None,
    };
    let result = state
        .event_svc
        .list(q, Some(&claims.user_id))
        .await
        .map_err(map_app_error)?;
    return Ok(srv_paginated_events_to_web(result));
}

#[server(GetMerchantEventDetail, "/api-fn")]
pub async fn get_merchant_event_detail(slug: String) -> Result<EventWithVariants, ServerFnError> {
    let _claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;
    let result = state
        .event_svc
        .get(&slug)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_event_with_variants_to_web(result));
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
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<String, ServerFnError> {
    use crate::models::events::{CreateEventRequest, CreateVariantInline};
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;

    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let event_date_dt: chrono::DateTime<chrono::Utc> = event_date
        .parse()
        .map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?;
    let start_time_dt: Option<chrono::DateTime<chrono::Utc>> = if start_time.is_empty() {
        None
    } else {
        Some(start_time.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };

    // Get merchant name from merchant profile
    let merchant_name = state
        .merchant_svc
        .get_profile(&claims.user_id)
        .await
        .map(|m| m.store_name)
        .unwrap_or_else(|_| claims.name.clone());

    let req = CreateEventRequest {
        merchant_name: merchant_name.clone(),
        name,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        venue: if venue.is_empty() { None } else { Some(venue) },
        city: if city.is_empty() { None } else { Some(city) },
        latitude,
        longitude,
        category: cats,
        event_date: event_date_dt,
        start_time: start_time_dt,
        end_time: None,
        variants: vec![CreateVariantInline {
            name: "Umum".into(),
            description: None,
            price: 0.0,
            sale_price: None,
            sale_price_start_date: None,
            sale_price_end_date: None,
            quota: 100,
            max_per_order: None,
            sort_order: None,
        }],
        detail_images: vec![],
    };

    let result = state
        .event_svc
        .create(&claims.user_id, &merchant_name, req, None)
        .await
        .map_err(map_app_error)?;
    return Ok(result.slug);
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
    latitude: Option<f64>,
    longitude: Option<f64>,
) -> Result<(), ServerFnError> {
    use crate::models::events::UpdateEventRequest;
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;

    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let event_date_dt: Option<chrono::DateTime<chrono::Utc>> = if event_date.is_empty() {
        None
    } else {
        Some(event_date.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };
    let start_time_dt: Option<chrono::DateTime<chrono::Utc>> = if start_time.is_empty() {
        None
    } else {
        Some(start_time.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };

    // Find the event by slug to get its id and owner.
    let event = state
        .event_svc
        .get(&slug)
        .await
        .map_err(map_app_error)?;

    // Admin can update any event; merchant can only update their own.
    let effective_merchant_id = if claims.role == "admin" {
        event.merchant_id.clone()
    } else {
        claims.user_id.clone()
    };

    let req = UpdateEventRequest {
        name: if name.is_empty() { None } else { Some(name) },
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        cover_url: None,
        venue: if venue.is_empty() { None } else { Some(venue) },
        city: if city.is_empty() { None } else { Some(city) },
        latitude,
        longitude,
        event_date: event_date_dt,
        category: cats,
        start_time: start_time_dt,
        end_time: None,
        status: Some("edited".into()),
        detail_images: None,
        variants: None,
    };

    state
        .event_svc
        .update(&event.id, &effective_merchant_id, req)
        .await
        .map_err(map_app_error)?;
    return Ok(());
}
