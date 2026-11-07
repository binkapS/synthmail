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

/// Delete a message by moving it to Trash; the Maildir equivalent of
/// IMAP `EXPUNGE`-after-delete. Returns true when a file was moved.
/// When `folder` is already `Trash`, `move_to_trash` renames the file
/// in place and still returns true (documented no-op corner).
pub async fn delete_message(root: &str, email: &str, folder: &str, uid: u32) -> Result<bool> {
    move_to_trash(root, email, folder, uid).await
}

fn valid_folder(folder: &str) -> Result<()> {
    if folder.is_empty()
        || folder.contains('/')
        || folder.contains('\\')
        || folder.contains('\0')
        || folder == "."
        || folder == ".."
    {
        anyhow::bail!("invalid folder name");
    }
    Ok(())
}

static CREATE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Build the unique Maildir filename base (without the `:2,` charset
/// suffix): unix-timestamp micros + pid + monotonic counter + 4 random
/// bytes from a v4 UUID. Std-only entropy (no new deps); collisions are
/// still detected against the destination directory in `create_message`.
fn unique_filename_base() -> String {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    let counter = CREATE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let bytes = uuid::Uuid::new_v4().into_bytes();
    format!(
        "{micros}.{}.{counter:x}.{:02x}{:02x}{:02x}{:02x}",
        std::process::id(),
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3]
    )
}

/// Create a message in the given folder's `cur/` directory and return
/// its positional UID (the same numbering `fetch_messages` exposes, so
/// `create_message` -> `fetch_message` round-trips).
///
/// The message is written to `tmp/` first and renamed into `cur/` so a
/// concurrent reader never sees a partial file; the final file is
/// created with mode `0600` on Unix. `raw` is normalized to end with
/// CRLF. Folders are validated (no path separators) and the caller
/// (routes) already maps JMAP mailbox ids onto fixed, safe names.
pub async fn create_message(root: &str, email: &str, folder: &str, raw: &[u8]) -> Result<u32> {
    valid_folder(folder)?;
    let mbox = mailbox_dir(root, email).context("bad email")?;
    let dir = folder_dir(&mbox, folder);
    let cur = dir.join("cur");
    let tmp = dir.join("tmp");
    std::fs::create_dir_all(&cur).context("create cur")?;
    std::fs::create_dir_all(&tmp).context("create tmp")?;
    let mut bytes = raw.to_vec();
    if !bytes.ends_with(b"\r\n") {
        if bytes.ends_with(b"\n") {
            let len = bytes.len();
            bytes.insert(len - 1, b'\r');
        } else {
            bytes.extend_from_slice(b"\r\n");
        }
    }
    let mut last_err = None;
    for _ in 0..8 {
        let name = format!("{}:2,", unique_filename_base());
        let tmp_path = tmp.join(&name);
        let cur_path = cur.join(&name);
        if tmp_path.exists() || cur_path.exists() {
            continue;
        }
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        match opts.open(&tmp_path) {
            Ok(mut f) => {
                if f.write_all(&bytes).is_err() {
                    last_err = Some("write message failed".to_string());
                    let _ = std::fs::remove_file(&tmp_path);
                    continue;
                }
            }
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        }
        if let Err(e) = std::fs::rename(&tmp_path, &cur_path) {
            last_err = Some(e.to_string());
            let _ = std::fs::remove_file(&tmp_path);
            continue;
        }
        let files = list_files(&dir);
        let pos = files
            .iter()
            .position(|p| p == &cur_path)
            .context("created message not found in listing")?;
        return Ok((pos + 1) as u32);
    }
    anyhow::bail!(last_err.unwrap_or_else(|| "could not create unique filename".to_string()))
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

    fn fixture_maildir_empty() -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("synthmail-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("example.com/alice")).unwrap();
        (dir, "alice@example.com".to_string())
    }

    #[tokio::test]
    async fn create_and_delete_roundtrip() {
        let (root, email) = fixture_maildir_empty();
        let root_s = root.to_string_lossy().into_owned();
        let raw = b"From: alice@example.com\r\nTo: bob@example.org\r\nSubject: hi\r\n\r\nbody\r\n";
        let uid = create_message(&root_s, &email, "Drafts", raw)
            .await
            .unwrap();
        assert_eq!(uid, 1);
        let cur = root.join("example.com/alice/.Drafts/cur");
        let files: Vec<_> = std::fs::read_dir(&cur).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let written = &files[0].path();
        assert!(
            written
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with(":2,"),
            "charset suffix present"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(written).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let msg = fetch_message(&root_s, &email, "Drafts", uid).await.unwrap();
        assert_eq!(msg.subject, "hi");
        assert!(
            delete_message(&root_s, &email, "Drafts", uid)
                .await
                .unwrap()
        );
        assert!(
            fetch_messages(&root_s, &email, "Drafts", 10)
                .await
                .unwrap()
                .is_empty()
        );
        let trash = root.join("example.com/alice/.Trash/cur");
        let trashed: Vec<_> = std::fs::read_dir(&trash).unwrap().flatten().collect();
        assert_eq!(trashed.len(), 1);
        assert!(
            !delete_message(&root_s, &email, "Drafts", uid)
                .await
                .unwrap()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn create_rejects_bad_folder_and_email() {
        let (root, email) = fixture_maildir_empty();
        let root_s = root.to_string_lossy().into_owned();
        assert!(
            create_message(&root_s, &email, "../etc", b"Subject: x\n\n")
                .await
                .is_err()
        );
        assert!(
            create_message(&root_s, "not-an-email", "Drafts", b"Subject: x\n\n")
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
