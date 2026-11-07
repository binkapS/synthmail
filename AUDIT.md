# SynthMail — Project Audit

**Audit date:** 2026-11-07  
**Revision:** `3edb73d` (`main`, working tree has uncommitted config changes)  
**Scope:** Rust workspace, SQL migrations, Docker/compose deployment, CI, and documentation  
**Method:** Full static review of all source files + `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`, and `docker compose config` validation. No live stack, database, or SMTP/IMAP flow was exercised.

> **Headline:** this is a well-organized, compiling skeleton with good crate separation and useful primitives, but it is **not production-ready** and several statements in `README.md` / `Plan.md` / `docs/releases/v1.0.0.md` overstate completion. The most serious problems are unauthenticated admin APIs, a 2FA takeover path, a webmail login that bypasses 2FA, default credentials, no TLS by default, and an unauthenticated SMTP shim.

> **Progress (branch `fix/security-audit`, 2026-11-07):** the checkboxes in §7 track what has landed. All P0 items except C-06 (TLS/ACME) are closed or reduced to documented residual risks; see `CHANGELOG-AUDIT.md` for the full summary. Workspace: 67 tests pass, `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo fmt --check` clean.

---

## 1. Executive summary

| Severity | Count | Meaning |
|---|---:|---|
| **Critical** | 7 | Remote compromise, auth bypass, credential exposure, or default-credential takeover |
| **High** | 14 | Broken core feature, privilege escalation, or production outage |
| **Medium** | 24 | Data integrity, correctness, DoS, or deployment reliability |
| **Low / hygiene** | 13 | Code quality, maintainability, docs accuracy, hardening |

**Top 7 things to fix before any deployment:**

1. Add authentication/authorization to every admin read endpoint (`synth-control`).
2. Fix TOTP enrollment/verification; it currently allows replacing/clearing any user's 2FA.
3. Make webmail login enforce TOTP (it currently bypasses it).
4. Remove default/known secrets and default admin seeding.
5. Enable TLS end-to-end and stop advertising STARTTLS/AUTH that are not implemented.
6. Authenticate the SMTP shim or do not deploy it.
7. Add `user_id` scoping to all webmail/CardDAV/CalDAV/search/push queries.

---

## 2. Verification snapshot

| Check | Result | Notes |
|---|---|---|
| `cargo check --workspace --all-targets` | ✅ Pass | Compiles; many warnings |
| `cargo test --workspace` | ✅ Pass | **39 tests pass**, but almost all are unit/smoke tests |
| `cargo fmt --check` | ✅ Pass | Formatting is clean |
| `cargo clippy -p synth-common -p synth-filter -p synth-dkim -- -D warnings` | ✅ Pass | Only these three crates are enforced in CI |
| `cargo clippy --workspace --all-targets` | ⚠️ Warnings | 40+ warnings, including deprecated Redis API, dead code, unused imports |
| `docker compose config -q` | ✅ Pass | Syntax valid; runtime wiring has problems (see H-06/H-08/H-09) |
| Live DB migration | ❌ Not run | CI migration step is a no-op (see M-18) |
| End-to-end mail flow | ❌ Not tested | Postfix/Dovecot/Rspamd wiring is incomplete |

**Test depth:** 39 tests, but several are placeholders (`assert!(true)`, JSON shape checks). There are no integration tests for authentication, authorization, database access, SMTP, IMAP, or the HTTP API.

---

## 3. Critical findings (P0)

### C-01 — Admin read endpoints are unauthenticated

**Location:** `crates/synth-control/src/routes.rs` (router lines 17–82; handlers below)

Only mutating handlers call `require_admin`. The following `GET` handlers have **no auth check at all**:

| Endpoint | Handler | Leaks |
|---|---|---|
| `GET /api/v1/domains` | `list_domains` (`:275`) | All domains + DKIM state |
| `GET /api/v1/domains/{id}` | `get_domain` (`:309`) | Domain config |
| `GET /api/v1/domains/{id}/dns` | `domain_dns` (`:385`) | DNS/DKIM details |
| `GET /api/v1/domains/{id}/verify` | `domain_verify` (`:418`) | Outbound DNS queries (abuse vector) |
| `GET /api/v1/users` | `list_users` (`:525`) | **All user emails, quotas, last-login, status** |
| `GET /api/v1/users/{id}` | `get_user` (`:571`) | User PII |
| `GET /api/v1/aliases` / `{id}` | `list_aliases` (`:691`), `get_alias` (`:725`) | Full alias map |
| `GET /api/v1/quota` | `quota_overview` (`:794`) | All mailbox usage |
| `GET /api/v1/queues` | `queue_status` (`:802`) | Queue activity |
| `GET /api/v1/dkim/{domain_id}` | `dkim_get` (`:878`) | DKIM public keys/selectors |
| `GET /api/v1/filters/{user_id}` | `list_filters` (`:889`) | **Sieve script contents** |
| `GET /api/v1/vacation/{user_id}` | `get_vacation` (`:979`) | Vacation message/PII |
| `GET /api/v1/admin/audit` | `list_audit` (`:1029`) | Audit trail/actor UUIDs |
| `GET /api/v1/admin/bans` | `list_bans` (`:1042`) | Failed-login emails |
| `GET /api/v1/config/export` | `config_export` (`:1065`) | **Full domains/users/aliases export** |
| `GET /api/v1/dmarc/reports` | `list_dmarc_reports` (`:1146`) | Raw DMARC XML |
| `GET /api/v1/push/subscribe` | `list_push_subs` (`:1189`) | Push endpoints |
| `DELETE /api/v1/push/subscribe/{id}` | `delete_push_sub` (`:1204`) | Anyone can delete subscriptions |

**Impact:** an unauthenticated visitor can enumerate the entire tenant, read user emails and usage, read Sieve scripts (which often contain forwarding addresses), and export configuration. This is a full control-plane information disclosure.

**Fix:** put all `/api/v1/*` routes behind an auth middleware/extractor except `/healthz`, `/readyz`, `/metrics`, `/api/v1/auth/login`, and the well-known files. Add per-resource authorization (admin vs self) and return `401`/`403` consistently. Add regression tests for every route.

---

### C-02 — 2FA enrollment/verification allows takeover, bypass, or lockout

**Location:** `crates/synth-control/src/routes.rs:1255–1307`

Evidence:

- `totp_enroll` (`:1255`) accepts the target `email` from the request body:
  ```rust
  let email = req.get("email").cloned().unwrap_or(claims.email.clone());
  let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email=$1")...
  ```
  It then **upserts the TOTP secret for that user and sets `enabled=false`** (`:1269`). Any authenticated user can therefore reset any other user's TOTP, including an admin's.
- `totp_verify` (`:1284`) has **no auth extractor**, and although the request struct contains `password: String` (`:1280`), it is never read (the compiler confirms: “field `password` is never read”). It sets `enabled=true` for the target email (`:1304`).
- `require_totp_or_ok` (`:1325`) only checks the current code; `last_used_step` is written but never checked, so a code can be replayed inside the ±1-step window.

**Impact:**
- An attacker with a low-privilege session can silently disable an admin's 2FA, then log in with only a stolen password.
- An unauthenticated attacker who can call `enroll` (or an authenticated attacker) can install their own secret and then mark 2FA enabled.
- Repeated calls can lock legitimate users out of their accounts.

**Fix:** derive the target from `claims.sub` only; require the current password (and existing TOTP) to change 2FA; require authentication on `totp_verify`; verify the password if the field is kept; encrypt the secret at rest; enforce single-use codes (`last_used_step` must store the numeric step, not `now()`); rate-limit both endpoints. Add tests for cross-user attempts.

