use anyhow::Result;

use crate::repository::db::{exec_drop, exec_one, exec_rows};
use super::helpers::{ADMIN_UPDATE_EVENT_STATUS, EVENT_COLS, VARIANT_STATS_LATERAL};
use super::{EventFilterOwned, EventListFilter, PgEventRepository};
use crate::models::events::Event;
use crate::utils::ulid::id_to_vec;

impl PgEventRepository {
    pub(super) async fn exec_admin_list_by_status(
        &self,
        f: &EventListFilter<'_>,
    ) -> Result<Vec<Event>> {
        let owned = EventFilterOwned::from_filter(f)?;
        let mut sql = format!(
            "SELECT {cols} FROM events e {lateral} WHERE 1 = 1",
            cols = EVENT_COLS,
            lateral = VARIANT_STATS_LATERAL,
        );
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(6);
        let mut idx = 1usize;
        owned.push_where(&mut sql, &mut refs, &mut idx, "e.", false);
        sql.push_str(&format!(
            " ORDER BY e.updated_at DESC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        refs.push(&f.limit);
        refs.push(&f.offset);

        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    pub(super) async fn exec_admin_count_by_status(
        &self,
        f: &EventListFilter<'_>,
    ) -> Result<i64> {
        let owned = EventFilterOwned::from_filter(f)?;
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM events WHERE 1 = 1");
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(4);
        let mut idx = 1usize;
        owned.push_where(&mut sql, &mut refs, &mut idx, "", false);

        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    pub(super) async fn exec_admin_update_status(&self, id: &str, status: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, ADMIN_UPDATE_EVENT_STATUS, &[&id_vec, &status]).await?;
        Ok(())
    }
}
