//! Pure ESMTP session state machine (no I/O), unit-tested here.
//!
//! Production role:
//! - STARTTLS: advertised when the server was started with certificate/key
//!   files (`--tls-cert`/`--tls-key`). Issuing STARTTLS resets the session
//!   (RFC 3207 §4) — no prior transaction state survives the upgrade.
//! - AUTH (SASL PLAIN / LOGIN, RFC 4954): accepted only over an active TLS
//!   session. Verification happens in [`crate::auth`]; this module tracks
//!   SASL step state and yields [`Action::AuthVerify`] with decoded
//!   credentials for the I/O layer to check against the user store.
//! - Relay policy: authenticated users may submit to any domain
//!   (submission). Unauthenticated sessions may only deliver to
//!   `relay_domains` (the local mail domain); anything else gets
//!   `554 5.7.1 Relay access denied`. The shim is therefore never an open
//!   relay; `--open-relay` restores the old trusted-networks-only spike
//!   behavior explicitly (logged loudly at startup).

/// RFC 5321 §4.5.3.1 sizes.
pub const MAX_MESSAGE_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
pub const MAX_RECIPIENTS: usize = 50;
pub const MAX_LINE_LENGTH: usize = 1000; // octets incl. CRLF

/// SASL mechanism selected by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMech {
    Plain,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthStep {
    /// Waiting for the base64 client message carrying the credentials
    /// (PLAIN follow-up) or the username (LOGIN follow-up).
    ExpectUserOrPlain,
    /// LOGIN: username seen, waiting for the base64 password.
    ExpectLoginPass,
}

/// What the I/O layer must do after processing one line.
#[derive(Debug)]
pub enum Action {
    /// Send this reply, then continue reading.
    Reply(String),
    /// A DATA payload line was accepted (no reply).
    DataLine,
    /// DATA terminator seen and message assembled. The I/O layer queues it
    /// and replies `250 QUEUED id=...` on success or `451...` on failure —
    /// queue loss must be visible to the sender, never silent.
    TransactionQueued(QueuedMessage),
    /// Client asked for STARTTLS with TLS configured; the I/O layer performs
    /// the handshake and flips `tls_active`.
    StartTls,
    /// SASL verification request with already-decoded credentials.
    AuthVerify {
        email: String,
        secret: String,
        mech: AuthMech,
    },
}

#[derive(Debug, Clone)]
pub struct QueuedMessage {
    pub id: String,
    pub mail_from: String,
    pub rcpt_to: Vec<String>,
    /// Raw message bytes, verbatim (dot-decoded). Binary-safe; serialized
    /// base64 to Redis (`queue.rs`).
    pub data: Vec<u8>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

/// Per-connection SMTP session state.
#[derive(Debug)]
pub struct Session {
    /// Server greeting name (set at construction).
    server_name: String,
    /// Server-level TLS availability (cert/key loaded at startup).
    tls_configured: bool,
    /// True once the STARTTLS handshake completed.
    pub tls_active: bool,
    /// Authenticated principal (user email), if any.
    pub authenticated: Option<String>,
    helo: Option<String>,
    mail_from: Option<String>,
    rcpts: Vec<String>,
    data: Vec<u8>,
    in_data: bool,
    auth_mech: Option<AuthMech>,
    auth_step: Option<AuthStep>,
    login_user: Vec<u8>,
}

fn reset_transaction(s: &mut Session) {
    s.mail_from = None;
    s.rcpts.clear();
    s.data.clear();
    s.in_data = false;
}

/// RFC 3207 §4: discard everything known about the session after STARTTLS.
fn reset_after_starttls(s: &mut Session) {
    reset_transaction(s);
    s.helo = None;
    s.authenticated = None;
    s.auth_mech = None;
    s.auth_step = None;
    s.login_user.clear();
}

/// Extract the bare envelope address from the remainder of a
/// `MAIL FROM:` / `RCPT TO:` command. Recognises `<addr>` plus optional
/// ESMTP parameters (`SIZE=...`, etc.). Returns the address without angle
/// brackets (empty string for `<>`).
pub fn extract_envelope_address(remainder: &str) -> String {
    if let Some(start) = remainder.find('<') {
        if let Some(end_off) = remainder[start..].find('>') {
            return remainder[start + 1..start + end_off].to_string();
        }
    }
    if let Some(colon) = remainder.find(':') {
        let s = &remainder[colon + 1..];
        return parse_addr_no_angel(s);
    }
    parse_addr_no_angel(remainder)
}

fn parse_addr_no_angel(s: &str) -> String {
    let first = s.split_whitespace().next().unwrap_or("");
    first
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
}

/// Domain part of an address, lowercased; None when there is no domain.
pub fn domain_of(addr: &str) -> Option<String> {
    let (_, d) = addr.split_once('@')?;
    if d.contains('@') {
        return None;
    }
    Some(d.to_ascii_lowercase())
}

/// Relay policy: may `rcpt` be accepted in an unauthenticated session?
fn unauth_relay_allowed(relay_domains: &[String], rcpt: &str) -> bool {
    match domain_of(rcpt) {
        Some(d) => relay_domains.iter().any(|a| a.eq_ignore_ascii_case(&d)),
        // Addresses without a domain are not deliverable.
        None => false,
    }
}

/// Strict base64 (RFC 4648) SASL payload decode.
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .ok()
        .filter(|v| !v.is_empty())
}

