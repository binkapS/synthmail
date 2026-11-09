//! SASL credential verification against the Postgres user store.
//!
//! Mirrors `synth-control` login semantics (see its M-08 audit note):
//! unknown / inactive / wrong-password all take the same wall-clock path by
//! always running one Argon2id verification (dummy hash when the account
//! does not exist) and never distinguishing the causes in the reply.
//!
//! Per-IP attempt throttling reuses the control plane's shared
//! (`synth_common::rate_limit`) limiter — Redis-backed so multi-replica
//! deployments share the budget, with a bounded local fallback when Redis
//! is unreachable. It gates authentication *attempts* per IP per minute,
//! which bounds the Argon2 CPU cost of credential-stuffing floods.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use std::time::Duration;
use synth_common::rate_limit::RateLimiter;

/// Credential store handle.
pub struct Authenticator {
    pool: PgPool,
    attempts: RateLimiter,
}

impl Authenticator {
    /// Connect with a bounded pool: SASL lookups are cheap and infrequent
    /// relative to HTTP traffic; 5 connections is plenty.
    pub async fn connect(
        url: &str,
        auth_attempt_limit: usize,
        shared_redis: Option<redis::Client>,
    ) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|e| anyhow::anyhow!("auth db: {e}"))?;
        let attempts = match shared_redis {
            Some(c) => RateLimiter::with_redis(auth_attempt_limit, c),
            None => RateLimiter::new(auth_attempt_limit),
        };
        Ok(Self { pool, attempts })
    }

    /// Attempt budget check for an IP (Redis-shared across replicas when the
    /// shared client is configured). Consumes one slot; the I/O layer calls
    /// it before starting a SASL exchange, so failed and flooding attempts
    /// both deplete it.
    pub async fn allow_attempt(&self, ip: std::net::IpAddr) -> bool {
        self.attempts.check(&format!("smtp:auth:{ip}")).await
    }

    /// Verify SASL credentials. Returns the authenticated email on success.
    ///
    /// Failure causes are indistinguishable to the client (`535`), which is
    /// intentional: no account enumeration.
    pub async fn verify(&self, email: &str, secret: &str) -> Result<String, AuthError> {
        let email_lc = email.trim().to_lowercase();
        if email_lc.is_empty() || secret.is_empty() {
            return Err(AuthError::Invalid);
        }

        let row: Option<String> = sqlx::query_scalar(
            "SELECT password_hash FROM users WHERE email = $1 AND active = true",
        )
        .bind(&email_lc)
        .fetch_optional(&self.pool)
        .await
        .map_err(AuthError::Service)?;

        // Timing parity with the control plane's login (M-08): always run a
        // real Argon2id verification, dummy hash when the account is unknown
        // or inactive.
        let hash = row.unwrap_or_else(dummy_hash);
        if !synth_common::verify_password(&hash, secret) {
            return Err(AuthError::Invalid);
        }
        Ok(email_lc)
    }
}

pub enum AuthError {
    /// Wrong credentials (indistinguishable causes).
    Invalid,
    /// Store unreachable (transient).
    Service(sqlx::Error),
}

static DUMMY_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
fn dummy_hash() -> String {
    DUMMY_HASH
        .get_or_init(|| {
            synth_common::hash_password("dummy-password-for-timing-parity").unwrap_or_default()
        })
        .clone()
}