---

### C-03 — Webmail login bypasses TOTP

**Location:** `crates/synth-webmail/src/routes.rs:105–131`

`web_login` checks only `active` and `verify_password`, then issues a JWT with the **same `SESSION_SECRET`** used by `synth-control`. It never consults `user_totp`. Because the control plane trusts any valid JWT signed with that secret, an attacker who knows an admin password can obtain an admin session through `:3000/api/v1/auth/login` even when 2FA is enabled on `:8000`.

It also has no rate limiting, no lockout, no `force_password_change` handling, and returns `kind` from the DB (so the token is a full admin token).

**Fix:** centralize authentication in one service/function, or have webmail call the control-plane login flow. Both must run the same TOTP check and the same rate limiter. Do not issue tokens until every required factor has passed. Add a test that TOTP-enabled accounts cannot log in via webmail without a code.

---

### C-04 — Webmail, JMAP, CardDAV/CalDAV are unauthenticated and cross-user

**Location:** `crates/synth-webmail/src/routes.rs:12–46`, `carddav.rs:30–195`, `routes.rs:167–186`

- Every mail/JMAP/DAV route is registered without an auth layer. There is no `extract_claims` call in this crate.
- `search` (`routes.rs:167`) queries `mail_search` **without a `user_id` filter** and returns results for all users.
- `send` (`routes.rs:194`) accepts arbitrary `from`/`to` with no auth and then claims `"status": "queued"` even though `imap::send_via_smtp` is a no-op.
- `list_contacts`, `list_events`, `query_events`, and `delete_contact` (`carddav.rs`) query/delete by ID with no user filter; `create_contact`/`create_event` assign everything to **the first user in the database** (`SELECT id FROM users ORDER BY created_at LIMIT 1`).
- `jmap_session` returns a session for hardcoded `user@example.com`; `jmap_dispatch` has no auth and returns static data.

**Impact:** any unauthenticated network client can read/modify every tenant's mail metadata, contacts, and calendars, and (when the send stub is wired) send mail as anyone.

**Fix:** add an auth extractor that validates the JWT and resolves `user_id`; scope every SQL query and mutation by `user_id`; enforce ownership on delete; make send use the authenticated identity for `From` and reject spoofing; return `501 Not Implemented` for stubbed features instead of fake success. Add integration tests with two users.

---

### C-05 — Default/known secrets and automatic default admin

**Location:** `crates/synth-common/src/config.rs:60–65`, `crates/synth-control/src/db.rs:14–46`, `compose.yaml:144–163`, `.env.example:20–25`

- `Config::default()` uses `session_secret = "dev-secret-change-me-32bytes-min-0000"`. `validate()` only checks length, so this **passes** and is accepted in production.
- `compose.yaml` falls back to the same secret: `SESSION_SECRET=${SESSION_SECRET:-dev-secret-change-me-32bytes-min-0000}`.
- If no admin exists, `seed_admin_if_empty` creates `admin@example.com` with `ChangeMe123!Admin` (or whatever `INITIAL_ADMIN_*` is set to) on every fresh database.
- `compose.yaml` does **not** pass `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD` into the `synth-control` container (grep confirms they are absent), so the `.env` values are ignored and the built-in defaults are used.
- `--seed-admin` calls `seed_admin`, whose `ON CONFLICT (email) DO UPDATE SET password_hash=EXCLUDED.password_hash` **resets the admin password on every boot**.

**Impact:** a default deployment is trivially compromised (known JWT secret lets an attacker forge tokens; known admin password gives immediate access). The known secret also makes all sessions forgeable.

**Fix:** remove defaults from `Config`/compose; fail startup if `SESSION_SECRET` is missing, shorter than 32 bytes, or equal to a known placeholder; require `INITIAL_ADMIN_PASSWORD` on first boot or generate and print a one-time random password; never reset an existing admin unless explicitly forced; pass the documented env vars through compose.

---

### C-06 — Mail services run without TLS; ACME is not implemented

**Location:** `docker/dovecot/dovecot.conf:1–14`, `docker/dovecot/Dockerfile` entrypoint, `docker/postfix/main.cf:20–28`, `docker/edge/nginx.conf`, `crates/synth-smtp/src/main.rs:70–72`

- Dovecot ships `ssl = no`. The entrypoint only flips it to `yes` if a certificate already exists in `/etc/letsencrypt/live/mail.example.com`; nothing in the repo creates that certificate. IMAP/POP3/submission therefore start in plaintext.
- Postfix points at the same certificate paths; if they do not exist, TLS is unavailable (and the service may log/start in a degraded state).
- Edge nginx only enables its 443 server when the same certificate exists. There is no certbot/lego/ACME client container; `synth-watchdog` only serves challenge tokens from a local path nobody populates.
- `synth-smtp` advertises `250 STARTTLS` but has no `STARTTLS` branch in `handle_line`; the command falls through to `502 Command not implemented`.

**Impact:** credentials and mail content can be intercepted; STARTTLS clients may downgrade or fail; the documented TLS/ACME feature does not work.

**Fix:** add a real ACME client (certbot/lego) with renewal, or document/ship certificate provisioning; fail closed when certs are missing; disable plaintext listeners or redirect them; set `ssl = required` and `smtpd_tls_auth_only = yes`; implement STARTTLS or stop advertising it.

---

### C-07 — SMTP shim is an unauthenticated relay

**Location:** `crates/synth-smtp/src/main.rs:37–89`, `:96–203`

- `EHLO` advertises `AUTH PLAIN LOGIN` and `STARTTLS`, but neither is implemented (`AUTH`/`STARTTLS` hit the `502` fallback).
- No authentication is required before `MAIL FROM` / `RCPT TO` / `DATA`; any client can queue mail for any domain.
- Redis accepts the message and the code returns `250 QUEUED`. If `forward_to_postfix` were functional, this would be an open relay; today it is still an unauthenticated mail-injection endpoint and queue-stuffing DoS.
- There is no message-size limit, line-length limit, recipient limit, or connection timeout.

**Impact:** spam/abuse, queue flooding, and (once forwarding is fixed) open relay. The crate is not in the default compose file but is shipped and documented as runnable.

**Fix:** do not expose it publicly; bind to loopback or the compose network only; implement SASL via Dovecot auth or require mTLS; implement STARTTLS; enforce size/recipient/rate limits; or remove the crate from the default product until it is complete.

---

## 4. High findings (P1)

### H-01 — API tokens ignore expiry and scopes

**Location:** `crates/synth-control/src/routes.rs:1383–1485`

`create_api_token` stores `scopes` and `expires_at`, but `authenticate_api_key` filters only `revoked_at IS NULL` (`:1467–1470`) and returns the owning user id. `require_admin` then checks only the user's DB `kind` (`:216–240`). Consequences:

- An expired token still works forever.
- A token created with `scopes: ["read"]` is effectively a full admin token.
- Scopes are never enforced anywhere in the codebase.

**Fix:** filter `(expires_at IS NULL OR expires_at > now())`; return a structured principal `{ user_id, scopes }`; enforce required scopes per route; add tests for expired/read-only/revoked tokens. Consider a per-token secret with a non-reversible indexed hash (Argon2 is fine, but a keyed HMAC/index is faster for lookup).

### H-02 — No brute-force protection; rate limiting and fail2ban are non-functional

**Location:** `crates/synth-common/src/rate_limit.rs`, `crates/synth-control/src/routes.rs:122–176`, `docker/fail2ban/jail.local`

