//! Per-connection I/O: bounded line reader (no unbounded buffering), TLS
//! upgrade, timeouts, and the session/action loop that ties the pure state
//! machine (`smtp.rs`) to Redis queueing, SASL verification and Postfix
//! relay.
//!
//! DoS model (unauthenticated-capable listener):
//! - Line reads are capped at [`smtp::MAX_LINE_LENGTH`] octets: a client can
//!   no longer OOM the process by streaming one endless line (the spike's
//!   `read_line` buffered arbitrarily before any length check ran).
//! - Overall session wall-clock cap (`--max-session-secs`) on top of the
//!   per-line idle/read timeout: a client dribbling bytes under the line
//!   timeout cannot hold a semaphore slot forever.
//! - Connection semaphore + Redis-shared per-IP connection rate limit.
//! - Queue writes fail loud (`451 4.7.0`) — never a silent 250 after a lost
//!   enqueue.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use crate::auth::{AuthError, Authenticator};
use crate::queue;
use crate::smtp::{Action, MAX_LINE_LENGTH, Session};
use synth_common::rate_limit::RateLimiter;

/// One accepted connection, in either transport mode.
///
/// The TLS variant is boxed: `TlsStream<TcpStream>` carries ~1.2 KiB of
/// rustls state (`TlsStream`'s inner `Server` holds certificate state,
/// keystream keys, etc.), while a bare `TcpStream` is a few words.
/// Unboxed, the whole enum would grow to 1224 bytes and `large_enum_variant`
/// would fire; a connection is short-lived, so the extra indirection is
/// acceptable for keeping the enum cheap.
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

// `TcpStream` is Unpin; `TlsStream<TcpStream>` contains no `Pin` state
// either, so the boxed TLS variant and the enum as a whole are Unpin.
impl AsyncRead for Conn {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Plain(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            Conn::Tls(s) => std::pin::Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl Conn {
    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.write_all(buf).await,
            Conn::Tls(s) => {
                s.write_all(buf).await?;
                s.flush().await
            }
        }
    }

    async fn shutdown(&mut self) {
        match self {
            Conn::Plain(s) => {
                let _ = s.shutdown().await;
            }
            Conn::Tls(s) => {
                let _ = s.shutdown().await;
            }
        }
    }
}

/// Outcome of a capped line read.
#[derive(Debug)]
pub enum LineOutcome {
    /// Complete line without CRLF.
    Line(Vec<u8>),
    /// Line exceeded the cap (rest of the overlong line was drained).
    TooLong,
    /// Peer closed (or dropped the line without a terminator).
    Eof,
}

/// Pull the next newline-terminated line out of `pending`, if any.
/// Only through this function may bytes be considered "a line"; everything
/// after the first newline stays pending for the next call.
fn take_line(pending: &mut Vec<u8>) -> Option<Vec<u8>> {
    let nl = pending.iter().position(|b| *b == b'\n')?;
    let mut line: Vec<u8> = pending.drain(..=nl).collect();
    line.pop(); // '\n'
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Some(line)
}

/// Read one SMTP line, buffering at most [`MAX_LINE_LENGTH`] octets.
/// A newline found mid-chunk splits the line off; the tail stays pending.
/// A line start that exceeds the cap is drained (without buffering) until
/// its terminating newline, keeping memory bounded no matter what a peer
/// streams.
pub async fn read_line<S: AsyncRead + Unpin>(
    s: &mut S,
    pending: &mut Vec<u8>,
) -> std::io::Result<LineOutcome> {
    if let Some(line) = take_line(pending) {
        return Ok(clamp_line(line));
    }
    let mut chunk = [0u8; 1024];
    loop {
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            return Ok(LineOutcome::Eof);
        }
        pending.extend_from_slice(&chunk[..n]);
        if let Some(line) = take_line(pending) {
            return Ok(clamp_line(line));
        }
        if pending.len() > MAX_LINE_LENGTH {
            // Overlong start: abandon buffered bytes, drain the wire until
            // the line's terminator, then report.
            pending.clear();
            loop {
                let n = s.read(&mut chunk).await?;
                if n == 0 {
                    return Ok(LineOutcome::Eof);
                }
                if chunk[..n].contains(&b'\n') {
                    return Ok(LineOutcome::TooLong);
                }
            }
        }
    }
}

fn clamp_line(line: Vec<u8>) -> LineOutcome {
    if line.len() > MAX_LINE_LENGTH {
        LineOutcome::TooLong
    } else {
        LineOutcome::Line(line)
    }
}

/// Shared immutable service wiring passed to every connection.
pub struct ServiceCtx {
    pub server_name: String,
    pub tls: Option<Arc<tokio_rustls::TlsAcceptor>>,
    pub auth: Option<Arc<Authenticator>>,
    pub require_auth: bool,
    pub open_relay: bool,
    pub relay_domains: Vec<String>,
    pub redis: redis::aio::ConnectionManager,
    pub forward: bool,
    pub postfix_host: String,
    pub line_timeout: Duration,
    pub max_session: Duration,
    pub conn_limiter: Arc<RateLimiter>,
    pub shutdown: tokio::sync::watch::Receiver<bool>,
}

pub fn peer_ip(stream: &TcpStream) -> IpAddr {
    stream
        .peer_addr()
        .map(|a| a.ip())
        .unwrap_or_else(|_| std::net::Ipv4Addr::UNSPECIFIED.into())
}

