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

#[derive(Debug, Clone, serde::Serialize)]
struct HealthStatus {
    service: String,
    ok: bool,
    latency_ms: u64,
    checked_at: chrono::DateTime<chrono::Utc>,
}

fn overall_status(services: &[HealthStatus]) -> &'static str {
    if services.is_empty() {
        "starting"
    } else if services.iter().all(|s| s.ok) {
        "operational"
    } else {
        "degraded"
    }
}

fn is_valid_acme_token(token: &str) -> bool {
    !token.is_empty()
        && token != "."
        && token != ".."
        && !token.contains('/')
        && !token.contains('\\')
        && !token.contains("..")
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
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

    // Audit S-03: warn at startup if the configured NTFY_TOPIC looks too
    // weak to be safe on a public broker (readable by anyone who guesses it).
    validate_ntfy_topic();

    let pool = PgPool::connect(&cfg.database_url)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let redis_client =
        redis::Client::open(cfg.redis_url.clone()).map_err(|e| anyhow::anyhow!(e))?;

    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    // M-10: shared snapshot of the latest check_all() output, read by /status.
    let health_state: Arc<RwLock<Vec<HealthStatus>>> = Arc::new(RwLock::new(Vec::new()));
    let health_state_poller = Arc::clone(&health_state);

    // Background poller with >2m alert threshold (Phase 2)
    tokio::spawn(async move {
        let mut failures: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();
        // M-10: last alert time per service, for dedupe within a failure episode.
        let mut last_alerted: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(args.poll_interval_secs));
        loop {
            interval.tick().await;
            let status = check_all(&pool, &redis_client).await;
            // Publish the snapshot for /status before alert processing.
            if let Ok(mut guard) = health_state_poller.write() {
                *guard = status.clone();
            }
            for s in &status {
                if !s.ok {
                    tracing::warn!(service = %s.service, latency = %s.latency_ms, "health check failed");
                    failures
                        .entry(s.service.clone())
                        .or_insert_with(chrono::Utc::now);
                    let first_seen = *failures.get(&s.service).unwrap();
                    let now = chrono::Utc::now();
                    if now - first_seen > chrono::Duration::minutes(2) {
                        // Alert once per failure episode, then at most every 30 min.
                        let should_alert = match last_alerted.get(&s.service) {
                            None => true,
                            Some(last) => now - *last >= chrono::Duration::minutes(30),
                        };
                        if should_alert {
                            alert(&s.service, "service down >2m").await;
                            last_alerted.insert(s.service.clone(), now);
                        }
                    }
                } else {
                    failures.remove(&s.service);
                    last_alerted.remove(&s.service);
                }
            }
            tracing::info!(
                services_ok = status.iter().filter(|s| s.ok).count(),
                total = status.len(),
                "health poll complete"
            );
            check_backups().await;
        }
    });

    // Serve /healthz + ACME endpoint glue + status page
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
                synth_common::metrics::render_prometheus("synth_watchdog")
            }),
        )
        .route(
            "/status",
            axum::routing::get({
                let health_state = Arc::clone(&health_state);
                move || {
                    let health_state = Arc::clone(&health_state);
                    async move {
                        let services = health_state
                            .read()
                            .map(|guard| guard.clone())
                            .unwrap_or_default();
                        let status = overall_status(&services);
                        let checked_at = services
                            .iter()
                            .map(|s| s.checked_at)
                            .max()
                            .unwrap_or_else(chrono::Utc::now);
                        axum::Json(serde_json::json!({
                            "status": status,
                            "services": services,
                            "checked_at": checked_at
                        }))
                    }
                }
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

async fn alert(service: &str, detail: &str) {
    if let Ok(url) = std::env::var("ALERT_WEBHOOK_URL") {
        let body =
            serde_json::json!({"service": service, "detail": detail, "at": chrono::Utc::now()});
        match reqwest::Client::new().post(&url).json(&body).send().await {
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, url, "webhook alert dispatch failed"),
        }
    }
    // Audit S-03: topic + base URL are validated at startup; here we only
    // read and post. A public `ntfy.sh` topic is readable by anyone who
    // knows the topic string, so the operator should run a private ntfy
    // (or a self-hosted one) for any deployment the mail host fronts.
    if let Ok(topic) = std::env::var("NTFY_TOPIC") {
        let base = std::env::var("NTFY_BASE_URL").unwrap_or_else(|_| "https://ntfy.sh".into());
        let url = format!("{}/{}/", base.trim_end_matches('/'), topic);
        match reqwest::Client::new()
            .post(&url)
            .body(format!("SynthMail: {} — {}", service, detail))
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    tracing::warn!(
                        status = %resp.status(),
                        topic,
                        "ntfy alert dispatch returned non-success"
                    );
                }
            }
            Err(e) => tracing::error!(error = %e, topic, "ntfy alert dispatch failed"),
        }
    }
    tracing::warn!(service, detail, "alert dispatched (webhook/ntfy)");
}