- `RateLimiter` is implemented but **never instantiated** (grep: only its own test uses it).
- `login_attempts` rows are written for successful/password-known failures, but `ip` is never populated, so IP-based banning cannot work.
- `metrics::inc_auth_failure` is never called.
- `jail.local` uses `banaction = dummy`, so bans have no effect; its comment claims “rate limiting … enforced in-app”, which is false; `filter.d/` is empty.
- nginx has no `limit_req`/`limit_conn`.

**Impact:** unlimited password guessing, credential stuffing, and login DoS.

**Fix:** add a Redis-backed rate limiter (per IP + per account) middleware around auth and admin APIs; record `ip` from `X-Forwarded-For` (trusted proxy only) into `login_attempts`; implement account lockout/backoff; wire fail2ban to a real action or use nginx `limit_req`; emit auth failure metrics.

### H-03 — Password policy and HIBP breach checks are not wired in

**Location:** `crates/synth-common/src/hibp.rs`, `crates/synth-control/src/routes.rs:540–568`, `:642–666`

`check_breached`/`enforce_not_breached` exist but are never called. `create_user` and `reset_password` accept any string, including `a`, `password`, or the default admin password. There is no minimum length, complexity, or compromised-password check despite `.env` advertising `HIBP_ENABLED=true` and `Plan.md` claiming it is implemented.

**Fix:** enforce a minimum length (≥12) and breached-password check on create/reset/change; call HIBP with a timeout and fail-open only if explicitly configured; add unit tests with a mocked range response.

### H-04 — JWTs cannot be revoked; logout and password reset do not invalidate sessions

**Location:** `crates/synth-common/src/auth.rs:33–63`, `crates/synth-control/src/routes.rs:178–180`, `:642–666`

- `auth_logout` returns `{"status":"ok"}` and does nothing.
- Tokens have a 24-hour lifetime, no `jti`, no session table, and no `iss`/`aud` claims.
- `reset_password` changes the hash but leaves all existing JWTs and API tokens valid.
- `active=false` users are not checked on `auth_me` or general JWT use; only login checks `active`.
- `force_password_change` is stored but never enforced.

**Fix:** add a short access-token TTL plus refresh/session records (or a `token_version`/`jti` denylist in Redis); invalidate on logout, password reset, 2FA change, and deactivation; check `active`/`force_password_change` in the auth extractor; set `iss`/`aud` and validate them.

### H-05 — Application and monitoring ports are published directly, bypassing edge/TLS

**Location:** `compose.yaml:144–163`, `:268–284`, `:286–297`

`synth-control` publishes `8000:8000`, `synth-webmail` publishes `3000:3000`, Grafana publishes `3001:3000`, Prometheus `9090`, Radicale `5232`. Anyone on the host network can hit the plaintext services directly, bypassing nginx, HSTS, and any future IP allowlist. Grafana uses `GF_SECURITY_ADMIN_PASSWORD=admin`.

**Fix:** publish only `edge` (80/443) and bind admin/monitoring ports to `127.0.0.1` or a private network; remove Grafana’s default password; put all admin surfaces behind auth/VPN.

### H-06 — Postfix cannot resolve virtual maps (missing package and files)

**Location:** `docker/postfix/Dockerfile`, `docker/postfix/main.cf:10–14`

`main.cf` declares:

```
virtual_mailbox_domains = pgsql:/etc/postfix/pgsql/virtual_domains.cf
virtual_mailbox_maps    = pgsql:/etc/postfix/pgsql/virtual_mailboxes.cf
virtual_alias_maps      = pgsql:/etc/postfix/pgsql/virtual_aliases.cf
```

The Dockerfile installs `postfix` but **not `postfix-pgsql`**, and never creates/copies `/etc/postfix/pgsql/*.cf`. Postfix therefore cannot use the maps; recipient validation and local delivery fail.

**Fix:** install `postfix-pgsql`, add the three map files (with the DB host/user/password), ensure file permissions, and add a container smoke test (`postmap -q user@example.com pgsql:...`). Alternatively, switch to Dovecot LMTP with static transport maps and remove the PostgreSQL dependency from Postfix.

### H-07 — DKIM private keys are plaintext and never reach OpenDKIM

**Location:** `crates/synth-control/src/routes.rs:831–876`, `crates/synth-control/migrations/20261107000000_init.sql:75–88`, `docker/opendkim/*`

- `dkim_generate` stores `pair.private_key_pem` directly in `dkim_keys.private_key_enc` (`:845–846`). The column is named `_enc` but nothing encrypts it.
- The `dkim` volume is mounted only in the `opendkim` container; `synth-control` does not share it, and there is no job that writes private keys, `KeyTable`, or `SigningTable`.
- `opendkim` starts with empty `KeyTable`/`SigningTable` (`Dockerfile` touches empty files). Outbound mail will not be signed.
- `opendkim.conf` sets `MinimumKeyBits 1024`, weaker than the generated 2048-bit key.
- `generate_ed25519` wraps the raw 32-byte seed in a PKCS#8 PEM header; that is not a valid Ed25519 PKCS#8 structure, so the stored PEM cannot be loaded.

**Impact:** DKIM signing does not work; private keys are exposed to any DB read/backup; ed25519 keys are unusable.

**Fix:** encrypt private keys with a KMS/age/SOPS key or use a secrets manager; never return them; write key files to the shared volume (or run a sidecar) and generate `KeyTable`/`SigningTable` entries; reload/restart OpenDKIM on change; implement ed25519 as proper PKCS#8 or drop it; set `MinimumKeyBits 2048`.

### H-08 — Redis configuration is self-contradictory

**Location:** `.env` / `.env.example`, `compose.yaml:200–212`, `crates/synth-control/src/app.rs`, `crates/synth-webmail/src/app.rs`

`.env` sets `REDIS_URL=redis://:CHANGE_ME_REDIS@redis:6379`, but the `redis` service starts with no `--requirepass` and `REDIS_PASSWORD` is never passed to it. Compose passes `REDIS_URL` to the apps, so the client will attempt `AUTH` against a server with no password and fail (`ERR Client sent AUTH, but no password is set`). Either the apps cannot start or health degrades depending on the client behavior.

**Fix:** choose one model: either `command: ["redis-server", "--requirepass", "${REDIS_PASSWORD}", ...]` with the password passed to clients, or remove auth from the URL. Add a startup connectivity check and a compose smoke test.

### H-09 — Backup service cannot work

**Location:** `compose.yaml:286–297`

The `backup` service uses `image: rclone/rclone` (based on Alpine, no PostgreSQL client) and runs:

```
pg_dump -Fc -h postgres -U synth synthmail > /backups/... && tar -czf ... /var/vmail && rclone sync ...
```

Problems:

- No `pg_dump` (no `postgresql-client`) in the image.
- No `PGPASSWORD`/connection env.
- `/var/vmail` is not mounted into this container.
- No `depends_on`/health condition.
- Errors are swallowed by `|| echo`.
- No restore test, checksum, retention, or encryption.

**Fix:** build a backup image with PostgreSQL client + tar + rclone; mount `maildir` and a dedicated backup volume; use `PGPASSWORD` from secrets; encrypt and checksum; add a scheduled job and a documented restore test.

### H-10 — Webmail mail features are stubs or fake success

**Location:** `crates/synth-webmail/src/imap.rs`, `routes.rs:135–223`, `fts_indexer.rs`

