use std::sync::Arc;
use validator::Validate;

use crate::models::product_variants::{
    ProductVariant, ProductVariantResponse, UpdateProductVariantRequest,
};
use crate::models::products::{
    CreateProductRequest, CreateVariantInline, Product, ProductListQuery, ProductWithVariants,
    PaginatedProducts, UpdateProductRequest,
};
use crate::repository::product::{ProductListFilter, ProductRepository};
use crate::utils::error::{AppError, AppResult};

pub struct ProductService {
    repo: Arc<dyn ProductRepository>,
}

impl ProductService {
    pub fn new(repo: Arc<dyn ProductRepository>) -> Self {
        Self { repo }
    }

    /// Distinct categories dari semua active product.
    pub async fn list_categories(&self) -> AppResult<Vec<String>> {
        Ok(self.repo.list_categories().await?)
    }

    // ── List ──────────────────────────────────────────────────────────────────

    pub async fn list(
        &self,
        q: ProductListQuery,
        merchant_id: Option<&str>,
    ) -> AppResult<PaginatedProducts> {
        let page = q.page.unwrap_or(1).max(1);
        let per_page = q.per_page.unwrap_or(20).clamp(1, 100);
        let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);

        let filter = ProductListFilter {
            city: q.city.as_deref(),
            status: q.status.as_deref(),
            merchant_id,
            category: q.category.as_deref(),
            search: q.search.as_deref(),
            limit: per_page,
            offset,
        };

        let (data, total) = tokio::try_join!(self.repo.list(&filter), self.repo.count(&filter))?;
        Ok(PaginatedProducts {
            total_pages: (total + per_page - 1) / per_page,
            data,
            total,
            page,
            per_page,
        })
    }

    // ── Get by slug ───────────────────────────────────────────────────────────

    pub async fn get(&self, slug: &str) -> AppResult<ProductWithVariants> {
        let (product, variants) = self
            .repo
            .find_by_slug_with_variants(slug)
            .await?
            .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
        Ok(self.to_with_variants(product, variants))
    }

    /// Detail product untuk PEMILIKNYA (atau admin).
    ///
    /// Jalur merchant sebelumnya memanggil `get()` polos, yang hanya mencari
    /// berdasarkan slug — artinya merchant A bisa membaca product merchant B
    /// cukup dengan menebak/menyalin slug-nya, termasuk product yang belum
    /// terbit. Login saja bukan otorisasi.
    ///
    /// Balasan untuk product milik orang lain adalah **NotFound**, bukan
    /// Forbidden. Forbidden akan memberi tahu bahwa slug itu ADA dan dimiliki
    /// orang lain — cukup untuk memetakan katalog pesaing sedikit demi sedikit.
    /// NotFound tak membocorkan apa pun.
    pub async fn get_for_merchant(
        &self,
        slug: &str,
        merchant_id: &str,
        is_admin: bool,
    ) -> AppResult<ProductWithVariants> {
        let product = self.get(slug).await?;
        if !is_admin && product.merchant_id != merchant_id {
            return Err(AppError::NotFound("Product not found".into()));
        }
        Ok(product)
    }

    // ── Get by id (dipakai setelah update) ────────────────────────────────────

    async fn get_by_id(&self, id: &str) -> AppResult<ProductWithVariants> {
        let (product, variants) = self
            .repo
            .find_by_id_with_variants(id)
            .await?
            .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
        Ok(self.to_with_variants(product, variants))
    }

    // ── Create ────────────────────────────────────────────────────────────────

    pub async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: CreateProductRequest,
        cover_url: Option<&str>,
    ) -> AppResult<ProductWithVariants> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        for v in &req.variants {
            v.validate()
                .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        }

        let product = self
            .repo
            .create(merchant_id, merchant_name, &req, cover_url)
            .await?;
        let variants = self
            .repo
            .create_variants_bulk(&product.id, &req.variants)
            .await?;

        // Refresh product dari DB agar price/display_price reflect variant baru
        self.get_by_id(&product.id)
            .await
            .or_else(|_| Ok(self.to_with_variants(product, variants)))
    }

    // ── Update ────────────────────────────────────────────────────────────────

    pub async fn update(
        &self,
        id: &str,
        merchant_id: &str,
        req: UpdateProductRequest,
    ) -> AppResult<ProductWithVariants> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        for v in req.variants.iter().flatten() {
            v.validate()
                .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        }

        // Update product fields
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
        req: UpdateProductVariantRequest,
    ) -> AppResult<ProductVariantResponse> {
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

    fn to_with_variants(&self, product: Product, variants: Vec<ProductVariant>) -> ProductWithVariants {
        ProductWithVariants {
            category: product.category,
            detail_images: product.detail_images,
            id: product.id,
            merchant_id: product.merchant_id,
            name: product.name,
            slug: product.slug,
            description: product.description,
            cover_url: product.cover_url,
            cover_focus: product.cover_focus,
            venue: product.venue,
            city: product.city,
            latitude: product.latitude,
            longitude: product.longitude,
            event_date: product.event_date,
            start_time: product.start_time,
            end_time: product.end_time,
            status: product.status,
            created_at: product.created_at,
            price: product.price,
            sale_price: product.sale_price,
            sale_price_start_date: product.sale_price_start_date,
            sale_price_end_date: product.sale_price_end_date,
            display_price: product.display_price,
            total_sold: product.total_sold,
            total_quota: product.total_quota,
            merchant_name: product.merchant_name,
            merchant: product.merchant,
            product_variants: variants.into_iter().map(Into::into).collect(),
        }
    }

    /// Admin: list product dengan status "edited" (menunggu review), dengan paginasi yang benar.
    pub async fn list_cancelled_products(
        &self,
        page: i64,
        per_page: i64,
        search: Option<&str>,
    ) -> AppResult<PaginatedProducts> {
        let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);
        let filter = ProductListFilter {
            search,
            city: None,
            status: Some("edited"),
            category: None,
            merchant_id: None,
            limit: per_page,
            offset,
        };

        let (data, total) = tokio::try_join!(
            self.repo.admin_list_by_status(&filter),
            self.repo.admin_count_by_status(&filter)
        )?;

        Ok(PaginatedProducts {
            total_pages: (total + per_page - 1) / per_page,
            data,
            total,
            page,
            per_page,
        })
    }

    /// Admin-only: update status product.
    /// Status valid: "active" | "cancelled" | "completed" | "edited"
    pub async fn admin_update_status(
        &self,
        event_id: &str,
        status: &str,
    ) -> AppResult<ProductWithVariants> {
        let allowed = ["active", "cancelled", "completed", "edited"];
        if !allowed.contains(&status) {
            return Err(AppError::UnprocessableEntity(format!(
                "Status tidak valid: '{}'. Pilihan: {}",
                status,
                allowed.join(", ")
            )));
        }
        self.repo.admin_update_status(event_id, status).await?;
        self.get_by_id(event_id).await
    }
}
