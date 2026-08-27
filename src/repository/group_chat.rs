use anyhow::Result;
use async_trait::async_trait;
use deadpool_postgres::Pool;

use crate::models::group_chat::{GroupMessage, GroupRoom, MsgType, TicketCard};
use crate::repository::db::{col_opt_str, exec_drop, exec_rows};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_arr, ulid_to_vec};

// ── Trait ─────────────────────────────────────────────────────────────────────

/// FIX: Ekstrak semua method ke trait agar GroupChatService bisa bergantung pada
/// abstraksi (Arc<dyn GroupChatRepository>), bukan concrete type.
/// Ini konsisten dengan OrderRepository, ProductRepository, dll, dan memungkinkan
/// mock/test tanpa database sungguhan.
#[async_trait]
pub trait GroupChatRepository: Send + Sync {
    // Rooms
    /// Percakapan berdua antara satu pembeli dan satu toko.
    async fn find_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<Option<GroupRoom>>;

    /// Buat percakapan berdua. Bila sudah ada (balapan dua permintaan),
    /// kembalikan yang sudah ada alih-alih gagal.
    async fn create_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<GroupRoom>;

    async fn find_by_id(&self, room_id: &str) -> Result<Option<GroupRoom>>;

    /// Inbox, terbaru di atas — diurut `last_message_at`, bukan `created_at`.
    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>>;

    // Peserta — dibaca dari baris `chats`, tak ada tabel anggota lagi.
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool>;
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

/// Bentuk SELECT untuk satu percakapan yang dicari lewat pasangannya.
/// Dipakai `find_dm` dan `create_dm` supaya kolom yang dibaca `row_to_room`
/// tak pernah berbeda di antara keduanya.
#[allow(dead_code)]
const SQL_PILIH_CHAT_PASANGAN: &str = r#"
    SELECT c.id, c.merchant_id, c.created_at,
           COALESCE(d.store_name, u.name, '') AS name,
           d.logo_url AS cover_url
    FROM chats c
    JOIN users u ON u.id = c.merchant_id
    LEFT JOIN merchant_details d ON d.user_id = c.merchant_id
    WHERE c.buyer_id = $1 AND c.merchant_id = $2
"#;

// ── Postgres impl ─────────────────────────────────────────────────────────────

pub struct PgGroupChatRepository {
    pool: Pool,
}

impl PgGroupChatRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Kolom `event_id`, `created_by`, dan `member_count` sudah tak ada di
    /// tabel (migrasi 029). Bentuk `GroupRoom` sengaja DIPERTAHANKAN supaya
    /// seluruh pemanggil di web dan WebSocket tak perlu ikut berubah:
    ///   * `event_id`     → kosong, percakapan tak terikat produk mana pun
    ///   * `created_by`   → `merchant_id`, satu-satunya nilai yang pernah diisi
    ///   * `member_count` → selalu 2, dan itu memang definisinya sekarang
    fn row_to_room(row: &tokio_postgres::Row) -> Result<GroupRoom> {
        let id_b: Vec<u8> = row.try_get("id")?;
        let merchant_b: Vec<u8> = row.try_get("merchant_id")?;
        Ok(GroupRoom {
            id: bin_to_ulid(id_b)?,
            event_id: String::new(),
            name: row.try_get("name")?,
            cover_url: col_opt_str(row, "cover_url")?,
            created_by: bin_to_ulid(merchant_b)?,
            created_at: row.try_get("created_at")?,
            member_count: 2,
        })
    }

}

#[async_trait]
impl GroupChatRepository for PgGroupChatRepository {
    // ── Rooms ─────────────────────────────────────────────────────────────────

    async fn find_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<Option<GroupRoom>> {
        let buyer_b = id_to_vec(buyer_id)?;
        let merch_b = id_to_vec(merchant_id)?;
        let rows = exec_rows(&self.pool, SQL_PILIH_CHAT_PASANGAN, &[&buyer_b, &merch_b]).await?;
        rows.first().map(|r| Self::row_to_room(r)).transpose()
    }

