mod admin;
mod helpers;
mod read;
mod variants;
mod write;

use helpers::escape_like;

use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use tokio_postgres::Row;

use crate::models::product_variants::{cmp_by_effective_price, ProductVariant, ProductVariantJson};
use crate::models::products::{CreateProductRequest, CreateVariantInline, Product, UpdateProductRequest};
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

// ── Filter ────────────────────────────────────────────────────────────────────

pub struct ProductListFilter<'a> {
    pub city: Option<&'a str>,
    pub status: Option<&'a str>,
    pub category: Option<&'a str>,
    pub search: Option<&'a str>,
    pub merchant_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

/// Owned copies of filter values, needed to keep borrows alive while building
/// the params Vec for tokio-postgres.
struct ProductFilterOwned {
    mid_vec: Option<Vec<u8>>,
    city_pat: Option<String>,
    status_own: Option<String>,
    cat_json: Option<serde_json::Value>,
    search_pat: Option<String>,
}

impl ProductFilterOwned {
    fn from_filter(f: &ProductListFilter<'_>) -> Result<Self> {
        Ok(Self {
            mid_vec: f.merchant_id.map(id_to_vec).transpose()?,
            city_pat: f.city.map(|c| format!("%{}%", escape_like(c))),
            status_own: f.status.map(|s| s.to_string()),
            cat_json: f.category.map(|c| serde_json::to_value(vec![c])).transpose()?,
            search_pat: f.search.map(|q| format!("%{}%", escape_like(q))),
        })
    }

