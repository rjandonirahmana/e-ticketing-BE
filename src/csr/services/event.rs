use serde::Serialize;

use super::backend::{
    event_to_fe, event_with_variants_to_fe, BeEventWithVariants, BePaginatedEvents,
};
use super::client::{get_private, get_public, ApiError};
use crate::csr::models::*;
use crate::csr::services::client::{post_multipart_private, put_multipart_private};

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3); // worst case: semua byte di-encode jadi %XX
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ── List & detail ────────────────────────────────────────────────────────────

pub async fn list_events(req: &ListEventsRequest) -> Result<ListEventsResponse, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if !req.category.is_empty() {
        params.push(format!("category={}", url_encode(&req.category)));
    }
    if !req.query.is_empty() {
        params.push(format!("search={}", url_encode(&req.query)));
    }
    if req.page > 0 {
        params.push(format!("page={}", req.page));
    }
    if req.page_size > 0 {
        params.push(format!("per_page={}", req.page_size));
    }
    let path = if params.is_empty() {
        "/events".to_string()
    } else {
        format!("/events?{}", params.join("&"))
    };
    let resp: BePaginatedEvents = get_public(&path).await?;
    Ok(ListEventsResponse {
        events: resp.data.into_iter().map(event_to_fe).collect(),
        total: resp.total as i32,
    })
}

pub async fn get_categories() -> Result<Vec<String>, ApiError> {
    #[derive(serde::Deserialize)]
    struct CatResp {
        data: Vec<String>,
    }
    let resp: CatResp = get_public("/events/categories").await?;
    Ok(resp.data)
}

pub async fn list_mine(req: &ListEventsRequest) -> Result<ListEventsResponse, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if req.page > 0 {
        params.push(format!("page={}", req.page));
    }
    if req.page_size > 0 {
        params.push(format!("per_page={}", req.page_size));
    }
    let path = if params.is_empty() {
        "/merchant/events".to_string()
    } else {
        format!("/merchant/events?{}", params.join("&"))
    };
    let resp: BePaginatedEvents = get_private(&path).await?;
    Ok(ListEventsResponse {
        events: resp.data.into_iter().map(event_to_fe).collect(),
        total: resp.total as i32,
    })
}

pub async fn get_event(slug: &str) -> Result<Event, ApiError> {
    let path = format!("/events/{}", url_encode(slug));
    let resp: BeEventWithVariants = get_public(&path).await?;
    Ok(event_with_variants_to_fe(resp))
}

pub async fn get_event_merchant(slug: &str) -> Result<Event, ApiError> {
    let path = format!("/events/{}", url_encode(slug));
    let resp: BeEventWithVariants = get_public(&path).await?;
    Ok(event_with_variants_to_fe(resp))
}

// ── Variant payload types ────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct NewVariant {
    pub name: String,
    pub price: f64,
    pub quota: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price_start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price_end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

#[derive(serde::Serialize, Clone)]
pub struct UpdateVariantPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price_start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sale_price_end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_per_order: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
}

// ── Detail image ─────────────────────────────────────────────────────────────

pub use crate::csr::models::DetailImagePayload;

/// Metadata per foto detail — dikemas sebagai JSON array di field
/// `detail_image_meta` pada multipart. Dicocokkan by-index dengan
/// urutan field `detail_image` (file).
#[derive(serde::Serialize, Clone)]
pub struct DetailImageMeta {
    /// "map" | "seat" | "price" | "other"
    pub image_type: String,
    /// Keterangan singkat, boleh kosong.
    pub caption: String,
}

/// Satu unit siap kirim: file bytes + content-type + metadata.
pub struct DetailImageUploadItem {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub meta: DetailImageMeta,
}

// ── Create ───────────────────────────────────────────────────────────────────

