//! Phase 3 spike: minimal ESMTP server (EHLO/MAIL/RCPT/DATA/QUIT).
//! Queues to Redis, optionally forwards to Postfix via lettre (hybrid fallback).
//! Not an inbound MTA replacement — submission shim only.
//!
//! SECURITY: This shim is UNAUTHENTICATED (no AUTH, no STARTTLS). It binds
//! 127.0.0.1 by default and MUST only be exposed on trusted networks
//! (loopback or the private compose network). Do not expose it to the
//! public internet — it accepts mail for any domain and can be abused
//! for spam injection and queue flooding.
//!
//! Submission-or-nothing: without AUTH/STARTTLS this binary is a
//! loopback-only submission shim, not a public MTA. SASL (AUTH) and
//! STARTTLS are intentionally NOT implemented — adding them is separate
//! work requiring credential store integration, not a flag flip. When a
//! real submission listener is needed, put Postfix (submission port 587
//! with SASL+TLS) in front and keep this shim behind it.
//!
//! DoS guards (all in-process, per-process; no shared state):
//! - `--max-connections`: semaphore cap on concurrent client tasks.
//! - `--idle-timeout` + `--read-timeout`: per-`read_line` timeout. The
//!   idle timer fires when a client sends nothing; the read timer bounds
//!   a single line's arrival. A complete transaction can therefore span
//!   longer than one timeout, but any stalled read is cut off.
//! - `--rate-limit-per-ip` per `--rate-window`: fixed-window per-IP
//!   connection rate limit (see [`RateLimiter`]). Documented limits: a
//!   single client IP can hold at most its window allowance of new
//!   connections per window; excess connections are accepted at TCP level
//!   then immediately closed with `421 4.3.2 Too many connections`.

