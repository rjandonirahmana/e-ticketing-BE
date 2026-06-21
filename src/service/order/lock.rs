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
    ) -> Result<Self, AppError> {
        let mut sorted: Vec<&str> = variant_ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let lock_val: Arc<str> = Arc::from(new_ulid().as_str());
        let keys: Vec<String> = sorted
            .iter()
            .map(|id| format!("order:lock:variant:{}", id))
            .collect();

        let mut acquired: Vec<String> = Vec::with_capacity(keys.len());
        let mut redis_conn = redis;

        for key in &keys {
            let mut ok = false;
            for attempt in 0..=LOCK_RETRIES {
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
                    Ok(None) if attempt < LOCK_RETRIES => {
                        sleep(Duration::from_millis(LOCK_DELAY_MS)).await;
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