- `imap::list_folders` ignores credentials and returns a hardcoded list.
- `imap::fetch_messages` always returns `Ok(vec![])`.
- `list_messages` always returns an empty list with a “Phase 1” note.
- `send_via_smtp` only logs; `send` returns `{"status":"queued"}` regardless.
- `list_folders`/`list_messages`/`get_message`/`list_drafts`/`delete_draft` are static.
- `fts_indexer::reindex_batch` is never spawned, and `mail_search.body_tsv` is never populated on insert, so search only works for manually seeded rows.

**Impact:** the advertised webmail does not send or read mail; users may believe messages were sent when they were not.

**Fix:** implement IMAP/JMAP and SMTP submission with real credentials, or return `501 Not Implemented` and hide the feature in the UI until complete. Schedule indexing (or add a DB trigger) and populate `body_tsv` on write.

### H-11 — Admin UI is not built or served

**Location:** `crates/synth-ui/*`, `docker/edge/nginx.conf`, `crates/synth-webmail/src/routes.rs`

- `synth-ui` is a Leptos CSR crate, but no Dockerfile/compose service builds it with Trunk. The `dist/` output is never copied or mounted.
- Edge routes `/admin/` and `/` to `synth-webmail`, whose router has no static-file route (the `tower-http` `fs` feature is enabled but unused).
- The UI is demo markup: the login button has an empty handler, `HomePage` is dead code, and the admin/webmail pages show hardcoded sample data.
- `manifest.webmanifest` references `/icon-192.png` and `/icon-512.png`, which do not exist.

**Impact:** the documented admin UI is unreachable/non-functional.

**Fix:** add a Trunk build stage, serve `dist/` from webmail or nginx, wire the login/form handlers to the API, and add icons or remove the manifest references.

### H-12 — Push subscriptions are unauthenticated and assigned to the first user

**Location:** `crates/synth-control/src/routes.rs:1163–1214`

`create_push_sub` has no auth and resolves the owner with `SELECT id FROM users ORDER BY created_at LIMIT 1`. `list_push_subs` exposes every endpoint to anyone; `delete_push_sub` lets anyone delete any subscription by UUID. Endpoints/p256dh/auth values are sensitive capability material.

**Fix:** require auth and use `claims.sub`; list/delete only the caller’s rows; validate the Web Push payload; add a uniqueness constraint on `(user_id, endpoint)`.

### H-13 — Rspamd config is likely invalid or ignored

**Location:** `docker/rspamd/rspamd.conf` → mounted at `/etc/rspamd/local.d/rspamd.conf`

The file uses full top-level `options { ... }`, `modules { include "modules.d" }`, `clamav { clamav_sock = ... }`, and `dkim { domain = ""; ... }` blocks. In Rspamd, `local.d/` files are snippets merged into specific sections; `modules { include ... }` and `clamav_sock` are not the expected keys (`clamav` uses `servers = "clamav:3310"`). `dkim` with an empty domain and a single key path is also suspect. `docker compose config` cannot catch this.

**Impact:** Rspamd may fail to start or silently run without AV/DKIM/greylisting/Bayes.

**Fix:** split config into `local.d/options.inc`, `local.d/antivirus.conf`, `local.d/dkim_signing.conf`, etc.; validate with `rspamadm configtest` in CI; add a container smoke test that checks `/ping` and a known symbol.

### H-14 — SMTP shim loses the message body and stores raw commands

**Location:** `crates/synth-smtp/src/main.rs:37–89`, `:122–157`

- `handle_line` builds a `QueuedMessage` with `data: std::mem::take(data)` (`:54`), then discards that struct and returns only the id. `handle_client` then builds a second `QueuedMessage` with `data: data.clone()` (`:138`), but `data` is already empty. **Every queued message has an empty body.**
- `MAIL FROM:` and `RCPT TO:` are stored as the whole command string (e.g. `"MAIL FROM: <a@b>"`), not the address, so forwarding cannot parse them.
- `handle_line` trims every DATA line (`:45`), destroying leading/trailing whitespace and making DKIM/signature verification impossible after forwarding.
- Redis keys have no TTL and the `pending` list has no consumer; memory grows without bound.

**Fix:** have `handle_line` return the complete `QueuedMessage` (or write it to Redis directly); parse addresses with a proper envelope parser; preserve bytes verbatim; add TTLs/trimming and a consumer; add a test asserting the queued body equals the submitted body.

---

## 5. Medium findings (P2)

### M-01 — Generated Sieve scripts are invalid

**Location:** `crates/synth-filter/src/lib.rs:19–85`

- `generate_vacation_sieve` emits `require ["vacation", "variables", "extension", ...]`; `"extension"` is not a valid Sieve capability and will fail to load.
- It renders `:from {from}` **without quotes** and `:addresses [${{from}}]` with an unquoted address, both parse errors.
- It emits `if true { ... }` and ignores `cfg.enabled` and `cfg.start`/`cfg.end`, so date-bounded vacation is not implemented.
- `generate_spam_filter_sieve` matches `X-Spam-Score` with `:contains`, so a score of `7` matches `17`, `27`, etc.
- `generate_alias_sieve` uses `redirect :address [...] "alias"` (two positional args; `:address` is not a `redirect` option) and redirects to the alias rather than the `goto` destinations.
- `validate_sieve` only checks for non-empty content and the presence of `if`/`require`; it does not parse the script.

**Fix:** generate valid Sieve (quote all strings, correct `require` capabilities, `if` guards based on `enabled` dates), and validate with `sievec`/`sieve-test` in tests. Add a test that compiles the generated script.

### M-02 — Blocking work in async handlers

**Location:** `crates/synth-control/src/routes.rs:802–823`, `crates/synth-dkim/src/lib.rs:16–36`, `crates/synth-watchdog/src/main.rs:158–161`

- `queue_status` calls `std::fs::read_dir` inside an async handler.
- `dkim_generate` calls RSA-2048 key generation synchronously on the Tokio runtime (can block for hundreds of ms).
- `acme_challenge` uses `std::fs::read_to_string` in an async handler.

**Fix:** wrap in `tokio::task::spawn_blocking`; use async file I/O where practical; add timeouts.

### M-03 — Read-modify-write and multi-step writes are not transactional

**Location:** `crates/synth-control/src/routes.rs:319–361`, `:581–613`, `:913–938`, `:992–1025`

`update_domain`/`update_user` fetch, mutate, and update without a transaction, so concurrent updates lose data. `save_filters` deletes all user scripts and then inserts new ones; if an insert fails, the user loses every filter. `save_vacation` inserts a new `sieve_scripts` row each time instead of updating the `vacation` script, creating duplicates.

**Fix:** use `BEGIN … COMMIT` (`sqlx::Transaction`) for multi-step operations; use `INSERT … ON CONFLICT` for the vacation script; add optimistic concurrency (`updated_at`/version) if needed.

### M-04 — Autoconfig response is malformed and hardcoded

**Location:** `crates/synth-control/src/routes.rs:1218–1231`

The XML namespace is corrupted (`http://test.kណ្ម� art.com/autoconfig/1.2`), the domain is hardcoded to `example.com`, and the handler returns `String`, so Axum sends `text/plain` rather than `application/xml`. Thunderbird/Outlook autoconfig will reject it.

**Fix:** use the correct `http://autoconfig.thunderbird.net/v1.1`/`http://autoconfig.example.com/...` namespace (or the standard one), render the actual domain, and set `Content-Type: application/xml`.

### M-05 — DKIM DNS output uses the TLD as the domain

**Location:** `crates/synth-control/src/routes.rs:872`

```rust
"dns_record": format!("{}._domainkey TXT {}", domain_row.domain.split('.').last().unwrap_or(""), pair.dns_txt),
```

For `example.com` this emits `com._domainkey TXT …`. The `full_dns` field is correct; `dns_record` is wrong.

