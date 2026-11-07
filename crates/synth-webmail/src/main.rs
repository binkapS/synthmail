// Webmail mail access is Maildir-backed (see imap.rs); JMAP Email/set is
// the only remaining honest stub (modeled, not implemented).
// supporting helpers below are intentionally kept.
#![allow(dead_code)]
mod app;
mod carddav;
mod fts_indexer;
mod imap;
mod jmap;
mod routes;

use clap::Parser;
use synth_common::{Config, init_telemetry};

#[derive(Parser, Debug)]
#[command(
    name = "synth-webmail",
    about = "SynthMail webmail backend - IMAP/JMAP proxy"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 3000)]
    port: u16,
    #[arg(long)]
    database_url: Option<String>,
    #[arg(long)]
    redis_url: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_telemetry("synth-webmail");
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

    cfg.validate().map_err(|e| anyhow::anyhow!(e))?;
    let state = app::AppState::new(cfg.clone()).await?;

    // Audit N-04: run the FTS indexer periodically instead of leaving
    // body_tsv NULL (search would otherwise stay empty). Interval batching
    // is idempotent — the same rows are recomputed until indexed.
    {
        let pool = state.pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                match fts_indexer::reindex_batch(&pool, 500).await {
                    Ok(n) => {
                        if n > 0 {
                            tracing::info!(reindexed = n, "fts reindex batch complete");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "fts reindex batch failed"),
                }
            }
        });
    }

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", state.config.host, state.config.port))
            .await?;
    tracing::info!(addr = %listener.local_addr().unwrap(), "synth-webmail listening");
    axum::serve(listener, routes::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
