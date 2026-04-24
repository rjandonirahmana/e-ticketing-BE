use std::sync::Arc;
use validator::Validate;

use crate::models::event_variant::{
    CreateTicketVariantRequest, TicketVariantResponse, UpdateTicketVariantRequest,
};
use crate::models::events::{
    CreateEventRequest, Event, EventListQuery, EventWithVariants, PaginatedEvents,
    UpdateEventRequest,
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
            limit: per_page,
            offset,
        };

        let (data, total) = tokio::try_join!(self.repo.list(&filter), self.repo.count(&filter))?;
        let total_pages = (total + per_page - 1) / per_page;

        Ok(PaginatedEvents {
            data,
            total,
            page,
            per_page,
            total_pages,
        })
    }

    pub async fn get_with_variants(&self, id: &str) -> AppResult<EventWithVariants> {
        let event = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Event not found".into()))?;
        let variants = self.repo.list_variants(id).await?;
        Ok(EventWithVariants {
            id: event.id,
            merchant_id: event.merchant_id,
            name: event.name,
            description: event.description,
            venue: event.venue,
            city: event.city,
            event_date: event.event_date,
            start_time: event.start_time,
            end_time: event.end_time,
            status: event.status,
            created_at: event.created_at,
            ticket_variants: variants.into_iter().map(Into::into).collect(),
        })
    }

    pub async fn create(&self, merchant_id: &str, req: CreateEventRequest) -> AppResult<Event> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        Ok(self.repo.create(merchant_id, &req).await?)
    }

    pub async fn update(
        &self,
        id: &str,
        merchant_id: &str,
        req: UpdateEventRequest,
    ) -> AppResult<Event> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let existing = self.ensure_owner(id, merchant_id).await?;
        self.repo.update(id, &req).await?;
        Ok(self.repo.find_by_id(id).await?.unwrap_or(existing))
    }

    pub async fn delete(&self, id: &str, merchant_id: &str) -> AppResult<()> {
        self.ensure_owner(id, merchant_id).await?;
        self.repo.delete(id).await?;
        Ok(())
    }

    // ── Variants ────────────────────────────────────────────────────────────

    pub async fn create_variant(
        &self,
        event_id: &str,
        merchant_id: &str,
        req: CreateTicketVariantRequest,
    ) -> AppResult<TicketVariantResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        self.ensure_owner(event_id, merchant_id).await?;

        let v = self
            .repo
            .create_variant(
                event_id,
                &req.name,
                req.description.as_deref(),
                req.price,
                req.quota,
                req.max_per_order,
                req.sort_order.unwrap_or(0),
            )
            .await?;
        Ok(v.into())
    }

    pub async fn update_variant(
        &self,
        variant_id: &str,
        merchant_id: &str,
        req: UpdateTicketVariantRequest,
    ) -> AppResult<TicketVariantResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let variant = self
            .repo
            .find_variant(variant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Variant not found".into()))?;
        self.ensure_owner(&variant.event_id, merchant_id).await?;

        self.repo
            .update_variant(
                variant_id,
                req.name.as_deref(),
                req.description.as_deref(),
                req.price,
                req.quota,
                req.max_per_order,
                req.is_active,
                req.sort_order,
            )
            .await?;
        let v = self
            .repo
            .find_variant(variant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Variant not found".into()))?;
        Ok(v.into())
    }

    pub async fn delete_variant(&self, variant_id: &str, merchant_id: &str) -> AppResult<()> {
        let variant = self
            .repo
            .find_variant(variant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Variant not found".into()))?;
        self.ensure_owner(&variant.event_id, merchant_id).await?;
        self.repo.delete_variant(variant_id).await?;
        Ok(())
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    async fn ensure_owner(&self, event_id: &str, merchant_id: &str) -> AppResult<Event> {
        let event = self
            .repo
            .find_by_id(event_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Event not found".into()))?;
        if event.merchant_id != merchant_id {
            return Err(AppError::Forbidden("You do not own this event".into()));
        }
        Ok(event)
    }
}
