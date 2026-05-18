use anyhow::Result;
use async_trait::async_trait;
use deadpool_postgres::Pool;

use crate::models::group_chat::{GroupMessage, GroupRoom, MemberRole, MsgType, TicketCard};
use crate::repository::db::{col_opt_str, exec_drop, exec_rows};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_arr, ulid_to_vec};

// ── Trait ─────────────────────────────────────────────────────────────────────

/// FIX: Ekstrak semua method ke trait agar GroupChatService bisa bergantung pada
/// abstraksi (Arc<dyn GroupChatRepository>), bukan concrete type.
/// Ini konsisten dengan OrderRepository, EventRepository, dll, dan memungkinkan
/// mock/test tanpa database sungguhan.
#[async_trait]
pub trait GroupChatRepository: Send + Sync {
    // Rooms
    async fn upsert_event_room(
        &self,
        event_id: &str,
        name: &str,
        cover_url: Option<&str>,
        created_by: &str,
    ) -> Result<GroupRoom>;
    async fn find_by_event(&self, event_id: &str) -> Result<Option<GroupRoom>>;
    async fn find_by_id(&self, room_id: &str) -> Result<Option<GroupRoom>>;
    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>>;

    // Members
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool>;
    async fn add_member(&self, room_id: &str, user_id: &str, role: MemberRole) -> Result<()>;
    async fn get_member_ids(&self, room_id: &str) -> Result<Vec<String>>;

    // Messages
    async fn save_message(&self, msg: &GroupMessage) -> Result<()>;
    async fn save_message_if_under_limit(&self, msg: &GroupMessage) -> Result<bool>;
    async fn count_user_messages(&self, room_id: &str, user_id: &str) -> Result<i64>;
    async fn get_history(
        &self,
        room_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

pub struct PgGroupChatRepository {
    pool: Pool,
}

impl PgGroupChatRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_room(row: &tokio_postgres::Row) -> Result<GroupRoom> {
        let id_b: Vec<u8> = row.try_get("id")?;
        let event_b: Vec<u8> = row.try_get("event_id")?;
        let creator_b: Vec<u8> = row.try_get("created_by")?;
        Ok(GroupRoom {
            id: bin_to_ulid(id_b)?,
            event_id: bin_to_ulid(event_b)?,
            name: row.try_get("name")?,
            cover_url: col_opt_str(row, "cover_url")?,
            created_by: bin_to_ulid(creator_b)?,
            created_at: row.try_get("created_at")?,
            member_count: row.try_get("member_count")?,
        })
    }
}

#[async_trait]
impl GroupChatRepository for PgGroupChatRepository {
    // ── Rooms ─────────────────────────────────────────────────────────────────

    async fn upsert_event_room(
        &self,
        event_id: &str,
        name: &str,
        cover_url: Option<&str>,
        created_by: &str,
    ) -> Result<GroupRoom> {
        let id_b = ulid_to_vec(&new_ulid())?;
        let event_b = id_to_vec(event_id)?;
        let creator_b = id_to_vec(created_by)?;
        // FIX: RETURNING menghilangkan round-trip find_by_event setelah insert.
        // DO UPDATE SET ... agar ON CONFLICT juga return row yang existing.
        // member_count diambil via subquery skalar — tidak perlu GROUP BY terpisah.
        let rows = exec_rows(
            &self.pool,
            r#"
            INSERT INTO group_rooms (id, event_id, name, cover_url, created_by, created_at)
            VALUES ($1,$2,$3,$4,$5,NOW())
            ON CONFLICT (event_id) DO UPDATE
                SET name      = EXCLUDED.name,
                    cover_url = EXCLUDED.cover_url
            RETURNING
                id, event_id, name, cover_url, created_by, created_at,
                (SELECT COUNT(*)::BIGINT FROM group_members WHERE room_id = group_rooms.id) AS member_count
            "#,
            &[&id_b, &event_b, &name, &cover_url, &creator_b],
        )
        .await?;
        rows.first()
            .map(|r| Self::row_to_room(r))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("Room not found after upsert"))
    }

    async fn find_by_event(&self, event_id: &str) -> Result<Option<GroupRoom>> {
        let event_b = id_to_vec(event_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT r.id, r.event_id, r.name, r.cover_url, r.created_by, r.created_at,
                   COUNT(m.user_id)::BIGINT AS member_count
            FROM group_rooms r
            LEFT JOIN group_members m ON m.room_id = r.id
            WHERE r.event_id = $1
            GROUP BY r.id LIMIT 1
        "#,
            &[&event_b],
        )
        .await?;
        rows.first().map(|r| Self::row_to_room(r)).transpose()
    }

