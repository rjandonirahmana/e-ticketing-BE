use anyhow::Result;

use crate::repository::db::{exec_drop, exec_first};
use super::helpers::{
    DELETE_VARIANT, DETACH_VARIANT_FROM_OPEN_CARTS, FIND_VARIANT_BY_ID, UPDATE_VARIANT,
};
use super::PgProductRepository;
use crate::models::product_variants::ProductVariant;
use crate::utils::ulid::id_to_vec;

impl PgProductRepository {
    pub(super) async fn exec_find_variant(&self, id: &str) -> Result<Option<ProductVariant>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_VARIANT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_variant).transpose()
    }

    pub(super) async fn exec_update_variant(
        &self,
        id: &str,
        merchant_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        sale_price: Option<f64>,
        sale_price_start_date: Option<chrono::DateTime<chrono::Utc>>,
        sale_price_end_date: Option<chrono::DateTime<chrono::Utc>>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;

        exec_drop(
            &self.pool,
            UPDATE_VARIANT,
            &[
                &id_vec,
                &merchant_id_vec,
                &name,
                &description,
                &price,
                &sale_price,
                &sale_price_start_date,
                &sale_price_end_date,
                &quota,
                &max_per_order,
                &is_active,
                &sort_order,
            ],
        )
        .await?;
        Ok(())
    }

    pub(super) async fn exec_delete_variant(&self, id: &str, merchant_id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let mid_vec = id_to_vec(merchant_id)?;

        // Lepaskan dulu dari keranjang yang masih terbuka. Foreign key
        // `cart_items` ke varian bersifat RESTRICT karena tabel itu kini juga
        // memuat rincian pesanan yang sudah dibayar — tanpa langkah ini, satu
        // orang yang menaruhnya di keranjang sudah cukup untuk mengunci varian
        // itu selamanya. Yang tersisa menghalangi hanyalah keranjang yang sudah
        // menjadi pesanan, dan itu memang disengaja.
        exec_drop(&self.pool, DETACH_VARIANT_FROM_OPEN_CARTS, &[&id_vec]).await?;

        exec_drop(&self.pool, DELETE_VARIANT, &[&id_vec, &mid_vec]).await?;
        Ok(())
    }
}