/// POST /events — buat event baru (multipart/form-data).
///
/// Fields multipart:
///   - `data`              (text) : JSON → CreateEventRequest (tanpa detail_images)
///   - `image`             (file) : cover opsional
///   - `detail_image`      (file) : 0..N foto detail (field name sama berulang)
///   - `detail_image_meta` (text) : JSON array [{ image_type, caption }]
pub async fn create_event(
    merchant_name: String,
    name: String,
    description: String,
    category: Vec<String>,
    event_date: String,
    start_time: Option<String>,
    end_time: Option<String>,
    venue: String,
    city: String,
    variants: Vec<NewVariant>,
    // Foto detail: file bytes + mime + metadata (sudah disiapkan dari DetailImageDraft)
    detail_items: Vec<DetailImageUploadItem>,
    image_bytes: Option<Vec<u8>>,
    image_type: Option<String>,
) -> Result<Event, ApiError> {
    use web_sys::js_sys::{Array, Uint8Array};
    use web_sys::{Blob, FormData};

    // Field `data` — JSON tanpa detail_images; BE override dari file upload
    let data_obj = serde_json::json!({
        "merchant_name": merchant_name,
        "name": name,
        "description": description,
        "category": category,
        "event_date": event_date,
        "start_time": start_time,
        "end_time": end_time,
        "venue": venue,
        "city": city,
        "variants": variants,
        // Kirim array kosong — BE akan override dengan hasil upload file
        "detail_images": [],
    });
    let data_str = serde_json::to_string(&data_obj)
        .map_err(|e| ApiError::network(format!("serialize create_event: {e}")))?;

    let form = FormData::new().map_err(|_| ApiError::network("FormData init failed"))?;
    form.append_with_str("data", &data_str)
        .map_err(|_| ApiError::network("append data failed"))?;

    // Cover image
    if let (Some(bytes), Some(_mime)) = (image_bytes, image_type) {
        let arr = Uint8Array::from(bytes.as_slice());
        // (BlobPropertyBag opts removed - using new_with_u8_array_sequence)
        let blob = Blob::new_with_u8_array_sequence(&Array::of1(&arr))
            .map_err(|_| ApiError::network("blob cover create failed"))?;
        form.append_with_blob_and_filename("image", &blob, "cover")
            .map_err(|_| ApiError::network("append cover failed"))?;
    }

    // Detail images — append setiap file sebagai field `detail_image`
    // + satu field `detail_image_meta` berisi JSON array metadata
    if !detail_items.is_empty() {
        let mut metas: Vec<DetailImageMeta> = Vec::with_capacity(detail_items.len());

        for item in detail_items {
            let arr = Uint8Array::from(item.bytes.as_slice());
        // (BlobPropertyBag opts removed - using new_with_u8_array_sequence)
            let blob = Blob::new_with_u8_array_sequence(&Array::of1(&arr))
                .map_err(|_| ApiError::network("blob detail create failed"))?;
            form.append_with_blob_and_filename("detail_image", &blob, "detail")
                .map_err(|_| ApiError::network("append detail_image failed"))?;
            metas.push(item.meta); // move, zero clone
        }

        let meta_str = serde_json::to_string(&metas)
            .map_err(|e| ApiError::network(format!("serialize detail_image_meta: {e}")))?;
        form.append_with_str("detail_image_meta", &meta_str)
            .map_err(|_| ApiError::network("append detail_image_meta failed"))?;
    }

    let be: BeEventWithVariants = post_multipart_private("/events", form).await?;
    Ok(event_with_variants_to_fe(be))
}

// ── Update ───────────────────────────────────────────────────────────────────

/// PUT /events/:id — update event (multipart/form-data).
///
/// Fields multipart:
///   - `data`              (text) : JSON → UpdateEventRequest
///   - `image`             (file) : opsional, ganti cover
///   - `detail_image`      (file) : 0..N foto detail baru
///   - `detail_image_meta` (text) : JSON array [{ image_type, caption }]
///
/// `data["detail_images"]` SELALU dikirim agar BE tahu foto lama mana yang dipertahankan:
///   - Array berisi URL → BE retain URL tsb + merge dengan file baru yang diupload
///   - Array kosong     → BE hapus semua foto lama (user sengaja clear)
///
///
///
// ─── Struct payload (taruh di dekat fungsi atau models) ─────────────────────