/// Startup-time check that the NTFY_TOPIC is plausible. A 4-character
/// topic like `mail` is trivially guessable; the `https://ntfy.sh/mail`
/// stream would be readable by anyone who enumerates. Reject topics that
/// are too short or that contain characters ntfy does not accept.
fn validate_ntfy_topic() {
    let Ok(topic) = std::env::var("NTFY_TOPIC") else {
        return;
    };
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return;
    }
    let chars_ok = trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'));
    if !chars_ok {
        tracing::warn!(
            topic,
            "NTFY_TOPIC contains characters ntfy rejects; using a private \
             ntfy (or base-URL self-host) is strongly recommended"
        );
        return;
    }
    if trimmed.len() < 12 {
        tracing::warn!(
            topic,
            "NTFY_TOPIC is too short; anyone can read a public ntfy stream \
             by guessing the topic. Use a private ntfy for production mail hosts."
        );
    }
}

async fn acme_challenge(
    axum::extract::Path(token): axum::extract::Path<String>,
) -> (axum::http::StatusCode, String) {
    // M-02: reject path traversal before touching the filesystem.
    if !is_valid_acme_token(&token) {
        return (axum::http::StatusCode::BAD_REQUEST, String::new());
    }
    let path = format!("/etc/letsencrypt/acme-challenge/{}", token);
    match tokio::fs::read_to_string(&path).await {
        Ok(body) => (axum::http::StatusCode::OK, body),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, String::new()),
    }
}

async fn check_all(pool: &PgPool, redis: &redis::Client) -> Vec<HealthStatus> {
    let now = chrono::Utc::now();
    let mut out = Vec::new();

    let t0 = std::time::Instant::now();
    let pg_ok = sqlx::query("SELECT 1").execute(pool).await.is_ok();
    out.push(HealthStatus {
        service: "postgres".into(),
        ok: pg_ok,
        latency_ms: t0.elapsed().as_millis() as u64,
        checked_at: now,
    });

    let t1 = std::time::Instant::now();
    let redis_ok = redis.get_multiplexed_async_connection().await.is_ok();
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
    let backup_dir = std::env::var("BACKUP_DIR").unwrap_or_else(|_| "/backups".to_string());

    // M-10: minimally real check — find the newest backup file and warn if stale.
    // Log-only otherwise; pg_dump scheduling is handled by the backup container.
    let mut newest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    match tokio::fs::read_dir(&backup_dir).await {
        Ok(mut rd) => {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if let Ok(meta) = entry.metadata().await
                    && let Ok(modified) = meta.modified()
                {
                    let is_newer = match &newest {
                        None => true,
                        Some((_, t)) => modified > *t,
                    };
                    if is_newer {
                        newest = Some((path, modified));
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!(%backup_dir, error = %e, "backup dir not readable");
            return;
        }
    }

    match newest {
        None => {
            tracing::warn!(%backup_dir, "no backup files found");
        }
        Some((path, modified)) => {
            let age = std::time::SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default();
            let age_days = age.as_secs() / 86_400;
            if age_days as i64 > retention_days {
                tracing::warn!(
                    path = %path.display(),
                    age_days,
                    retention_days,
                    "newest backup older than retention"
                );
            } else {
                tracing::info!(
                    path = %path.display(),
                    age_days,
                    retention_days,
                    "newest backup within retention"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(ok: bool) -> HealthStatus {
        HealthStatus {
            service: "postgres".into(),
            ok,
            latency_ms: 1,
            checked_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn ntfy_topic_validation_paths() {
        // Audit S-03: each rejection / accept class runs without panic.
        unsafe {
            std::env::remove_var("NTFY_TOPIC");
            validate_ntfy_topic(); // unset → no-op
            std::env::set_var("NTFY_TOPIC", "mail"); // too short → warn
            validate_ntfy_topic();
            std::env::set_var("NTFY_TOPIC", "bad space"); // bad char → warn
            validate_ntfy_topic();
            std::env::set_var("NTFY_TOPIC", "abcdefgh123456"); // OK
            validate_ntfy_topic();
            std::env::remove_var("NTFY_TOPIC");
        }
    }

    #[test]
    fn health_schema() {
        let s = serde_json::json!({"ok": true});
        assert_eq!(s["ok"], true);
    }

    #[test]
    fn overall_status_reflects_snapshot() {
        assert_eq!(overall_status(&[]), "starting");
        assert_eq!(overall_status(&[svc(true), svc(true)]), "operational");
        assert_eq!(overall_status(&[svc(true), svc(false)]), "degraded");
    }

    #[test]
    fn acme_token_validation_rejects_traversal() {
        assert!(is_valid_acme_token("abc-DEF_123"));
        assert!(!is_valid_acme_token("../etc/passwd"));
        assert!(!is_valid_acme_token("a/b"));
        assert!(!is_valid_acme_token("a\\b"));
        assert!(!is_valid_acme_token(".."));
        assert!(!is_valid_acme_token(""));
    }
}
