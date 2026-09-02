use anyhow::Result;
use async_trait::async_trait;
use deadpool_postgres::Pool;

use crate::models::group_chat::{GroupMessage, GroupRoom, KutipanPesan, MsgType, TicketCard};
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

    /// Tandai percakapan sudah dibaca sampai SEKARANG oleh `user_id`.
    ///
    /// Sisi mana yang ditulis (`buyer_read_at` atau `merchant_read_at`)
    /// ditentukan di dalam SQL dari baris itu sendiri, bukan dari parameter —
    /// pemanggil tak perlu tahu ia pembeli atau merchant, dan karena itu tak
    /// bisa salah menebaknya.
    async fn mark_read(&self, room_id: &str, user_id: &str) -> Result<()>;

    // Peserta — dibaca dari baris `chats`, tak ada tabel anggota lagi.
    async fn is_member(&self, room_id: &str, user_id: &str) -> Result<bool>;
    async fn get_member_ids(&self, room_id: &str) -> Result<Vec<String>>;

    // Messages
    /// Simpan pesan, periksa keanggotaan, dan naikkan percakapan ke puncak
    /// inbox — SEMUANYA dalam satu perjalanan ke basis data.
    ///
    /// Mengembalikan `false` bila pengirim bukan peserta percakapan itu.
    async fn save_message(&self, msg: &GroupMessage) -> Result<bool>;

    /// Kutipan satu pesan, HANYA bila ia memang berada di ruangan itu.
    ///
    /// `room_id` bukan kenyamanan, melainkan pagar: tanpanya, klien mana pun
    /// bisa membalas id pesan dari percakapan ORANG LAIN dan isinya akan ikut
    /// tersalin ke kutipan yang dikirim balik ke layarnya. Pemeriksaannya di
    /// dalam SQL, bukan sesudahnya, supaya tak ada jalan melewatinya.
    async fn kutipan(&self, room_id: &str, msg_id: &str) -> Result<Option<KutipanPesan>>;
    async fn count_user_messages(&self, room_id: &str, user_id: &str) -> Result<i64>;
    async fn get_history(
        &self,
        room_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)>;

    // Retensi
    /// Hapus satu angkatan pesan yang sudah lewat masa simpan.
    ///
    /// Mengembalikan `(jumlah baris terhapus, alamat berkas yang ikut hilang)`.
    /// Jumlahnya lebih kecil dari `batas` berarti sudah tak ada sisa.
    async fn hapus_kadaluarsa(&self, hari: i64, batas: i64) -> Result<(u64, Vec<String>)>;
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
            // Toleran: kueri yang tak menyertakan kolom ini (mis. `find_by_id`)
            // tetap memakai mapper yang sama. Nol adalah jawaban yang benar di
            // sana — bukan galat.
            unread_count: row.try_get("unread_count").unwrap_or(0),
        })
    }

}

/// Umur dihitung di dalam SQL (`NOW()`), bukan dikirim sebagai timestamp dari
/// Rust: jam proses aplikasi dan jam server basis data tak selalu sama, dan
/// yang menentukan batasnya harus SATU jam saja — kalau tidak, "30 hari" berarti
/// dua hal berbeda tergantung mesin mana yang bertanya.
/// Hapus satu angkatan pesan kedaluwarsa DAN kembalikan alamat berkasnya
/// sekaligus, dalam SATU pernyataan.
///
/// ── KENAPA BUKAN DUA KUERI ────────────────────────────────────────────────
/// Versi sebelumnya bertanya dua kali per putaran: sekali mencari `media_url`,
/// sekali menghapus. Keduanya memakai `ORDER BY sent_at`, dan indeks yang
/// mendukungnya belum terpasang di produksi — sehingga tiap putaran menjadi DUA
/// pemindaian penuh tabel berikut pengurutannya, berulang tiap 200 ms tanpa
/// batas. Basis datanya jenuh, dan karena render SSR menunggu koneksi sebelum
/// mengirim satu bita pun header, seluruh halaman berhenti terlayani sementara
/// berkas statis tetap kencang — gejala yang tampak seperti masalah jaringan.
///
/// `RETURNING` menghapus separuh pekerjaan itu dan membuat keduanya atomik.
///
/// `ORDER BY` DIBUANG, bukan lupa: untuk pekerjaan ini tak ada bedanya baris
/// kedaluwarsa mana yang pergi lebih dulu — semuanya akan pergi. Yang tersisa
/// hanyalah biayanya.
///
/// ── HARGA YANG DIBAYAR ────────────────────────────────────────────────────
/// Urutannya jadi terbalik dari niat semula: barisnya hilang sebelum berkasnya.
/// Bila proses mati tepat di celah itu, berkasnya jadi yatim. Celahnya sempit
/// dan akibatnya beberapa berkas tak terpakai — jauh lebih murah daripada
/// menjenuhkan basis data setiap hari, yang sudah terbukti menjatuhkan seluruh
/// situs.
const SQL_HAPUS_KADALUARSA: &str = r#"
    DELETE FROM chat_messages
    WHERE ctid IN (
        SELECT ctid
        FROM chat_messages
        WHERE sent_at < NOW() - ($1 || ' days')::INTERVAL
        LIMIT $2
    )
    RETURNING media_url
