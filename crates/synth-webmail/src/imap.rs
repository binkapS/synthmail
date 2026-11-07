//! Mail access via the shared Maildir volume (no plaintext passwords needed).
//!
//! webmail authenticates users against Postgres (Argon2id hashes), so it
//! cannot do password SASL against Dovecot. Instead it reads the same
//! Maildir Dovecot/postfix write (`/var/vmail/<domain>/<localpart>/`,
//! per the postfix virtual_mailbox_maps), with every path scoped to the
//! authenticated principal's own email address. Sending goes out over SMTP
//! to postfix on the mailnet (network-trusted; external submission still
//! requires SASL — see docs/runbooks/security.md).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct MailMessage {
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub date: String,
    pub preview: String,
    pub seen: bool,
    pub message_id: Option<String>,
}

/// Resolve the Maildir base for an email address. Returns None for
/// malformed addresses (defense in depth — callers pass the DB email).
pub fn mailbox_dir(root: &str, email: &str) -> Option<PathBuf> {
    let (local, domain) = email.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
        || local.contains('/')
        || local.contains('\\')
        || domain.contains('/')
        || domain.contains('\\')
    {
        return None;
    }
    Some(PathBuf::from(root).join(domain).join(local))
}

fn header_value(raw: &str, name: &str) -> Option<String> {
    // Unfold continuation lines, then match case-insensitively.
    let mut unfolded = String::new();
    for line in raw.split('\n') {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix([' ', '\t'])
            && !unfolded.is_empty()
        {
            unfolded.push(' ');
            unfolded.push_str(rest);
            continue;
        }
        if !unfolded.is_empty() {
            unfolded.push('\n');
        }
        unfolded.push_str(line);
    }
    for line in unfolded.split('\n') {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case(name)
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn parse_message(uid: u32, path: &Path) -> Result<MailMessage> {
    let raw = std::fs::read_to_string(path).context("read message")?;
    let seen = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.contains(":2,S") || n.contains(":2,RS") || n.contains(":2,SR"))
        .unwrap_or(false);
    let body_start = raw.find("\n\n").map(|i| i + 2).unwrap_or(raw.len());
    let preview = raw[body_start..]
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    Ok(MailMessage {
        uid,
        subject: header_value(&raw, "subject").unwrap_or_default(),
        from: header_value(&raw, "from").unwrap_or_default(),
        date: header_value(&raw, "date").unwrap_or_default(),
        preview: sanitize_preview(&preview),
        seen,
        message_id: header_value(&raw, "message-id"),
    })
}

/// Map a Dovecot folder name onto the Maildir directory:
/// INBOX is the mailbox root (cur/new/tmp); other folders are dot-dirs
/// (`.Sent`, `.Drafts`, `.Junk`, `.Trash`).
pub fn folder_dir(mbox: &Path, folder: &str) -> PathBuf {
    if folder.eq_ignore_ascii_case("INBOX") {
        mbox.to_path_buf()
    } else {
        mbox.join(format!(".{}", folder))
    }
}

fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for sub in ["cur", "new"] {
        let subdir = dir.join(sub);
        if let Ok(entries) = std::fs::read_dir(&subdir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_file() {
                    files.push(p);
                }
            }
        }
    }
    files.sort_by_key(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
    files
}

pub async fn list_folders(root: &str, email: &str) -> Result<Vec<String>> {
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let mut folders = vec!["INBOX".to_string()];
    if let Ok(entries) = std::fs::read_dir(&mbox) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir()
                && let Some(name) = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.strip_prefix('.'))
                && !name.is_empty()
                && !matches!(name, "cur" | "new" | "tmp")
            {
                folders.push(name.to_string());
            }
        }
    }
    folders.sort();
    folders.dedup();
    Ok(folders)
}

pub async fn fetch_messages(
    root: &str,
    email: &str,
    folder: &str,
    limit: usize,
) -> Result<Vec<MailMessage>> {
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let dir = folder_dir(&mbox, folder);
    let files = list_files(&dir);
    let mut out = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if out.len() >= limit {
            break;
        }
        if let Ok(m) = parse_message((i + 1) as u32, f) {
            out.push(m);
        }
    }
    Ok(out)
}

pub async fn fetch_message(root: &str, email: &str, folder: &str, uid: u32) -> Result<MailMessage> {
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let dir = folder_dir(&mbox, folder);
    let files = list_files(&dir);
    let f = files
        .get((uid - 1) as usize)
        .context("message uid out of range")?;
    parse_message(uid, f)
}

/// Full raw message by uid (for JMAP Email/get).
pub async fn fetch_raw(root: &str, email: &str, folder: &str, uid: u32) -> Result<String> {
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let dir = folder_dir(&mbox, folder);
    let f = list_files(&dir)
        .into_iter()
        .nth((uid - 1) as usize)
        .context("message uid out of range")?;
    std::fs::read_to_string(&f).context("read raw message")
}

/// Move a message to Trash (drafts delete). Returns true if moved.
pub async fn move_to_trash(root: &str, email: &str, folder: &str, uid: u32) -> Result<bool> {
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let dir = folder_dir(&mbox, folder);
    let files = list_files(&dir);
    let Some(src) = files.get((uid - 1) as usize) else {
        return Ok(false);
    };
    let trash = mbox.join(".Trash/cur");
    std::fs::create_dir_all(&trash).context("create trash")?;
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| uid.to_string());
    std::fs::rename(src, trash.join(name)).context("move to trash")?;
    Ok(true)
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
    use std::io::Write;

    #[test]
    fn preview_sanitizes() {
        assert_eq!(sanitize_preview("hello\nworld"), "hello world");
    }

    #[test]
    fn mailbox_dir_rejects_traversal() {
        assert!(mailbox_dir("/var/vmail", "../etc/passwd").is_none());
        assert!(mailbox_dir("/var/vmail", "a/b@example.com").is_none());
        assert_eq!(
            mailbox_dir("/var/vmail", "alice@example.com").unwrap(),
            std::path::PathBuf::from("/var/vmail/example.com/alice")
        );
    }

    fn fixture_maildir() -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("synthmail-test-{}", uuid::Uuid::new_v4()));
        let mbox = dir.join("example.com/alice");
        std::fs::create_dir_all(mbox.join("cur")).unwrap();
        let msg = b"From: bob@example.org\r\nSubject: Hello\r\nDate: Sat, 07 Nov 2026 12:00:00 +0100\r\nMessage-ID: <m1@example.org>\r\n\r\nBody text here\r\n";
        let mut f = std::fs::File::create(mbox.join("cur/1234567.hostname:2,S")).unwrap();
        f.write_all(msg).unwrap();
        (dir, "alice@example.com".to_string())
    }

    #[tokio::test]
    async fn list_and_fetch_roundtrip() {
        let (root, email) = fixture_maildir();
        let root_s = root.to_string_lossy().into_owned();
        let folders = list_folders(&root_s, &email).await.unwrap();
        assert!(folders.contains(&"INBOX".to_string()));
        let msgs = fetch_messages(&root_s, &email, "INBOX", 10).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].subject, "Hello");
        assert_eq!(msgs[0].from, "bob@example.org");
        assert!(msgs[0].seen);
        let one = fetch_message(&root_s, &email, "INBOX", 1).await.unwrap();
        assert_eq!(one.uid, 1);
        assert!(fetch_message(&root_s, &email, "INBOX", 9).await.is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