    async fn find_by_id(&self, room_id: &str) -> Result<Option<GroupRoom>> {
        let room_b = id_to_vec(room_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT r.id, r.event_id, r.name, r.cover_url, r.created_by, r.created_at,
                   COUNT(m.user_id)::BIGINT AS member_count
            FROM group_rooms r
            LEFT JOIN group_members m ON m.room_id = r.id
            WHERE r.id = $1
            GROUP BY r.id LIMIT 1
        "#,
            &[&room_b],
        )
        .await?;
        rows.first().map(|r| Self::row_to_room(r)).transpose()
    }

    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>> {
        let user_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            // P1 FIX: Ganti LEFT JOIN group_members m2 + GROUP BY dengan correlated subquery.
            // Self-join menghasilkan Cartesian product sebelum aggregate — untuk room 10k member
            // ini membengkak jadi O(n²). Subquery berjalan O(n) per row event.
            r#"
            SELECT r.id, r.event_id, r.name, r.cover_url, r.created_by, r.created_at,
                   (SELECT COUNT(*)::BIGINT FROM group_members WHERE room_id = r.id) AS member_count
            FROM group_rooms r
            JOIN group_members m ON m.room_id = r.id AND m.user_id = $1
            ORDER BY r.created_at DESC
        "#,
            &[&user_b],
        )
        .await?;
        rows.iter().map(|r| Self::row_to_room(r)).collect()
    }

    // ── Members ───────────────────────────────────────────────────────────────

    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool> {
        let room_b = id_to_vec(room_id)?;
        let user_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            "SELECT 1 FROM group_members WHERE room_id=$1 AND user_id=$2",
            &[&room_b, &user_b],
        )
        .await?;
        Ok(!rows.is_empty())
    }

    async fn add_member(&self, room_id: &str, user_id: &str, role: MemberRole) -> Result<()> {
        let room_b = id_to_vec(room_id)?;
        let user_b = id_to_vec(user_id)?;
        exec_drop(
            &self.pool,
            r#"
            INSERT INTO group_members (room_id, user_id, role, joined_at)
            VALUES ($1,$2,$3,NOW())
            ON CONFLICT (room_id, user_id) DO NOTHING
        "#,
            &[&room_b, &user_b, &role.as_str()],
        )
        .await?;
        Ok(())
    }

    async fn get_member_ids(&self, room_id: &str) -> Result<Vec<String>> {
        let room_b = id_to_vec(room_id)?;
        let rows = exec_rows(
            &self.pool,
            "SELECT user_id FROM group_members WHERE room_id=$1",
            &[&room_b],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let b: Vec<u8> = r.try_get("user_id")?;
                bin_to_ulid(b).map_err(Into::into)
            })
            .collect()
    }

    // ── Messages ──────────────────────────────────────────────────────────────

    async fn save_message(&self, msg: &GroupMessage) -> Result<()> {
        // FIX: ulid_to_arr → stack [u8;16], tidak ada Vec heap alloc untuk msg ID
        let id_arr = ulid_to_arr(&msg.id)?;
        let room_b = id_to_vec(&msg.room_id)?;
        let sender_b = id_to_vec(&msg.sender_id)?;
        let ticket_json: Option<serde_json::Value> = msg
            .ticket_card
            .as_ref()
            .map(|t| serde_json::to_value(t))
            .transpose()?;
        exec_drop(
            &self.pool,
            r#"
            INSERT INTO group_messages
                (id, room_id, sender_id, sender_name, msg_type,
                 content, media_url, ticket_card, is_system, sent_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8::jsonb,$9,$10)
            ON CONFLICT (id) DO NOTHING
        "#,
            &[
                &&id_arr[..],
                &room_b,
                &sender_b,
                &msg.sender_name,
                &msg.msg_type.as_str(),
                &msg.content,
                &msg.media_url,
                &ticket_json,
                &msg.is_system,
                &msg.sent_at,
            ],
        )
        .await?;
        Ok(())
    }

    /// Atomic INSERT + limit check via CTE.
    /// Count check dan insert dalam satu query — tidak ada race condition.
    /// Return `Ok(true)` jika berhasil insert, `Ok(false)` jika limit sudah tercapai.
    async fn save_message_if_under_limit(&self, msg: &GroupMessage) -> Result<bool> {
        let id_arr = ulid_to_arr(&msg.id)?;
        let room_b = id_to_vec(&msg.room_id)?;
        let sender_b = id_to_vec(&msg.sender_id)?;
        let ticket_json: Option<serde_json::Value> = msg
            .ticket_card
            .as_ref()
            .map(|t| serde_json::to_value(t))
            .transpose()?;

        // FIX: NOT EXISTS + LIMIT 1 menggantikan COUNT(*) untuk short-circuit.
        // COUNT harus scan semua row yang match; EXISTS berhenti di row pertama.
        // Untuk CUSTOMER_MSG_LIMIT=1, EXISTS O(1) vs COUNT O(n messages).
        // Jika limit dinaikkan di masa depan, pertimbangkan kembali ke COUNT.
        let rows = exec_rows(
            &self.pool,
            r#"
            WITH can_send AS (
                SELECT NOT EXISTS (
                    SELECT 1 FROM group_messages
                    WHERE room_id = $2 AND sender_id = $3 AND is_system = FALSE
                    LIMIT 1
                ) AS ok
            )
            INSERT INTO group_messages
                (id, room_id, sender_id, sender_name, msg_type,
                 content, media_url, ticket_card, is_system, sent_at)
            SELECT $1,$2,$3,$4,$5,$6,$7,$8::jsonb,$9,$10
            FROM can_send WHERE ok = TRUE
            ON CONFLICT (id) DO NOTHING
            RETURNING id
            "#,
            &[
                &&id_arr[..],
                &room_b,
                &sender_b,
                &msg.sender_name,
                &msg.msg_type.as_str(),
                &msg.content,
                &msg.media_url,
                &ticket_json,
                &msg.is_system,
                &msg.sent_at,
            ],
        )
        .await?;

        Ok(!rows.is_empty())
    }

    async fn count_user_messages(&self, room_id: &str, user_id: &str) -> Result<i64> {
        let room_b = id_to_vec(room_id)?;
        let sender_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT COUNT(*)::BIGINT AS cnt FROM group_messages
            WHERE room_id=$1 AND sender_id=$2 AND is_system=FALSE
        "#,
            &[&room_b, &sender_b],
        )
        .await?;
        Ok(rows
            .first()
            .and_then(|r| r.try_get::<_, i64>("cnt").ok())
            .unwrap_or(0))
    }

    /// FIX: Subquery duplikasi → CTE.
    /// `(SELECT sent_at FROM group_messages WHERE id = $3)` sebelumnya ditulis 2×.
    /// Dengan CTE, cursor di-resolve sekali lalu di-reuse.
    async fn get_history(
        &self,
        room_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)> {
        let room_b = id_to_vec(room_id)?;
        let fetch = limit + 1;
        let rows = if let Some(bid) = before_id {
            let bid_b = id_to_vec(bid)?;
            exec_rows(
                &self.pool,
                r#"
                WITH cursor AS (
                    SELECT sent_at, id FROM group_messages WHERE id = $3
                )
                SELECT m.id, m.room_id, m.sender_id, m.sender_name, m.msg_type,
                       m.content, m.media_url, m.ticket_card, m.is_system, m.sent_at
                FROM group_messages m, cursor c
                WHERE m.room_id = $1
                  AND (m.sent_at < c.sent_at OR (m.sent_at = c.sent_at AND m.id < c.id))
                ORDER BY m.sent_at DESC, m.id DESC LIMIT $2
            "#,
                &[&room_b, &fetch, &bid_b],
            )
            .await?
        } else {
            exec_rows(
                &self.pool,
                r#"
                SELECT id, room_id, sender_id, sender_name, msg_type,
                       content, media_url, ticket_card, is_system, sent_at
                FROM group_messages
                WHERE room_id=$1
                ORDER BY sent_at DESC, id DESC LIMIT $2
            "#,
                &[&room_b, &fetch],
            )
            .await?
        };

        let has_more = rows.len() > limit as usize;
        let slice = if has_more {
            &rows[..limit as usize]
        } else {
            &rows[..]
        };
        let mut msgs: Vec<GroupMessage> = slice
            .iter()
            .map(|row| {
                let id_b: Vec<u8> = row.try_get("id")?;
                let room_b2: Vec<u8> = row.try_get("room_id")?;
                let sender_b: Vec<u8> = row.try_get("sender_id")?;
                let type_str: String = row.try_get("msg_type")?;
                // FIX: Parse ticket_card langsung dari JSONB via serde_json::Value.
                // Sebelumnya: ticket_card::text → String → serde_json::from_str (2 alloc).
                // Sekarang: jsonb → serde_json::Value → serde_json::from_value (1 alloc).
                let ticket_card: Option<TicketCard> = row
                    .try_get::<_, Option<serde_json::Value>>("ticket_card")
                    .ok()
                    .flatten()
                    .map(serde_json::from_value)
                    .transpose()?;
                Ok(GroupMessage {
                    id: bin_to_ulid(id_b)?,
                    room_id: bin_to_ulid(room_b2)?,
                    sender_id: bin_to_ulid(sender_b)?,
                    sender_name: row.try_get("sender_name")?,
                    msg_type: MsgType::from_str(&type_str),
                    content: row.try_get("content")?,
                    media_url: col_opt_str(row, "media_url")?,
                    ticket_card,
                    sent_at: row.try_get("sent_at")?,
                    is_system: row.try_get("is_system")?,
                })
            })
            .collect::<Result<_>>()?;
        msgs.reverse();
        Ok((msgs, has_more))
    }
}
