use sqlx::postgres::PgPool;
use synth_common::Config;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: PgPool,
    pub redis_client: redis::Client,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let pool = PgPool::connect(&config.database_url)
            .await
            .map_err(|e| anyhow::anyhow!("db connect failed: {e}"))?;
        let redis_client = redis::Client::open(config.redis_url.clone())
            .map_err(|e| anyhow::anyhow!("redis client failed: {e}"))?;
        Ok(Self {
            config,
            pool,
            redis_client,
        })
    }

    pub async fn redis_conn(&self) -> anyhow::Result<redis::aio::Connection> {
        self.redis_client
            .get_async_connection()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}