#[derive(Serialize)]
struct UpdateEventPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    venue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variants: Option<Vec<UpdateVariantPayload>>,
    // Selalu dikirim (bisa [] untuk hapus semua)
    detail_images: Vec<DetailImagePayload>,
}

// ─── Fungsi yang sudah diperbaiki ───────────────────────────────────────────

pub async fn update_event(
    event_id: &str,
    name: Option<String>,
    description: Option<String>,
    venue: Option<String>,
    city: Option<String>,
    event_date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    category: Option<Vec<String>>,
    variants: Option<Vec<UpdateVariantPayload>>,
    detail_items: Vec<DetailImageUploadItem>,
    retained_detail_images: Option<Vec<DetailImagePayload>>,
    image_file: Option<web_sys::File>,
) -> Result<Event, ApiError> {
    use web_sys::js_sys::{Array, Uint8Array};
    use web_sys::{Blob, FormData};

    let payload = UpdateEventPayload {
        name,
        description,
        venue,
        city,
        event_date,
        start_time,
        end_time,
        category,
        variants,
        detail_images: retained_detail_images.unwrap_or_default(),
    };

    let data_str = serde_json::to_string(&payload)
        .map_err(|e| ApiError::network(format!("serialize update_event: {e}")))?;

    let form = FormData::new().map_err(|_| ApiError::network("FormData init failed"))?;
    form.append_with_str("data", &data_str)
        .map_err(|_| ApiError::network("append data failed"))?;

    // Cover baru
    if let Some(file) = image_file {
        let filename = file.name();
        let blob: &web_sys::Blob = file.as_ref();
        form.append_with_blob_and_filename("image", blob, &filename)
            .map_err(|_| ApiError::network("append cover failed"))?;
    }

    // Detail images baru sebagai file upload
    if !detail_items.is_empty() {
        let mut metas: Vec<DetailImageMeta> = Vec::with_capacity(detail_items.len());

        for item in detail_items {
            let arr = Uint8Array::from(item.bytes.as_slice());
        // (BlobPropertyBag opts removed - using new_with_u8_array_sequence)
            let blob = Blob::new_with_u8_array_sequence(&Array::of1(&arr))
                .map_err(|_| ApiError::network("blob detail create failed"))?;
            form.append_with_blob_and_filename("detail_image", &blob, "detail")
                .map_err(|_| ApiError::network("append detail_image failed"))?;
            metas.push(item.meta); // move, zero clone
        }

        let meta_str = serde_json::to_string(&metas)
            .map_err(|e| ApiError::network(format!("serialize detail_image_meta: {e}")))?;
        form.append_with_str("detail_image_meta", &meta_str)
            .map_err(|_| ApiError::network("append detail_image_meta failed"))?;
    }

    let path = format!("/events/{}", url_encode(event_id));
    let be: BeEventWithVariants = put_multipart_private(&path, form).await?;
    Ok(event_with_variants_to_fe(be))
}

// ── Admin ────────────────────────────────────────────────────────────────────

pub async fn admin_update_event_status(event_id: &str, status: &str) -> Result<Event, ApiError> {
    use crate::csr::services::client::put_private;

    #[derive(serde::Serialize)]
    struct StatusBody<'a> {
        status: &'a str,
    }

    let path = format!("/admin/events/{}/status", url_encode(event_id));
    let be: BeEventWithVariants = put_private(&path, &StatusBody { status }).await?;
    Ok(event_with_variants_to_fe(be))
}

pub async fn admin_list_all_events(
    page: i32,
    per_page: i32,
    status_filter: Option<&str>,
) -> Result<ListEventsResponse, ApiError> {
    let mut params = vec![format!("page={}", page), format!("per_page={}", per_page)];
    if let Some(st) = status_filter {
        params.push(format!("status={}", url_encode(st)));
    }
    let path = format!("/admin/events?{}", params.join("&"));
    let resp: BePaginatedEvents = get_private(&path).await?;
    Ok(ListEventsResponse {
        events: resp.data.into_iter().map(event_to_fe).collect(),
        total: resp.total as i32,
    })
}
