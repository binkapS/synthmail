//! IMAP proxy helpers via async-imap.
//!
//! Connects to Dovecot IMAP (internal TLS) and translates to JSON for webmail API.

use anyhow::{Context, Result};
use async_imap::Client as ImapClient;
use std::net::SocketAddr;

pub struct MailMessage {
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub preview: String,
    pub seen: bool,
}

pub async fn list_folders(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    use_tls: bool,
) -> Result<Vec<String>> {
    // For Phase 1 we use dovecot IMAP; addr resolution via tokio
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .or_else(|_| {
            // Fallback: resolve "dovecot" host via DNS
            // In Docker, "dovecot" resolves to container IP
            format!("{}:{}", "127.0.0.1", port)
                .parse()
                .map_err(anyhow::Error::from)
        })
        .context("parse imap addr")?;

    // NOTE: Full TLS implementation uses native-tls / rustls. For MVP we call via
    // IMAP session and leave TLS to use_tls flag.
    // async-imap requires a Tokio stream; we use TcpStream + optional upgrade.
    let _ = (user, pass, use_tls);
    let _ = addr;
    // Stubbed for Phase 0 skeleton; real implementation in Phase 1 integration tests.
    // Return standard Dovecot folders.
    Ok(vec![
        "INBOX".into(),
        "Sent".into(),
        "Drafts".into(),
        "Junk".into(),
        "Trash".into(),
    ])
}

pub async fn fetch_messages(
    _host: &str,
    _port: u16,
    _user: &str,
    _pass: &str,
    _folder: &str,
    _limit: usize,
) -> Result<Vec<MailMessage>> {
    // Phase 1: SELECT folder, FETCH 1:* (FLAGS ENVELOPE BODY[HEADER.FIELDS...] BODY[TEXT]<0.200>)
    // Implemented against Dovecot in integration test with greenmail.
    Ok(vec![])
}

pub async fn send_via_smtp(
    _host: &str,
    _port: u16,
    _user: &str,
    _pass: &str,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    // Uses lettre with STARTTLS (submission 587). Credentials via SASL.
    // Construct MIME message
    let _ = (from, to, subject, body);
    // lettre example:
    // let email = Message::builder().from(parse(from)?).to(parse(to)?).subject(subject).body(body.to_string())?;
    // SmtpTransport::relay(host)?.credentials(Credentials::new(user.into(), pass.into())).send(&email)?;
    tracing::info!(
        from,
        to,
        subject,
        "send_via_smtp stubbed - wire to Postfix submission in Phase 1"
    );
    Ok(())
}

pub fn sanitize_preview(body: &str) -> String {
    body.chars()
        .take(200)
        .collect::<String>()
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_sanitizes() {
        assert_eq!(sanitize_preview("hello\nworld"), "hello world");
    }
}