**Fix:** use `domain_row.domain`, not `split('.').last()`.

### M-06 — Config import silently ignores users; export leaks everything

**Location:** `crates/synth-control/src/routes.rs:1065–1115`

`config_export` has no auth (C-01) and includes all users/domains/aliases. `config_import` declares `users: Vec<serde_json::Value>` but never reads it (compiler warning), imports only domains, does not validate values, does not use a transaction, and counts `ON CONFLICT DO NOTHING` as “imported.”

**Fix:** authenticate both; either implement user import with hashing/validation or remove it from the contract; wrap in a transaction; report created/updated/skipped counts.

### M-07 — Error responses leak internal details

**Location:** `crates/synth-common/src/error.rs:36–46`

`AppError::Database`/`Internal` are returned to clients with the full Display text (`detail: self.to_string()`), which can include SQL errors, table names, and connection strings. This aids attackers.

**Fix:** log the full error server-side with a correlation id; return a generic `detail` for 5xx; keep validation details for 4xx.

### M-08 — Login allows account enumeration via status/timing

**Location:** `crates/synth-control/src/routes.rs:126–144`

- Unknown email → `401 Unauthorized` immediately (no Argon2 work).
- Inactive user → `403 Forbidden`.
- Wrong password → Argon2 verification, then `401`.

An attacker can enumerate accounts by status code and timing.

**Fix:** always perform a dummy Argon2 verification and return the same `401` for unknown/inactive/wrong-password; optionally add small jitter.

### M-09 — `Config::from_env` ignores documented variables

**Location:** `crates/synth-common/src/config.rs:79–133`

The struct has `control_url`, `webmail_url`, and the `.env` documents `CONTROL_URL`, `WEBMAIL_URL`, `ADMIN_URL`, `RATE_LIMIT_PER_MIN`, `HIBP_ENABLED`, `TOTP_ENFORCED_FOR_ADMIN`, `POSTFIX_QSHAPE_PATH`, `RADICALE_URL`, `BACKUP_RETENTION_DAYS`, etc., but `from_env` reads almost none of them. `control_url`/`webmail_url` remain `example.com` defaults.

**Fix:** parse all documented variables (with validation) or remove them from `.env` and docs. Add a test that a full env is round-tripped.

### M-10 — Watchdog `/status` is fake; ACME/backup glue is log-only

**Location:** `crates/synth-watchdog/src/main.rs:95–182`

- `/status` always returns `"status": "operational"` and a hardcoded service list, regardless of `check_all` results (the poller state is local to the spawned task).
- `acme_challenge` reads `/etc/letsencrypt/acme-challenge/{token}`; no component writes there, and no ACME client requests certs.
- `check_backups` only logs.
- `alert` has no deduplication/backoff; it fires every poll after 2 minutes.

**Fix:** store health in shared state (e.g., `Arc<RwLock<…>>` or Redis) and render it from `/status`/`/metrics`; add a real ACME client or remove the endpoint; validate backup age/content; add alert dedupe.

### M-11 — Full-text indexer is never scheduled and search is tenant-global

**Location:** `crates/synth-webmail/src/fts_indexer.rs`, `crates/synth-webmail/src/routes.rs:167–186`, migrations

`reindex_batch` is never called; there is no trigger or worker. `mail_search.body_tsv` stays `NULL` unless an external process populates it. `search` (C-04) also returns all users’ rows. `mail_search` has no unique `(user_id, message_id)` constraint, so duplicate syncs can duplicate rows.

**Fix:** add an indexer worker (or a DB trigger/`GENERATED` column) and a `user_id` filter; add a unique constraint and an `updated_at` column.

### M-12 — Ed25519 DKIM key PEM is invalid

**Location:** `crates/synth-dkim/src/lib.rs:38–66`

The code base64-encodes the raw 32-byte signing seed and wraps it in a generic PKCS#8 header (`BEGIN PRIVATE KEY`) without the required Ed25519 algorithm identifier. OpenDKIM/OpenSSL will reject it. The public key computation is fine.

**Fix:** use `ed25519_dalek` + `pkcs8`/`ed25519` to encode a real PKCS#8 v2 document, or drop Ed25519 until a compatible signer is wired.

### M-13 — DMARC report ingestion is not validated or wired

**Location:** `crates/synth-control/src/routes.rs:1119–1161`, `migrations/...phase2.sql`

`create_dmarc_report` accepts arbitrary `xml` (up to Axum’s default body limit) and stores it without parsing, size limits, or deduplication. `report_id` is not unique. There is no mail-to-API ingestion path from Postfix/OpenDMARC, so the viewer will stay empty.

**Fix:** parse/validate XML safely (disable external entities), enforce size limits, add a unique `(domain, report_id)` constraint, and document/implement the ingestion path (e.g., OpenDMARC history or a report mailbox fetcher).

### M-14 — Input validation is weak across admin APIs

**Location:** `crates/synth-control/src/routes.rs` (`create_domain` `:281`, `create_user` `:540`, `create_alias` `:701`, `update_user` `:581`)

- Domains are validated only by “contains a dot”; invalid characters/length are accepted.
- User emails are validated only by “contains `@`”; the domain need not exist.
- `create_user` accepts `kind` from the client (admin can create `superadmin`), and `quota_mb` can be negative because only `.min(102_400)` is applied.
- Aliases accept arbitrary `goto`, with no cycle detection (mail loops) and no per-address validation.
- `update_user` can demote/deactivate the last admin, and there is no distinction between `admin` and `superadmin` in authorization.
- `dmarc_policy` accepts arbitrary strings.

**Fix:** use a validation crate (e.g., `validator`) or explicit regex/length checks; validate domain membership; clamp quotas to `[0, max]`; define RBAC rules (only superadmin can create admins; cannot remove the last superadmin); detect alias cycles; whitelist DMARC policies (`none|quarantine|reject`).

### M-15 — TOTP secrets are stored in plaintext

**Location:** `crates/synth-control/migrations/20261107000001_phase2.sql:5–12`, `routes.rs:1269`

The migration comment says “secret encrypted at rest in app layer via SOPS/age in Phase 2 prod”, but the code inserts the Base32 secret directly. A DB read/backup exposes all second factors.

**Fix:** encrypt secrets with an application KMS/age key before insert and decrypt only during verification; rotate the encryption key; never log the secret (currently `TotpEnrollResp` returns it to the caller by design, which is correct once).

### M-16 — Container supply-chain and runtime hygiene

**Location:** `compose.yaml`, all Dockerfiles

- Many images use floating tags: `rspamd/rspamd:latest`, `clamav/clamav:stable-debian`, `kozea/radicale:latest`, `prom/...:latest`, `grafana/grafana:latest`, `stalwartlabs/stalwart:v1`. Builds are not reproducible.
- Application containers run as root.
- There is no `.dockerignore`, so `target/` and other large/local files are sent in the build context.
- No `--locked` in `cargo build`, so the lock file can be ignored for resolution in the builder.
- No `LICENSE` file despite `license = "MIT"`.
- No `rust-toolchain.toml`; edition 2024 depends on a recent toolchain.

**Fix:** pin image digests/tags, add `.dockerignore`, run as a non-root `USER`, use `cargo build --locked`, add the MIT license file, and pin the toolchain.

### M-17 — HA overlay does not form a working replica

**Location:** `docker/compose.ha.yaml`

`postgres-replica` passes `primary_conninfo='host=postgres port=5432 user=synth'` as a `-c` argument with embedded quotes; it never runs `pg_basebackup`, so the data directory is empty and the replica cannot start streaming. `postfix-2` has no ports/health dependency on the edge/LB. The file is labeled “Patroni-ready” but contains no Patroni.

