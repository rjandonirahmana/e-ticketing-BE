use std::sync::Arc;
use validator::Validate;

use crate::models::orders::{CreateOrderRequest, Order, OrderDetailResponse, PayOrderRequest};
use crate::repository::order::{CreateOrderItemSpec, OrderRepository};
use crate::utils::error::{AppError, AppResult};

pub struct OrderService {
    repo: Arc<dyn OrderRepository>,
}

impl OrderService {
    pub fn new(repo: Arc<dyn OrderRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        customer_id: &str,
        req: CreateOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let specs: Vec<CreateOrderItemSpec> = req
            .items
            .iter()
            .map(|i| CreateOrderItemSpec {
                variant_id: i.ticket_variant_id.as_str(),
                quantity: i.quantity,
            })
            .collect();

        let order = self
            .repo
            .create_order(customer_id, &specs)
            .await
            // Stock / validation errors come back as anyhow::Error and we want
            // to surface them as 4xx instead of 500.
            .map_err(map_business_err)?;

        self.detail(&order.id, customer_id).await
    }

    pub async fn detail(&self, order_id: &str, viewer_id: &str) -> AppResult<OrderDetailResponse> {
        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;

        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }

        let items = self.repo.list_items(order_id).await?;
        Ok(OrderDetailResponse {
            id: order.id,
            customer_id: order.customer_id,
            order_code: order.order_code,
            status: order.status,
            total_amount: order.total_amount,
            payment_method: order.payment_method,
            paid_at: order.paid_at,
            expired_at: order.expired_at,
            created_at: order.created_at,
            items,
        })
    }

    pub async fn list_mine(
        &self,
        customer_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<Order>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = (page - 1) * per_page;
        Ok(self
            .repo
            .list_for_customer(customer_id, per_page, offset)
            .await?)
    }

    pub async fn pay(
        &self,
        order_id: &str,
        viewer_id: &str,
        req: PayOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        // ownership check before mutating
        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;
        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }

        self.repo
            .mark_paid_and_issue_tickets(order_id, &req.payment_method)
            .await
            .map_err(map_business_err)?;

        self.detail(order_id, viewer_id).await
    }

    pub async fn cancel(&self, order_id: &str, viewer_id: &str) -> AppResult<()> {
        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;
        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }

        self.repo.cancel(order_id).await.map_err(map_business_err)?;
        Ok(())
    }
}

/// Treat repository errors that come from business validation (insufficient
/// stock, double-pay, etc.) as 400 Bad Request instead of opaque 500s.
fn map_business_err(e: anyhow::Error) -> AppError {
    let msg = e.to_string();
    if msg.contains("Insufficient stock")
        || msg.contains("not active")
        || msg.contains("at most")
        || msg.contains("not in pending")
        || msg.contains("not pending")
        || msg.contains("at least one item")
    {
        AppError::BadRequest(msg)
    } else if msg.contains("not found") || msg.contains("Variant '") {
        AppError::NotFound(msg)
    } else {
        AppError::Internal(e)
    }
}
