use std::sync::Arc;
use validator::Validate;

use crate::models::event_variants::{EventVariantResponse, UpdateEventVariantRequest};
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

    // ── Get by slug — satu JOIN query ────────────────────────────────────────

    pub async fn get(&self, slug: &str) -> AppResult<EventWithVariants> {
        let (event, variants) = self
            .repo
            .find_by_slug_with_variants(slug)
            .await?
            .ok_or_else(|| AppError::NotFound("Event not found".into()))?;
        Ok(self.to_with_variants(event, variants))
    }

    // ── Create — event + variants + cover_url satu call ───────────────────────

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
        Ok(self.to_with_variants(event, variants))
    }

    // ── Update — event fields + variants sekaligus ───────────────────────────

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

        // Update variants jika ada (opsional — FE bisa kirim partial)
        if let Some(variants) = &req.variants {
            for v in variants {
                if let Some(vid) = &v.id {
                    // Update existing variant
                    self.repo
                        .update_variant(
                            vid,
                            merchant_id,
                            v.name.as_deref(),
                            v.description.as_deref(),
                            v.price,
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
                        quota: v.quota.unwrap_or(0),
                        max_per_order: v.max_per_order,
                        sort_order: v.sort_order,
                    };
                    self.repo.create_variants_bulk(id, &[inline]).await?;
                }
            }
        }

        self.get(id).await
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

    fn to_with_variants(
        &self,
        event: Event,
        variants: Vec<crate::models::event_variants::EventVariant>,
    ) -> EventWithVariants {
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
            event_variants: variants.into_iter().map(Into::into).collect(),
        }
    }
}