**Fix:** use an actual replication setup (primary init + `pg_basebackup`, or Patroni/`repmgr`), pass secrets correctly, and add a failover test. If HA is out of scope, mark it explicitly as not implemented.

### M-18 — CI is false-green

**Location:** `.github/workflows/ci.yml`

- `cargo clippy -p synth-control -p synth-webmail -p synth-watchdog -p synth-migrate 2>&1 | head -n 50` returns the exit status of `head`, not `cargo` (no `pipefail`), so failures are hidden.
- `cargo test --workspace --offline` will fail on a cold runner unless dependencies are cached/vendored.
- `cargo sqlx migrate run ... || echo "migration syntax checked"` hides failures and `sqlx-cli` is not installed; the job proves nothing.
- `smoke` starts only `postgres` and `redis`; it never starts the apps or checks health.
- No `cargo audit`/`cargo deny`, no secret scan, no WASM/UI build, no image scan.
- Only three crates are clippy-enforced with `-D warnings`.

**Fix:** add `set -o pipefail` (or remove `| head`), install `sqlx-cli` and run migrations against the service, use `cargo test` without `--offline` (or vendor), add integration tests, run clippy `-D warnings` for the whole workspace, build `synth-ui`, add security/dependency scanning, and make smoke start the full stack and assert `/healthz`.

### M-19 — Documentation overstates completion

**Location:** `README.md`, `Plan.md:595–626`, `docs/releases/v1.0.0.md`, `docs/runbooks/security.md`

- `Plan.md` marks Phases 0–4 “complete” and v1.0.0 GA; `docs/releases/v1.0.0.md` claims GA and signed images.
- In reality: IMAP/JMAP/send are stubs, the UI is not served, DKIM signing is not wired, ACME is absent, backups are broken, webmail is unauthenticated, and migrations/CI are not truly validated.
- `docs/runbooks/security.md` claims rate limiting, HIBP, and a `synth-webmail` fail2ban jail are enforced; none are.

**Impact:** operators may deploy believing the system is production-ready.

**Fix:** add a prominent “Status / Known limitations” section; mark each feature as implemented/stubbed/broken; do not call a release v1.0/GA until the P0/P1 items are closed; keep the runbooks in sync with code.

### M-20 — Metrics are incomplete and not instrumented

**Location:** `crates/synth-common/src/metrics.rs`, all services

Only `inc_auth_attempt` is called (and only on successful password verification, `routes.rs:145`). `inc_auth_failure`, `inc_api_request`, `inc_mail_sent`, and `inc_imap_fetch` are never called. `/metrics` therefore reports mostly zeros. There is no HTTP middleware for request counts/latency/status, no queue depth, no DB pool metrics, and no health gauges. `prometheus-client` is a dependency but unused.

**Fix:** add an Axum middleware for request count/duration/status; increment auth failure; expose queue/DB/pool gauges; use labels carefully; add dashboards (currently empty, see M-21).

### M-21 — Monitoring stack is empty/duplicated

**Location:** `docker/monitoring/*`, `compose.yaml:248–284`

- `grafana-provisioning/` and `grafana-dashboards/` are empty, so Grafana has no datasource or dashboards.
- `prometheus.yml` scrapes `synth-watchdog:8081` twice (`synth-watchdog` and `postfix-queue`); there is no Postfix exporter.
- Grafana uses the default `admin` password and is exposed on `:3001`.

**Fix:** add datasource/dashboard provisioning, a real Postfix/Dovecot exporter (or remove the fake job), set a strong Grafana password/secret, and bind the port to localhost.

### M-22 — Deprecated APIs, dead code, and warning debt

**Location:** workspace-wide

`redis::Client::get_async_connection` / `redis::aio::Connection` are deprecated (use `get_multiplexed_async_connection`); `synth-control` has 24 clippy warnings, `synth-webmail` 17, plus dead code (`redis_conn`, `TotpReq`, `sanitize_preview`, `fetch_messages`, `reindex_batch`, `MailMessage`, etc.), unused imports (`Arc`, `AppResult`, `put`, `ImapClient`), and useless `format!` calls. CI only denies warnings for three crates.

**Fix:** enable `-D warnings` for the workspace, remove dead code or wire it, migrate to multiplexed connections, and add `#![deny(warnings)]` only after the debt is cleared.

### M-23 — Domain/user/alias data model is not relationally enforced

**Location:** `crates/synth-control/migrations/20261107000000_init.sql`

`users` has no `domain_id` FK; `aliases` has no domain FK; `transport` has nullable columns in a composite primary key (effectively always required); `quota_overrides` is unused; deleting a domain leaves its users active; `audit_log.actor` is not an FK (acceptable for retention, but undocumented). These allow orphaned mailboxes and inconsistent state.

**Fix:** add domain references (or a normalized `domains`/`users` relationship), cascade/deactivate users when a domain is removed, drop unused tables or wire them, and document intentional denormalization.

### M-24 — `seed_admin` resets existing credentials and migration logs hash prefixes

**Location:** `crates/synth-control/src/db.rs:20–64`, `crates/synth-migrate/src/main.rs:232–238`

- `seed_admin` is invoked by `--seed-admin` and uses `ON CONFLICT (email) DO UPDATE SET password_hash=EXCLUDED.password_hash`, so it resets an existing admin’s password on every run.
- `synth-migrate verify` prints the first 16 characters of every password hash, leaking material into logs.

**Fix:** change the conflict clause to `DO NOTHING` (or require `--force-rotate`); print only “hash present/valid” without any hash characters.

---

## 6. Low / hygiene findings (P3)

| ID | Finding | Location | Fix |
|---|---|---|---|
| L-01 | TOTP `last_used_step` is `TIMESTAMPTZ` and never checked; codes can be replayed within the window | `migrations/...phase2.sql`, `routes.rs:1304` | Store the numeric step and reject reuse |
| L-02 | Panic path on malformed `sub`: `Uuid::parse_str(&claims.sub).unwrap()` after earlier `unwrap_or(nil)` | `routes.rs:237` | Return `Unauthorized` consistently |
| L-03 | `--seed-admin` resets the password every boot | `db.rs:52–63` | Use `DO NOTHING` or explicit rotation |
| L-04 | CSV parser splits on every comma; quoted fields/passwords containing commas corrupt data | `synth-migrate/src/main.rs:50–67` | Use `csv` crate |
| L-05 | `synth-migrate verify` prints hash prefixes | `synth-migrate/src/main.rs:232–238` | Do not print hash material |
| L-06 | `parse_csv_line` checks `cols.len() < 1` (always false) | `synth-migrate/src/main.rs:52` | Use `is_empty()` |
| L-07 | `synth-bench` tests are trivial; no perf regression gate in CI | `synth-bench/src/main.rs` | Add thresholds/benchmarking job |
| L-08 | `scripts/sign-release.sh` has a syntax error: `docker lockfiles="$(mktemp)"` | `scripts/sign-release.sh:12` | Remove/fix; use `mktemp` directly |
| L-09 | No graceful shutdown, request timeouts, or CORS policy | `synth-control/src/main.rs`, `synth-webmail/src/main.rs` | Use `axum::serve(...).with_graceful_shutdown`, `tower-http::timeout`, configure CORS |
| L-10 | `queue_status` returns fake/zero active/hold counts and a misleading note; `POSTFIX_QSHAPE_PATH` is not passed by compose | `routes.rs:802–823`, `compose.yaml` | Poll real `qshape`/Postfix stats and pass the env |
| L-11 | `auth_me` and JWT acceptance do not check `active`; `force_password_change` is never enforced | `routes.rs:182–198` | Centralize principal loading and checks |
| L-12 | `synth-ui` has dead `HomePage`, unused `leptos_router`, and a test that asserts `true` | `synth-ui/src/lib.rs` | Remove dead code; add real component tests |
| L-13 | `sw.js` caches `/` and `/index.html` but there is no offline fallback; manifest icons are missing | `crates/synth-ui/*` | Add icons and a proper offline fallback |

