//! Unit tests for the pure ESMTP state machine. These encode the security
//! invariants a reviewer should be able to trust from reading them:
//! no AUTH/STARTTLS advertisement without TLS, no AUTH without TLS,
//! no open relay, envelope hygiene, and clean transaction resets.

use crate::smtp::*;
use base64::Engine as _;

/// Run a line through the session and unwrap the reply.
fn reply(session: &mut Session, line: &[u8]) -> String {
    reply_args(session, line, false, true, &[])
}

fn reply_args(
    session: &mut Session,
    line: &[u8],
    require_auth: bool,
    open_relay: bool,
    domains: &[String],
) -> String {
    match session.handle_line(line, require_auth, open_relay, domains) {
        Action::Reply(r) => r,
        other => panic!("expected Reply, got {other:?}"),
    }
}

fn code(r: &str) -> &str {
    r.split(' ').next().unwrap_or("")
}

/// Happy-path steps: assert each answer is a 2xx/3xx success.
macro_rules! flow {
    ($s:ident; $($line:expr),+ $(,)?) => {{
        $( let r = reply(&mut $s, $line);
           assert!(r.starts_with('2') || r.starts_with('3'), "step failed: {r}"); )+
    }};
}

#[test]
fn ehlo_never_advertises_auth_without_tls() {
    let mut s = Session::new("mail.example.com", true);
    let r = reply(&mut s, b"EHLO client.example.org");
    assert!(r.contains("250-STARTTLS"), "STARTTLS advertised: {r}");
    assert!(!r.contains("AUTH"), "no AUTH before TLS: {r}");
}

#[test]
fn ehlo_without_tls_conf_advertises_neither() {
    let mut s = Session::new("mail.example.com", false);
    let r = reply(&mut s, b"EHLO client.example.org");
    assert!(!r.contains("STARTTLS"), "{r}");
    assert!(!r.contains("AUTH"), "{r}");
}

#[test]
fn ehlo_after_tls_advertises_auth_not_starttls() {
    let mut s = Session::new("mail.example.com", true);
    s.tls_active = true;
    let r = reply(&mut s, b"EHLO client.example.org");
    assert!(!r.contains("STARTTLS"), "{r}");
    assert!(r.contains("250-AUTH PLAIN LOGIN"), "{r}");
    assert!(r.contains("8BITMIME"), "{r}");
}

#[test]
fn auth_requires_tls() {
    let mut s = Session::new("mail.example.com", true);
    // Well-formed PLAIN message (a@b:NUL:secret) but plaintext session.
    let r = reply(&mut s, b"AUTH PLAIN AGFAYgBzZWNyZXQ=");
    assert_eq!(code(&r), "538", "AUTH without active TLS refused: {r}");
}

#[test]
fn starttls_not_configured_rejected() {
    let mut s = Session::new("mail.example.com", false);
    assert_eq!(code(&reply(&mut s, b"STARTTLS")), "502");
}

#[test]
fn starttls_mid_transaction_rejected() {
    let mut s = Session::new("mail.example.com", true);
    flow!(s; b"EHLO x.example", b"MAIL FROM:<a@b>");
    assert_eq!(code(&reply(&mut s, b"STARTTLS")), "503");
}

#[test]
fn starttls_action_resets_session() {
    let mut s = Session::new("mail.example.com", true);
    flow!(s; b"EHLO x.example", b"MAIL FROM:<a@b>", b"RCPT TO:<c@d>", b"RSET");
    assert!(
        matches!(
            s.handle_line(b"STARTTLS", false, false, &[]),
            Action::StartTls
        ),
        "expected StartTls"
    );
    // The I/O layer then does exactly this after a successful handshake:
    s.tls_active = true;
    s.reset_for_tls();
    // Envelope and helo are gone (RFC 3207 §4: no state survives).
    assert!(s.authenticated.is_none());
    assert_eq!(code(&reply(&mut s, b"RCPT TO:<c@d>")), "503");
    // TLS is active now: AUTH must be offered, and issuing STARTTLS again fails.
    let r = reply(&mut s, b"EHLO x.example");
    assert!(r.contains("250-AUTH PLAIN LOGIN"), "{r}");
    assert_eq!(code(&reply(&mut s, b"STARTTLS")), "503");
}