    /// `ON CONFLICT ... DO UPDATE`, BUKAN `DO NOTHING`.
    ///
    /// Dengan `DO NOTHING`, permintaan yang kalah balapan tidak menerima baris
    /// apa pun dari `RETURNING` — dan pemanggil mendapat "gagal membuat" untuk
    /// percakapan yang sebenarnya ADA. `DO UPDATE` yang menyentuh satu kolom
    /// tak berbahaya selalu mengembalikan barisnya, sehingga dua tab yang
    /// menekan tombol bersamaan mendarat di percakapan yang sama.
    async fn create_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<GroupRoom> {
        let id_b = ulid_to_vec(&new_ulid())?;
        let buyer_b = id_to_vec(buyer_id)?;
        let merch_b = id_to_vec(merchant_id)?;
        exec_drop(
            &self.pool,
            r#"
            INSERT INTO chats (id, buyer_id, merchant_id, created_at, last_message_at)
            VALUES ($1, $2, $3, NOW(), NOW())
            ON CONFLICT (buyer_id, merchant_id) DO UPDATE
                SET last_message_at = chats.last_message_at
            "#,
            &[&id_b, &buyer_b, &merch_b],
        )
        .await?;
        let rows = exec_rows(&self.pool, SQL_PILIH_CHAT_PASANGAN, &[&buyer_b, &merch_b]).await?;
        rows.first()
            .map(|r| Self::row_to_room(r))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("Percakapan tak ditemukan sesudah dibuat"))
    }

    async fn find_by_id(&self, room_id: &str) -> Result<Option<GroupRoom>> {
        let room_b = id_to_vec(room_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT c.id, c.merchant_id, c.created_at,
                   COALESCE(d.store_name, u.name, '') AS name,
                   d.logo_url AS cover_url
            FROM chats c
            JOIN users u ON u.id = c.merchant_id
            LEFT JOIN merchant_details d ON d.user_id = c.merchant_id
            WHERE c.id = $1
            "#,
            &[&room_b],
        )
        .await?;
        rows.first().map(|r| Self::row_to_room(r)).transpose()
    }

    /// ── NAMA YANG TAMPIL ADALAH LAWAN BICARA ────────────────────────────────
    ///
    /// Dulu setiap room menyimpan `name`-nya sendiri — salinan nama toko yang
    /// tak pernah diperbarui, sehingga toko yang berganti nama tetap tampil
    /// dengan nama lamanya selamanya. Kini di-JOIN, dan sekalian dibetulkan:
    /// pembeli melihat NAMA TOKO, sedangkan merchant melihat NAMA PEMBELI.
    /// Sebelumnya keduanya melihat teks yang sama, sehingga inbox merchant
    /// berisi deretan baris yang seluruhnya bernama tokonya sendiri.
    ///
    /// Urut `last_message_at DESC`, bukan `created_at`: yang baru menerima
    /// pesan harus naik ke atas. Index `idx_chats_pembeli` / `idx_chats_toko`
    /// sudah memuat kolom urutnya, jadi tak ada penyortiran saat baca.
    async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>> {
        let user_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT c.id, c.merchant_id, c.created_at,
                   CASE WHEN c.buyer_id = $1
                        THEN COALESCE(d.store_name, um.name, '')
                        ELSE COALESCE(ub.name, '') END AS name,
                   CASE WHEN c.buyer_id = $1 THEN d.logo_url ELSE NULL END AS cover_url
            FROM chats c
            JOIN users um ON um.id = c.merchant_id
            JOIN users ub ON ub.id = c.buyer_id
            LEFT JOIN merchant_details d ON d.user_id = c.merchant_id
            WHERE c.buyer_id = $1 OR c.merchant_id = $1
            ORDER BY c.last_message_at DESC
            "#,
            &[&user_b],
        )
        .await?;
        rows.iter().map(|r| Self::row_to_room(r)).collect()
    }

    // ── Peserta ───────────────────────────────────────────────────────────────

    /// Keanggotaan dibaca dari baris `chats` sendiri — `buyer_id` dan
    /// `merchant_id` MEMANG kedua pesertanya. Tabel `group_members` dibuang
    /// (migrasi 029) justru karena ia sumber kebenaran kedua yang bisa
    /// berselisih dengan baris ini, dan selisihnya berbentuk percakapan yang
    /// ada tetapi tak bisa dimasuki siapa pun.
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool> {
        let room_b = id_to_vec(room_id)?;
        let user_b = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            "SELECT 1 FROM chats WHERE id = $1 AND (buyer_id = $2 OR merchant_id = $2)",
            &[&room_b, &user_b],
        )
        .await?;
        Ok(!rows.is_empty())
    }

    /// Penerima fanout WebSocket: tepat dua orang, satu baris, tanpa join.
    async fn get_member_ids(&self, room_id: &str) -> Result<Vec<String>> {
        let room_b = id_to_vec(room_id)?;
        let rows = exec_rows(
            &self.pool,
            "SELECT buyer_id, merchant_id FROM chats WHERE id = $1",
            &[&room_b],
        )
        .await?;
        let Some(r) = rows.first() else {
            return Ok(Vec::new());
        };
        let b: Vec<u8> = r.try_get("buyer_id")?;
        let m: Vec<u8> = r.try_get("merchant_id")?;
        Ok(vec![bin_to_ulid(b)?, bin_to_ulid(m)?])
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
            INSERT INTO chat_messages
                (id, chat_id, sender_id, sender_name, msg_type,
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

        // ── Naikkan percakapan ke puncak inbox ──────────────────────────────
        // Inilah yang membuat `ORDER BY last_message_at` bermakna. Tanpa baris
        // ini, kolomnya membeku di waktu pembuatan dan urutannya kembali persis
        // seperti cacat yang diperbaiki migrasi 029: pesan baru dari pembeli
        // tenggelam di bawah percakapan lama yang sudah selesai.
        //
        // `GREATEST` menjaga agar pesan yang datang TERLAMBAT — retry jaringan,
        // atau pesan lama yang baru tersimpan — tak menarik mundur percakapan
        // yang sudah punya pesan lebih baru.
        //
        // Kegagalannya tidak menggagalkan penyimpanan pesan: pesannya sudah
        // tersimpan, dan urutan inbox yang meleset jauh lebih ringan daripada
        // pesan yang hilang.
        if let Err(e) = exec_drop(
            &self.pool,
            "UPDATE chats SET last_message_at = GREATEST(last_message_at, $2) WHERE id = $1",
            &[&room_b, &msg.sent_at],
        )
        .await
        {
            tracing::warn!(error = %e, room_id = %msg.room_id,
                "gagal memperbarui last_message_at (pesan tetap tersimpan)");
        }
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
                    SELECT 1 FROM chat_messages
                    WHERE chat_id = $2 AND sender_id = $3 AND is_system = FALSE
                    LIMIT 1
                ) AS ok
            )
            INSERT INTO chat_messages
                (id, chat_id, sender_id, sender_name, msg_type,
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
            SELECT COUNT(*)::BIGINT AS cnt FROM chat_messages
            WHERE chat_id = $1 AND sender_id=$2 AND is_system=FALSE
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
    /// `(SELECT sent_at FROM chat_messages WHERE id = $3)` sebelumnya ditulis 2×.
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
                    SELECT sent_at, id FROM chat_messages WHERE id = $3
                )
                SELECT m.id, m.chat_id, m.sender_id, m.sender_name, m.msg_type,
                       m.content, m.media_url, m.ticket_card, m.is_system, m.sent_at
                FROM chat_messages m, cursor c
                WHERE m.chat_id = $1
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
                FROM chat_messages
                WHERE chat_id = $1
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
