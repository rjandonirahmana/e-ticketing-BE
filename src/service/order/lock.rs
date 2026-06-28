use std::sync::Arc;

use tokio::time::{sleep, Duration};

use crate::utils::error::AppError;
use crate::utils::ulid::new_ulid;
use crate::repository::order::LUA_RELEASE;

pub(super) const LOCK_TTL_MS: u64 = 25_000;
pub(super) const LOCK_RETRIES: u8 = 3;
pub(super) const LOCK_DELAY_MS: u64 = 80;

#[derive(Debug, Clone, Default)]
pub enum QueueMode {
    #[default]
    Off,
    #[allow(dead_code)]
    Soft { max_rps: u32, window_ms: u64 },
    #[allow(dead_code)]
    Strict,
}

pub(crate) struct VariantLockGuard {
    redis: redis::aio::ConnectionManager,
    pub(crate) acquired_keys: Arc<Vec<String>>,
    pub(crate) lock_val: Arc<str>,
}

impl VariantLockGuard {
    pub async fn acquire(
        redis: redis::aio::ConnectionManager,
        variant_ids: &[&str],
        is_premium: bool,
    ) -> Result<Self, AppError> {
        let mut sorted: Vec<&str> = variant_ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let retries = if is_premium { 6u8 } else { LOCK_RETRIES };
        let delay_ms = if is_premium { 0u64 } else { LOCK_DELAY_MS };

        let lock_val: Arc<str> = Arc::from(new_ulid().as_str());
        let keys: Vec<String> = sorted
            .iter()
            .map(|id| format!("order:lock:variant:{}", id))
            .collect();

        let mut acquired: Vec<String> = Vec::with_capacity(keys.len());
        let mut redis_conn = redis;

        for key in &keys {
            let mut ok = false;
            for attempt in 0..=retries {
                let res: redis::RedisResult<Option<String>> = redis::cmd("SET")
                    .arg(key)
                    .arg(lock_val.as_ref())
                    .arg("NX")
                    .arg("PX")
                    .arg(LOCK_TTL_MS)
                    .query_async(&mut redis_conn)
                    .await;

                match res {
                    Ok(Some(_)) => {
                        ok = true;
                        break;
                    }
                    Ok(None) if attempt < retries => {
                        if delay_ms > 0 {
                            sleep(Duration::from_millis(delay_ms)).await;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        release_keys(&mut redis_conn, &acquired, &lock_val).await;
                        return Err(AppError::Internal(anyhow::anyhow!("Redis lock error: {e}")));
                    }
                }
            }

            if !ok {
                release_keys(&mut redis_conn, &acquired, &lock_val).await;
                return Err(AppError::Conflict(
                    "Tiket sedang dipesan pengguna lain, coba lagi sebentar".into(),
                ));
            }
            acquired.push(key.clone());
        }

        Ok(Self {
            redis: redis_conn,
            acquired_keys: Arc::new(acquired),
            lock_val,
        })
    }

    pub async fn release(&mut self) {
        if self.acquired_keys.is_empty() {
            return;
        }
        release_keys(&mut self.redis, &self.acquired_keys, &self.lock_val).await;
        self.acquired_keys = Arc::new(Vec::new());
    }
}

impl Drop for VariantLockGuard {
    fn drop(&mut self) {
        if !self.acquired_keys.is_empty() {
            tracing::error!(
                keys = ?self.acquired_keys.as_ref(),
                "VariantLockGuard dropped tanpa release! Lock expire via Redis TTL."
            );
        }
    }
}

pub(super) async fn release_keys(
    redis: &mut redis::aio::ConnectionManager,
    keys: &[String],
    lock_val: &str,
) {
    let script = redis::Script::new(LUA_RELEASE);
    for key in keys {
        let _ = script
            .key(key.as_str())
            .arg(lock_val)
            .invoke_async::<i64>(redis)
            .await;
    }
}

#[cfg(test)]
mod tests {
    //! Integration test untuk Redis distributed lock per-variant.
    //!
    //! Butuh Redis hidup — di-`#[ignore]` agar `cargo test` biasa tetap hijau
    //! tanpa Redis. Jalankan dengan:
    //!   TEST_REDIS_URL=redis://127.0.0.1/ cargo test --features ssr -- --ignored lock
    use super::*;

    async fn conn() -> redis::aio::ConnectionManager {
        let url =
            std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
        let client = redis::Client::open(url).expect("redis client");
        redis::aio::ConnectionManager::new(client)
            .await
            .expect("redis connection (apakah Redis hidup?)")
    }

    /// Dua pembeli pada varian yang sama tidak boleh memegang lock bersamaan;
    /// setelah lock pertama dilepas, akuisisi berikutnya berhasil.
    #[tokio::test]
    #[ignore = "butuh Redis; set TEST_REDIS_URL lalu jalankan dengan --ignored"]
    async fn same_variant_is_mutually_exclusive() {
        let redis = conn().await;
        let vid = format!("test-variant-{}", new_ulid());

        // Pembeli #1 memperoleh lock.
        let mut g1 = VariantLockGuard::acquire(redis.clone(), &[vid.as_str()], false)
            .await
            .expect("akuisisi pertama harus berhasil");

        // Pembeli #2 untuk varian yang sama → Conflict (lock masih dipegang #1).
        let g2 = VariantLockGuard::acquire(redis.clone(), &[vid.as_str()], false).await;
        let is_conflict = matches!(g2, Err(AppError::Conflict(_)));
        // Kalau ternyata berhasil (bug), lepas agar tidak memicu Drop warning.
        if let Ok(mut g) = g2 {
            g.release().await;
        }
        assert!(is_conflict, "akuisisi kedua harus Conflict saat lock masih dipegang");

        // Lepas lock #1 → varian bebas lagi.
        g1.release().await;

        let mut g3 = VariantLockGuard::acquire(redis.clone(), &[vid.as_str()], false)
            .await
            .expect("setelah release, akuisisi harus berhasil");
        g3.release().await;
    }

    /// Varian berbeda tidak saling memblok (lock granular per-varian).
    #[tokio::test]
    #[ignore = "butuh Redis; set TEST_REDIS_URL lalu jalankan dengan --ignored"]
    async fn different_variants_do_not_block() {
        let redis = conn().await;
        let a = format!("test-variant-a-{}", new_ulid());
        let b = format!("test-variant-b-{}", new_ulid());

        let mut g1 = VariantLockGuard::acquire(redis.clone(), &[a.as_str()], false)
            .await
            .expect("lock varian A");
        let mut g2 = VariantLockGuard::acquire(redis.clone(), &[b.as_str()], false)
            .await
            .expect("lock varian B (varian berbeda, tak boleh terblok)");

        g1.release().await;
        g2.release().await;
    }
}