#[test]
fn auth_plain_initial_response_yields_verify() {
    let mut s = Session::new("mail.example.com", true);
    s.tls_active = true;
    // authzid("") NUL authcid("a@b") NUL secret("secret")
    match s.handle_line(b"AUTH PLAIN AGFAYgBzZWNyZXQ=", false, false, &[]) {
        Action::AuthVerify {
            email,
            secret,
            mech,
        } => {
            assert_eq!(email, "a@b");
            assert_eq!(secret, "secret");
            assert_eq!(mech, AuthMech::Plain);
        }
        other => panic!("expected AuthVerify, got {other:?}"),
    }
}

#[test]
fn auth_plain_followup_flow() {
    let mut s = Session::new("mail.example.com", true);
    s.tls_active = true;
    assert!(reply(&mut s, b"AUTH PLAIN").starts_with("334"));
    match s.handle_line(b"AGFAYgBzZWNyZXQ=", false, false, &[]) {
        Action::AuthVerify { email, secret, .. } => {
            assert_eq!(email, "a@b");
            assert_eq!(secret, "secret");
        }
        other => panic!("expected AuthVerify, got {other:?}"),
    }
}

#[test]
fn auth_plain_malformed_is_501_and_unlocks_session() {
    let mut s = Session::new("m", true);
    s.tls_active = true;
    // No NUL separators.
    let msg = base64::engine::general_purpose::STANDARD.encode(b"a@b");
    assert_eq!(
        code(&reply(&mut s, format!("AUTH PLAIN {msg}").as_bytes())),
        "501"
    );
    // Empty authcid.
    let msg = base64::engine::general_purpose::STANDARD.encode(b"NUL");
    assert_eq!(
        code(&reply(&mut s, format!("AUTH PLAIN {msg}").as_bytes())),
        "501"
    );
    // The machine is usable again afterwards.
    assert_eq!(code(&reply(&mut s, b"NOOP")), "250");
}

#[test]
fn auth_login_full_flow() {
    let mut s = Session::new("m", true);
    s.tls_active = true;
    assert!(reply(&mut s, b"AUTH LOGIN").starts_with("334 VXNlcm5hbWU6"));
    let user = base64::engine::general_purpose::STANDARD.encode(b"u@dom");
    assert!(reply(&mut s, user.as_bytes()).starts_with("334 UGFzc3dvcmQ6"));
    let pass = base64::engine::general_purpose::STANDARD.encode(b"p4ss");
    match s.handle_line(pass.as_bytes(), false, false, &[]) {
        Action::AuthVerify {
            email,
            secret,
            mech,
        } => {
            assert_eq!(email, "u@dom");
            assert_eq!(secret, "p4ss");
            assert_eq!(mech, AuthMech::Login);
        }
        other => panic!("expected AuthVerify, got {other:?}"),
    }
}

#[test]
fn auth_login_with_initial_user() {
    let mut s = Session::new("m", true);
    s.tls_active = true;
    let user = base64::engine::general_purpose::STANDARD.encode(b"u@dom");
    assert!(reply(&mut s, format!("AUTH LOGIN {user}").as_bytes()).starts_with("334 UGFzc3dvcmQ6"));
}

#[test]
fn auth_cancel_with_star() {
    let mut s = Session::new("m", true);
    s.tls_active = true;
    assert!(reply(&mut s, b"AUTH PLAIN").starts_with("334"));
    let r = reply(&mut s, b"*");
    assert_eq!(code(&r), "501", "{r}");
    assert_eq!(code(&reply(&mut s, b"NOOP")), "250");
    // Invalid base64 also cancels cleanly.
    assert!(reply(&mut s, b"AUTH LOGIN").starts_with("334"));
    assert_eq!(code(&reply(&mut s, b"!!!not base64!!!")), "501");
    assert_eq!(code(&reply(&mut s, b"NOOP")), "250");
}

