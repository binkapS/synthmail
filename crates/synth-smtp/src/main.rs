//! ESMTP submission service (successor to the Phase 3 spike shim).
//!
//! Role: an authenticated, TLS-capable submission/relay entry point on the
//! trusted mail network, with Postfix as the delivery relay and Redis as
//! the spool of record.
//!
//! Transport security:
//! - STARTTLS (RFC 3207) when `--tls-cert`/`--tls-key` are provided; the
//!   stack's posture is SASL accepted only over TLS, so EHLO advertises
//!   AUTH PLAIN LOGIN only on a session already upgraded to TLS.
//! - AUTH PLAIN / AUTH LOGIN (RFC 4954) verified against the same Postgres
//!   user store (`users.password_hash`, Argon2id) the control plane uses,
//!   with identical timing-parity treatment (dummy hash, no cause leaks)
//!   and a Redis-shared per-IP login attempt budget.
//! - Relay policy: authenticated users submit anywhere; unauthenticated
//!   sessions may deliver only to `--relay-domains` (default: the local
//!   mail domain). The shim is never an open relay; `--open-relay` restores
//!   the old trusted-networks-only spike behavior explicitly (logged loudly).
//!
//! Resource safety (all enforced per connection and in the accept loop):
//! - Semaphore cap on concurrent connections (`--max-connections`).
//! - Redis-shared per-IP connection rate limit (`--conn-rate-limit`).
//! - Per-line idle/read timeout (`--idle-timeout-secs`/`--read-timeout-secs`).
//! - Hard session wall clock (`--max-session-secs`).
//! - Bounded line reads (RFC 5321 1000-octet lines enforced during the read,
//!   not after the buffer has accumulated).
//! - Bounded queue: pending list capped; clients get backpressure via
//!   `451 4.7.0` when the queue write fails (never a silent 250).
//!
//! Lifecycle: SIGTERM/SIGINT stop the accept loop; in-flight connections
//! are drained (up to `--shutdown-grace-secs`) before exit.

mod auth;
mod queue;
mod service;
mod smtp;
mod tls;

#[cfg(test)]
mod smtp_tests;

#[cfg(test)]
mod service_tests;

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use synth_common::Config;
use tokio::io::AsyncWriteExt;

#[derive(Parser, Debug)]
#[command(
    name = "synth-smtp",
    about = "SynthMail ESMTP submission service (STARTTLS + AUTH, Postfix relay)"
)]
struct Args {
    /// Bind address. Loopback by default: even with AUTH+STARTTLS, exposing
    /// a relay is a deliberate operator decision.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 2525)]
    port: u16,
    /// Greeting hostname sent in banners and EHLO responses.
    #[arg(long)]
    server_name: Option<String>,
    #[arg(long, default_value = "redis://redis:6379")]
    redis_url: String,
    /// Optional override; falls back to DATABASE_URL / shared config.
    #[arg(long)]
    database_url: Option<String>,
    /// PEM certificate chain (starts a TLS-capable STARTTLS listener).
    #[arg(long, requires = "tls_key")]
    tls_cert: Option<String>,
    /// PEM private key (fails closed on unparseable material).
    #[arg(long, requires = "tls_cert")]
    tls_key: Option<String>,
    /// Require AUTH before MAIL FROM (recommended when TLS is enabled).
    #[arg(long)]
    require_auth: bool,
    /// Domains unauthenticated sessions may deliver to (CSV). Empty means
    /// the local mail domain from the shared config.
    #[arg(long, value_delimiter = ',')]
    relay_domains: Vec<String>,
    /// Explicitly restore spike-era "accept any recipient" semantics.
    #[arg(long)]
    open_relay: bool,
    // Accept, then relay verbatim via lettre to this host (port 25, plain
    // SMTP — trusted mailnet only).
    #[arg(long, default_value_t = true)]
    forward_to_postfix: bool,
    #[arg(long, default_value = "postfix:25")]
    postfix_host: String,
    /// Max concurrent client connections (semaphore cap); excess TCP
    /// connections are refused with a 421.
    #[arg(long, default_value_t = 100)]
    max_connections: usize,
    /// Idle timeout: max time to wait for the next line (seconds).
    #[arg(long, default_value_t = 60)]
    idle_timeout_secs: u64,
    /// Read timeout: max time to receive a single line (seconds).
    #[arg(long, default_value_t = 60)]
    read_timeout_secs: u64,
    /// Hard cap on a connection's total lifetime (seconds).
    #[arg(long, default_value_t = 1800)]
    max_session_secs: u64,
    /// Per-IP connection rate limit per minute (Redis-shared).
    #[arg(long, default_value_t = 60)]
    conn_rate_limit: usize,
    /// Per-IP SASL attempt budget per minute (Redis-shared; Argon2 burn is
    /// bounded by this).
    #[arg(long, default_value_t = 15)]
    auth_attempt_limit: usize,
    /// Grace period to drain in-flight connections at shutdown (seconds).
    #[arg(long, default_value_t = 10)]
    shutdown_grace_secs: u64,
}

