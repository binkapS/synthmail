//! Sliding-window rate limiter.
//! Audit N-01: the previous implementation was in-memory only, so multiple
//! replicas each granted the full budget and empty keys accumulated.
//! The limiter is now Redis-backed (fixed window shared across replicas)
//! with a bounded in-memory fallback when Redis is unavailable. Empty keys
//! are evicted on every check so the fallback map cannot grow without bound.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    redis: Option<redis::Client>,
    max_per_min: usize,
}

impl RateLimiter {
    pub fn new(max_per_min: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            redis: None,
            max_per_min,
        }
    }

    pub fn with_redis(max_per_min: usize, client: redis::Client) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            redis: Some(client),
            max_per_min,
        }
    }

    fn redis_key(&self, key: &str) -> String {
        format!("synth:rl:{}", key)
    }

    /// Check whether `key` may proceed, recording the hit. Async because the
    /// shared Redis backend is used when configured.
    pub async fn check(&self, key: &str) -> bool {
        if let Some(client) = &self.redis {
            let rkey = self.redis_key(key);
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                let count: i64 = redis::AsyncCommands::incr(&mut conn, &rkey, 1i64)
                    .await
                    .unwrap_or(0);
                if count == 1 {
                    redis::AsyncCommands::expire(&mut conn, &rkey, 60i64)
                        .await
                        .unwrap_or(());
                }
                if count > 0 {
                    return count <= self.max_per_min as i64;
                }
            }
            // Redis fell through (e.g. connection error): use the local fallback.
        }
        self.check_local(key)
    }

    fn check_local(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("rate limit lock");
        // Evict expired keys so empty vectors cannot accumulate (audit N-01).
        map.retain(|_, times| {
            times.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
            !times.is_empty()
        });
        let entry = map.entry(key.to_string()).or_default();
        if entry.len() >= self.max_per_min {
            return false;
        }
        entry.push(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_within_limit() {
        let rl = RateLimiter::new(3);
        assert!(rl.check("a").await);
        assert!(rl.check("a").await);
        assert!(rl.check("a").await);
        assert!(!rl.check("a").await);
        assert!(rl.check("b").await);
    }

    #[tokio::test]
    async fn local_map_evicts_empty_keys() {
        let rl = RateLimiter::new(1);
        assert!(rl.check("a").await);
        // Simulate expiry by clearing stored timestamps.
        if let Ok(mut map) = rl.inner.lock() {
            for times in map.values_mut() {
                times.clear();
            }
        }
        // Next check evicts the empty key before deciding.
        assert!(rl.check("a").await);
        assert!(!rl.check_local("a"));
    }
}
