use sqlx::postgres::PgPool;
use synth_common::{Config, rate_limit::RateLimiter};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: PgPool,
    pub redis_client: redis::Client,
    pub limiter: RateLimiter,
    pub tight_limiter: RateLimiter,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let pool = PgPool::connect(&config.database_url)
            .await
            .map_err(|e| anyhow::anyhow!("db connect failed: {e}"))?;
        let redis_client = redis::Client::open(config.redis_url.clone())
            .map_err(|e| anyhow::anyhow!("redis client failed: {e}"))?;
        Ok(Self {
            limiter: RateLimiter::new(config.rate_limit_per_min as usize),
            tight_limiter: RateLimiter::new((config.rate_limit_per_min as usize / 2).max(5)),
            config,
            pool,
            redis_client,
        })
    }
}