---

## 7. What needs fixing — prioritized plan

### P0 — Blockers (do first, no deployment until closed)

- [x] Protect every admin read endpoint and add per-resource authorization (C-01). ✅ `fix/security-audit`
- [x] Rewrite TOTP enroll/verify: bind to `claims.sub`, require password re-auth, auth on verify, replay protection (C-02). ⚠️ **Encryption of the secret at rest (M-15) still open** — needs a managed KMS/age key.
- [x] Enforce TOTP in webmail login or remove the separate login (C-03). ✅
- [x] Add authentication + `user_id` scoping to all webmail/JMAP/DAV/search/push routes (C-04, H-12). ✅
- [x] Remove default JWT secret and default admin; fail startup on placeholders; pass `INITIAL_ADMIN_*` correctly (C-05). ✅
- [x] Implement TLS end-to-end (ACME client + fail-closed listeners) (C-06). ✅ Opt-in `acme` profile (certbot HTTP-01, renewal loop); `REQUIRE_TLS=true` fail-closed in postfix+dovecot; dovecot `ssl = required`; postfix `smtpd_tls_auth_only = yes`; plaintext ports loopback-bound; edge 443 only with certs. (Re-verified 2026-11-07.)
- [x] Restrict the SMTP shim (documented trusted-networks-only + startup warning) and fix the lost-body bug (C-07, H-14). ✅ ⚠️ Shim is still unauthenticated by design — bind it to loopback/a private network in production.
- [x] Enforce API token expiry and scopes (H-01). ✅

### P1 — Must fix for a usable beta

- [x] Add in-app rate limiting/lockout (H-02). ✅ ⚠️ fail2ban `banaction = dummy` and nginx limits still unwired.
- [x] Enforce password policy + HIBP (H-03). ✅
- [x] Add session revocation on logout/password reset/2FA change (H-04). ✅ (24h TTL retained, `token_version` revocation)
- [x] Stop publishing internal ports; bind admin/monitoring to localhost; remove Grafana `admin/admin` (H-05). ✅
- [x] Add `postfix-pgsql` and the pgsql map files (H-06). ✅ ⚠️ Container smoke test (`postmap -q`) not yet run.
- [x] Encrypt DKIM keys and sync them to OpenDKIM; generate KeyTable/SigningTable (H-07). ✅ AES-256-GCM `synth_common::secrets` (ENCRYPTION_KEY) at rest; synth-control syncs key files + tables to the shared `dkim` volume (`/var/dkim-sync`); `MinimumKeyBits 2048`. (Re-verified 2026-11-07.)
- [x] Fix Redis auth/config mismatch (H-08). ✅ (`--requirepass` + healthcheck auth)
- [x] Replace the broken backup service (H-09). ✅ ⚠️ postgres:16-alpine + PGPASSWORD + mounts; encryption/checksum/restore test still open.
- [x] Return `501` for stubbed webmail features instead of fake success (H-10); search is `user_id`-scoped (M-11). ✅
- [x] Build and serve the admin UI (H-11). ✅ Trunk multi-stage build (`docker/synth-ui/Dockerfile`, wasm32), `synth-ui` compose service, edge `/admin/` (80+443), WASM check in CI. (Re-verified 2026-11-07.)
- [x] Fix Rspamd config (H-13). ✅ (split into `local.d/` snippets) ⚠️ Pass `rspamadm configtest` in CI.
- [x] Make CI truly fail on clippy/migration/test failures (M-18). ✅ workspace `-D warnings`, `pipefail`, real `sqlx migrate run`
- [x] Correct the project status claims in docs (M-19). ✅ (README status section + CHANGELOG-AUDIT.md)

### P2 — Correctness, security hardening, operations

- [x] Fix Sieve generation with unit tests (M-01). ✅ ⚠️ `sievec` external validation not wired.
- [x] Move blocking work to `spawn_blocking` (M-02).
- [x] Use transactions for multi-step writes (M-03).
- [x] Fix autoconfig XML/domain/content type (M-04).
- [x] Fix DKIM `dns_record` TLD bug (M-05).
- [x] Fix config import/export (M-06).
- [x] Stop leaking internal errors (M-07).
- [x] Prevent account enumeration (M-08).
- [x] Wire documented env vars (M-09).
- [x] Make watchdog status real; ACME challenge validated + async I/O; backup age checked (M-10). ✅ ⚠️ No real ACME client — challenge endpoint only serves a manually/populated directory.
- [x] Validate DMARC ingestion (M-13).
- [x] Strengthen input validation/RBAC/last-admin protection (M-14).
- [x] Pin images, add `.dockerignore`, non-root users, LICENSE, toolchain (M-16). ✅ `.dockerignore`, `LICENSE`, `rust-toolchain.toml` (stable), `cargo build --locked` in Rust Dockerfiles, non-root `appuser` (synth-control chowns dkim volume then drops privileges), versioned image pins (rspamd 3.6.0, certbot v4.0.0 verified on Docker Hub; clamav is upstream channel-pinned).
- [x] Fix or remove the HA overlay (M-17). ✅ Removed (commit `1528979`); `docs/runbooks/ha.md` documents the Patroni future path. `docker/compose.ha.yaml` no longer exists.
- [x] Add real metrics and dashboards (M-20, M-21). ✅ DB pool + postfix queue gauges, HTTP latency histogram (`{service}_http_request_latency_seconds`), Grafana datasource + overview dashboard with new panels; duplicate `postfix-queue` job removed (queue gauge is served by synth-control).
- [x] Clear clippy/deprecation debt and enforce workspace warnings (M-22).
- [x] Normalize the data model (M-23). ✅ `users.domain_id`/`aliases.domain_id` FKs + backfill + index, linked at user creation, domain delete deactivates users/aliases; `quota_overrides` dropped (drift eliminated); email-domain trigger (`fn_users_domain_managed`, `20261107000004_recheck.sql`).
- [x] Stop resetting credentials on seed; stop logging hashes (M-24).

### P3 — Hygiene

- [x] Address L-01 through L-13. ✅ L-07 bench exits non-zero on regression + CI bench job; L-10 real deferred/active/hold spool counts in `/api/v1/queues`; L-13 PWA icons (192/512) + offline fallback document + Trunk copy-file.
- [x] Add integration tests. ✅ DB-backed suite (`crates/synth-control/tests/integration_db.rs`: migrations apply, two-user `mail_search` isolation mirroring C-04/M-11) + `integration` CI job with Postgres service; skips gracefully without `DATABASE_URL`.
- [x] Add `cargo audit`/`cargo deny`, secret scanning, and image scanning to CI. ✅ `security_scan` job: cargo-audit, gitleaks, trivy on the three app images.
- [x] Add end-to-end compose smoke. ✅ `scripts/smoke.sh` (health wait, admin login, domain+user provisioning, send→delivery check via webmail APIs). CI smoke stays best-effort; run the script locally against a Docker host.

---

## 8. Positive findings

