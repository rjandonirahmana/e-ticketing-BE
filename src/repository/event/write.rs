use anyhow::{Context, Result};

use super::db::{exec_drop, exec_rows, get_conn};
use super::helpers::{
    generate_slug, is_unique_violation, DELETE_EVENT, INSERT_EVENT, UPDATE_EVENT, VARIANT_COLS,
    VARIANT_INSERT_COLS,
};
use super::{PgEventRepository};
use crate::models::events::{CreateEventRequest, CreateVariantInline, Event, UpdateEventRequest};
use crate::models::event_variants::EventVariant;
use crate::utils::ulid::{id_to_vec, new_ulid, ulid_to_vec};

impl PgEventRepository {
    pub(super) async fn exec_create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateEventRequest,
        cover_url: Option<&str>,
    ) -> Result<Event> {
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
                    &"edited",
                    &detail_images_json,
                ],
            )
            .await;

            match result {
                Ok(rows) => {
                    let row = rows.into_iter().next().ok_or_else(|| {
                        anyhow::anyhow!("INSERT returned no rows for event: {}", id)
                    })?;
                    return Self::row_to_event_no_agg(&row);
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
    ) -> Result<Vec<EventVariant>> {
        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let event_vec = id_to_vec(event_id)?;
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
            "INSERT INTO event_variants \
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
            params.push(Box::new(event_vec.clone()));
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

    /// Atomic create event + variants in one DB transaction.
    pub(super) async fn exec_create_with_variants(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateEventRequest,
        variants: &[CreateVariantInline],
        cover_url: Option<&str>,
    ) -> Result<(Event, Vec<EventVariant>)> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        let category_json = serde_json::to_value(&req.category)?;
        let detail_images_json = serde_json::to_value(&req.detail_images)?;

        let mut client = get_conn(&self.pool).await?;
        let tx = client.transaction().await?;

        let mut last_err = anyhow::anyhow!("slug generation failed after max retries");
        let mut inserted_event: Option<Event> = None;
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
                        &"edited",
                        &detail_images_json,
                    ],
                )
                .await;
            match result {
                Ok(rows) => {
                    let row = rows
                        .into_iter()
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("INSERT returned no rows"))?;
                    inserted_event = Some(Self::row_to_event_no_agg(&row)?);
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
        let event_base = inserted_event.ok_or(last_err)?;

        let event_variants = if variants.is_empty() {
            Vec::new()
        } else {
            type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
            let event_vec = id_vec.clone();
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
                "INSERT INTO event_variants \
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
                params.push(Box::new(event_vec.clone()));
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
        Ok((event_base, event_variants))
    }

    pub(super) async fn exec_update(
        &self,
        id: &str,
        merchant_id: &str,
        req: &UpdateEventRequest,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;

        let category_json: Option<serde_json::Value> = if req.category.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&req.category)?)
        };
        let detail_images_json: Option<serde_json::Value> = req
            .detail_images
            .as_ref()
            .map(|di| serde_json::to_value(di))
            .transpose()?;

        exec_drop(
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
                &"edited",
                &category_json,
                &detail_images_json,
            ],
        )
        .await?;
        Ok(())
    }

    pub(super) async fn exec_delete(&self, id: &str, merchant_id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        exec_drop(&self.pool, DELETE_EVENT, &[&id_vec, &mid_vec]).await?;
        Ok(())
    }
}
