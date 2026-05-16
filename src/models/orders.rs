use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub customer_id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: Decimal,
    pub payment_method: Option<String>,
    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub id: String,
    pub order_id: String,
    pub ticket_variant_id: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OrderItemResponse {
    pub id: String,
    pub ticket_variant_id: String,
    pub variant_name: String,
    pub event_id: String,
    pub event_name: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug, Serialize, Clone)]
pub struct OrderDetailResponse {
    pub id: String,
    pub customer_id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: Decimal,
    pub payment_method: Option<String>,
    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub items: Vec<OrderItemResponse>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateOrderItemRequest {
    pub ticket_variant_id: String,
    #[validate(range(min = 1, max = 100, message = "quantity must be 1..=100"))]
    pub quantity: i32,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderRequest {
    /// Opsional — di-generate client (UUID/ULID).
    /// Request ulang dengan key yang sama mengembalikan order yang sudah ada
    /// tanpa membuat order baru (mencegah double order akibat retry/double-click).
    pub idempotency_key: Option<String>,

    #[validate(length(min = 1, message = "items must not be empty"))]
    #[validate(nested)]
    pub items: Vec<CreateOrderItemRequest>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PayOrderRequest {
    #[validate(length(min = 1, max = 50))]
    pub payment_method: String,
}
