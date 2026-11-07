mod app;
mod db;
mod routes;

use clap::Parser;
use synth_common::{Config, init_telemetry};

#[derive(Parser, Debug)]
#[command(
    name = "synth-control",
    about = "SynthMail control plane - Admin REST API"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 8000)]
    port: u16,
    #[arg(long)]
    database_url: Option<String>,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long, action = clap::ArgAction::SetTrue, default_value_t = false)]
    migrate: bool,
    #[arg(long, action = clap::ArgAction::SetTrue, default_value_t = false)]
    seed_admin: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_telemetry("synth-control");
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

    tracing::info!(host = %cfg.host, port = cfg.port, "starting synth-control");
    cfg.validate().map_err(|e| anyhow::anyhow!(e))?;

    let state = app::AppState::new(cfg.clone()).await?;

    if args.migrate {
        db::run_migrations(&state.pool).await?;
        tracing::info!("migrations applied");
    }
    // Auto-migrate on boot (idempotent)
    db::run_migrations(&state.pool).await?;

    if args.seed_admin {
        db::seed_admin(&state.pool).await?;
    } else {
        // Check if admin exists; if none, create from env
        match db::seed_admin_if_empty(&state.pool).await {
            Ok(true) => tracing::info!("seeded initial admin from env"),
            _ => {}
        }
    }

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", state.config.host, state.config.port))
            .await?;
    tracing::info!(addr = %listener.local_addr().unwrap(), "synth-control listening");
    axum::serve(listener, routes::router(state)).await?;
    Ok(())
}