    fn push_where<'a>(
        &'a self,
        sql: &mut String,
        refs: &mut Vec<&'a (dyn tokio_postgres::types::ToSql + Sync)>,
        idx: &mut usize,
        alias: &str,
        with_merchant: bool,
    ) {
        if with_merchant {
            if let Some(ref v) = self.mid_vec {
                sql.push_str(&format!(" AND {alias}merchant_id = ${idx}"));
                refs.push(v);
                *idx += 1;
            }
        }
        if let Some(ref v) = self.city_pat {
            sql.push_str(&format!(" AND {alias}city ILIKE ${idx}"));
            refs.push(v);
            *idx += 1;
        }
        if let Some(ref v) = self.status_own {
            sql.push_str(&format!(" AND {alias}status = ${idx}"));
            refs.push(v);
            *idx += 1;
        }
        if let Some(ref v) = self.cat_json {
            sql.push_str(&format!(" AND {alias}category @> ${idx}::jsonb"));
            refs.push(v);
            *idx += 1;
        }
        if let Some(ref v) = self.search_pat {
            sql.push_str(&format!(
                " AND ({alias}name ILIKE ${idx} OR {alias}venue ILIKE ${idx} OR {alias}city ILIKE ${idx})"
            ));
            refs.push(v);
            *idx += 1;
        }
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait ProductRepository: Send + Sync {
    async fn list(&self, f: &ProductListFilter<'_>) -> Result<Vec<Product>>;
    async fn count(&self, f: &ProductListFilter<'_>) -> Result<i64>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Product>>;
    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>>;
    async fn list_categories(&self) -> Result<Vec<String>>;
    async fn find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>>;
    async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        cover_url: Option<&str>,
    ) -> Result<Product>;
    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<ProductVariant>>;
    async fn create_with_variants(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        variants: &[CreateVariantInline],
        cover_url: Option<&str>,
    ) -> Result<(Product, Vec<ProductVariant>)>;
    async fn update(&self, id: &str, merchant_id: &str, req: &UpdateProductRequest) -> Result<()>;
    async fn delete(&self, id: &str, merchant_id: &str) -> Result<()>;
    async fn find_variant(&self, id: &str) -> Result<Option<ProductVariant>>;
    async fn update_variant(
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
    ) -> Result<()>;
    async fn delete_variant(&self, id: &str, merchant_id: &str) -> Result<()>;
    async fn admin_list_by_status(&self, f: &ProductListFilter<'_>) -> Result<Vec<Product>>;
    async fn admin_count_by_status(&self, f: &ProductListFilter<'_>) -> Result<i64>;
    async fn admin_update_status(&self, id: &str, status: &str) -> Result<()>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgProductRepository {
    pool: Pool,
}

impl PgProductRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_product(row: &Row) -> Result<Product> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        let category_json: Option<serde_json::Value> = row.try_get("category")?;
        let category = match category_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        let detail_images_json: Option<serde_json::Value> =
            row.try_get("detail_images").ok().flatten();
        let detail_images = match detail_images_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Product {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            detail_images,
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get::<_, Option<String>>("cover_url")?,
            // Baris lama (sebelum migrasi 020) bisa saja belum punya kolomnya
            // saat aplikasi baru di-deploy mendahului migrasinya — jatuh ke
            // tengah alih-alih menggagalkan seluruh pembacaan product.
            cover_focus: row
                .try_get::<_, Option<String>>("cover_focus")
                .ok()
                .flatten()
                .unwrap_or_else(crate::models::products::fokus_tengah),
            price: row.try_get::<_, f64>("price").unwrap_or(0.0),
            sale_price: row.try_get("sale_price").ok().flatten(),
            sale_price_start_date: row.try_get("sale_price_start_date").ok().flatten(),
            sale_price_end_date: row.try_get("sale_price_end_date").ok().flatten(),
            display_price: row.try_get::<_, f64>("display_price").unwrap_or(0.0),
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            latitude: row.try_get("latitude").ok().flatten(),
            longitude: row.try_get("longitude").ok().flatten(),
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
            total_sold: row.try_get("total_sold").unwrap_or(0),
            total_quota: row.try_get("total_quota").unwrap_or(0),
            // Toleran: kolom hanya ada pada query ber-MERCHANT_JOIN.
            merchant_name: row.try_get("merchant_name").ok().flatten(),
            // List tidak menyertakan MERCHANT_INFO_COLS (mahal per baris).
            merchant: None,
        })
    }

    fn row_to_product_no_agg(row: &Row) -> Result<Product> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        let category_json: Option<serde_json::Value> = row.try_get("category")?;
        let category = match category_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        let detail_images_json: Option<serde_json::Value> =
            row.try_get("detail_images").ok().flatten();
        let detail_images = match detail_images_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Product {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            detail_images,
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get::<_, Option<String>>("cover_url")?,
            // Baris lama (sebelum migrasi 020) bisa saja belum punya kolomnya
            // saat aplikasi baru di-deploy mendahului migrasinya — jatuh ke
            // tengah alih-alih menggagalkan seluruh pembacaan product.
            cover_focus: row
                .try_get::<_, Option<String>>("cover_focus")
                .ok()
                .flatten()
                .unwrap_or_else(crate::models::products::fokus_tengah),
            price: 0.0,
            sale_price: None,
            sale_price_start_date: None,
            sale_price_end_date: None,
            display_price: 0.0,
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            latitude: row.try_get("latitude").ok().flatten(),
            longitude: row.try_get("longitude").ok().flatten(),
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
            total_sold: 0,
            total_quota: 0,
            // Toleran: None pada INSERT RETURNING (tanpa join) — tak dipakai di
            // jalur itu (dashboard merchant sendiri).
            merchant_name: row.try_get("merchant_name").ok().flatten(),
            merchant: Self::row_to_merchant_summary(row),
        })
    }

    /// Ringkasan merchant dari MERCHANT_INFO_COLS — toleran: None bila kolom
    /// tak ada di query (INSERT RETURNING) sehingga mapper tetap dipakai semua
    /// jalur. `merchant_verified` dijadikan kolom penanda keberadaan blok.
    fn row_to_merchant_summary(row: &Row) -> Option<crate::models::products::MerchantSummary> {
        let verified: bool = row.try_get("merchant_verified").ok()?;
        Some(crate::models::products::MerchantSummary {
            logo_url: row.try_get("merchant_logo").ok().flatten(),
            header_url: row.try_get("merchant_header").ok().flatten(),
            description: row.try_get("merchant_desc").ok().flatten(),
            verified,
            followers: row.try_get("merchant_followers").unwrap_or(0),
            products_count: row.try_get("merchant_products_count").unwrap_or(0),
            rating_avg: row.try_get("merchant_rating_avg").unwrap_or(0.0),
            rating_count: row.try_get("merchant_rating_count").unwrap_or(0),
        })
    }

    fn row_to_variant(row: &Row) -> Result<ProductVariant> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let product_bytes: Vec<u8> = row.try_get("event_id").context("event_id")?;
        Ok(ProductVariant {
            id: bin_to_ulid(id_bytes)?,
            event_id: bin_to_ulid(product_bytes)?,
            name: row.try_get("name").context("name")?,
            description: row.try_get("description").context("description")?,
            price: row.try_get("price").context("price")?,
            sale_price: row
                .try_get::<_, Option<f64>>("sale_price")
                .context("sale_price")?,
            sale_price_start_date: row.try_get("sale_price_start_date")?,
            sale_price_end_date: row.try_get("sale_price_end_date")?,
            quota: row.try_get("quota").context("quota")?,
            sold: row.try_get("sold").context("sold")?,
            max_per_order: row.try_get("max_per_order")?,
            is_active: row.try_get::<_, Option<bool>>("is_active")?.unwrap_or(true),
            sort_order: row.try_get::<_, Option<i32>>("sort_order")?.unwrap_or(0),
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }

    fn apply_cheapest_variant_price(product: &mut Product, variants: &[ProductVariant]) {
        if let Some(cheapest) = variants.iter().min_by(|a, b| cmp_by_effective_price(a, b)) {
            product.price = cheapest.price;
            product.sale_price = cheapest.sale_price;
            product.sale_price_start_date = cheapest.sale_price_start_date;
            product.sale_price_end_date = cheapest.sale_price_end_date;
            product.display_price = cheapest.effective_price();
        }
        product.total_sold = variants.iter().map(|v| v.sold).sum();
        product.total_quota = variants.iter().map(|v| v.quota).sum();
    }

    fn parse_variants_json(row: &Row) -> Result<Vec<ProductVariant>> {
        let variants_json: serde_json::Value = row.try_get("variants_json")?;
        serde_json::from_value::<Vec<ProductVariantJson>>(variants_json)
            .context("deserialize variants_json")?
            .into_iter()
            .map(ProductVariantJson::into_variant)
            .collect::<Result<_>>()
    }
}

// ── Trait impl — delegates to sub-module inherent methods ─────────────────────

#[async_trait]
impl ProductRepository for PgProductRepository {
    async fn list(&self, f: &ProductListFilter<'_>) -> Result<Vec<Product>> {
        self.exec_list(f).await
    }
    async fn count(&self, f: &ProductListFilter<'_>) -> Result<i64> {
        self.exec_count(f).await
    }
    async fn list_categories(&self) -> Result<Vec<String>> {
        self.exec_list_categories().await
    }
    async fn find_by_id(&self, id: &str) -> Result<Option<Product>> {
        self.exec_find_by_id(id).await
    }
    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>> {
        self.exec_find_by_id_with_variants(id).await
    }
    async fn find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Product, Vec<ProductVariant>)>> {
        self.exec_find_by_slug_with_variants(slug).await
    }
    async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        cover_url: Option<&str>,
    ) -> Result<Product> {
        self.exec_create(merchant_id, merchant_name, req, cover_url).await
    }
    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<ProductVariant>> {
        self.exec_create_variants_bulk(event_id, variants).await
    }
    async fn create_with_variants(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        variants: &[CreateVariantInline],
        cover_url: Option<&str>,
    ) -> Result<(Product, Vec<ProductVariant>)> {
        self.exec_create_with_variants(merchant_id, merchant_name, req, variants, cover_url)
            .await
    }
    async fn update(&self, id: &str, merchant_id: &str, req: &UpdateProductRequest) -> Result<()> {
        self.exec_update(id, merchant_id, req).await
    }
    async fn delete(&self, id: &str, merchant_id: &str) -> Result<()> {
        self.exec_delete(id, merchant_id).await
    }
    async fn find_variant(&self, id: &str) -> Result<Option<ProductVariant>> {
        self.exec_find_variant(id).await
    }
    async fn update_variant(
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
        self.exec_update_variant(
            id,
            merchant_id,
            name,
            description,
            price,
            sale_price,
            sale_price_start_date,
            sale_price_end_date,
            quota,
            max_per_order,
            is_active,
            sort_order,
        )
        .await
    }
    async fn delete_variant(&self, id: &str, merchant_id: &str) -> Result<()> {
        self.exec_delete_variant(id, merchant_id).await
    }
    async fn admin_list_by_status(&self, f: &ProductListFilter<'_>) -> Result<Vec<Product>> {
        self.exec_admin_list_by_status(f).await
    }
    async fn admin_count_by_status(&self, f: &ProductListFilter<'_>) -> Result<i64> {
        self.exec_admin_count_by_status(f).await
    }
    async fn admin_update_status(&self, id: &str, status: &str) -> Result<()> {
        self.exec_admin_update_status(id, status).await
    }
}
