// Webmail IMAP/JMAP proxy handlers are honest 501 stubs until the Dovecot
// integration lands (H-10); supporting helpers below are intentionally kept.
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

    // Serve embedded UI if present (Phase 2: synth-ui static)
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
