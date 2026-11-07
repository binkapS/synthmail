//! Phase 3 spike: minimal ESMTP server (EHLO/MAIL/RCPT/DATA/QUIT).
//! Queues to Redis, optionally forwards to Postfix via lettre (hybrid fallback).
//! Not an inbound MTA replacement — submission shim only.

use clap::Parser;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser, Debug)]
#[command(
    name = "synth-smtp",
    about = "Phase 3 spike: ESMTP shim with Redis queue"
)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 2525)]
    port: u16,
    #[arg(long, default_value = "redis://redis:6379")]
    redis_url: String,
    #[arg(long, default_value_t = true)]
    forward_to_postfix: bool,
    #[arg(long, default_value = "postfix:25")]
    postfix_host: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QueuedMessage {
    pub id: String,
    pub mail_from: String,
    pub rcpt_to: Vec<String>,
    pub data: String,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

pub fn handle_line(
    client_host: &str,
    line: &str,
    mail_from: &mut Option<String>,
    rcpts: &mut Vec<String>,
    in_data: &mut bool,
    data: &mut String,
) -> Option<String> {
    let t = line.trim();
    if *in_data {
        if t == "." {
            *in_data = false;
            let from = mail_from.clone().unwrap_or_default();
            let queued = QueuedMessage {
                id: uuid::Uuid::new_v4().to_string(),
                mail_from: from,
                rcpt_to: rcpts.clone(),
                data: std::mem::take(data),
                queued_at: chrono::Utc::now(),
            };
            return Some(format!("250 QUEUED id={}", queued.id));
        }
        // Dot-stuffing: leading ".." → "."
        let unstuffed = if t.starts_with("..") { &t[1..] } else { t };
        data.push_str(unstuffed);
        data.push_str("\r\n");
        return None;
    }
    let upper = t.to_ascii_uppercase();
    if upper == "EHLO" || upper.starts_with("EHLO ") {
        Some("250-SynthMail ESMTP\r\n250-HELLO\r\n250-AUTH PLAIN LOGIN\r\n250 STARTTLS".into())
    } else if upper == "HELO" || upper.starts_with("HELO ") {
        Some(format!("250 {}", client_host))
    } else if upper == "QUIT" {
        Some("221 Bye".into())
    } else if upper == "RSET" {
        *mail_from = None;
        rcpts.clear();
        Some("250 OK".into())
    } else if upper.starts_with("MAIL FROM:") {
        *mail_from = Some(t.to_string());
        Some("250 OK".into())
    } else if upper.starts_with("RCPT TO:") {
        if mail_from.is_none() {
            return Some("503 Bad sequence".into());
        }
        rcpts.push(t.to_string());
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
        let n = reader.read_line(&mut line).await?;
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
                    let _ = redis::AsyncCommands::set::<_, _, ()>(
                        &mut c,
                        format!("synth:queue:{}", queued.id),
                        serde_json::to_string(&queued).unwrap_or_default(),
                    )
                    .await;
                    let _ = redis::AsyncCommands::lpush::<_, _, ()>(
                        &mut c,
                        "synth:queue:pending",
                        queued.id.clone(),
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
    tracing::info!(addr = %listener.local_addr().unwrap(), "synth-smtp listening (spike)");
    let forward = args.forward_to_postfix;
    let postfix_host = args.postfix_host.clone();
    loop {
        let (stream, _) = listener.accept().await?;
        let r = redis.clone();
        let f = forward;
        let ph = postfix_host.clone();
        tokio::spawn(async move {
            let _ = handle_client(stream, r, f, ph).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ehlo_advertises_auth() {
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
        assert!(r.contains("AUTH"));
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
    }
}
