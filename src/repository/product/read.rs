use anyhow::Result;

use crate::repository::db::{exec_first, exec_one, exec_rows};
use super::helpers::{EVENT_COLS, FIND_EVENT_BY_ID, FIND_EVENT_WITH_VARIANTS_BY_ID,
    FIND_EVENT_WITH_VARIANTS_BY_SLUG, VARIANT_STATS_LATERAL};
use super::{ProductFilterOwned, ProductListFilter, PgProductRepository};
use crate::models::products::Product;
use crate::models::product_variants::ProductVariant;
use crate::utils::ulid::id_to_vec;

impl PgProductRepository {
    pub(super) async fn exec_list(&self, f: &ProductListFilter<'_>) -> Result<Vec<Product>> {
        let owned = ProductFilterOwned::from_filter(f)?;
        let mut sql = format!(
            "SELECT {cols} FROM products e {lateral} {mjoin} WHERE 1 = 1",
            cols = EVENT_COLS,
            lateral = VARIANT_STATS_LATERAL,
            mjoin = super::helpers::MERCHANT_JOIN,
        );
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(7);
        let mut idx = 1usize;
        owned.push_where(&mut sql, &mut refs, &mut idx, "e.", true);
        sql.push_str(&format!(
            " ORDER BY e.event_date ASC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        refs.push(&f.limit);
        refs.push(&f.offset);

        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_product).collect()
    }

    pub(super) async fn exec_count(&self, f: &ProductListFilter<'_>) -> Result<i64> {
        let owned = ProductFilterOwned::from_filter(f)?;
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM products WHERE 1 = 1");
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(5);
        let mut idx = 1usize;
        owned.push_where(&mut sql, &mut refs, &mut idx, "", true);

        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    pub(super) async fn exec_list_categories(&self) -> Result<Vec<String>> {
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT DISTINCT jsonb_array_elements_text(category) AS cat
            FROM products
            WHERE status = 'active'
              AND category IS NOT NULL
              AND jsonb_array_length(category) > 0
            ORDER BY cat ASC
            "#,
            &[],
        )
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<_, String>("cat").ok())
            .filter(|s| !s.is_empty())
            .collect())
    }

    pub(super) async fn exec_find_by_id(&self, id: &str) -> Result<Option<Product>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_product).transpose()
    }

    pub(super) async fn exec_find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_WITH_VARIANTS_BY_ID, &[&id_vec]).await?;
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        let mut product = Self::row_to_product_no_agg(&row)?;
        let variants = Self::parse_variants_json(&row)?;
        Self::apply_cheapest_variant_price(&mut product, &variants);
        Ok(Some((product, variants)))
    }

    pub(super) async fn exec_find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>> {
        let row =
            exec_first(&self.pool, &FIND_EVENT_WITH_VARIANTS_BY_SLUG, &[&slug]).await?;
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        let mut product = Self::row_to_product_no_agg(&row)?;
        let variants = Self::parse_variants_json(&row)?;
        Self::apply_cheapest_variant_price(&mut product, &variants);
        Ok(Some((product, variants)))
    }
}
