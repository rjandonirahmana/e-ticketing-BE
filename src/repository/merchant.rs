use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::merchant::{
    FollowedMerchant, MerchantDetail, MerchantFollower, MerchantPublicProfile, MerchantReviewItem,
    MerchantReviewSummary, MerchantSearchItem, UserPublicProfile, UserReviewItem,
};
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

static MERCHANT_COLS: &str = r#"
    user_id,
    store_name,
    description,
    logo_url,
    header_url,
    verified,
    created_at,
    updated_at
"#;

static FIND_BY_USER_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM merchant_details WHERE user_id = $1",
        MERCHANT_COLS
    )
});

static INSERT_MERCHANT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO merchant_details (user_id, store_name, description, logo_url) \
         VALUES ($1, $2, $3, $4) RETURNING {}",
        MERCHANT_COLS
    )
});

static UPDATE_MERCHANT: &str = r#"
    UPDATE merchant_details
       SET store_name  = COALESCE($2, store_name),
           description = COALESCE($3, description),
           logo_url    = COALESCE($4, logo_url),
           header_url  = COALESCE($5, header_url)
     WHERE user_id = $1
"#;

#[async_trait]
pub trait MerchantRepository: Send + Sync {
    async fn find(&self, user_id: &str) -> Result<Option<MerchantDetail>>;

    async fn create(
        &self,
        user_id: &str,
        store_name: &str,
        description: Option<&str>,
        logo_url: &str,
    ) -> Result<MerchantDetail>;

    async fn update(
        &self,
        user_id: &str,
        store_name: Option<&str>,
        description: Option<&str>,
        logo_url: Option<&str>,
        header_url: Option<&str>,
    ) -> Result<()>;

    // ── Profil publik + rating & follow ──────────────────────────────────────

    /// Profil merchant untuk halaman publik: info toko + agregat follower,
    /// jumlah product aktif, rata-rata & jumlah rating (satu query).
    async fn public_profile(&self, merchant_id: &str) -> Result<Option<MerchantPublicProfile>>;

    /// Ringkasan rating: avg, total, distribusi per bintang (FILTER agregat).
    async fn review_summary(&self, merchant_id: &str) -> Result<MerchantReviewSummary>;

