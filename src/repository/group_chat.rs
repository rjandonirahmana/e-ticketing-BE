use anyhow::Result;
use deadpool_postgres::Pool;

use crate::models::group_chat::{GroupMessage, GroupRoom, MemberRole, MsgType, TicketCard};
use crate::repository::db::{col_opt_str, exec_drop, exec_rows};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

pub struct GroupChatRepository {
    pool: Pool,
}

impl GroupChatRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // ── Rooms ─────────────────────────────────────────────────────────────────

    /// Buat room untuk event — satu event satu room (unique event_id).
    pub async fn upsert_event_room(
        &self,
        event_id: &str,
        name: &str,
        cover_url: Option<&str>,
        created_by: &str,
    ) -> Result<GroupRoom> {
        let id_b = ulid_to_vec(&new_ulid())?;
        let event_b = id_to_vec(event_id)?;
        let creator_b = id_to_vec(created_by)?;
        exec_drop(
            &self.pool,
            r#"
            INSERT INTO group_rooms (id, event_id, name, cover_url, created_by, created_at)
            VALUES ($1,$2,$3,$4,$5,NOW())
            ON CONFLICT (event_id) DO NOTHING
        "#,
            &[&id_b, &event_b, &name, &cover_url, &creator_b],
        )
        .await?;
        self.find_by_event(event_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found after upsert"))
    }

    pub async fn find_by_event(&self, event_id: &str) -> Result<Option<GroupRoom>> {
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

    pub async fn find_by_id(&self, room_id: &str) -> Result<Option<GroupRoom>> {
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

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>> {
        let user_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT r.id, r.event_id, r.name, r.cover_url, r.created_by, r.created_at,
                   COUNT(m2.user_id)::BIGINT AS member_count
            FROM group_rooms r
            JOIN group_members m ON m.room_id = r.id AND m.user_id = $1
            LEFT JOIN group_members m2 ON m2.room_id = r.id
            GROUP BY r.id ORDER BY r.created_at DESC
        "#,
            &[&user_b],
        )
        .await?;
        rows.iter().map(|r| Self::row_to_room(r)).collect()
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

    // ── Members ───────────────────────────────────────────────────────────────

    pub async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool> {
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

    pub async fn add_member(&self, room_id: &str, user_id: &str, role: MemberRole) -> Result<()> {
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

    pub async fn get_member_ids(&self, room_id: &str) -> Result<Vec<String>> {
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

    pub async fn save_message(&self, msg: &GroupMessage) -> Result<()> {
        let id_b = ulid_to_vec(&msg.id)?;
        let room_b = id_to_vec(&msg.room_id)?;
        let sender_b = id_to_vec(&msg.sender_id)?;
        // OPTIMISASI: serde_json::to_value() bukan to_string().
        // to_string(): TicketCard → JSON String → PostgreSQL parse String → jsonb  (2×)
        // to_value():  TicketCard → serde_json::Value → tokio-postgres kirim langsung ke jsonb (1×)
        // Value implements ToSql for jsonb — tidak ada intermediate String allocation.
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
                &id_b,
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

    /// Hitung berapa pesan sudah dikirim user di room — untuk enforce limit customer
    pub async fn count_user_messages(&self, room_id: &str, user_id: &str) -> Result<i64> {
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

    pub async fn get_history(
        &self,
        room_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)> {
        let room_b = id_to_vec(room_id)?;
        let fetch = limit + 1;
        let rows = if let Some(bid) = before_id {
            let bid_b = id_to_vec(bid)?;
            exec_rows(&self.pool, r#"
                SELECT m.id, m.room_id, m.sender_id, m.sender_name, m.msg_type,
                       m.content, m.media_url, m.ticket_card::text AS ticket_card, m.is_system, m.sent_at
                FROM group_messages m
                CROSS JOIN (SELECT sent_at AS cs, id AS ci FROM group_messages WHERE id=$3) cur
                WHERE m.room_id=$1 AND (m.sent_at < cur.cs OR (m.sent_at=cur.cs AND m.id < cur.ci))
                ORDER BY m.sent_at DESC, m.id DESC LIMIT $2
            "#, &[&room_b, &fetch, &bid_b]).await?
        } else {
            exec_rows(
                &self.pool,
                r#"
                SELECT id, room_id, sender_id, sender_name, msg_type,
                       content, media_url, ticket_card::text AS ticket_card, is_system, sent_at
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
                let tj: Option<String> = col_opt_str(row, "ticket_card")?;
                let ticket_card = tj
                    .as_deref()
                    .map(|j| serde_json::from_str::<TicketCard>(j))
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