"#;


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
                   CASE WHEN c.buyer_id = $1 THEN d.logo_url ELSE NULL END AS cover_url,
                   -- Pesan LAWAN BICARA yang tiba sesudah terakhir kali kita
                   -- membuka percakapan ini.
                   --
                   -- `sender_id <> $1`: pesan yang baru saja kita kirim bukan
                   -- pesan yang belum kita baca. Tanpa syarat ini, mengirim
                   -- pesan justru menaikkan lencana "belum dibaca" sendiri.
                   -- Hitungan SIMPANAN, bukan dihitung ulang di sini.
                   --
                   -- Dulu baris ini adalah subkueri berkorelasi: satu `COUNT(*)`
                   -- untuk SETIAP percakapan, tiap kali inbox dibuka. Indeksnya
                   -- (migrasi 034) membuat tiap hitungan murah, tapi jumlah
                   -- hitungannya tumbuh bersama jumlah percakapan yang dipunyai
                   -- orang itu — dua ratus percakapan berarti dua ratus
                   -- subkueri untuk satu halaman.
                   --
                   -- Sekarang dinaikkan pada pernyataan yang sama dengan yang
                   -- menyimpan pesannya (lihat `save_message`), jadi membacanya
                   -- tinggal memilih kolom.
                   (CASE WHEN c.buyer_id = $1
                         THEN c.buyer_unread
                         ELSE c.merchant_unread END)::BIGINT AS unread_count
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
    async fn mark_read(&self, room_id: &str, user_id: &str) -> Result<()> {
        let room_b = id_to_vec(room_id)?;
        let user_b = id_to_vec(user_id)?;
        // Satu UPDATE untuk kedua sisi. `WHERE` memastikan hanya peserta
        // percakapan ini yang bisa menandainya — tanpa itu, siapa pun yang tahu
        // id percakapan bisa menghapus lencana milik orang lain.
        exec_drop(
            &self.pool,
            r#"
            UPDATE chats
               SET buyer_read_at    = CASE WHEN buyer_id    = $2 THEN NOW() ELSE buyer_read_at    END,
                   merchant_read_at = CASE WHEN merchant_id = $2 THEN NOW() ELSE merchant_read_at END,
                   -- Penanda waktu TETAP ditulis di samping hitungan yang
                   -- dinolkan. Ia yang membuat hitungannya bisa dibangun ulang
                   -- bila kelak melenceng — angka simpanan tanpa cara
                   -- memverifikasinya adalah angka yang tak bisa dipercaya.
                   buyer_unread    = CASE WHEN buyer_id    = $2 THEN 0 ELSE buyer_unread    END,
                   merchant_unread = CASE WHEN merchant_id = $2 THEN 0 ELSE merchant_unread END
             WHERE id = $1 AND (buyer_id = $2 OR merchant_id = $2)
            "#,
            &[&room_b, &user_b],
        )
        .await?;
        Ok(())
    }

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

    async fn save_message(&self, msg: &GroupMessage) -> Result<bool> {
        let id_arr = ulid_to_arr(&msg.id)?;
        let room_b = id_to_vec(&msg.room_id)?;
        let sender_b = id_to_vec(&msg.sender_id)?;
        let ticket_json: Option<serde_json::Value> = msg
            .ticket_card
            .as_ref()
            .map(|t| serde_json::to_value(t))
            .transpose()?;
        // Id pesan yang dibalas → biner, atau NULL. Id yang tak sah
        // diperlakukan sebagai "tanpa balasan" alih-alih menggagalkan
        // pengiriman: kehilangan kutipan jauh lebih ringan daripada kehilangan
        // pesannya.
        let balas_b: Option<Vec<u8>> = msg
            .reply_to
            .as_ref()
            .and_then(|k| id_to_vec(&k.id).ok());

        // ── SATU perjalanan, bukan tiga ───────────────────────────────────
        // Dulu: SELECT keanggotaan → INSERT pesan → UPDATE percakapan. Tiga
        // kali bolak-balik ke basis data untuk setiap pesan yang dikirim
        // siapa pun, dan pada beban tinggi ketiganya berebut kolam koneksi
        // yang sama dengan yang melayani halaman.
        //
        // Menyatukannya juga menutup celah yang tak kentara: di antara SELECT
        // dan INSERT ada jeda tempat keanggotaan bisa berubah. Di sini
        // `INSERT ... SELECT FROM sah` menjadikan pemeriksaannya bagian dari
        // penulisan itu sendiri — bukan janji yang dibuat sesaat sebelumnya.
        //
        // CTE yang MENGUBAH data di Postgres selalu dijalankan sampai tuntas,
        // dipakai atau tidak oleh kueri utamanya. Jadi `sisip` dan `naik`
        // benar-benar berjalan meski yang dikembalikan hanya `anggota`.
        let rows = exec_rows(
            &self.pool,
            r#"
            WITH sah AS (
                SELECT id FROM chats
                WHERE id = $2 AND ($3 = buyer_id OR $3 = merchant_id)
            ),
            sisip AS (
                INSERT INTO chat_messages
                    (id, chat_id, sender_id, sender_name, msg_type,
                     content, media_url, ticket_card, is_system, sent_at, reply_to_id)
                SELECT $1, $2, $3, $4, $5, $6, $7, $8::jsonb, $9, $10, $11
                FROM sah
                ON CONFLICT (id) DO NOTHING
                RETURNING 1
            ),
            naik AS (
                -- Inilah yang membuat `ORDER BY last_message_at` bermakna.
                -- `GREATEST` menjaga agar pesan yang datang TERLAMBAT — retry
                -- jaringan, atau pesan lama yang baru tersimpan — tak menarik
                -- mundur percakapan yang sudah punya pesan lebih baru.
                UPDATE chats
                SET last_message_at = GREATEST(last_message_at, $10),
                    -- Hitungan belum-dibaca ikut naik DI SINI, di pernyataan
                    -- yang sama dengan yang menyimpan pesannya: tak ada
                    -- perjalanan tambahan, dan mustahil ada keadaan di mana
                    -- pesannya tersimpan tapi hitungannya tidak.
                    --
                    -- Hanya sisi PENERIMA yang naik. Pesan sendiri tak pernah
                    -- jadi pesan belum dibaca.
                    buyer_unread = buyer_unread
                        + CASE WHEN $3 <> buyer_id THEN 1 ELSE 0 END,
                    merchant_unread = merchant_unread
                        + CASE WHEN $3 <> merchant_id THEN 1 ELSE 0 END
                WHERE id = $2 AND EXISTS (SELECT 1 FROM sisip)
                RETURNING 1
            )
            -- Dikembalikan dari `sah`, BUKAN dari `sisip`: pesan berulang
            -- (id yang sama dikirim dua kali karena retry) menghasilkan
            -- `sisip` kosong, dan melaporkannya sebagai "bukan anggota" akan
            -- menampilkan galat izin untuk pesan yang sebenarnya sudah aman
            -- tersimpan.
            SELECT EXISTS (SELECT 1 FROM sah) AS anggota
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
                &balas_b,
            ],
        )
        .await?;

        Ok(rows.first().and_then(|r| r.try_get("anggota").ok()).unwrap_or(false))
    }

    /// Atomic INSERT + limit check via CTE.
    /// Count check dan insert dalam satu query — tidak ada race condition.
    /// Return `Ok(true)` jika berhasil insert, `Ok(false)` jika limit sudah tercapai.
    async fn kutipan(&self, room_id: &str, msg_id: &str) -> Result<Option<KutipanPesan>> {
        let room_b = id_to_vec(room_id)?;
        let msg_b = id_to_vec(msg_id)?;
        let rows = exec_rows(
            &self.pool,
            "SELECT sender_name, content, msg_type FROM chat_messages \
             WHERE id = $1 AND chat_id = $2",
            &[&msg_b, &room_b],
        )
        .await?;
        let Some(r) = rows.first() else {
            return Ok(None);
        };
        let jenis: String = r.try_get("msg_type")?;
        Ok(Some(KutipanPesan {
            id: msg_id.to_string(),
            sender_name: r.try_get("sender_name")?,
            content: KutipanPesan::potong(r.try_get("content")?),
            is_image: jenis == "image",
        }))
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
                       m.content, m.media_url, m.ticket_card, m.is_system, m.sent_at,
                       m.reply_to_id,
                       b.sender_name AS balas_nama, b.content AS balas_isi,
                       b.msg_type    AS balas_jenis
                FROM chat_messages m
                LEFT JOIN chat_messages b ON b.id = m.reply_to_id, cursor c
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
                SELECT m.id, m.chat_id, m.sender_id, m.sender_name, m.msg_type,
                       m.content, m.media_url, m.ticket_card, m.is_system, m.sent_at,
                       m.reply_to_id,
                       b.sender_name AS balas_nama, b.content AS balas_isi,
                       b.msg_type    AS balas_jenis
                FROM chat_messages m
                LEFT JOIN chat_messages b ON b.id = m.reply_to_id
                WHERE m.chat_id = $1
                ORDER BY m.sent_at DESC, m.id DESC LIMIT $2
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
                // Kolomnya bernama `chat_id` sejak migrasi 029
                // (`RENAME COLUMN room_id TO chat_id`). Nama Rust-nya tetap
                // `room_id` karena `GroupMessage` dipakai lintas WebSocket dan
                // web — hanya pembacaannya yang harus mengikuti nama kolom.
                let room_b2: Vec<u8> = row.try_get("chat_id")?;
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
                // Kutipan hanya terbentuk bila pesan asalnya MASIH ADA.
                // `reply_to_id` yang terisi tapi tanpa baris pasangan berarti
                // pesannya sudah dibuang retensi — balasannya tetap tampil,
                // tanpa kutipan.
                let reply_to = row
                    .try_get::<_, Option<Vec<u8>>>("reply_to_id")
                    .ok()
                    .flatten()
                    .and_then(|b| bin_to_ulid(b).ok())
                    .and_then(|id| {
                        let nama = col_opt_str(row, "balas_nama").ok().flatten()?;
                        let isi = col_opt_str(row, "balas_isi").ok().flatten()?;
                        let jenis = col_opt_str(row, "balas_jenis").ok().flatten()?;
                        Some(KutipanPesan {
                            id,
                            sender_name: nama,
                            content: KutipanPesan::potong(&isi),
                            is_image: jenis == "image",
                        })
                    });

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
                    reply_to,
                })
            })
            .collect::<Result<_>>()?;
        msgs.reverse();
        Ok((msgs, has_more))
    }

    // ── Retensi ───────────────────────────────────────────────────────────────

    async fn hapus_kadaluarsa(&self, hari: i64, batas: i64) -> Result<(u64, Vec<String>)> {
        let rows = exec_rows(
            &self.pool,
            SQL_HAPUS_KADALUARSA,
            &[&hari.to_string(), &batas],
        )
        .await?;
        // Jumlah baris yang DIKEMBALIKAN adalah jumlah yang terhapus; sebagian
        // besar di antaranya tak punya berkas sama sekali.
        let jumlah = rows.len() as u64;
        let media = rows
            .iter()
            .filter_map(|r| col_opt_str(r, "media_url").ok().flatten())
            .filter(|u| !u.is_empty())
            .collect();
        Ok((jumlah, media))
    }

}

