//! repository/post.rs
//!
//! Akses data untuk Marketplace C2C: tabel `posts`, `post_likes`, `post_comments`.

use anyhow::Result;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::post::{CreatePostRequest, PaginatedComments, PaginatedPosts, Post, PostComment};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Row mappers ───────────────────────────────────────────────────────────────

/// Dipakai listing DAN detail — keduanya memproyeksikan `liked_by_viewer` lewat
/// subquery berparameter `$viewer_id` (NULL bila viewer anonim/tak dikirim),
/// jadi bentuk row-nya identik dan satu mapper cukup.
fn row_to_post(row: &Row) -> Result<Post> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    let images_json: Option<JsonValue> = row.try_get("images")?;
    let images: Vec<JsonValue> = match images_json {
        Some(JsonValue::Array(a)) => a,
        _ => vec![],
    };
    Ok(Post {
        id: bin_to_ulid(id_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        user_name: row.try_get("user_name")?,
        user_avatar: row.try_get("user_avatar")?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        price: row.try_get("price")?,
        condition: row.try_get("condition")?,
        category: row.try_get("category")?,
        city: row.try_get("city")?,
        images,
        status: row.try_get("status")?,
        like_count: row.try_get("like_count")?,
        comment_count: row.try_get("comment_count")?,
        liked_by_viewer: row.try_get("liked_by_viewer")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_comment(row: &Row) -> Result<PostComment> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let post_bytes: Vec<u8> = row.try_get("post_id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    Ok(PostComment {
        id: bin_to_ulid(id_bytes)?,
        post_id: bin_to_ulid(post_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        user_name: row.try_get("user_name")?,
        user_avatar: row.try_get("user_avatar")?,
        body: row.try_get("body")?,
        created_at: row.try_get("created_at")?,
    })
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Kolom dasar + join user + liked_by_viewer, dipakai list & detail.
/// `$1` = viewer_id (bytea, boleh NULL untuk anonim).
const POST_SELECT: &str = r#"
    SELECT
        p.id, p.user_id, p.kind, p.title, p.description, p.price, p.condition,
        p.category, p.city, p.images, p.status, p.like_count, p.comment_count,
        p.created_at, p.updated_at,
        COALESCE(u.name, 'Unknown') AS user_name,
        COALESCE(u.avatar_url, '')  AS user_avatar,
        CASE WHEN $1::bytea IS NULL THEN NULL
             ELSE EXISTS(SELECT 1 FROM post_likes pl WHERE pl.post_id = p.id AND pl.user_id = $1)
        END AS liked_by_viewer
    FROM posts p
    JOIN users u ON u.id = p.user_id
"#;

// ── Repository trait ──────────────────────────────────────────────────────────

#[async_trait]
pub trait PostRepository: Send + Sync {
    async fn create(&self, user_id: &str, req: &CreatePostRequest) -> Result<Post>;
    #[allow(clippy::too_many_arguments)]
    async fn list_feed(
        &self,
        viewer_id: Option<&str>,
        kind: Option<&str>,
        category: Option<&str>,
        city: Option<&str>,
        q: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedPosts>;
    async fn get_by_id(&self, id: &str, viewer_id: Option<&str>) -> Result<Option<Post>>;
    /// Seluruh postingan MILIK `user_id`, SEMUA status (aktif/terjual/
    /// diarsipkan) — beda dari `list_feed` yang cuma `status='active'`.
    /// Pemiliknya sendiri harus bisa melihat postingan yang sudah ditandai
    /// terjual/diarsipkan, bukan cuma yang masih tayang publik.
    async fn list_mine(&self, user_id: &str, page: i64, per_page: i64) -> Result<PaginatedPosts>;
    /// Toggle like (murni self-contained: INSERT bila belum ada, DELETE bila
    /// sudah ada). Return `true` bila status akhirnya LIKED.
    async fn toggle_like(&self, post_id: &str, user_id: &str) -> Result<bool>;
    async fn add_comment(&self, post_id: &str, user_id: &str, body: &str) -> Result<PostComment>;
    async fn list_comments(&self, post_id: &str, page: i64, per_page: i64) -> Result<PaginatedComments>;
    /// Owner-enforced — `Ok(false)` bila baris bukan milik `user_id` atau sudah terhapus.
    async fn soft_delete_comment(&self, comment_id: &str, user_id: &str) -> Result<bool>;
    /// Owner-enforced.
    async fn update_status(&self, post_id: &str, user_id: &str, status: &str) -> Result<bool>;
    /// Owner-enforced.
    async fn soft_delete_post(&self, post_id: &str, user_id: &str) -> Result<bool>;
}

// ── Postgres implementation ───────────────────────────────────────────────────

pub struct PgPostRepository {
    pool: Pool,
}

impl PgPostRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PostRepository for PgPostRepository {
    async fn create(&self, user_id: &str, req: &CreatePostRequest) -> Result<Post> {
        let id = new_ulid();
        let id_bytes = ulid_to_vec(&id)?;
        let user_bytes = id_to_vec(user_id)?;
        let images_val = JsonValue::Array(req.images.clone());

        exec_drop(
            &self.pool,
            r#"
            INSERT INTO posts
                (id, user_id, kind, title, description, price, condition, category, city, images)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            &[
                &id_bytes.as_slice(),
                &user_bytes.as_slice(),
                &req.kind,
                &req.title,
                &req.description,
                &req.price,
                &req.condition,
                &req.category,
                &req.city,
                &images_val,
            ],
        )
        .await?;

        self.get_by_id(&id, Some(user_id))
            .await?
            .ok_or_else(|| anyhow::anyhow!("post baru tak ditemukan setelah INSERT"))
    }

    async fn list_feed(
        &self,
        viewer_id: Option<&str>,
        kind: Option<&str>,
        category: Option<&str>,
        city: Option<&str>,
        q: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> Result<PaginatedPosts> {
        let viewer_bytes: Option<Vec<u8>> = viewer_id.map(id_to_vec).transpose()?;
        let per_page = per_page.clamp(1, 50);
        let page = page.max(1);
        let offset = (page - 1) * per_page;
        let q_like = q
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));

        // NB: list_query dan count_query numbering param-nya SENGAJA TIDAK
        // dibagi dari satu string `where_clause` bersama — count_query tak
        // butuh `viewer_id` sama sekali (tak ada `liked_by_viewer` di situ),
        // dan menyisakan placeholder `$1` yang tak dirujuk di teks query
        // adalah persis pola "could not determine data type of parameter $N"
        // yang sudah pernah menggigit proyek ini (lihat AUDIT PRA-DEPLOY di
        // memory eticketing-live-stack). Dua query, dua penomoran independen.
        let list_query = format!(
            "{POST_SELECT} \
             WHERE p.status = 'active' AND p.deleted_at IS NULL \
               AND ($2::varchar IS NULL OR p.kind = $2) \
               AND ($3::varchar IS NULL OR p.category = $3) \
               AND ($4::varchar IS NULL OR p.city = $4) \
               AND ($5::text IS NULL OR p.title ILIKE $5) \
             ORDER BY p.created_at DESC LIMIT $6 OFFSET $7"
        );
        let count_query = "
            SELECT COUNT(*) AS n FROM posts p
            WHERE p.status = 'active' AND p.deleted_at IS NULL
              AND ($1::varchar IS NULL OR p.kind = $1)
              AND ($2::varchar IS NULL OR p.category = $2)
              AND ($3::varchar IS NULL OR p.city = $3)
              AND ($4::text IS NULL OR p.title ILIKE $4)
        ";

        let rows = exec_rows(
            &self.pool,
            &list_query,
            &[
                &viewer_bytes.as_deref(),
                &kind,
                &category,
                &city,
                &q_like,
                &per_page,
                &offset,
            ],
        )
        .await?;

        let total_row = exec_one(
            &self.pool,
            count_query,
            &[&kind, &category, &city, &q_like],
        )
        .await?;
        let total: i64 = total_row.try_get("n")?;

        let data = rows.iter().map(row_to_post).collect::<Result<Vec<_>>>()?;
        let total_pages = if total == 0 { 0 } else { (total + per_page - 1) / per_page };

        Ok(PaginatedPosts { data, total, page, per_page, total_pages })
    }

    async fn list_mine(&self, user_id: &str, page: i64, per_page: i64) -> Result<PaginatedPosts> {
        let uid = id_to_vec(user_id)?;
        let per_page = per_page.clamp(1, 50);
        let page = page.max(1);
        let offset = (page - 1) * per_page;

        // `$1` (viewer_id di POST_SELECT) dan pemilik yang difilter SAMA di
        // sini dengan sengaja — pemilik selalu melihat status suka miliknya
        // sendiri atas postingannya sendiri, dan itu tak masalah.
        let list_query = format!(
            "{POST_SELECT} WHERE p.user_id = $1 AND p.deleted_at IS NULL \
             ORDER BY p.created_at DESC LIMIT $2 OFFSET $3"
        );
        let rows = exec_rows(&self.pool, &list_query, &[&uid.as_slice(), &per_page, &offset]).await?;

        let total_row = exec_one(
            &self.pool,
            "SELECT COUNT(*) AS n FROM posts WHERE user_id = $1 AND deleted_at IS NULL",
            &[&uid.as_slice()],
        )
        .await?;
        let total: i64 = total_row.try_get("n")?;

        let data = rows.iter().map(row_to_post).collect::<Result<Vec<_>>>()?;
        let total_pages = if total == 0 { 0 } else { (total + per_page - 1) / per_page };

        Ok(PaginatedPosts { data, total, page, per_page, total_pages })
    }

    async fn get_by_id(&self, id: &str, viewer_id: Option<&str>) -> Result<Option<Post>> {
        let id_bytes = id_to_vec(id)?;
        let viewer_bytes: Option<Vec<u8>> = viewer_id.map(id_to_vec).transpose()?;
        let query = format!("{POST_SELECT} WHERE p.id = $2 AND p.deleted_at IS NULL");
        let row = exec_first(
            &self.pool,
            &query,
            &[&viewer_bytes.as_deref(), &id_bytes.as_slice()],
        )
        .await?;
        row.map(|r| row_to_post(&r)).transpose()
    }

    async fn toggle_like(&self, post_id: &str, user_id: &str) -> Result<bool> {
        let pid = id_to_vec(post_id)?;
        let uid = id_to_vec(user_id)?;
        let inserted = exec_drop(
            &self.pool,
            "INSERT INTO post_likes (post_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&pid.as_slice(), &uid.as_slice()],
        )
        .await?;
        if inserted > 0 {
            return Ok(true);
        }
        exec_drop(
            &self.pool,
            "DELETE FROM post_likes WHERE post_id = $1 AND user_id = $2",
            &[&pid.as_slice(), &uid.as_slice()],
        )
        .await?;
        Ok(false)
    }

    async fn add_comment(&self, post_id: &str, user_id: &str, body: &str) -> Result<PostComment> {
        let id = new_ulid();
        let id_bytes = ulid_to_vec(&id)?;
        let pid = id_to_vec(post_id)?;
        let uid = id_to_vec(user_id)?;

        exec_drop(
            &self.pool,
            "INSERT INTO post_comments (id, post_id, user_id, body) VALUES ($1, $2, $3, $4)",
            &[&id_bytes.as_slice(), &pid.as_slice(), &uid.as_slice(), &body],
        )
        .await?;

        let row = exec_one(
            &self.pool,
            r#"
            SELECT c.id, c.post_id, c.user_id, c.body, c.created_at,
                   COALESCE(u.name, 'Unknown') AS user_name,
                   COALESCE(u.avatar_url, '')  AS user_avatar
            FROM post_comments c
            JOIN users u ON u.id = c.user_id
            WHERE c.id = $1
            "#,
            &[&id_bytes.as_slice()],
        )
        .await?;
        row_to_comment(&row)
    }

    async fn list_comments(&self, post_id: &str, page: i64, per_page: i64) -> Result<PaginatedComments> {
        let pid = id_to_vec(post_id)?;
        let per_page = per_page.clamp(1, 50);
        let page = page.max(1);
        let offset = (page - 1) * per_page;

        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT c.id, c.post_id, c.user_id, c.body, c.created_at,
                   COALESCE(u.name, 'Unknown') AS user_name,
                   COALESCE(u.avatar_url, '')  AS user_avatar
            FROM post_comments c
            JOIN users u ON u.id = c.user_id
            WHERE c.post_id = $1 AND c.deleted_at IS NULL
            ORDER BY c.created_at ASC
            LIMIT $2 OFFSET $3
            "#,
            &[&pid.as_slice(), &per_page, &offset],
        )
        .await?;

        let total_row = exec_one(
            &self.pool,
            "SELECT COUNT(*) AS n FROM post_comments WHERE post_id = $1 AND deleted_at IS NULL",
            &[&pid.as_slice()],
        )
        .await?;
        let total: i64 = total_row.try_get("n")?;

        let data = rows.iter().map(row_to_comment).collect::<Result<Vec<_>>>()?;
        let total_pages = if total == 0 { 0 } else { (total + per_page - 1) / per_page };

        Ok(PaginatedComments { data, total, page, per_page, total_pages })
    }

    async fn soft_delete_comment(&self, comment_id: &str, user_id: &str) -> Result<bool> {
        let cid = id_to_vec(comment_id)?;
        let uid = id_to_vec(user_id)?;
        let affected = exec_drop(
            &self.pool,
            "UPDATE post_comments SET deleted_at = NOW() \
             WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
            &[&cid.as_slice(), &uid.as_slice()],
        )
        .await?;
        Ok(affected == 1)
    }

    async fn update_status(&self, post_id: &str, user_id: &str, status: &str) -> Result<bool> {
        let pid = id_to_vec(post_id)?;
        let uid = id_to_vec(user_id)?;
        let affected = exec_drop(
            &self.pool,
            "UPDATE posts SET status = $3, updated_at = NOW() \
             WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
            &[&pid.as_slice(), &uid.as_slice(), &status],
        )
        .await?;
        Ok(affected == 1)
    }

    async fn soft_delete_post(&self, post_id: &str, user_id: &str) -> Result<bool> {
        let pid = id_to_vec(post_id)?;
        let uid = id_to_vec(user_id)?;
        let affected = exec_drop(
            &self.pool,
            "UPDATE posts SET deleted_at = NOW() \
             WHERE id = $1 AND user_id = $2 AND deleted_at IS NULL",
            &[&pid.as_slice(), &uid.as_slice()],
        )
        .await?;
        Ok(affected == 1)
    }
}
