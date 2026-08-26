use anyhow::{Context, Result};

use crate::repository::db::{exec_drop, exec_rows, get_conn};
use super::helpers::{
    generate_slug, is_unique_violation, DELETE_EVENT, INSERT_EVENT, UPDATE_EVENT, VARIANT_COLS,
    VARIANT_INSERT_COLS,
};
use super::{PgProductRepository};
use crate::models::products::{
    CreateProductRequest, CreateVariantInline, Product, UpdateProductRequest, STATUS_MENUNGGU_REVIEW,
};
use crate::models::product_variants::ProductVariant;
use crate::utils::ulid::{id_to_vec, new_ulid, ulid_to_vec};

impl PgProductRepository {
    pub(super) async fn exec_create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        cover_url: Option<&str>,
    ) -> Result<Product> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        let category_json = serde_json::to_value(&req.category)?;
        let detail_images_json = serde_json::to_value(&req.detail_images)?;

        let mut last_err = anyhow::anyhow!("slug generation failed after max retries");
        for _ in 0..5u8 {
            let slug = generate_slug(merchant_name, &req.name);
            let result = exec_rows(
                &self.pool,
                INSERT_EVENT,
                &[
                    &id_vec,
                    &mid_vec,
                    &req.name,
                    &slug,
                    &req.description,
                    &cover_url,
                    &0f64,
                    &req.venue,
                    &req.city,
                    &req.event_date,
                    &req.start_time,
                    &req.end_time,
                    &category_json,
                    &STATUS_MENUNGGU_REVIEW,
                    &detail_images_json,
                    &req.latitude,
                    &req.longitude,
                    // $18 — titik fokus cover. Product baru selalu mulai dari
                    // tengah; merchant menggesernya lewat editor sesudah itu.
                    &crate::models::products::fokus_tengah(),
            ],
            )
            .await;

            match result {
                Ok(rows) => {
                    let row = rows.into_iter().next().ok_or_else(|| {
                        anyhow::anyhow!("INSERT returned no rows for product: {}", id)
                    })?;
                    return Self::row_to_product_no_agg(&row);
                }
                Err(e) if is_unique_violation(&e) => {
                    last_err = e;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    pub(super) async fn exec_create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<ProductVariant>> {
        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let product_vec = id_to_vec(event_id)?;
        type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(variants.len());
        for _ in 0..variants.len() {
            let id = new_ulid();
            ids.push(ulid_to_vec(&id)?);
        }

        let cols = VARIANT_INSERT_COLS;
        let mut value_clauses = Vec::with_capacity(variants.len());
        for i in 0..variants.len() {
            let base = i * cols + 1;
            let placeholders: Vec<String> = (base..base + cols)
                .enumerate()
                .map(|(offset, n)| {
                    if offset == 4 || offset == 5 {
                        format!("(${n}::float8)::numeric")
                    } else {
                        format!("${n}")
                    }
                })
                .collect();
            value_clauses.push(format!("({})", placeholders.join(",")));
        }

        let sql = format!(
            "INSERT INTO product_variants \
             (id, event_id, name, description, price, sale_price, sale_price_start_date, \
              sale_price_end_date, quota, max_per_order, sort_order) \
             VALUES {} \
             RETURNING {}",
            value_clauses.join(","),
            VARIANT_COLS,
        );

        let mut params: Vec<BoxParam> = Vec::with_capacity(variants.len() * cols);
        for (i, v) in variants.iter().enumerate() {
            let sort_order = v.sort_order.unwrap_or(i as i32);
            params.push(Box::new(ids[i].clone()));
            params.push(Box::new(product_vec.clone()));
            params.push(Box::new(v.name.clone()));
            params.push(Box::new(v.description.clone()));
            params.push(Box::new(v.price));
            params.push(Box::new(v.sale_price));
            params.push(Box::new(v.sale_price_start_date));
            params.push(Box::new(v.sale_price_end_date));
            params.push(Box::new(v.quota));
            params.push(Box::new(v.max_per_order));
            params.push(Box::new(sort_order));
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_variant).collect()
    }

    /// Atomic create product + variants in one DB transaction.
    pub(super) async fn exec_create_with_variants(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateProductRequest,
        variants: &[CreateVariantInline],
        cover_url: Option<&str>,
    ) -> Result<(Product, Vec<ProductVariant>)> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        let category_json = serde_json::to_value(&req.category)?;
        let detail_images_json = serde_json::to_value(&req.detail_images)?;

        let mut client = get_conn(&self.pool).await?;
        let tx = client.transaction().await?;

        let mut last_err = anyhow::anyhow!("slug generation failed after max retries");
        let mut inserted_product: Option<Product> = None;
        for _ in 0..5u8 {
            let slug = generate_slug(merchant_name, &req.name);
            let result = tx
                .query(
                    INSERT_EVENT,
                    &[
                        &id_vec,
                        &mid_vec,
                        &req.name,
                        &slug,
                        &req.description,
                        &cover_url,
                        &0f64,
                        &req.venue,
                        &req.city,
                        &req.event_date,
                        &req.start_time,
                        &req.end_time,
                        &category_json,
                        &STATUS_MENUNGGU_REVIEW,
                        &detail_images_json,
                        &req.latitude,
                        &req.longitude,
                        // $18 — cover_focus. Sempat tertinggal di sini padahal
                        // `INSERT_EVENT` menuntut 18 parameter, jadi jalur ini
                        // pasti gagal saat bind ("17 parameters, requires 18").
                        // Belum ada yang memanggilnya, jadi tak pernah terlihat.
                        &crate::models::products::fokus_tengah(),
                    ],
                )
                .await;
            match result {
                Ok(rows) => {
                    let row = rows
                        .into_iter()
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("INSERT returned no rows"))?;
                    inserted_product = Some(Self::row_to_product_no_agg(&row)?);
                    break;
                }
                Err(e) => {
                    if e.as_db_error()
                        .map(|db| db.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION)
                        .unwrap_or(false)
                    {
                        last_err = anyhow::anyhow!("{}", e);
                        continue;
                    }
                    return Err(anyhow::anyhow!("{}", e));
                }
            }
        }
        let product_base = inserted_product.ok_or(last_err)?;

        let product_variants = if variants.is_empty() {
            Vec::new()
        } else {
            type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
            let product_vec = id_vec.clone();
            let cols = VARIANT_INSERT_COLS;
            let mut var_ids: Vec<Vec<u8>> = Vec::with_capacity(variants.len());
            for _ in 0..variants.len() {
                var_ids.push(ulid_to_vec(&new_ulid())?);
            }

            let mut value_clauses = Vec::with_capacity(variants.len());
            for i in 0..variants.len() {
                let base = i * cols + 1;
                let placeholders: Vec<String> = (base..base + cols)
                    .enumerate()
                    .map(|(offset, n)| {
                        if offset == 4 || offset == 5 {
                            format!("(${n}::float8)::numeric")
                        } else {
                            format!("${n}")
                        }
                    })
                    .collect();
                value_clauses.push(format!("({})", placeholders.join(",")));
            }

            let sql = format!(
                "INSERT INTO product_variants \
                 (id, event_id, name, description, price, sale_price, sale_price_start_date, \
                  sale_price_end_date, quota, max_per_order, sort_order) \
                 VALUES {} RETURNING {}",
                value_clauses.join(","),
                VARIANT_COLS,
            );

            let mut params: Vec<BoxParam> = Vec::with_capacity(variants.len() * cols);
            for (i, v) in variants.iter().enumerate() {
                let sort_order = v.sort_order.unwrap_or(i as i32);
                params.push(Box::new(var_ids[i].clone()));
                params.push(Box::new(product_vec.clone()));
                params.push(Box::new(v.name.clone()));
                params.push(Box::new(v.description.clone()));
                params.push(Box::new(v.price));
                params.push(Box::new(v.sale_price));
                params.push(Box::new(v.sale_price_start_date));
                params.push(Box::new(v.sale_price_end_date));
                params.push(Box::new(v.quota));
                params.push(Box::new(v.max_per_order));
                params.push(Box::new(sort_order));
            }

            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|p| p.as_ref() as _).collect();

            let rows = tx
                .query(&sql, &refs)
                .await
                .map_err(|e| anyhow::anyhow!("variant insert failed: {}", e))?;
            rows.iter()
                .map(Self::row_to_variant)
                .collect::<Result<Vec<_>>>()?
        };

        tx.commit().await.context("transaction commit failed")?;
        Ok((product_base, product_variants))
    }

    pub(super) async fn exec_update(
        &self,
        id: &str,
        merchant_id: &str,
        req: &UpdateProductRequest,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;

        // `None` = tak dikirim (COALESCE mempertahankan yang lama);
        // `Some(vec![])` = dikosongkan dengan sengaja → tersimpan sebagai `[]`.
        let category_json: Option<serde_json::Value> = req
            .category
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let detail_images_json: Option<serde_json::Value> = req
            .detail_images
            .as_ref()
            .map(|di| serde_json::to_value(di))
            .transpose()?;

        // Jumlah baris DIPERIKSA, tak lagi dibuang.
        //
        // `WHERE id = $1 AND merchant_id = $2` bisa mencocokkan NOL baris —
        // product milik merchant lain, atau id yang sudah tak ada. `exec_drop`
        // mengembalikan jumlah baris terpengaruh, tapi nilainya selama ini
        // langsung dibuang, jadi keadaan itu tak bisa dibedakan dari sukses:
        // server menjawab Ok, layar menampilkan "tersimpan", dan tak satu pun
        // perubahan benar-benar masuk. Persis gejala "simpan tak bisa" yang
        // tak meninggalkan jejak galat di mana pun.
        let terpengaruh = exec_drop(
            &self.pool,
            UPDATE_EVENT,
            &[
                &id_vec,
                &merchant_id_vec,
                &req.name,
                &req.description,
                &req.cover_url,
                &req.venue,
                &req.city,
                &req.event_date,
                &req.start_time,
                &req.end_time,
                // Status diambil dari permintaan, bukan dipaku "edited" di sini.
                // Nilai paku itu membuat `UpdateProductRequest.status` jadi field
                // yang tampak berfungsi padahal diabaikan diam-diam — siapa pun
                // yang mengisinya (termasuk jalur admin di kemudian hari) akan
                // melihat perubahannya lenyap tanpa galat. Yang memaksa "edited"
                // untuk suntingan merchant sekarang server function-nya, tempat
                // aturan itu memang berlaku.
                &req.status,
                &category_json,
                &detail_images_json,
                &req.latitude,
                &req.longitude,
                &req.cover_focus,
            ],
        )
        .await?;

        if terpengaruh == 0 {
            anyhow::bail!(
                "Product tidak ditemukan atau bukan milik merchant ini \
                 (product {id}, merchant {merchant_id})"
            );
        }
        Ok(())
    }

    pub(super) async fn exec_delete(&self, id: &str, merchant_id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        exec_drop(&self.pool, DELETE_EVENT, &[&id_vec, &mid_vec]).await?;
        Ok(())
    }
}