fn main() -> anyhow::Result<()> {
    synth_common::init_telemetry("synth-smtp");
    let args = Args::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run(args))
}

async fn run(args: Args) -> anyhow::Result<()> {
    let cfg = Config::from_env();
    if let Err(e) = cfg.validate() {
        // Not fatal for an SMTP shim with auth disabled; fatal the moment
        // auth is requested (DB must be reachable — checked below).
        tracing::warn!(error = %e, "shared config validation failed");
    }

    let server_name = args
        .server_name
        .clone()
        .unwrap_or_else(|| cfg.mail_host.clone());

    // ----- TLS: fail closed on unparseable certificate material -----
    let tls_acceptor = match (&args.tls_cert, &args.tls_key) {
        (Some(cert), Some(key)) => Some(tls::load(cert, key)?),
        _ => None,
    };
    if args.require_auth && tls_acceptor.is_none() {
        anyhow::bail!(
            "--require-auth requires TLS (--tls-cert/--tls-key): this stack \
             accepts SASL only over an encrypted session"
        );
    }

    // ----- Credential store (only when AUTH is possible) -----
    let auth = if tls_acceptor.is_some() {
        let db_url = args
            .database_url
            .clone()
            .unwrap_or_else(|| cfg.database_url.clone());
        let redis_client = redis::Client::open(args.redis_url.clone()).ok();
        Some(Arc::new(
            auth::Authenticator::connect(&db_url, args.auth_attempt_limit, redis_client).await?,
        ))
    } else {
        None
    };

    // ----- Queue (fail loud: an unreachable Redis means accepted mail
    // cannot be durably queued) -----
    let redis_client =
        redis::Client::open(args.redis_url.clone()).map_err(|e| anyhow::anyhow!("redis: {e}"))?;
    let mut cm = None;
    for attempt in 1..=5 {
        match redis::aio::ConnectionManager::new(redis_client.clone()).await {
            Ok(c) => {
                cm = Some(c);
                break;
            }
            Err(e) => tracing::warn!(error = %e, attempt, "redis connect failed"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let redis_conn = cm.ok_or_else(|| {
        anyhow::anyhow!(
            "redis unreachable after 5 attempts; refusing to start — mail \
             must not be accepted without a durable queue"
        )
    })?;

    // Relay policy: no explicit domains configured means the local mail
    // domain from the shared config.
    let relay_domains = if args.relay_domains.is_empty() {
        vec![cfg.mail_domain.clone()]
    } else {
        args.relay_domains.clone()
    };

    // ----- Rate limiting (Redis-shared, local fallback) -----
    let conn_limiter = Arc::new(synth_common::rate_limit::RateLimiter::with_redis(
        args.conn_rate_limit,
        redis_client.clone(),
    ));

    // Lifecycle channel created before the service ctx: connection handlers
    // poll it between lines and close themselves with a 421 at drain time.
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let ctx = Arc::new(service::ServiceCtx {
        server_name: server_name.clone(),
        tls: tls_acceptor.clone(),
        auth,
        require_auth: args.require_auth,
        open_relay: args.open_relay,
        relay_domains: relay_domains.clone(),
        redis: redis_conn,
        forward: args.forward_to_postfix,
        postfix_host: args.postfix_host.clone(),
        line_timeout: Duration::from_secs(args.idle_timeout_secs.max(args.read_timeout_secs)),
        max_session: Duration::from_secs(args.max_session_secs),
        conn_limiter,
        shutdown: shutdown_rx.clone(),
    });

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", args.host, args.port)).await?;
    tracing::info!(
        addr = %listener.local_addr().unwrap(),
        server_name = %server_name,
        starttls = ctx.tls.is_some(),
        require_auth = args.require_auth,
        open_relay = args.open_relay,
        relay_domains = ?relay_domains,
        "synth-smtp listening"
    );
    if args.open_relay {
        tracing::warn!(
            "OPEN RELAY: --open-relay accepts any recipient without \
             authentication; bind to loopback / trusted networks only"
        );
    }
    if !args.require_auth && !args.open_relay {
        tracing::info!(
            "unauthenticated sessions restricted to relay_domains {:?}",
            relay_domains
        );
    }

    if tls_acceptor.is_none() {
        tracing::warn!(
            "no TLS configured: neither STARTTLS nor AUTH can be offered; \
             trusted networks only"
        );
    }

    // ----- Acceptor with shutdown handling -----
    let semaphore = Arc::new(tokio::sync::Semaphore::new(args.max_connections));
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    tokio::spawn({
        let tx = shutdown_tx.clone();
        async move {
            shutdown_signal().await;
            tracing::info!("shutdown signal received; draining connections");
            let _ = tx.send(true);
        }
    });

    loop {
        let (stream, _peer_addr) = tokio::select! {
            _ = shutdown_rx.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok(x) => x,
                Err(e) => {
                    // Transient accept errors (EMFILE etc.) must not kill the
                    // process; log and back off briefly.
                    tracing::error!(error = %e, "accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
        };
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                // At cap: answer briefly so well-behaved clients learn
                // instantly instead of timing out on a silent drop.
                let mut s = stream;
                let _ = s
                    .write_all(b"421 4.3.2 Too many connections, try again later\r\n")
                    .await;
                let _ = s.shutdown().await;
                continue;
            }
        };

        // Per-IP connection rate limit (Redis-shared). Rejects are told
        // politely before the socket is dropped.
        let ip = service::peer_ip(&stream);
        if !ctx.conn_limiter.check(&format!("smtp:conn:{ip}")).await {
            let mut s = stream;
            let _ = s
                .write_all(b"421 4.3.2 Too many connections from your address\r\n")
                .await;
            let _ = s.shutdown().await;
            tracing::debug!(%ip, "connection rate limited");
            continue;
        }

        active.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let ctx2 = ctx.clone();
        let active2 = active.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let conn = service::Conn::Plain(stream);
            service::handle_connection(conn, ip, ctx2).await;
            active2.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }

    // Draining: stop accepting, wait for in-flight (they poll the watch
    // channel between lines and close themselves with a 421).
    drop(listener);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(args.shutdown_grace_secs);
    while active.load(std::sync::atomic::Ordering::SeqCst) > 0 {
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(
                remaining = active.load(std::sync::atomic::Ordering::SeqCst),
                "shutdown grace exceeded; abandoning in-flight connections"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tracing::info!("synth-smtp stopped");
    Ok(())
}

/// SIGINT + SIGTERM (missing unix signal support never blocks).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                    return;
                }
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
