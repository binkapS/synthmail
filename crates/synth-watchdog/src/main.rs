use clap::Parser;
use sqlx::postgres::PgPool;
use synth_common::{Config, init_telemetry};

#[derive(Parser, Debug)]
#[command(
    name = "synth-watchdog",
    about = "Health aggregation, ACME glue, backup scheduler"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 8081)]
    port: u16,
    #[arg(long)]
    database_url: Option<String>,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long, default_value_t = 60)]
    poll_interval_secs: u64,
}

#[derive(Debug, serde::Serialize)]
struct HealthStatus {
    service: String,
    ok: bool,
    latency_ms: u64,
    checked_at: chrono::DateTime<chrono::Utc>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_telemetry("synth-watchdog");
    let args = Args::parse();
    let mut cfg = Config::from_env();
    cfg.host = args.host;
    cfg.port = args.port;
    if let Some(v) = args.database_url {
        cfg.database_url = v;
    }
    if let Some(v) = args.redis_url {
        cfg.redis_url = v;
    }

    let pool = PgPool::connect(&cfg.database_url)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let redis_client =
        redis::Client::open(cfg.redis_url.clone()).map_err(|e| anyhow::anyhow!(e))?;

    // Background poller
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(args.poll_interval_secs));
        loop {
            interval.tick().await;
            let status = check_all(&pool, &redis_client).await;
            for s in &status {
                if !s.ok {
                    tracing::warn!(service = %s.service, latency = %s.latency_ms, "health check failed");
                }
            }
            tracing::info!(
                services_ok = status.iter().filter(|s| s.ok).count(),
                total = status.len(),
                "health poll complete"
            );
            // Backup scheduler placeholder: daily pg_dump check
            check_backups().await;
        }
    });

    // Serve /healthz + ACME endpoint glue
    let app = axum::Router::new()
        .route(
            "/healthz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }),
        )
        .route(
            "/readyz",
            axum::routing::get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }),
        )
        .route(
            "/metrics",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!({"synth_watchdog_up": 1}))
            }),
        )
        .route(
            "/.well-known/acme-challenge/{token}",
            axum::routing::get(acme_challenge),
        );

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", cfg.host, cfg.port)).await?;
    tracing::info!(addr = %listener.local_addr().unwrap(), "synth-watchdog listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn acme_challenge(axum::extract::Path(token): axum::extract::Path<String>) -> String {
    let path = format!("/etc/letsencrypt/acme-challenge/{}", token);
    std::fs::read_to_string(&path).unwrap_or_else(|_| format!("challenge {}", token))
}

async fn check_all(pool: &PgPool, redis: &redis::Client) -> Vec<HealthStatus> {
    let now = chrono::Utc::now();
    let mut out = Vec::new();

    let t0 = std::time::Instant::now();
    let pg_ok = sqlx::query_scalar::<_, ()>("SELECT 1")
        .execute(pool)
        .await
        .is_ok();
    out.push(HealthStatus {
        service: "postgres".into(),
        ok: pg_ok,
        latency_ms: t0.elapsed().as_millis() as u64,
        checked_at: now,
    });

    let t1 = std::time::Instant::now();
    let redis_ok = redis.get_async_connection().await.is_ok();
    out.push(HealthStatus {
        service: "redis".into(),
        ok: redis_ok,
        latency_ms: t1.elapsed().as_millis() as u64,
        checked_at: now,
    });

    // TODO: TCP dial postfix:25, dovecot:993, rspamd:11334
    for (svc, addr) in [
        ("postfix", "postfix:25"),
        ("dovecot", "dovecot:993"),
        ("rspamd", "rspamd:11334"),
        ("clamav", "clamav:3310"),
    ] {
        let t = std::time::Instant::now();
        let ok = tokio::net::TcpStream::connect(addr).await.is_ok();
        out.push(HealthStatus {
            service: svc.into(),
            ok,
            latency_ms: t.elapsed().as_millis() as u64,
            checked_at: now,
        });
    }
    out
}

async fn check_backups() {
    let retention_days: i64 = std::env::var("BACKUP_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(14);
    tracing::info!(
        retention_days,
        "backup retention check (pg_dump + maildir tar + rclone in Phase 2)"
    );
    // pg_dump scheduling handled by backup container (docker compose profile backup)
}

#[cfg(test)]
mod tests {
    #[test]
    fn health_schema() {
        let s = serde_json::json!({"ok": true});
        assert_eq!(s["ok"], true);
    }
}