- The workspace is cleanly separated into `common`, `control`, `webmail`, `watchdog`, `filter`, `dkim`, `migrate`, `smtp`, `ui`, and `bench`; responsibilities are clear.
- `cargo check`, `cargo test` (39 tests), `cargo fmt --check`, and `docker compose config` all pass.
- Password hashing uses Argon2id, and password hash comparison uses the library verifier rather than a hand-rolled compare.
- TOTP implements RFC 4226/6238 HOTP correctly, with base32 encode/decode and ±1-step skew; the RFC vector test passes.
- HIBP k-anonymity, DNS-over-HTTPS checks, Sieve escaping, and rate-limiter primitives are present and unit-tested even if not wired in.
- SQL queries consistently use bind parameters; no SQL injection was found in the source reviewed.
- The migrations are idempotent (`IF NOT EXISTS`) and use `citext`, UUIDs, cascading FKs for the tables that have them, and useful indexes.
- Docker healthchecks, JSON logging, `.env`-based config, and a CI skeleton are already in place; the foundation is good.
- The `synth-smtp` and `synth-bench` crates are correctly labeled as spikes/experiments in their comments, which is the right instinct — the issue is that the docs present the overall product as further along than the code.

---

## Appendix A — Control-plane auth matrix

Legend: ✅ auth enforced; ❌ no auth; ⚠️ partial/self-or-admin.

| Route | Auth | Notes |
|---|---|---|
| `POST /api/v1/auth/login` | ❌ (public by design) | No rate limit/lockout |
| `POST /api/v1/auth/logout` | ❌ | No-op; should revoke |
| `GET /api/v1/auth/me` | ✅ JWT | Does not check `active` |
| `POST /api/v1/auth/totp/enroll` | ✅ JWT (any user) | Can target any email (C-02) |
| `POST /api/v1/auth/totp/verify` | ❌ | Password field ignored (C-02) |
| `POST /api/v1/auth/totp/disable` | ✅ JWT | No password re-auth |
| `GET/POST /api/v1/domains` | ⚠️ | GET public; POST admin |
| `GET/PUT/DELETE /api/v1/domains/{id}` | ⚠️ | GET public; PUT/DELETE admin |
| `GET /api/v1/domains/{id}/dns` | ❌ | Public |
| `GET /api/v1/domains/{id}/verify` | ❌ | Public, triggers DNS |
| `GET/POST /api/v1/users` | ⚠️ | GET public; POST admin |
| `GET/PUT/DELETE /api/v1/users/{id}` | ⚠️ | GET public; PUT/DELETE admin |
| `POST /api/v1/users/{id}/password` | ✅ admin | No session revocation |
| `GET/POST /api/v1/aliases` | ⚠️ | GET public; POST admin |
| `GET/PUT/DELETE /api/v1/aliases/{id}` | ⚠️ | GET public; PUT/DELETE admin |
| `GET /api/v1/quota` | ❌ | Public |
| `GET /api/v1/queues` | ❌ | Public |
| `POST /api/v1/dkim/{id}/generate` | ✅ admin | Plaintext key |
| `GET /api/v1/dkim/{id}` | ❌ | Public |
| `GET/PUT /api/v1/filters/{user_id}` | ⚠️ | GET public; PUT admin-or-self |
| `GET/PUT /api/v1/vacation/{user_id}` | ⚠️ | GET public; PUT admin-or-self |
| `GET /api/v1/admin/audit` | ❌ | Public |
| `GET /api/v1/admin/bans` | ❌ | Public |
| `GET /api/v1/config/export` | ❌ | Public |
| `POST /api/v1/config/import` | ✅ admin | Users ignored |
| `GET/POST /api/v1/dmarc/reports` | ⚠️ | GET public; POST admin |
| `GET/POST /api/v1/push/subscribe` | ❌ | Assigned to first user |
| `DELETE /api/v1/push/subscribe/{id}` | ❌ | Anyone can delete |
| `GET/POST /api/v1/api-tokens` | ✅ admin | Expiry/scopes not enforced |
| `DELETE /api/v1/api-tokens/{id}` | ✅ admin | Good user scoping |

**Webmail crate:** every route (`/api/v1/mail/*`, `/jmap*`, `/api/v1/carddav/*`, `/api/v1/caldav/*`, `/dav`, `/api/v1/auth/login`) has ❌ no auth except that `web_login` issues a token. There is no extractor or middleware.

---

## Appendix B — Documented env vars vs. actual wiring

| Variable | Documented | Read by `Config::from_env` | Passed by compose | Notes |
|---|---|---|---|---|
| `MAIL_HOST` | ✅ | ✅ | ✅ | |
| `MAIL_DOMAIN` | ✅ | ✅ | ❌ | Defaults to `example.com` in control |
| `SESSION_SECRET` | ✅ | ✅ | ✅ (weak fallback) | No placeholder check |
| `DATABASE_URL` | ✅ | ✅ | ✅ | |
| `REDIS_URL` | ✅ | ✅ | ✅ | Password mismatch with server |
| `POSTGRES_PASSWORD` | ✅ | ❌ | ✅ (postgres/dovecot) | Not passed to backup |
| `INITIAL_ADMIN_EMAIL` | ✅ | ❌ | ❌ | Container uses default |
| `INITIAL_ADMIN_PASSWORD` | ✅ | ❌ | ❌ | Container uses default |
| `LETSENCRYPT_EMAIL` | ✅ | ✅ | ❌ | Only used by config, not ACME |
| `RATE_LIMIT_PER_MIN` | ✅ | ❌ | ❌ | Limiter unused |
| `HIBP_ENABLED` | ✅ | ❌ | ❌ | HIBP unused |
| `TOTP_ENFORCED_FOR_ADMIN` | ✅ | ❌ | ❌ | Not implemented |
| `POSTFIX_QSHAPE_PATH` | ✅ | ❌ (read directly in handler) | ❌ | Queue status fake |
| `ALERT_WEBHOOK_URL` / `NTFY_TOPIC` | ✅ | ❌ (read directly) | ❌ | Watchdog only |
| `RADICALE_URL` | ✅ | ❌ | ❌ | Radicale not used |
| `BACKUP_RETENTION_DAYS` | ✅ | ❌ (read directly) | ❌ | Backup broken |
| `RCLONE_REMOTE` | ✅ | ❌ | ❌ | Backup broken |

---

## Appendix C — Files reviewed

- `Cargo.toml`, `Cargo.lock`, `README.md`, `Plan.md`, `.env.example`, `.env`
- `crates/synth-common/src/*.rs` (auth, config, dns_check, error, hibp, metrics, rate_limit, telemetry, totp)
- `crates/synth-control/src/*.rs` + all three migrations
- `crates/synth-webmail/src/*.rs` (app, carddav, fts_indexer, imap, jmap, main, routes)
- `crates/synth-smtp/src/main.rs`
- `crates/synth-dkim/src/lib.rs`
- `crates/synth-filter/src/lib.rs`
- `crates/synth-migrate/src/main.rs`
- `crates/synth-watchdog/src/main.rs`
- `crates/synth-bench/src/main.rs`
- `crates/synth-ui/*` (Cargo, lib, index.html, Trunk.toml, manifest, sw.js)
- `compose.yaml`, `docker/compose.stalwart.yaml`, `docker/compose.ha.yaml`
- All Dockerfiles, Postfix/Dovecot/Rspamd/OpenDKIM/OpenDMARC/fail2ban/edge/monitoring configs
- `.github/workflows/ci.yml`, `scripts/sign-release.sh`
- `docs/phase3.md`, `docs/releases/v1.0.0.md`, `docs/runbooks/security.md`

---

*End of audit.*
