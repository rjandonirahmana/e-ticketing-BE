use std::sync::Arc;
use validator::Validate;

use crate::models::event_variants::{EventVariant, EventVariantResponse, UpdateEventVariantRequest};
use crate::models::events::{
    CreateEventRequest, CreateVariantInline, Event, EventListQuery, EventWithVariants,
    PaginatedEvents, UpdateEventRequest,
};
use crate::repository::event::{EventListFilter, EventRepository};
use crate::utils::error::{AppError, AppResult};

pub struct EventService {
    repo: Arc<dyn EventRepository>,
}

impl EventService {
    pub fn new(repo: Arc<dyn EventRepository>) -> Self {
        Self { repo }
    }

    /// Distinct categories dari semua active event.
    pub async fn list_categories(&self) -> AppResult<Vec<String>> {
        Ok(self.repo.list_categories().await?)
    }

    // ── List ──────────────────────────────────────────────────────────────────

    pub async fn list(
        &self,
        q: EventListQuery,
        merchant_id: Option<&str>,
    ) -> AppResult<PaginatedEvents> {
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
        let offset = (page - 1) * per_page;

        let filter = EventListFilter {
            city: q.city.as_deref(),
            status: q.status.as_deref(),
            merchant_id,
            category: q.category.as_deref(),
            search: q.search.as_deref(),
            limit: per_page,
            offset,
        };

        let (data, total) = tokio::try_join!(self.repo.list(&filter), self.repo.count(&filter))?;
        Ok(PaginatedEvents {
            total_pages: (total + per_page - 1) / per_page,
            data,
            total,
            page,
            per_page,
        })
    }

    // ── Get by slug ───────────────────────────────────────────────────────────

    pub async fn get(&self, slug: &str) -> AppResult<EventWithVariants> {
        let (event, variants) = self
            .repo
            .find_by_slug_with_variants(slug)
            .await?
            .ok_or_else(|| AppError::NotFound("Event not found".into()))?;
        Ok(self.to_with_variants(event, variants))
    }

    // ── Get by id (dipakai setelah update) ────────────────────────────────────

    async fn get_by_id(&self, id: &str) -> AppResult<EventWithVariants> {
        let (event, variants) = self
            .repo
            .find_by_id_with_variants(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Event not found".into()))?;
        Ok(self.to_with_variants(event, variants))
    }

    // ── Create ────────────────────────────────────────────────────────────────

    pub async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: CreateEventRequest,
        cover_url: Option<&str>,
    ) -> AppResult<EventWithVariants> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        for v in &req.variants {
            v.validate()
                .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        }

        let event = self
            .repo
            .create(merchant_id, merchant_name, &req, cover_url)
            .await?;
        let variants = self
            .repo
            .create_variants_bulk(&event.id, &req.variants)
            .await?;

        // Refresh event dari DB agar price/display_price reflect variant baru
        self.get_by_id(&event.id).await
            .or_else(|_| Ok(self.to_with_variants(event, variants)))
    }

    // ── Update ────────────────────────────────────────────────────────────────

    pub async fn update(
        &self,
        id: &str,
        merchant_id: &str,
        req: UpdateEventRequest,
    ) -> AppResult<EventWithVariants> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        for v in req.variants.iter().flatten() {
            v.validate()
                .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        }

        // Update event fields
        self.repo.update(id, merchant_id, &req).await?;

        // Update/tambah variants jika dikirim
        if let Some(variants) = &req.variants {
            for v in variants {
                if let Some(vid) = &v.id {
                    self.repo
                        .update_variant(
                            vid,
                            merchant_id,
                            v.name.as_deref(),
                            v.description.as_deref(),
                            v.price,
                            v.sale_price,
                            v.sale_price_start_date,
                            v.sale_price_end_date,
                            v.quota,
                            v.max_per_order,
                            v.is_active,
                            v.sort_order,
                        )
                        .await?;
                } else {
                    // Tidak ada id → tambah variant baru
                    let inline = CreateVariantInline {
                        name: v.name.clone().unwrap_or_default(),
                        description: v.description.clone(),
                        price: v.price.unwrap_or(0.0),
                        sale_price: v.sale_price,
                        sale_price_start_date: v.sale_price_start_date,
                        sale_price_end_date: v.sale_price_end_date,
                        quota: v.quota.unwrap_or(0),
                        max_per_order: v.max_per_order,
                        sort_order: v.sort_order,
                    };
                    self.repo.create_variants_bulk(id, &[inline]).await?;
                }
            }
        }

        // BUG FIX: get_by_id bukan get(id) — get() menerima slug, bukan id
        self.get_by_id(id).await
    }

    // ── Variant ops (individual) ─────────────────────────────────────────────

    pub async fn update_variant(
        &self,
        variant_id: &str,
        merchant_id: &str,
        req: UpdateEventVariantRequest,
    ) -> AppResult<EventVariantResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        self.repo
            .update_variant(
                variant_id,
                merchant_id,
                req.name.as_deref(),
                req.description.as_deref(),
                req.price,
                req.sale_price,
                req.sale_price_start_date,
                req.sale_price_end_date,
                req.quota,
                req.max_per_order,
                req.is_active,
                req.sort_order,
            )
            .await?;
        Ok(self
            .repo
            .find_variant(variant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Variant not found".into()))?
            .into())
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn to_with_variants(&self, event: Event, variants: Vec<EventVariant>) -> EventWithVariants {
        EventWithVariants {
            category: event.category,
            id: event.id,
            merchant_id: event.merchant_id,
            name: event.name,
            slug: event.slug,
            description: event.description,
            cover_url: event.cover_url,
            venue: event.venue,
            city: event.city,
            event_date: event.event_date,
            start_time: event.start_time,
            end_time: event.end_time,
            status: event.status,
            created_at: event.created_at,
            price: event.price,
            sale_price: event.sale_price,
            sale_price_start_date: event.sale_price_start_date,
            sale_price_end_date: event.sale_price_end_date,
            display_price: event.display_price,
            total_sold: event.total_sold,
            total_quota: event.total_quota,
            event_variants: variants.into_iter().map(Into::into).collect(),
        }
    }
}
