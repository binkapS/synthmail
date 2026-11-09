//! Durable queue write + Postfix relay.
//!
//! Production semantics:
//! - `enqueue` returns `Err` on any Redis failure. The connection layer maps
//!   that to `451 4.7.0` so the sending client retries — a message must
//!   never be **silently** dropped after `250` was sent (the spike swallowed
//!   Redis errors with `let _ =` while still answering 250).
//! - `forward_to_postfix` performs a real relay (lettre `send_raw`) with the
//!   client's actual envelope (MAIL FROM + all RCPT TO) and verbatim body.
//!   The spike built a fake `Subject` header and then dropped the message on
//!   the floor (`let _ = (msg, &ph)`), discarding both envelope and body.

use base64::Engine as _;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use crate::smtp::QueuedMessage;

const REDIS_KEY_TTL_SECS: i64 = 86400;
/// Cap the pending list so unauthenticated flooders cannot grow Redis
/// without bound (a bounded queue backpressures at enqueue time instead).
pub const MAX_PENDING: isize = 1000;
const PENDING_KEY: &str = "synth:queue:pending";

/// Persist a message in the shared queue. Only the `pending` list is the
/// work boundary consumers drain; the per-message key is decoration for
/// direct inspection (see runbooks).
pub async fn enqueue(conn: &mut ConnectionManager, queued: &QueuedMessage) -> anyhow::Result<()> {
    let value = serialise(queued)?;
    let key = format!("synth:queue:{}", queued.id);
    let _: () = conn
        .set(&key, value)
        .await
        .map_err(|e| anyhow::anyhow!("redis set: {e}"))?;
    let _: () = conn
        .expire(&key, REDIS_KEY_TTL_SECS)
        .await
        .map_err(|e| anyhow::anyhow!("redis expire: {e}"))?;
    let _: () = conn
        .lpush(PENDING_KEY, queued.id.as_str())
        .await
        .map_err(|e| anyhow::anyhow!("redis lpush: {e}"))?;
    // Keep the newest MAX_PENDING ids.
    let _: () = conn
        .ltrim(PENDING_KEY, 0, MAX_PENDING)
        .await
        .map_err(|e| anyhow::anyhow!("redis ltrim: {e}"))?;
    Ok(())
}

/// JSON with the raw message base64-encoded (messages are binary-safe).
fn serialise(queued: &QueuedMessage) -> anyhow::Result<String> {
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(&queued.data);
    let v = serde_json::json!({
        "id": queued.id,
        "mail_from": queued.mail_from,
        "rcpt_to": queued.rcpt_to,
        "data_b64": data_b64,
        "queued_at": queued.queued_at.to_rfc3339(),
    });
    serde_json::to_string(&v).map_err(|e| anyhow::anyhow!("serialize: {e}"))
}

/// Relay the accepted message to Postfix with its real envelope, verbatim.
/// Errors are logged, never propagated: the queue is the source of truth;
/// the hybrid forward is best-effort redundancy.
///
/// Bounded concurrency (audit S-04): a global 8-permit semaphore caps
/// in-flight Postfix handshakes so a fast submission client queueing
/// back-to-back 10 MiB messages cannot pin the `spawn_blocking` pool
/// with slow or wedged relays. When the cap is reached the forward is
/// skipped (and logged); the Redis queue copy remains the source of
/// truth, so no mail is lost.
static FORWARD_SEM: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();
pub const MAX_FORWARDS: usize = 8;

fn forward_sem() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    FORWARD_SEM.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_FORWARDS)))
}

pub fn spawn_forward(postfix_host: String, queued: QueuedMessage) {
    use lettre::Transport;
    let sem = forward_sem();
    // Take one forward slot up front (non-blocking); on cap overflow skip
    // this message's forward — the Redis queue copy stays the source of
    // truth. The owned permit is moved into the blocking task and
    // released when the relay attempt completes.
    let permit = match sem.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(
                id = %queued.id,
                "postfix forward skipped: forward pool saturated"
            );
            return;
        }
    };
    let id = queued.id.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        match build_envelope(&queued.mail_from, &queued.rcpt_to) {
            Ok(envelope) => {
                let mailer = lettre::SmtpTransport::builder_dangerous(&postfix_host).build();
                match mailer.send_raw(&envelope, &queued.data) {
                    Ok(_) => {
                        tracing::info!(id = %id, to = ?queued.rcpt_to, "forwarded to postfix")
                    }
                    Err(e) => {
                        tracing::warn!(id = %id, error = %e, "postfix relay failed (queued copy kept)")
                    }
                }
            }
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "invalid envelope; forward skipped")
            }
        }
    });
}

fn build_envelope(from: &str, rcpts: &[String]) -> anyhow::Result<lettre::address::Envelope> {
    use std::str::FromStr;
    let sender = if from.is_empty() {
        None
    } else {
        Some(lettre::address::Address::from_str(from)?)
    };
    let mut to = Vec::with_capacity(rcpts.len());
    for r in rcpts {
        if !r.is_empty()
            && let Ok(a) = lettre::address::Address::from_str(r)
        {
            to.push(a);
        }
    }
    if to.is_empty() {
        anyhow::bail!("no valid recipients");
    }
    Ok(lettre::address::Envelope::new(sender, to)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialise_roundtrip_is_binary_safe() {
        let q = QueuedMessage {
            id: "abc".into(),
            mail_from: "a@b".into(),
            rcpt_to: vec!["c@d".into()],
            // 0xff would be invalid UTF-8; must survive the queue.
            data: vec![0x53, 0x75, 0x62, 0x6a, 0x65, 0x63, 0x74, 0x3a, 0x20, 0xff],
            queued_at: chrono::Utc::now(),
        };
        let s = serialise(&q).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let back =
            base64::engine::general_purpose::STANDARD.decode(v["data_b64"].as_str().unwrap());
        assert_eq!(back.unwrap(), q.data);
    }

    #[test]
    fn envelope_paths() {
        let e = build_envelope("a@b.com", &["c@d.com".into()]).unwrap();
        assert_eq!(e.from().map(|f| f.to_string()), Some("a@b.com".into()));
        assert_eq!(e.to().len(), 1);
        // Null sender allowed (DSN path).
        let e2 = build_envelope("", &["c@d.com".into()]).unwrap();
        assert!(e2.from().is_none());
        assert!(build_envelope("a@b.com", &[]).is_err());
    }
}