    /// Daftar ulasan terbaru (join nama user), paginasi limit/offset.
    async fn list_reviews(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantReviewItem>>;

    /// Simpan/perbarui ulasan user untuk merchant (satu ulasan per user).
    /// HANYA bila user punya ≥1 order 'paid' dengan merchant ini (order_id ikut
    /// dicatat). Mengembalikan jumlah baris terpengaruh: 0 = tak memenuhi syarat
    /// (belum pernah menyelesaikan pesanan) → caller menolak.
    async fn upsert_review(
        &self,
        merchant_id: &str,
        user_id: &str,
        rating: i16,
        comment: &str,
    ) -> Result<u64>;

    /// Apakah user pernah menyelesaikan (status 'paid') minimal 1 pesanan dengan
    /// merchant ini — syarat boleh menulis ulasan (dipakai gating form di UI).
    async fn has_purchased(&self, merchant_id: &str, user_id: &str) -> Result<bool>;

    /// Follow (true) / unfollow (false). Follow hanya berlaku untuk target yang
    /// memang merchant (guard EXISTS di SQL — bukan sekadar percaya input).
    async fn set_follow(&self, merchant_id: &str, follower_id: &str, follow: bool) -> Result<()>;

    async fn is_following(&self, merchant_id: &str, follower_id: &str) -> Result<bool>;

    /// Daftar follower merchant (user), terbaru dulu. Paginasi limit/offset.
    async fn list_followers(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantFollower>>;

    /// Jumlah follower merchant.
    async fn count_followers(&self, merchant_id: &str) -> Result<i64>;

    /// Toko yang DIIKUTI seorang pengguna (kebalikan `list_followers`).
    async fn list_following(
        &self,
        follower_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FollowedMerchant>>;

    async fn count_following(&self, follower_id: &str) -> Result<i64>;

    /// Cari merchant berdasarkan nama toko (ILIKE), terverifikasi dulu.
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<MerchantSearchItem>>;

    /// Merchant acak (overlay pencarian sebelum user mengetik). ORDER BY
    /// random() aman: merchant_details berukuran kecil (satu baris per toko).
    async fn random(&self, limit: i64) -> Result<Vec<MerchantSearchItem>>;

    /// Profil publik user biasa (nama + jumlah following/reviews/stories).
    async fn user_public(&self, user_id: &str) -> Result<Option<UserPublicProfile>>;

    /// Ulasan yang DITULIS user (join merchant), terbaru dulu, paginasi.
    async fn list_user_reviews(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UserReviewItem>>;
}

#[derive(Clone)]
pub struct PgMerchantRepository {
    pool: Pool,
}

impl PgMerchantRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_merchant(row: &Row) -> Result<MerchantDetail> {
        let id_bytes: Vec<u8> = row.try_get("user_id").context("user_id")?;
        Ok(MerchantDetail {
            user_id: bin_to_ulid(id_bytes)?,
            store_name: row.try_get("store_name").context("store_name")?,
            description: row.try_get("description").context("description")?,
            logo_url: row.try_get("logo_url").context("logo_url")?,
            header_url: row.try_get("header_url").context("header_url")?,
            verified: row.try_get("verified").context("verified")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }
}

#[async_trait]
impl MerchantRepository for PgMerchantRepository {
    async fn find(&self, user_id: &str) -> Result<Option<MerchantDetail>> {
        let id_vec = id_to_vec(user_id)?;
        let row = exec_first(&self.pool, &FIND_BY_USER_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_merchant).transpose()
    }

    async fn create(
        &self,
        user_id: &str,
        store_name: &str,
        description: Option<&str>,
        logo_url: &str,
    ) -> Result<MerchantDetail> {
        let id_vec = id_to_vec(user_id)?;
        let row = exec_one(
            &self.pool,
            &INSERT_MERCHANT,
            &[&id_vec, &store_name, &description, &logo_url],
        )
        .await?;
        Self::row_to_merchant(&row)
    }

    async fn update(
        &self,
        user_id: &str,
        store_name: Option<&str>,
        description: Option<&str>,
        logo_url: Option<&str>,
        header_url: Option<&str>,
    ) -> Result<()> {
        let id_vec = id_to_vec(user_id)?;
        exec_drop(
            &self.pool,
            UPDATE_MERCHANT,
            &[&id_vec, &store_name, &description, &logo_url, &header_url],
        )
        .await?;
        Ok(())
    }

    // ── Profil publik + rating & follow ──────────────────────────────────────

    async fn public_profile(&self, merchant_id: &str) -> Result<Option<MerchantPublicProfile>> {
        let id_vec = id_to_vec(merchant_id)?;
        let row = exec_first(
            &self.pool,
            r#"
            SELECT
                md.user_id, md.store_name, md.description, md.logo_url,
                md.header_url, md.verified,
                (SELECT COUNT(*)::BIGINT FROM merchant_follows f
                  WHERE f.merchant_id = md.user_id)                          AS followers,
                (SELECT COUNT(*)::BIGINT FROM products e
                  WHERE e.merchant_id = md.user_id AND e.status = 'active')  AS products_count,
                -- Rating agregat: dibaca langsung dari kolom denormalisasi
                -- (dijaga trigger reviews_rating_buckets) → tanpa scan `reviews`.
                md.total_avg_review                                          AS rating_avg,
                md.total_review                                             AS rating_count
            FROM merchant_details md
            WHERE md.user_id = $1
            "#,
            &[&id_vec],
        )
        .await?;
        row.map(|r| {
            let id_bytes: Vec<u8> = r.try_get("user_id")?;
            Ok(MerchantPublicProfile {
                merchant_id: bin_to_ulid(id_bytes)?,
                store_name: r.try_get("store_name")?,
                description: r.try_get("description")?,
                logo_url: r.try_get("logo_url")?,
                header_url: r.try_get("header_url")?,
                verified: r.try_get("verified")?,
                followers: r.try_get("followers")?,
                products_count: r.try_get("products_count")?,
                rating_avg: r.try_get("rating_avg")?,
                rating_count: r.try_get("rating_count")?,
            })
        })
        .transpose()
    }

    async fn review_summary(&self, merchant_id: &str) -> Result<MerchantReviewSummary> {
        let id_vec = id_to_vec(merchant_id)?;
        // Ringkasan (avg/total/distribusi) dibaca dari kolom denormalisasi
        // merchant_details — satu row, tanpa agregat scan `reviews`. `reviews`
        // hanya dipakai list_reviews (tampilan daftar). store_name None → merchant
        // tidak ada (dipakai server fn untuk membedakan "tidak ditemukan").
        let row = exec_first(
            &self.pool,
            r#"
            SELECT store_name,
                   total_avg_review AS avg,
                   total_review     AS total,
                   review_1 AS d1, review_2 AS d2, review_3 AS d3,
                   review_4 AS d4, review_5 AS d5
            FROM merchant_details
            WHERE user_id = $1
            "#,
            &[&id_vec],
        )
        .await?;
        match row {
            None => Ok(MerchantReviewSummary {
                store_name: None,
                avg: 0.0,
                total: 0,
                dist: [0; 5],
            }),
            Some(r) => Ok(MerchantReviewSummary {
                store_name: Some(r.try_get("store_name")?),
                avg: r.try_get("avg")?,
                total: r.try_get("total")?,
                dist: [
                    r.try_get("d1")?,
                    r.try_get("d2")?,
                    r.try_get("d3")?,
                    r.try_get("d4")?,
                    r.try_get("d5")?,
                ],
            }),
        }
    }

    async fn list_reviews(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantReviewItem>> {
        let id_vec = id_to_vec(merchant_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT r.user_id                     AS uid,
                   COALESCE(u.name, 'Pengguna') AS user_name,
                   r.rating::INT4               AS rating,
                   r.comment,
                   r.created_at
            FROM reviews r
            JOIN users u ON u.id = r.user_id
            WHERE r.merchant_id = $1
            ORDER BY r.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            &[&id_vec, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let uid: Vec<u8> = r.try_get("uid")?;
                Ok(MerchantReviewItem {
                    user_id: bin_to_ulid(uid)?,
                    user_name: r.try_get("user_name")?,
                    rating: r.try_get("rating")?,
                    comment: r.try_get("comment")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn user_public(&self, user_id: &str) -> Result<Option<UserPublicProfile>> {
        let uid = id_to_vec(user_id)?;
        let row = exec_first(
            &self.pool,
            r#"
            SELECT
                u.name,
                (SELECT COUNT(*)::BIGINT FROM merchant_follows f
                  WHERE f.follower_id = u.id)             AS following,
                (SELECT COUNT(*)::BIGINT FROM reviews r
                  WHERE r.user_id = u.id)                 AS reviews,
                (SELECT COUNT(*)::BIGINT FROM stories s
                  WHERE s.user_id = u.id)                 AS stories
            FROM users u
            WHERE u.id = $1
            "#,
            &[&uid],
        )
        .await?;
        row.map(|r| {
            Ok(UserPublicProfile {
                user_id: user_id.to_string(),
                name: r.try_get("name")?,
                following: r.try_get("following")?,
                reviews: r.try_get("reviews")?,
                stories: r.try_get("stories")?,
            })
        })
        .transpose()
    }

    async fn list_user_reviews(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UserReviewItem>> {
        let uid = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT r.merchant_id, md.store_name, r.rating::INT4 AS rating,
                   r.comment, r.created_at
            FROM   reviews r
            JOIN   merchant_details md ON md.user_id = r.merchant_id
            WHERE  r.user_id = $1
            ORDER BY r.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            &[&uid, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let mid: Vec<u8> = r.try_get("merchant_id")?;
                Ok(UserReviewItem {
                    merchant_id: bin_to_ulid(mid)?,
                    store_name: r.try_get("store_name")?,
                    rating: r.try_get("rating")?,
                    comment: r.try_get("comment")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn upsert_review(
        &self,
        merchant_id: &str,
        user_id: &str,
        rating: i16,
        comment: &str,
    ) -> Result<u64> {
        let mid = id_to_vec(merchant_id)?;
        let uid = id_to_vec(user_id)?;
        // Insert HANYA bila SELECT menemukan order 'paid' user↔merchant
        // (order_id ikut dicatat, order terbaru dulu). Tak ada order → 0 baris →
        // caller menolak. order_id tetap saat DO UPDATE (edit rating/komentar).
        let affected = exec_drop(
            &self.pool,
            r#"
            INSERT INTO reviews (merchant_id, user_id, rating, comment, order_id)
            SELECT $1, $2, $3, $4, o.id
            FROM   orders o
            JOIN   cart_items     ci ON ci.cart_id = o.cart_id
            JOIN   product_variants tv ON tv.id = ci.ticket_variant_id
            JOIN   products         e  ON e.id = tv.event_id
            WHERE  o.customer_id = $2
              AND  e.merchant_id = $1
              AND  o.status = 'paid'
            ORDER BY o.paid_at DESC NULLS LAST
            LIMIT 1
            ON CONFLICT (merchant_id, user_id) DO UPDATE
                SET rating = EXCLUDED.rating,
                    comment = EXCLUDED.comment,
                    updated_at = NOW()
            "#,
            &[&mid, &uid, &rating, &comment],
        )
        .await?;
        Ok(affected)
    }

    async fn has_purchased(&self, merchant_id: &str, user_id: &str) -> Result<bool> {
        let mid = id_to_vec(merchant_id)?;
        let uid = id_to_vec(user_id)?;
        let row = exec_one(
            &self.pool,
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM   orders o
                JOIN   cart_items     ci ON ci.cart_id = o.cart_id
                JOIN   product_variants tv ON tv.id = ci.ticket_variant_id
                JOIN   products         e  ON e.id = tv.event_id
                WHERE  o.customer_id = $2
                  AND  e.merchant_id = $1
                  AND  o.status = 'paid'
            ) AS ok
            "#,
            &[&mid, &uid],
        )
        .await?;
        Ok(row.try_get("ok")?)
    }

    async fn set_follow(&self, merchant_id: &str, follower_id: &str, follow: bool) -> Result<()> {
        let mid = id_to_vec(merchant_id)?;
        let fid = id_to_vec(follower_id)?;
        if follow {
            exec_drop(
                &self.pool,
                r#"
                INSERT INTO merchant_follows (merchant_id, follower_id)
                SELECT $1, $2
                WHERE EXISTS (SELECT 1 FROM merchant_details WHERE user_id = $1)
                ON CONFLICT DO NOTHING
                "#,
                &[&mid, &fid],
            )
            .await?;
        } else {
            exec_drop(
                &self.pool,
                "DELETE FROM merchant_follows WHERE merchant_id = $1 AND follower_id = $2",
                &[&mid, &fid],
            )
            .await?;
        }
        Ok(())
    }

    async fn is_following(&self, merchant_id: &str, follower_id: &str) -> Result<bool> {
        let mid = id_to_vec(merchant_id)?;
        let fid = id_to_vec(follower_id)?;
        let row = exec_first(
            &self.pool,
            "SELECT 1 AS x FROM merchant_follows WHERE merchant_id = $1 AND follower_id = $2",
            &[&mid, &fid],
        )
        .await?;
        Ok(row.is_some())
    }

    async fn list_followers(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantFollower>> {
        let mid = id_to_vec(merchant_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            -- role='merchant' ⟺ punya merchant_details (dijamin trigger
            -- users_ensure_merchant_details, migrasi 016) → tak perlu EXISTS.
            SELECT u.id AS uid, u.name, u.role, f.created_at
            FROM   merchant_follows f
            JOIN   users u ON u.id = f.follower_id
            WHERE  f.merchant_id = $1
            ORDER BY f.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            &[&mid, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let uid: Vec<u8> = r.try_get("uid")?;
                Ok(MerchantFollower {
                    user_id: bin_to_ulid(uid)?,
                    name: r.try_get("name")?,
                    role: r.try_get("role")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    /// Toko yang diikuti pengguna ini.
    ///
    /// JOIN ke `merchant_details`, bukan ke `users`: yang harus tampil di daftar
    /// adalah nama TOKO dan logonya. `users.name` adalah nama pemilik akun, dan
    /// keduanya kerap berbeda — menampilkan yang salah membuat daftar ini tak
    /// bisa dicocokkan dengan toko yang tampak di halaman produk.
    ///
    /// INNER JOIN disengaja: baris `merchant_follows` yang tokonya sudah tak
    /// punya `merchant_details` (akun dihapus) ikut tersaring keluar, alih-alih
    /// muncul sebagai baris kosong tanpa nama yang tak bisa diklik.
    ///
    /// Index `idx_merchant_follows_follower (follower_id)` dari migrasi 017
    /// sudah menopang WHERE-nya — tak perlu index baru.
    async fn list_following(
        &self,
        follower_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<FollowedMerchant>> {
        let fid = id_to_vec(follower_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT f.merchant_id AS mid,
                   d.store_name,
                   d.logo_url,
                   COALESCE(d.verified, FALSE) AS verified,
                   f.created_at
            FROM   merchant_follows f
            JOIN   merchant_details d ON d.user_id = f.merchant_id
            WHERE  f.follower_id = $1
            ORDER BY f.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            &[&fid, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let mid: Vec<u8> = r.try_get("mid")?;
                Ok(FollowedMerchant {
                    merchant_id: bin_to_ulid(mid)?,
                    store_name: r.try_get("store_name")?,
                    logo_url: r.try_get("logo_url")?,
                    verified: r.try_get("verified")?,
                    followed_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn count_following(&self, follower_id: &str) -> Result<i64> {
        let fid = id_to_vec(follower_id)?;
        let row = exec_one(
            &self.pool,
            "SELECT COUNT(*)::BIGINT AS n FROM merchant_follows WHERE follower_id = $1",
            &[&fid],
        )
        .await?;
        Ok(row.try_get("n")?)
    }

    async fn count_followers(&self, merchant_id: &str) -> Result<i64> {
        let mid = id_to_vec(merchant_id)?;
        let row = exec_one(
            &self.pool,
            "SELECT COUNT(*)::BIGINT AS c FROM merchant_follows WHERE merchant_id = $1",
            &[&mid],
        )
        .await?;
        Ok(row.try_get("c")?)
    }

    async fn search(&self, query: &str, limit: i64) -> Result<Vec<MerchantSearchItem>> {
        // Escape wildcard ILIKE agar input user literal (bukan pola).
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT user_id, store_name, logo_url, verified
            FROM   merchant_details
            WHERE  store_name ILIKE $1
            ORDER BY verified DESC, store_name ASC
            LIMIT $2
            "#,
            &[&pattern, &limit],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let id_bytes: Vec<u8> = r.try_get("user_id")?;
                Ok(MerchantSearchItem {
                    merchant_id: bin_to_ulid(id_bytes)?,
                    store_name: r.try_get("store_name")?,
                    logo_url: r.try_get("logo_url")?,
                    verified: r.try_get("verified")?,
                })
            })
            .collect()
    }

    async fn random(&self, limit: i64) -> Result<Vec<MerchantSearchItem>> {
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT user_id, store_name, logo_url, verified
            FROM   merchant_details
            WHERE  store_name <> ''
            ORDER BY random()
            LIMIT $1
            "#,
            &[&limit],
        )
        .await?;
        rows.iter()
            .map(|r| {
                let id_bytes: Vec<u8> = r.try_get("user_id")?;
                Ok(MerchantSearchItem {
                    merchant_id: bin_to_ulid(id_bytes)?,
                    store_name: r.try_get("store_name")?,
                    logo_url: r.try_get("logo_url")?,
                    verified: r.try_get("verified")?,
                })
            })
            .collect()
    }
}