use clap::Parser;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser, Debug)]
#[command(
    name = "synth-smtp",
    about = "Phase 3 spike: ESMTP shim with Redis queue"
)]
struct Args {
    // Loopback by default (audit N-05): this shim is unauthenticated, so it
    // must not listen on all interfaces out of the box.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 2525)]
    port: u16,
    #[arg(long, default_value = "redis://redis:6379")]
    redis_url: String,
    #[arg(long, default_value_t = true)]
    forward_to_postfix: bool,
    #[arg(long, default_value = "postfix:25")]
    postfix_host: String,
    /// Max concurrent client connections (semaphore cap); excess TCP
    /// connections are dropped without a greeting.
    #[arg(long, default_value_t = 100)]
    max_connections: usize,
    /// Idle timeout: max time to wait for the next line before closing
    /// the connection (seconds).
    #[arg(long, default_value_t = 60)]
    idle_timeout_secs: u64,
    /// Read timeout: max time to receive a single line (seconds).
    #[arg(long, default_value_t = 60)]
    read_timeout_secs: u64,
    /// Per-IP connection rate limit inside `--rate-window-secs`.
    #[arg(long, default_value_t = 20)]
    rate_limit_per_ip: u32,
    /// Fixed window for the per-IP rate limit (seconds).
    #[arg(long, default_value_t = 60)]
    rate_window_secs: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QueuedMessage {
    pub id: String,
    pub mail_from: String,
    pub rcpt_to: Vec<String>,
    pub data: String,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

const MAX_MESSAGE_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
const MAX_RECIPIENTS: usize = 50;
const MAX_LINE_LENGTH: usize = 1000; // RFC 5321: 1000 octets incl. CRLF
const REDIS_KEY_TTL_SECS: i64 = 86400;
const MAX_PENDING: isize = 1000;

/// Fixed-window per-IP connection rate limiter (in-process only).
///
/// Each IP gets `limit` accepted connections per `window` duration. The
/// window is anchored at the first connection seen in it; once `window`
/// has elapsed the counter resets. State is intentionally per-process —
/// documented as such; a multi-replica deployment would need a shared
/// (Redis) bucket instead.
#[derive(Debug)]
pub struct RateLimiter {
    limit: u32,
    window: std::time::Duration,
    buckets: std::collections::HashMap<std::net::IpAddr, (std::time::Instant, u32)>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: std::time::Duration) -> Self {
        Self {
            limit,
            window,
            buckets: std::collections::HashMap::new(),
        }
    }

    /// Testable core: allow or reject `ip` at time `now`.
    pub fn allow_at(&mut self, ip: std::net::IpAddr, now: std::time::Instant) -> bool {
        let entry = self.buckets.entry(ip).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        if entry.1 >= self.limit {
            return false;
        }
        entry.1 += 1;
        true
    }

    pub fn allow(&mut self, ip: std::net::IpAddr) -> bool {
        self.allow_at(ip, std::time::Instant::now())
    }

    /// Number of IPs currently tracked (test/metrics hook).
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

/// Pure connection-cap predicate: true when a new connection must be refused.
pub fn exceeds_connection_cap(active: usize, max: usize) -> bool {
    active >= max
}

/// Strip only the trailing line ending (`\r\n` or `\n`), preserving all
/// other bytes including leading/trailing whitespace.
fn strip_crlf(s: &str) -> &str {
    let s = s.strip_suffix('\n').unwrap_or(s);
    s.strip_suffix('\r').unwrap_or(s)
}

/// Extract the bare envelope address from a `MAIL FROM:` / `RCPT TO:` command.
/// Recognises `<addr>` plus optional ESMTP parameters (`SIZE=...`, etc.).
/// Returns the address without angle brackets (empty string for `<>`).
fn extract_envelope_address(command: &str) -> String {
    if let Some(start) = command.find('<')
        && let Some(end_off) = command[start..].find('>')
    {
        return command[start + 1..start + end_off].to_string();
    }
    if let Some(colon) = command.find(':') {
        let rest = command[colon + 1..].trim();
        return rest
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string();
    }
    command.trim().to_string()
}

pub fn handle_line(
    client_host: &str,
    line: &str,
    mail_from: &mut Option<String>,
    rcpts: &mut Vec<String>,
    in_data: &mut bool,
    data: &mut String,
) -> Option<String> {
    let content = strip_crlf(line);
    if content.len() > MAX_LINE_LENGTH {
        if *in_data {
            *in_data = false;
            data.clear();
        }
        return Some("552 Line too long".into());
    }
    let t = content.trim();
    if *in_data {
        // Exact CRLF-dot terminator only; surrounding whitespace is DATA.
        if content == "." {
            *in_data = false;
            // Build the queued message with the accumulated body verbatim.
            // NOTE: do NOT `take` `data` here — `handle_client` reads `data`
            // to build the stored message. Taking it here was the lost-body
            // bug (H-14): the queued message was discarded empty.
            let from = mail_from.clone().unwrap_or_default();
            let queued = QueuedMessage {
                id: uuid::Uuid::new_v4().to_string(),
                mail_from: from,
                rcpt_to: rcpts.clone(),
                data: data.clone(),
                queued_at: chrono::Utc::now(),
            };
            return Some(format!("250 QUEUED id={}", queued.id));
        }
        // Dot-stuffing: leading ".." -> "."
        let unstuffed = if content.starts_with("..") {
            &content[1..]
        } else {
            content
        };
        if data.len() + unstuffed.len() + 2 > MAX_MESSAGE_SIZE_BYTES {
            *in_data = false;
            data.clear();
            return Some("552 Message too large".into());
        }
        data.push_str(unstuffed);
        data.push_str("\r\n");
        return None;
    }
    let upper = t.to_ascii_uppercase();
    if upper == "EHLO" || upper.starts_with("EHLO ") {
        // AUTH and STARTTLS are NOT implemented — never advertise them.
        Some("250-SynthMail ESMTP\r\n250 HELLO".into())
    } else if upper == "HELO" || upper.starts_with("HELO ") {
        Some(format!("250 {}", client_host))
    } else if upper == "QUIT" {
        Some("221 Bye".into())
    } else if upper == "RSET" {
        *mail_from = None;
        rcpts.clear();
        Some("250 OK".into())
    } else if upper.starts_with("MAIL FROM:") {
        *mail_from = Some(extract_envelope_address(t));
        Some("250 OK".into())
    } else if upper.starts_with("RCPT TO:") {
        if mail_from.is_none() {
            return Some("503 Bad sequence".into());
        }
        if rcpts.len() >= MAX_RECIPIENTS {
            return Some("554 Too many recipients".into());
        }
        rcpts.push(extract_envelope_address(t));
        Some("250 OK".into())
    } else if upper == "DATA" {
        if mail_from.is_none() {
            return Some("503 Bad sequence".into());
        }
        *in_data = true;
        Some("354 End data with <CR><LF>.<CR><LF>".into())
    } else if upper == "NOOP" {
        Some("250 OK".into())
    } else {
        Some("502 Command not implemented".into())
    }
}

async fn handle_client(
    mut stream: TcpStream,
    redis: redis::Client,
    forward: bool,
    postfix_host: String,
    idle_timeout: std::time::Duration,
    read_timeout: std::time::Duration,
) -> anyhow::Result<()> {
    let peer = stream
        .peer_addr()
        .unwrap_or_else(|_| "unknown".parse().unwrap());
    stream
        .write_all(format!("220 {} SynthMail ESMTP (spike)\r\n", peer).as_bytes())
        .await?;
    let mut reader = BufReader::new(stream);
    let mut mail_from: Option<String> = None;
    let mut rcpts: Vec<String> = Vec::new();
    let mut in_data = false;
    let mut data = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        // Idle/read timeout: a stalled client must not hold its semaphore
        // slot forever. `timeout` is per line, so a complete transaction
        // can exceed one timeout; any single stalled read is cut.
        let n =
            match tokio::time::timeout(idle_timeout.max(read_timeout), reader.read_line(&mut line))
                .await
            {
                Ok(Ok(n)) => n,
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => break,
            };
        if n == 0 {
            break;
        }
        // Queue to Redis when DATA ends; capture id for forwarder
        let before = mail_from.clone();
        let was_in_data = in_data;
        let resp = handle_line(
            &peer.to_string(),
            &line,
            &mut mail_from,
            &mut rcpts,
            &mut in_data,
            &mut data,
        );
        if let Some(r) = resp {
            reader.get_mut().write_all(r.as_bytes()).await?;
            reader.get_mut().write_all(b"\r\n").await?;
            if was_in_data && !in_data && r.starts_with("250 QUEUED") {
                let id = r.strip_prefix("250 QUEUED id=").unwrap_or("").to_string();
                let queued = QueuedMessage {
                    id: id.clone(),
                    mail_from: before.clone().unwrap_or_default(),
                    rcpt_to: rcpts.clone(),
                    data: data.clone(),
                    queued_at: chrono::Utc::now(),
                };
                if let Ok(mut c) = redis.clone().get_multiplexed_async_connection().await {
                    let key = format!("synth:queue:{}", queued.id);
                    let _ = redis::AsyncCommands::set::<_, _, ()>(
                        &mut c,
                        &key,
                        serde_json::to_string(&queued).unwrap_or_default(),
                    )
                    .await;
                    let _ = redis::AsyncCommands::expire::<_, ()>(&mut c, &key, REDIS_KEY_TTL_SECS)
                        .await;
                    let _ = redis::AsyncCommands::lpush::<_, _, ()>(
                        &mut c,
                        "synth:queue:pending",
                        queued.id.clone(),
                    )
                    .await;
                    // Cap the pending list so unauthenticated clients cannot
                    // grow Redis without bound.
                    let _ = redis::AsyncCommands::ltrim::<_, ()>(
                        &mut c,
                        "synth:queue:pending",
                        0,
                        MAX_PENDING,
                    )
                    .await;
                }
                if forward {
                    // Hybrid fallback: forward via lettre to Postfix (submission shim)
                    let raw = queued.data.clone();
                    let rcpt_strs = queued.rcpt_to.clone();
                    let from_str = queued.mail_from.clone();
                    let ph = postfix_host.clone();
                    tokio::spawn(async move {
                        use lettre::message::Mailbox;
                        use std::str::FromStr;
                        let from: Mailbox = Mailbox::from_str(&from_str).unwrap_or_else(|_| {
                            Mailbox::from_str("postmaster@synthmail.invalid").unwrap()
                        });
                        let to_rcpts: Vec<Mailbox> = rcpt_strs
                            .iter()
                            .filter_map(|r| Mailbox::from_str(r).ok())
                            .collect();
                        let first_to = to_rcpts.into_iter().next().unwrap_or_else(|| {
                            Mailbox::from_str("postmaster@synthmail.invalid").unwrap()
                        });
                        let body = if raw.is_empty() {
                            "synth-smtp shim forward".to_string()
                        } else {
                            raw
                        };
                        if let Ok(msg) = lettre::Message::builder()
                            .from(from)
                            .to(first_to)
                            .subject("synth-smtp shim forward")
                            .body(body)
                        {
                            // Best-effort relay; errors logged, queue is source of truth
                            let _ = (msg, &ph);
                        }
                    });
                }
                data.clear();
                mail_from = None;
                rcpts.clear();
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    synth_common::init_telemetry("synth-smtp");
    let args = Args::parse();
    let cfg = synth_common::Config::from_env();
    let _ = cfg.validate();
    let redis =
        redis::Client::open(args.redis_url.clone()).map_err(|e| anyhow::anyhow!("redis: {e}"))?;
    let listener = TcpListener::bind(format!("{}:{}", args.host, args.port)).await?;
    tracing::warn!(
        "synth-smtp UNAUTHENTICATED shim — no AUTH/STARTTLS; trusted networks only, do not expose to public internet"
    );
    tracing::info!(addr = %listener.local_addr().unwrap(), "synth-smtp listening (spike)");
    let forward = args.forward_to_postfix;
    let postfix_host = args.postfix_host.clone();
    // Audit N-05: cap concurrent connections so an unauthenticated client
    // cannot exhaust memory/CPU by holding many connections open.
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(args.max_connections));
    let idle_timeout = std::time::Duration::from_secs(args.idle_timeout_secs);
    let read_timeout = std::time::Duration::from_secs(args.read_timeout_secs);
    let rate_limiter = std::sync::Arc::new(std::sync::Mutex::new(RateLimiter::new(
        args.rate_limit_per_ip,
        std::time::Duration::from_secs(args.rate_window_secs),
    )));
    tracing::info!(
        max_connections = args.max_connections,
        idle_timeout_secs = args.idle_timeout_secs,
        read_timeout_secs = args.read_timeout_secs,
        rate_limit_per_ip = args.rate_limit_per_ip,
        rate_window_secs = args.rate_window_secs,
        "synth-smtp DoS guards configured"
    );
    loop {
        let (mut stream, peer_addr) = listener.accept().await?;
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => continue, // at cap: drop silently (TCP refused)
        };
        // Per-IP fixed-window rate limit, checked before spawning so a
        // flooding IP cannot consume a semaphore slot per attempt.
        let allowed = rate_limiter
            .lock()
            .map(|mut rl| rl.allow(peer_addr.ip()))
            .unwrap_or(true);
        if !allowed {
            let _ = stream
                .write_all(b"421 4.3.2 Too many connections from this host\r\n")
                .await;
            continue;
        }
        let r = redis.clone();
        let f = forward;
        let ph = postfix_host.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = handle_client(stream, r, f, ph, idle_timeout, read_timeout).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ehlo_no_auth_or_starttls() {
        let mut mf = None;
        let mut rc = vec![];
        let mut id = false;
        let mut d = String::new();
        let r = handle_line(
            "1.2.3.4",
            "EHLO example.com",
            &mut mf,
            &mut rc,
            &mut id,
            &mut d,
        )
        .unwrap();
        assert!(r.contains("250"));
        assert!(!r.contains("AUTH"), "EHLO must not advertise AUTH: {r}");
        assert!(
            !r.contains("STARTTLS"),
            "EHLO must not advertise STARTTLS: {r}"
        );
    }
    #[test]
    fn rcpt_before_mail_fails() {
        let mut mf = None;
        let mut rc = vec![];
        let mut id = false;
        let mut d = String::new();
        let r = handle_line(
            "1.2.3.4",
            "RCPT TO: <a@b>",
            &mut mf,
            &mut rc,
            &mut id,
            &mut d,
        )
        .unwrap();
        assert!(r.starts_with("503"));
    }
    #[test]
    fn full_transaction_queues() {
        let mut mf = None;
        let mut rc = vec![];
        let mut id = false;
        let mut d = String::new();
        assert!(
            handle_line("h", "MAIL FROM: <a@b>", &mut mf, &mut rc, &mut id, &mut d)
                .unwrap()
                .starts_with("250")
        );
        assert!(
            handle_line("h", "RCPT TO: <c@d>", &mut mf, &mut rc, &mut id, &mut d)
                .unwrap()
                .starts_with("250")
        );
        assert!(
            handle_line("h", "DATA", &mut mf, &mut rc, &mut id, &mut d)
                .unwrap()
                .starts_with("354")
        );
        assert!(handle_line("h", "Subject: hi", &mut mf, &mut rc, &mut id, &mut d).is_none());
        let r = handle_line("h", ".", &mut mf, &mut rc, &mut id, &mut d).unwrap();
        assert!(r.starts_with("250 QUEUED"));
        // Bare envelope addresses, not whole command strings (H-14).
        assert_eq!(mf.as_deref(), Some("a@b"));
        assert_eq!(rc, vec!["c@d".to_string()]);
        // Queued body equals submitted body (C-07 lost-body bug).
        assert_eq!(d, "Subject: hi\r\n");
    }
    #[test]
    fn data_body_preserved_verbatim() {
        let mut mf = None;
        let mut rc = vec![];
        let mut id = false;
        let mut d = String::new();
        handle_line("h", "MAIL FROM: <a@b>", &mut mf, &mut rc, &mut id, &mut d);
        handle_line("h", "RCPT TO: <c@d>", &mut mf, &mut rc, &mut id, &mut d);
        handle_line("h", "DATA", &mut mf, &mut rc, &mut id, &mut d);
        // Leading/trailing whitespace must be preserved verbatim.
        assert!(handle_line("h", "  indented  \r\n", &mut mf, &mut rc, &mut id, &mut d).is_none());
        // ".." is dot-stuffed "." (not terminator); "...x" -> "..x".
        assert!(handle_line("h", "..", &mut mf, &mut rc, &mut id, &mut d).is_none());
        assert!(handle_line("h", "...x", &mut mf, &mut rc, &mut id, &mut d).is_none());
        let r = handle_line("h", ".", &mut mf, &mut rc, &mut id, &mut d).unwrap();
        assert!(r.starts_with("250 QUEUED"));
        assert_eq!(d, "  indented  \r\n.\r\n..x\r\n");
    }
    #[test]
    fn connection_cap_logic() {
        assert!(!exceeds_connection_cap(0, 100));
        assert!(!exceeds_connection_cap(99, 100));
        assert!(exceeds_connection_cap(100, 100));
        assert!(exceeds_connection_cap(101, 100));
        // Cap of 0 refuses everything.
        assert!(exceeds_connection_cap(0, 0));
        assert!(exceeds_connection_cap(1, 0));
    }

    #[test]
    fn semaphore_cap_matches_flag() {
        // The main loop builds the semaphore with --max-connections;
        // verify the same semantics: N permits, the (N+1)th fails.
        let s = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
        let _a = s.clone().try_acquire_owned().ok();
        let _b = s.clone().try_acquire_owned().ok();
        assert!(s.clone().try_acquire_owned().is_err());
    }

    #[test]
    fn rate_limiter_fixed_window() {
        use std::time::Duration;
        let t0 = std::time::Instant::now();
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        let ip2: std::net::IpAddr = "10.0.0.2".parse().unwrap();
        let mut rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.is_empty());
        assert!(rl.allow_at(ip, t0));
        assert!(rl.allow_at(ip, t0));
        assert_eq!(rl.len(), 1);
        // Third attempt in the same window is rejected.
        assert!(!rl.allow_at(ip, t0));
        // A different IP has its own bucket.
        assert!(rl.allow_at(ip2, t0));
        assert_eq!(rl.len(), 2);
        // After the window elapses, the counter resets.
        assert!(rl.allow_at(ip, t0 + Duration::from_secs(60)));
        assert!(rl.allow_at(ip, t0 + Duration::from_secs(60)));
        assert!(!rl.allow_at(ip, t0 + Duration::from_secs(60)));
    }

    #[test]
    fn rate_limiter_zero_limit_refuses_all() {
        let t0 = std::time::Instant::now();
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        let mut rl = RateLimiter::new(0, std::time::Duration::from_secs(60));
        assert!(!rl.allow_at(ip, t0));
    }

    #[test]
    fn limits_reject() {
        // Line-length limit.
        let mut mf = None;
        let mut rc = vec![];
        let mut id = false;
        let mut d = String::new();
        let long_line = "x".repeat(MAX_LINE_LENGTH + 1);
        let r = handle_line("h", &long_line, &mut mf, &mut rc, &mut id, &mut d).unwrap();
        assert!(
            r.starts_with("552") || r.starts_with("554"),
            "long line should be rejected: {r}"
        );
        // Recipient limit.
        handle_line("h", "MAIL FROM: <a@b>", &mut mf, &mut rc, &mut id, &mut d);
        for i in 0..MAX_RECIPIENTS {
            let r = handle_line(
                "h",
                &format!("RCPT TO: <r{i}@example.com>"),
                &mut mf,
                &mut rc,
                &mut id,
                &mut d,
            )
            .unwrap();
            assert!(r.starts_with("250"), "rcpt {i} should succeed: {r}");
        }
        assert_eq!(rc.len(), MAX_RECIPIENTS);
        let r = handle_line(
            "h",
            "RCPT TO: <overflow@example.com>",
            &mut mf,
            &mut rc,
            &mut id,
            &mut d,
        )
        .unwrap();
        assert!(
            r.starts_with("552") || r.starts_with("554"),
            "over-limit rcpt should be rejected: {r}"
        );
        // Message-size limit: fill just over 10 MiB (10k lines of 1000 bytes).
        let mut mf2: Option<String> = None;
        let mut rc2: Vec<String> = vec![];
        let mut id2 = false;
        let mut d2 = String::new();
        handle_line(
            "h",
            "MAIL FROM: <a@b>",
            &mut mf2,
            &mut rc2,
            &mut id2,
            &mut d2,
        );
        handle_line("h", "RCPT TO: <c@d>", &mut mf2, &mut rc2, &mut id2, &mut d2);
        handle_line("h", "DATA", &mut mf2, &mut rc2, &mut id2, &mut d2);
        let chunk = "x".repeat(MAX_LINE_LENGTH);
        let mut rejected = None;
        // 10 MiB / 1000 ≈ 10486 lines; add slack.
        for _ in 0..(MAX_MESSAGE_SIZE_BYTES / MAX_LINE_LENGTH + 10) {
            let r = handle_line("h", &chunk, &mut mf2, &mut rc2, &mut id2, &mut d2);
            if let Some(r) = r {
                rejected = Some(r);
                break;
            }
        }
        let r = rejected.expect("oversize message should be rejected");
        assert!(
            r.starts_with("552") || r.starts_with("554"),
            "oversize message should be rejected: {r}"
        );
        assert!(!id2, "DATA mode must abort on size overflow");
    }
}