#[test]
fn auth_twice_and_mid_transaction_refused() {
    let mut s = Session::new("m", true);
    s.tls_active = true;
    s.set_authenticated("u@d".into());
    assert_eq!(code(&reply(&mut s, b"AUTH PLAIN")), "503");
    let r = reply(&mut s, b"MAIL FROM:<u@d>");
    assert!(r.starts_with("250"), "{r}");
    assert_eq!(code(&reply(&mut s, b"AUTH PLAIN")), "503");
}

#[test]
fn authenticated_submission_relays_anywhere() {
    let mut s = Session::new("m", false);
    flow!(s; b"EHLO x.example");
    s.set_authenticated("user@dom".into());
    flow!(
        s;
        b"MAIL FROM:<user@dom>",
        b"RCPT TO:<stranger@elsewhere.example>"
    );
}

#[test]
fn unauth_relay_restricted_to_domains() {
    let mut s = Session::new("m", false);
    let doms = vec!["example.com".to_string()];
    flow!(s; b"HELO x", b"MAIL FROM:<a@example.com>");
    assert!(
        reply_args(
            &mut s,
            b"RCPT TO:<victim@elsewhere.net>",
            false,
            false,
            &doms
        )
        .starts_with("554"),
        "no open relay"
    );
    assert_eq!(
        code(&reply_args(
            &mut s,
            b"RCPT TO:<user@example.com>",
            false,
            false,
            &doms
        )),
        "250"
    );
}

#[test]
fn open_relay_flag_restores_spike_behavior() {
    let doms: Vec<String> = vec![];
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<a@b>");
    assert_eq!(
        code(&reply_args(&mut s, b"RCPT TO:<x@y>", false, true, &doms)),
        "250"
    );
}

#[test]
fn full_transaction_queues_and_resets() {
    let mut s = Session::new("m", false);
    flow!(s; b"HELO x", b"MAIL FROM:<a@b>", b"RCPT TO:<c@d>", b"DATA");
    match s.handle_line(b"Subject: hi", false, false, &[]) {
        Action::DataLine => {}
        other => panic!("expected DataLine, got {other:?}"),
    }
    let expected: Vec<u8> = b"Subject: hi\r\n".to_vec();
    match s.handle_line(b".", false, false, &[]) {
        Action::TransactionQueued(q) => {
            assert_eq!(q.data, expected);
        }
        other => panic!("expected TransactionQueued, got {other:?}"),
    }
    // Transaction fully reset: RCPT without MAIL fails.
    assert_eq!(code(&reply(&mut s, b"RCPT TO:<x@y>")), "503");
    // New transaction works immediately (MAIL after queued message).
    assert_eq!(code(&reply(&mut s, b"MAIL FROM:<z@w>")), "250");
}

#[test]
fn dot_stuffing_binary_verbatim() {
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM: <a@b>", b"RCPT TO: <c@d>", b"DATA");
    for line in [b"  indented  ".as_slice(), b"..", b"...x", b"\xff\xfe byte"] {
        match s.handle_line(line, false, false, &[]) {
            Action::DataLine => {}
            other => panic!("expected DataLine, got {other:?}"),
        }
    }
    let expected: Vec<u8> = {
        let lines = [
            b"  indented  ".as_slice(),
            b".".as_slice(),
            b"..x".as_slice(),
            b"\xff\xfe byte".as_slice(),
        ];
        let mut out = Vec::new();
        for line in lines {
            out.extend_from_slice(line);
            out.extend_from_slice(b"\r\n");
        }
        out
    };
    match s.handle_line(b".", false, false, &[]) {
        Action::TransactionQueued(q) => {
            assert_eq!(q.data, expected);
        }
        other => panic!("expected TransactionQueued, got {other:?}"),
    }
}

#[test]
fn data_mode_payload_cannot_escape_into_commands() {
    // "QUIT", "RSET", "MAIL..." inside DATA are payload, not commands.
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<a@b>", b"RCPT TO:<c@d>", b"DATA");
    let payloads = [
        b"QUIT".as_slice(),
        b"RSET",
        b"MAIL FROM:<x@y>",
        b"AUTH LOGIN",
    ];
    for p in payloads {
        assert!(matches!(
            s.handle_line(p, false, false, &[]),
            Action::DataLine
        ));
    }
    // Only the bare dot ends DATA.
    match s.handle_line(b".", false, false, &[]) {
        Action::TransactionQueued(_) => {}
        other => panic!("expected TransactionQueued, got {other:?}"),
    }
}

