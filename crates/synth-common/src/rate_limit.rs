use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_per_min: usize,
}

impl RateLimiter {
    pub fn new(max_per_min: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_per_min,
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("rate limit lock");
        let entry = map.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
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
    #[test]
    fn allows_within_limit() {
        let rl = RateLimiter::new(3);
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(rl.check("a"));
        assert!(!rl.check("a"));
        assert!(rl.check("b"));
    }
}