fn b64_encode(s: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

impl Session {
    pub fn new(server_name: &str, tls_configured: bool) -> Self {
        Self {
            server_name: server_name.to_string(),
            tls_configured,
            tls_active: false,
            authenticated: None,
            helo: None,
            mail_from: None,
            rcpts: Vec::new(),
            data: Vec::new(),
            in_data: false,
            auth_mech: None,
            auth_step: None,
            login_user: Vec::new(),
        }
    }

    fn ehlo_reply(&self) -> String {
        let mut lines = vec![format!("250-{}", self.server_name)];
        // STARTTLS advertised only while TLS is available and the
        // connection is still plaintext.
        if self.tls_configured && !self.tls_active {
            lines.push("250-STARTTLS".to_string());
        }
        // RFC 4954 §3: AUTH advertised only once TLS is active (stack-wide
        // posture: SASL accepted only over TLS).
        if self.tls_active {
            lines.push("250-AUTH PLAIN LOGIN".to_string());
        }
        lines.push("250 8BITMIME".to_string());
        lines.join("\r\n")
    }

    /// Process one DATA payload line (without CRLF). Returns a reply only
    /// for rejects; accepted payload is `None`/DataLine.
    fn data_line(&mut self, line: &[u8]) -> Option<String> {
        if self.data.len() + line.len() + 2 > MAX_MESSAGE_SIZE_BYTES {
            reset_transaction(self);
            return Some("552 5.3.4 Message too large".into());
        }
        // Dot-stuffing (RFC 5321 §4.5.2): a stuffed dot is removed once.
        let unstuffed: &[u8] = if line.starts_with(b"..") {
            &line[1..]
        } else {
            line
        };
        self.data.extend_from_slice(unstuffed);
        self.data.extend_from_slice(b"\r\n");
        None
    }

    /// Handle one full line from the wire (without CRLF). Callers must
    /// enforce [`MAX_LINE_LENGTH`] before calling (the I/O layer owns the
    /// capped read; this module never sees overlong lines).
    pub fn handle_line(
        &mut self,
        line: &[u8],
        require_auth: bool,
        open_relay: bool,
        relay_domains: &[String],
    ) -> Action {
        // ----- DATA mode: everything is payload (binary-safe) -----
        if self.in_data {
            if line == b"." {
                self.in_data = false;
                let from = self.mail_from.clone().unwrap_or_default();
                let msg = QueuedMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    mail_from: from,
                    rcpt_to: self.rcpts.clone(),
                    data: self.data.clone(),
                    queued_at: chrono::Utc::now(),
                };
                reset_after_queued(self);
                return Action::TransactionQueued(msg);
            }
            return match self.data_line(line) {
                None => Action::DataLine,
                Some(r) => Action::Reply(r),
            };
        }

        // ----- SASL exchange in progress: line is base64 response or '*' -----
        if let (Some(mech), Some(step)) = (self.auth_mech, self.auth_step) {
            return self.sasl_continue(mech, step, line);
        }

        // Commands are ASCII by RFC 5321.
        let line = match std::str::from_utf8(line) {
            Ok(s) => s,
            Err(_) => return Action::Reply("500 5.5.2 Command not recognized".into()),
        };
        let t = line.trim();
        let (head, rest) = match t.split_once(' ') {
            Some((h, r)) => (h, r),
            None => (t, ""),
        };
        self.handle_command(head, rest, require_auth, open_relay, relay_domains)
    }

    fn handle_command(
        &mut self,
        head: &str,
        rest: &str,
        require_auth: bool,
        open_relay: bool,
        relay_domains: &[String],
    ) -> Action {
        let upper = head.to_ascii_uppercase();
        match upper.as_str() {
            "EHLO" => {
                if rest.is_empty() {
                    return Action::Reply("501 5.4.2 HELO/EHLO requires domain".into());
                }
                self.helo = Some(rest.to_string());
                Action::Reply(self.ehlo_reply())
            }
            "HELO" => {
                if rest.is_empty() {
                    return Action::Reply("501 5.4.2 HELO/EHLO requires domain".into());
                }
                self.helo = Some(rest.to_string());
                Action::Reply(format!("250 {}", self.server_name))
            }
            "QUIT" => Action::Reply("221 2.0.0 Bye".into()),
            "RSET" => {
                reset_transaction(self);
                Action::Reply("250 2.0.0 OK".into())
            }
            "NOOP" => Action::Reply("250 2.0.0 OK".into()),
            "VRFY" => Action::Reply("252 2.1.5 Cannot VRFY user".into()),
            "STARTTLS" => {
                if !self.tls_configured {
                    return Action::Reply("502 5.5.1 STARTTLS not configured".into());
                }
                if self.tls_active {
                    return Action::Reply("503 5.5.1 TLS already active".into());
                }
                if self.mail_from.is_some() || self.in_data {
                    // RFC 3207 §2: only outside a transaction.
                    return Action::Reply("503 5.5.1 STARTTLS not permitted here".into());
                }
                Action::StartTls
            }
            "AUTH" => self.handle_auth(rest),
            "MAIL" => {
                let remainder = match strip_prefix_ci(rest, "FROM:") {
                    Some(r) => r,
                    None => return Action::Reply("501 5.5.4 Syntax: MAIL FROM:<address>".into()),
                };
                if self.mail_from.is_some() {
                    return Action::Reply("503 5.5.1 Nested MAIL command; use RSET first".into());
                }
                if require_auth && self.authenticated.is_none() {
                    let hint = if self.tls_active {
                        ""
                    } else {
                        " (issue STARTTLS first)"
                    };
                    return Action::Reply(format!("530 5.7.0 Authentication required{hint}"));
                }
                // Optional SIZE parameter (RFC 1870): reject up front.
                if let Some(size) = extract_size_param(remainder) {
                    if size as usize > MAX_MESSAGE_SIZE_BYTES {
                        return Action::Reply(
                            "552 5.3.4 Message size exceeds fixed maximum".into(),
                        );
                    }
                }
                // Null sender <> is allowed (bounce/DSN path).
                self.mail_from = Some(extract_envelope_address(remainder));
                Action::Reply("250 2.1.0 OK".into())
            }
            "RCPT" => {
                let remainder = match strip_prefix_ci(rest, "TO:") {
                    Some(r) => r,
                    None => return Action::Reply("501 5.5.4 Syntax: RCPT TO:<address>".into()),
                };
                if self.mail_from.is_none() {
                    return Action::Reply("503 5.5.1 MAIL first".into());
                }
                if self.rcpts.len() >= MAX_RECIPIENTS {
                    return Action::Reply("552 5.5.3 Too many recipients".into());
                }
                let addr = extract_envelope_address(remainder);
                if addr.is_empty() || domain_of(&addr).is_none() {
                    return Action::Reply("501 5.1.3 Bad recipient address syntax".into());
                }
                if self.authenticated.is_none()
                    && !open_relay
                    && !unauth_relay_allowed(relay_domains, &addr)
                {
                    // Never an open relay: unauthenticated sessions may
                    // deliver only to the configured local domain(s).
                    return Action::Reply(
                        "554 5.7.1 Relay access denied; authentication required".into(),
                    );
                }
                self.rcpts.push(addr);
                Action::Reply("250 2.1.5 OK".into())
            }
            "DATA" => {
                if self.mail_from.is_none() || self.rcpts.is_empty() {
                    return Action::Reply("503 5.5.1 MAIL and RCPT first".into());
                }
                self.data.clear();
                self.in_data = true;
                Action::Reply("354 End data with <CR><LF>.<CR><LF>".into())
            }
            _ => Action::Reply("502 5.5.2 Command not implemented".into()),
        }
    }

    fn handle_auth(&mut self, rest: &str) -> Action {
        if !self.tls_active {
            // Stack posture: SASL only over TLS.
            return Action::Reply(
                "538 5.7.11 Encryption required for requested authentication mechanism".into(),
            );
        }
        if self.authenticated.is_some() {
            return Action::Reply("503 5.5.1 Already authenticated".into());
        }
        if self.mail_from.is_some() {
            // RFC 4954 §4: no AUTH during a mail transaction.
            return Action::Reply("503 5.5.1 AUTH not permitted during mail transaction".into());
        }
        let mut parts = rest.splitn(2, ' ');
        let mech = match parts.next().unwrap_or("").to_ascii_uppercase().as_str() {
            "PLAIN" => AuthMech::Plain,
            "LOGIN" => AuthMech::Login,
            "" => return Action::Reply("501 5.5.4 Syntax: AUTH mechanism".into()),
            _ => return Action::Reply("504 5.5.4 Mechanism not supported".into()),
        };
        // Optional initial response (RFC 4954 §4); "*" or absent starts a
        // challenge round.
        self.auth_mech = Some(mech);
        match parts.next() {
            None | Some("*") => {
                self.auth_step = Some(AuthStep::ExpectUserOrPlain);
                match mech {
                    AuthMech::Plain => Action::Reply("334 ".into()),
                    AuthMech::Login => Action::Reply(format!("334 {}", b64_encode("Username:"))),
                }
            }
            Some(initial) => self.sasl_payload(mech, initial),
        }
    }

    /// Continue a multi-round SASL exchange with a raw client line.
    fn sasl_continue(&mut self, mech: AuthMech, step: AuthStep, line: &[u8]) -> Action {
        let line = match std::str::from_utf8(line) {
            Ok(s) => s.trim().to_string(),
            Err(_) => {
                self.cancel_auth();
                return Action::Reply("501 5.5.2 Cannot decode base64".into());
            }
        };
        if line == "*" {
            self.cancel_auth();
            return Action::Reply("501 5.7.0 Authentication cancelled".into());
        }
        let decoded = match b64_decode(&line) {
            Some(d) => d,
            None => {
                self.cancel_auth();
                return Action::Reply("501 5.5.2 Cannot decode base64".into());
            }
        };
        if step == AuthStep::ExpectLoginPass {
            let email = String::from_utf8_lossy(&self.login_user).to_string();
            let secret = String::from_utf8_lossy(&decoded).to_string();
            self.cancel_auth();
            return Action::AuthVerify {
                email,
                secret,
                mech,
            };
        }
        // ExpectUserOrPlain: PLAIN carries user+secret together; LOGIN
        // carries only the username.
        match mech {
            AuthMech::Plain => self.sasl_plain(scan_plain(&decoded), mech),
            AuthMech::Login => {
                self.login_user = decoded;
                self.auth_step = Some(AuthStep::ExpectLoginPass);
                Action::Reply(format!("334 {}", b64_encode("Password:")))
            }
        }
    }

    fn sasl_plain(&mut self, msg: Option<(String, String)>, mech: AuthMech) -> Action {
        match msg {
            Some((user, secret)) if !user.is_empty() => {
                self.cancel_auth();
                Action::AuthVerify {
                    email: user,
                    secret,
                    mech,
                }
            }
            _ => {
                self.cancel_auth();
                Action::Reply("501 5.5.2 Invalid PLAIN message".into())
            }
        }
    }

    /// Handle an initial-response credential payload.
    fn sasl_payload(&mut self, mech: AuthMech, initial: &str) -> Action {
        let decoded = match b64_decode(initial) {
            Some(d) => d,
            None => {
                self.cancel_auth();
                return Action::Reply("501 5.5.2 Cannot decode base64".into());
            }
        };
        match mech {
            AuthMech::Plain => self.sasl_plain(scan_plain(&decoded), mech),
            AuthMech::Login => {
                self.login_user = decoded;
                self.auth_step = Some(AuthStep::ExpectLoginPass);
                Action::Reply(format!("334 {}", b64_encode("Password:")))
            }
        }
    }

    pub fn cancel_auth(&mut self) {
        self.auth_mech = None;
        self.auth_step = None;
        self.login_user.clear();
    }

    /// I/O-layer hook: abort an in-progress DATA payload (overlong line).
    /// Ends the whole transaction so nothing stale is reused; returns true
    /// when DATA mode was actually active (the caller picks the matching
    /// reply code).
    pub fn abort_data(&mut self) -> bool {
        let was = self.in_data;
        reset_transaction(self);
        was
    }

    /// RFC 3207 §4 hook: discard all session knowledge after STARTTLS.
    pub fn reset_for_tls(&mut self) {
        reset_after_starttls(self);
    }

    /// Mark the session authenticated after `auth.rs` verified credentials.
    pub fn set_authenticated(&mut self, email: String) {
        self.cancel_auth();
        self.authenticated = Some(email);
    }
}

/// After a queued message the transaction is complete; the envelope is
/// cleared so the connection can immediately start another one.
fn reset_after_queued(s: &mut Session) {
    reset_transaction(s);
}

/// `authzid NUL authcid NUL password`
fn scan_plain(raw: &[u8]) -> Option<(String, String)> {
    if raw.iter().filter(|b| **b == 0).count() != 2 {
        return None;
    }
    let mut it = raw.split(|b| *b == 0);
    let _authzid = it.next()?;
    let authcid = it.next()?;
    let secret = it.next()?;
    Some((
        String::from_utf8_lossy(authcid).to_string(),
        String::from_utf8_lossy(secret).to_string(),
    ))
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Extract `SIZE=<n>` from envelope parameters (last argument wins).
fn extract_size_param(remainder: &str) -> Option<u64> {
    let mut out = None;
    for tok in remainder.split_whitespace().skip(1) {
        if let Some(v) = tok.strip_prefix("SIZE=")
            && let Ok(n) = v.parse::<u64>()
        {
            out = Some(n);
        }
    }
    out
}