#[test]
fn oversized_message_aborts_transaction() {
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<a@b>", b"RCPT TO:<c@d>", b"DATA");
    let chunk = vec![b'x'; MAX_LINE_LENGTH];
    let mut rejected = false;
    for _ in 0..(MAX_MESSAGE_SIZE_BYTES / MAX_LINE_LENGTH + 10) {
        match s.handle_line(&chunk, false, false, &[]) {
            Action::DataLine => {}
            Action::Reply(r) => {
                assert!(r.starts_with("552"), "{r}");
                rejected = true;
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(rejected, "oversize must be rejected");
    // Session is clean and immediately usable again.
    assert_eq!(code(&reply(&mut s, b"MAIL FROM:<a@b>")), "250");
}

#[test]
fn nested_mail_requires_rset() {
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<a@b>", b"RCPT TO:<c@d>");
    assert_eq!(code(&reply(&mut s, b"MAIL FROM:<x@y>")), "503");
    assert_eq!(code(&reply(&mut s, b"RSET")), "250");
    assert_eq!(code(&reply(&mut s, b"MAIL FROM:<x@y>")), "250");
    assert_eq!(code(&reply(&mut s, b"DATA")), "503"); // RSET cleared recipients
}

#[test]
fn data_requires_mail_and_rcpt() {
    let mut s = Session::new("m", false);
    assert_eq!(code(&reply(&mut s, b"DATA")), "503");
    let _ = reply(&mut s, b"MAIL FROM:<a@b>");
    assert_eq!(code(&reply(&mut s, b"DATA")), "503");
}

#[test]
fn null_sender_and_bad_recipients() {
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<>", b"RCPT TO:<c@d>");
    let mut s = Session::new("m", false);
    flow!(s; b"MAIL FROM:<a@b>");
    assert_eq!(code(&reply(&mut s, b"RCPT TO:<nodomain>")), "501");
    assert_eq!(code(&reply(&mut s, b"RCPT TO:<>")), "501");
    assert_eq!(code(&reply(&mut s, b"RCPT TO:<a@b@c>")), "501");
}

#[test]
fn require_auth_blocks_unauth_mail() {
    let mut s = Session::new("m", true);
    let doms = vec!["example.com".to_string()];
    let r = reply_args(&mut s, b"MAIL FROM:<a@b>", true, false, &doms);
    assert!(r.starts_with("530"), "{r}");
    // A rejected MAIL must not have produced a principal.
    assert!(s.authenticated.is_none(), "no principal leaked");
    // After an (out-of-band) successful authentication the gate opens.
    s.set_authenticated("a@b".into());
    flow!(s; b"EHLO x");
}

#[test]
fn size_parameter_enforced() {
    let mut s = Session::new("m", false);
    let big = MAX_MESSAGE_SIZE_BYTES as u64 + 1;
    let r = reply(&mut s, format!("MAIL FROM:<a@b> SIZE={big}").as_bytes());
    assert_eq!(code(&r), "552", "{r}");
    assert_eq!(code(&reply(&mut s, b"MAIL FROM:<a@b> SIZE=100")), "250");
}

#[test]
fn recipients_capped() {
    let mut s = Session::new("m", false);
    let _ = reply(&mut s, b"MAIL FROM:<a@b>");
    for _ in 0..MAX_RECIPIENTS {
        assert!(reply(&mut s, b"RCPT TO:<r@example.com>").starts_with("250"));
    }
    assert!(reply(&mut s, b"RCPT TO:<more@example.com>").starts_with("552"));
}

#[test]
fn envelope_address_extraction_and_domains() {
    assert_eq!(extract_envelope_address("<a@b>"), "a@b");
    assert_eq!(extract_envelope_address("<a@b> SIZE=10"), "a@b");
    assert_eq!(extract_envelope_address(""), "");
    assert_eq!(domain_of("a@B.CoM"), Some("b.com".into()));
    assert_eq!(domain_of("nodomain"), None);
    assert_eq!(domain_of("a@b@c"), None);
}