/// Run one connection to completion. Errors are logged, not propagated:
/// a single bad client must not affect the accept loop (it never did,
/// but unexpected task aborts now leave a note in the log).
pub async fn handle_connection(mut conn: Conn, ip: IpAddr, ctx: Arc<ServiceCtx>) {
    let started = Instant::now();
    let mut session = Session::new(&ctx.server_name, ctx.tls.is_some());
    let mut pending: Vec<u8> = Vec::with_capacity(1024);

    if let Err(e) = conn
        .write_all(format!("220 {} ESMTP SynthMail\r\n", ctx.server_name).as_bytes())
        .await
    {
        tracing::debug!(error = %e, "greeting write failed");
        return;
    }

    loop {
        if *ctx.shutdown.borrow() {
            let _ = conn.write_all(b"421 4.4.2 Service shutting down\r\n").await;
            break;
        }
        let elapsed = started.elapsed();
        if elapsed >= ctx.max_session {
            let _ = conn
                .write_all(b"421 4.4.2 Session duration exceeded\r\n")
                .await;
            break;
        }
        let budget = ctx.line_timeout.min(ctx.max_session - elapsed);
        let line = match tokio::time::timeout(budget, read_line(&mut conn, &mut pending)).await {
            Err(_) => {
                let _ = conn.write_all(b"421 4.4.2 Idle timeout\r\n").await;
                break;
            }
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "read/write error");
                break;
            }
            Ok(Ok(LineOutcome::Eof)) => break,
            Ok(Ok(LineOutcome::TooLong)) => {
                // Bounded-length invariant; never reached via state machine.
                if session_data_mode_abort(&mut session) {
                    let _ = conn.write_all(b"552 5.3.4 Line too long\r\n").await;
                } else {
                    let _ = conn.write_all(b"500 5.5.2 Line too long\r\n").await;
                }
                continue;
            }
            Ok(Ok(LineOutcome::Line(line))) => line,
        };

        match session.handle_line(&line, ctx.require_auth, ctx.open_relay, &ctx.relay_domains) {
            Action::Reply(r) => {
                if conn.write_all(format!("{r}\r\n").as_bytes()).await.is_err() {
                    break;
                }
                if r.starts_with("221") {
                    break; // QUIT acknowledged; close.
                }
            }
            Action::DataLine => {}
            Action::TransactionQueued(queued) => {
                match queue::enqueue(&mut ctx.redis.clone(), &queued).await {
                    Ok(()) => {
                        if ctx.forward {
                            queue::spawn_forward(ctx.postfix_host.clone(), queued.clone());
                        }
                        let reply = format!("250 QUEUED id={}\r\n", queued.id);
                        if conn.write_all(reply.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        // Never a silent loss: the client is told to retry.
                        tracing::error!(error = %e, id = %queued.id, "queue write failed");
                        if conn
                            .write_all(b"451 4.7.0 Unable to queue message, try again later\r\n")
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            Action::StartTls => {
                // RFC 3207: reply then negotiate on the same TCP stream.
                if conn
                    .write_all(b"220 2.0.0 Ready to start TLS\r\n")
                    .await
                    .is_err()
                {
                    break;
                }
                let acceptor = match &ctx.tls {
                    Some(a) => a.clone(),
                    None => break, // unreachable (guarded in smtp.rs)
                };
                let Conn::Plain(stream) = conn else { return };
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        tracing::debug!("STARTTLS negotiated");
                        // RFC 3207 §4: discard session state.
                        session.tls_active = true;
                        session.reset_for_tls();
                        conn = Conn::Tls(Box::new(tls_stream));
                    }
                    Err(e) => {
                        tracing::info!(error = %e, "STARTTLS handshake failed");
                        return;
                    }
                }
            }
            Action::AuthVerify {
                email,
                secret,
                mech,
            } => match handle_auth(&ctx, ip, &email, &secret, mech).await {
                Ok(authed) => {
                    session.set_authenticated(authed);
                    let _ = conn
                        .write_all(b"235 2.7.0 Authentication successful\r\n")
                        .await;
                }
                Err(reply) => {
                    if conn
                        .write_all(format!("{reply}\r\n").as_bytes())
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if reply.starts_with("421") {
                        break;
                    }
                }
            },
        }
    }
    let _ = conn.shutdown().await;
    tracing::debug!(%ip, elapsed_ms = started.elapsed().as_millis() as u64, "connection closed");
}

fn session_data_mode_abort(session: &mut Session) -> bool {
    session.abort_data()
}

/// SASL verification with per-IP attempt budget + reply mapping.
async fn handle_auth(
    ctx: &ServiceCtx,
    ip: IpAddr,
    email: &str,
    secret: &str,
    mech: crate::smtp::AuthMech,
) -> Result<String, String> {
    let Some(auth) = &ctx.auth else {
        // No credential store wired (e.g. AUTH attempted without TLS).
        return Err("538 5.7.11 Encryption required for requested authentication mechanism".into());
    };
    if !auth.allow_attempt(ip).await {
        synth_common::metrics::inc_auth_failure();
        return Err("421 4.7.0 Too many authentication attempts, try again later".into());
    }
    synth_common::metrics::inc_auth_attempt();
    tracing::debug!(%ip, ?mech, "sasl credentials presented");
    match auth.verify(email, secret).await {
        Ok(user) => {
            tracing::info!(%ip, user, "auth successful");
            Ok(user)
        }
        Err(AuthError::Invalid) => {
            synth_common::metrics::inc_auth_failure();
            tracing::warn!(%ip, "auth failed");
            Err("535 5.7.8 Authentication credentials invalid".into())
        }
        Err(AuthError::Service(e)) => {
            tracing::error!(error = %e, "auth store unreachable");
            Err("454 4.3.0 Temporary authentication failure".into())
        }
    }
}
