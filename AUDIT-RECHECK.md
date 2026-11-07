# SynthMail — Re-audit (second pass)

**Date:** 2026-11-07  
**Revision:** `3aefa77` — `Merge pull request #1 from binkapS/fix/security-audit`  
**Scope:** Re-audit after `d8edf83` (`fix(security): close audit P0/P1 findings`) against the findings in [`AUDIT.md`](AUDIT.md) (2026-11-07). All 54 original findings were re-checked source-by-source; residual and new issues are listed separately.

> **Headline:** the merge genuinely closes the large majority of the P0/P1 security issues. All seven criticals, and nearly all highs/mediums, are fixed or substantially fixed. **Remaining production blockers** are narrower: TLS/ACME provisioning (C-06), DKIM key encryption + OpenDKIM sync (H-07), TOTP secret encryption (M-15), full backup, HA overlay, and the admin UI build/serve path. A handful of new/minor issues were found in the fix itself.

---

## 1. Verification snapshot

| Check | Result |
|---|---|
| `cargo check --workspace --all-targets` | ✅ Pass, no warnings summary line |
| `cargo test --workspace` | ✅ **67 tests pass** (was 39) |
| `cargo fmt --check` | ✅ Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ Pass (clean) |
| `docker compose config -q` | ✅ Pass (base + stalwart + HA overlays) |
| Live DB / E2E mail flow | ❌ Still not exercised in this audit |

**Test growth:** 39 → 67, with new coverage for TOTP step verification, password policy, Sieve generation/validation, SMTP envelope/bounds, watchdog status/ACME validation, config validation, and domain/email/alias/DMARC validation.

---

## 2. Original findings — verdicts

### C-01 Authenticated admin endpoints — ✅ FIXED
**Evidence:** `crates/synth-control/src/routes.rs` router (`:20–87`) plus a uniform `authenticate` / `require_admin_principal` / `require_admin_or_self` family. Every former public `GET` now takes `headers` and calls `require_admin_principal(..., false)` (read gate) or `require_admin` (write gate):
- `list_domains` `:562`, `get_domain` `:610`, `domain_dns` `:711`, `domain_verify` `:752`, `list_users` `:876`, `get_user` `:970`, `list_aliases` `:1204`, `get_alias` `:1248`, `quota_overview` `:1317`, `queue_status` `:1327`, `dkim_get` `:1429`, `list_filters` `:1442`, `get_vacation` `:1523`, `list_audit` `:1589`, `list_bans` `:1605`, `config_export` `:1629`, `list_dmarc_reports` `:1767`.
- Side effects: `resolve_admin`, `map_domain_tables`, `fetch_domain`, `fetch_user`, `fetch_alias` helpers eliminate duplicated queries and make the gate consistent.
- `count_requests` middleware (`:89–92`) provides `inc_api_request` (M-20 partially).
- **Residual:** none for C-01 itself. `/healthz`, `/readyz`, `/metrics`, login, and well-known files remain public by design.

### C-02 TOTP enrollment/verification — ✅ FIXED (one usability caveat, R-02)
**Evidence:** `routes.rs:1884–2097` rewritten.
- `totp_enroll` (`:1899`) now authenticates the caller, uses **only `p.user_id` from the token**, requires the current password (`TotpEnrollReq { password, code }`), and — if 2FA is already active — also requires a current TOTP code (`check_totp_code`). No client-supplied `email` field exists.
- `totp_verify` (`:1962`) now requires auth, targets only the caller (`user_id` from the token), optionally re-checks a password, and uses replay protection.
- `totp_disable` (`:2035`) requires password re-auth and bumps `token_version`.
- Replay protection (`totp.rs:83–97` `verify_with_step` + `routes.rs:2013–2028` `check_totp_code`) returns the matched step and rejects `step <= last_used_step`; `user_totp.last_used_step` migrated from `TIMESTAMPTZ` to `BIGINT` (`20261107000003_audit.sql:4–15`).
- Rate limiting on enroll/verify (`tight_limiter`) is wired.
- **Residuals:** enrollment disables 2FA before the new code is confirmed, and `bump_token_version` invalidates the session mid-flow (see R-02). Secrets are still plaintext (see M-15 below).

### C-03 Webmail login bypassing TOTP — ✅ FIXED
**Evidence:** `crates/synth-webmail/src/routes.rs` — actually reverted to a stricter implementation:
- `web_login` (`:169–246`) now checks rate limit, records `login_attempts` with `ip`, enforces the same TOTP flow with replay protection (`:201–230`), and returns `429` on limit.
- Token now carries `token_version` and `kind` from the DB.
- **Residual:** webmail still issues JWTs signed with the same `SESSION_SECRET` as the control plane. That is safe only as long as both services validate `token_version`/`active` (they do: `authenticate_user` at `routes.rs:57–88`). The shared secret remains a blast-radius risk but is no longer an auth bypass.

### C-04 Webmail/JMAP/CardDAV/CalDAV cross-user exposure — ✅ FIXED
**Evidence:**
- `authenticate_user` (`webmail/routes.rs:57–88`) validates JWT, `active`, and `token_version`.
- `search` is now `WHERE user_id=$1` (`:304`).
- `send` only allows `from == user.email` and otherwise returns `Forbidden` (`:331–338`).
- CardDAV/CalDAV (`carddav.rs`) all authenticate and use `user.user_id` for list/create/delete/query (`:30–174`). The “first user in DB” pattern is gone.
- JMAP session/dispatch authenticate (`:90–123`).
- `dav_discovery` (`carddav.rs:176–191`) is still unauthenticated but returns **only static XML** with no user data — acceptable, but consider requiring auth for consistency (see N-03).
- **Residual:** JMAP `handle_method` still returns static mailbox counts (H-10, partial) — but it is authenticated and no longer impersonates a user.

### C-05 Default/known secrets and default admin — ✅ FIXED (with a weak seed note, R-01)
**Evidence:**
- `Config::default()` sets `session_secret = ""` (`config.rs:82`) and `validate()` rejects empty, `<32`, and placeholder values including the old `dev-secret-change-me-32bytes-min-0000` and `CHANGE_ME_32B_MIN_…` (`:172–195`).
- `compose.yaml`: `SESSION_SECRET=${SESSION_SECRET:?SESSION_SECRET must be set}` for both services (`:161`, `:186`); `INITIAL_ADMIN_EMAIL`/`INITIAL_ADMIN_PASSWORD` are forwarded (`:163–164`).
- `db.rs`: `initial_admin_password()` filter (`:20–24`) rejects empty and `changeme123!admin`, otherwise generates a random password and logs it once at `error` level; `seed_admin_if_empty` checks `kind IN ('admin','superadmin')`; `seed_admin_inner` uses `ON CONFLICT (email) DO NOTHING` and no longer resets existing credentials.
- **Residuals:** the one-time generated password is logged (acceptable for a 18-byte random, but copies remain in logs — see R-01); compose still passes `ChangeMe123!Admin` as the default env value, which the Rust code ignores by generating a random password, so the effective behavior is safe but the compose default is misleading. A strong user-provided seed password is *not* checked against the password policy (see R-03).

### C-06 TLS everywhere / ACME — ❌ NOT FIXED (acknowledged)
**Evidence:** unchanged.
- `docker/dovecot/dovecot.conf:5` still declares `ssl = no`; the container entrypoint only enables TLS if a cert already exists.
- `docker/postfix/main.cf:20–24` still uses `smtpd_tls_security_level = may`, `smtpd_tls_auth_only = no`, and cert paths under `/etc/letsencrypt` that nothing provisions.
- No ACME client container/job exists; `synth-watchdog` only serves the challenge endpoint from a directory nobody populates.
- `edge` only serves 443 when the same certs exist (`docker/edge/443.conf.example`).
- **Impact:** passwords/mail on plaintext Dovecot/Postfix listeners. Composing the stack still requires manual cert provisioning, which is undocumented.

### C-07 Unauthenticated SMTP shim — ✅ FIXED for a trusted-network scope, still unauthenticated by design
**Evidence:** `crates/synth-smtp/src/main.rs`:
- Header banner (`:5–9`) warns the shim is unauthenticated and must not be publicly exposed; a runtime `tracing::warn!` (`:290–292`) reinforces it.
- `EHLO` no longer advertises `AUTH`/`STARTTLS` (`:128–130`) and a test asserts this (`:311–331`).
- Body bug fixed: `handle_line` no longer `take`s `data`; `data.clone()` is preserved (`:95–110`), and `full_transaction_queues` asserts `d == "Subject: hi\r\n"` (`:350–378`).
- Envelope addresses are parsed bare (`extract_envelope_address`, `:58–75`); trailing-whitespace trimming replaced with `strip_crlf` (`:50–53`), preserving DATA verbatim.
- Limits added: 10 MiB message (`:118–122`), 50 recipients (`:146–148`), 1000-byte line (`:86–92`, RFC 5321), Redis TTL 24h + pending-list trim to 1000 (`:211–235`).
- **Residuals:** still binds `0.0.0.0`, no AUTH/STARTTLS, no connection/timeouts/rate limits, unbounded memory per connection up to 10 MiB. Treat as an internal-only spike (see N-05).

### H-01 API-token expiry/scopes — ✅ FIXED
**Evidence:** `routes.rs:2099–2238`.
- `authenticate_api_key` filters `(expires_at IS NULL OR expires_at > now())` (`:2219–2220`) and returns `(user_id, scopes)`.
- `Principal { scopes: Option<Vec<String>> }` (`:113–132`); JWT = unscoped (`None` → always passes), API token = enforced (`has_scope`).
- `require_admin_principal(write)` demands `read` for reads and `write` for writes; `create_api_token` defaults to `["admin"]` and validates `expires_in_days ∈ 1..=3650` (`:2119–2153`).
- **Note:** defaulting to full admin when no scopes are given is permissive but documented; `Some([])` would deny all. Consider changing the default to `["read"]`.

### H-02 Brute-force/rate-limit/fail2ban — ✅ FIXED (with a distributed limit caveat, N-02)
**Evidence:**
- `RateLimiter` now constructed in both `AppState`s from `cfg.rate_limit_per_min` (`control/app.rs:21–23`, `webmail/app.rs:14`).
- `auth_login` enforces per-IP+account limiter + DB lockout (≥20 failures in 15 min) (`:336–358`); `record_login_attempt` stores `ip`; metrics `inc_auth_failure`/`inc_auth_attempt` now wired.
- TOTP enroll/verify have a `tight_limiter` (`routes.rs:1906`, `:1969`).
- `docker/fail2ban/filter.d/synth-webmail.conf` ships a filter; `jail.local` uses `postfix[mode=auth]` for Debian 1.0.2 and an `entrypoint.sh` that pre-creates log files. `banaction = dummy` remains intentional (documented: enforcement is in-app).
- **Residual:** `RateLimiter` is in-memory per process (see N-02); the shared Redis backend is not used for limits, so multiple replicas split the budget.

### H-03 Password policy / HIBP — ✅ FIXED
**Evidence:**
- `crates/synth-common/src/policy.rs` — `validate_password_strength` (≥12 chars default, denylist, special-char, no leading/trailing whitespace) + `check_password_allowed` (HIBP when enabled).
- Wired into `create_user` (`:930`), `reset_password` (`:1117`), and `auth_change_password` (`:487`) — all three server-side password-setting paths.
- `Config` now reads `HIBP_ENABLED`, `RATE_LIMIT_PER_MIN`, `TOTP_ENFORCED_FOR_ADMIN`, `MIN_PASSWORD_LEN` (see M-09).
- **Residual:** `seed_admin` does not run `check_password_allowed` on a user-supplied `INITIAL_ADMIN_PASSWORD` (see R-01).

### H-04 Session revocation — ✅ FIXED
**Evidence:**
- `token_version` column (`20261107000003_audit.sql:2`) + `tv` claim + `iss`/`aud` validation (`auth.rs:11–24`, `:73–79`).
- `authenticate` rejects JWTs whose `tv` no longer matches the DB (`routes.rs:192–200`); `load_principal` rejects inactive users (`:134–153`).
- `auth_logout` bumps `token_version` per user (`:430–443`); `auth_change_password` (`:471–513`) and `reset_password` (`:1110–1143`) clear `force_password_change`-relevant flags and bump; `update_user` bumps on deactivation (`:1039–1046`); TOTP enroll/disable bump (`:1940`, `:2057`).
- `force_password_change` is enforced in `require_admin_principal` (`:217–221`) and surfaced in `auth_me` (`:459–462`).
- **Residuals:** `force_password_change` is only enforced for admin APIs, not for webmail/user self-APIs (see R-04); 24 h TTL is still long but revocation exists.

### H-05 Port exposure — ✅ FIXED
**Evidence:** `compose.yaml` — `synth-control` `127.0.0.1:8000`, `synth-webmail` `127.0.0.1:3000`, Prometheus `127.0.0.1:9090`, Grafana `127.0.0.1:3001`, Radicale `127.0.0.1:5232`; `synth-watchdog` publishes no host port; Grafana password is `${GRAFANA_ADMIN_PASSWORD:?…}` (`:292`).
- **Residual:** mail ports (25/465/587, 143/993/110/995/4190) remain on `0.0.0.0` by design; publishing plaintext 143/110 is a TLS concern (see C-06).

### H-06 Postfix `pgsql` maps — ✅ FIXED
**Evidence:** `docker/postfix/Dockerfile` installs `postfix-pgsql`, copies `docker/postfix/pgsql/*.cf` with `600`/`postfix` ownership; the entrypoint substitutes `${POSTGRES_PASSWORD}`; the three map files query `domains`, `users`, and `aliases` with correct Maildir path derivation.

### H-07 DKIM plaintext keys / no OpenDKIM sync — ❌ NOT FIXED
**Evidence:** `routes.rs:1385–1397` still inserts `pair.private_key_pem` into `dkim_keys.private_key_enc` (comment acknowledges H-07 residual). No code writes keys to `/etc/opendkim/keys` or generates `KeyTable`/`SigningTable`; `opendkim` still starts with empty table files and `MinimumKeyBits 1024`. Signing will not work; keys remain exposed to DB reads/backups.

### H-08 Redis auth contradiction — ✅ FIXED
**Evidence:** `redis` service now runs `redis-server --appendonly yes --requirepass ${REDIS_PASSWORD:?…}` with a matching healthcheck (`compose.yaml:231–246`), and `.env.example` documents `REDIS_PASSWORD`/`REDIS_URL` must match. Rspamd `local.d/redis.conf` imports `${REDIS_PASSWORD}` via UCL.

### H-09 Backup broken — ⚠️ PARTIALLY FIXED
**Evidence:** image is now `postgres:16-alpine` (ships `pg_dump`), `PGPASSWORD`/`POSTGRES_PASSWORD` are passed, `maildir:/var/vmail:ro` is mounted, `service_healthy` gates startup, `set -e` makes failures visible. 
- **Residual:** no `rclone` (not in `postgres:16-alpine`), so off-site sync is gone; no encryption/checksum/retention/restore test. `CHANGELOG-AUDIT.md` lists this as a residual.

### H-10 Webmail stubs / fake success — ✅ FIXED (honest 501s remain by design)
**Evidence:** `webmail/routes.rs` — `list_folders`, `list_messages`, `get_message`, `list_drafts`, `delete_draft` now authenticate and return `501 Not Implemented` via `not_implemented()` (`:257–261`); `send` enforces identity then returns 501.
- **Residual:** the actual IMAP/SMTP/JMAP implementation is still missing (the API is honest about it, but the feature is absent). `fts_indexer::reindex_batch` is still never scheduled (see N-04).

### H-11 Admin UI not built/served — ⚠️ PARTIALLY FIXED
**Evidence:** `crates/synth-ui/src/lib.rs` LoginPage now performs a real `POST /api/v1/auth/login` via `gloo-net` and reports status; dead `HomePage` removed; tests assert the login endpoint.
- **Residual:** no Trunk build stage, no `docker/synth-ui` image/service, no static-file serving in `synth-webmail`, no CI WASM build. The UI compiles but is still not deployed.

### H-12 Push subscriptions unauthenticated / first-user — ✅ FIXED
**Evidence:** `routes.rs:1790–1847` — all three handlers authenticate and use `p.user_id`; create validates `https://` and non-empty `p256dh`/`auth`; list/delete are self-scoped; unique index `(user_id, endpoint)` added in the migration.

### H-13 Rspamd config invalid — ✅ FIXED (config-test still missing)
**Evidence:** config split into `docker/rspamd/local.d/{options,antivirus,redis,bayes,fuzzy}.conf` with correct `clamav { servers }` and `servers`+`password` for Redis. The old monolithic `docker/rspamd/rspamd.conf` is removed.
- **Residual:** no `rspamadm configtest` in CI or a container smoke test (see N-07).

### H-14 SMTP body loss / raw commands — ✅ FIXED
**Evidence:** see C-07. Envelope addresses parse to bare addresses; body preserved verbatim with tests asserting exact bytes (`:379–396`).

### Mediums (M-01 … M-24)
Most are fixed. Spot-checked in detail:

| ID | Verdict | Evidence |
|---|---|---|
| M-01 Sieve invalid | ✅ | `generate_vacation_sieve` quoted, date-guarded, no `"extension"`; spam sieve keys on `X-Spam-Status`; alias emits plain `redirect "dest"`. `validate_sieve` rejects bad capability/`:address`, checks quote balance. Tests cover all three generators. |
| M-02 Blocking in async | ✅ | `queue_status` + `dkim_generate` use `spawn_blocking` (`routes.rs:1331`, `:1380`); watchdog ACME uses `tokio::fs`; `check_backups` uses async dir walk. |
| M-03 Transactions | ✅ | `update_domain`, `update_user`, `save_filters`, `save_vacation` use `BEGIN…COMMIT` with `FOR UPDATE`; vacation replaces its script row instead of duplicating. |
| M-04 Autoconfig XML | ✅ | Correct namespace, real domain, `application/xml` content type (`:1851–1875`). |
| M-05 DKIM dns_record | ✅ | Now `format!("{}._domainkey TXT {}", selector, pair.dns_txt)` (`:1417`). |
| M-06 Config import | ✅ | Transactional; reports `created` vs `skipped`; users explicitly reported as skipped (no plaintext import). |
| M-07 Error leak | ✅ | `error.rs` now returns generic `"an internal error occurred"` for 5xx and logs the detail (`:47–60`, `:66–81`). |
| M-08 Enumeration | ✅ | Unknown/inactive/wrong-password all return identical `401` after Argon2 (dummy hash for missing users) (`:368–389`). |
| M-09 Env round-trip | ✅ | `config.rs:101–170` reads `CONTROL_URL`, `WEBMAIL_URL`, `ADMIN_URL`, `RATE_LIMIT_PER_MIN`, `HIBP_ENABLED`, `TOTP_ENFORCED_FOR_ADMIN`, `MIN_PASSWORD_LEN`; env test asserts round-trip. |
| M-10 Watchdog fake status | ✅ | `/status` reads an `Arc<RwLock<Vec<HealthStatus>>>` snapshot (`main.rs:76–78`, `:143–167`); alert dedupe (once per episode, ≤every 30 min); ACME token traversal rejected (`:41–51`); backup staleness warning. ACME *issuance* still absent (part of C-06). |
| M-11 FTS tenant-global/no indexer | ⚠️ | `search` route is now user-scoped, but `reindex_batch` is still never called and `fts_indexer::search` still lacks a tenant filter (see N-04). |
| M-12 Ed25519 PKCS#8 | ✅ | `generate_ed25519` uses `SigningKey::to_pkcs8_pem` with a DB round-trip test (`dkim/src/lib.rs`). |
| M-13 DMARC ingestion | ✅ (basic) | Size cap 1 MB, `<?xml` / `<feedback` sanity check, `report_type` whitelist, `ON CONFLICT (domain, report_id) DO NOTHING` + unique index. No XML parsing/entity protection (medium-low). |
| M-14 Input validation/RBAC | ✅ | `is_valid_domain`/`is_valid_email`/`clamp_quota`/`check_dmarc_policy`/`check_alias_targets`; domain must be managed; only superadmin can create admins; last-admin protection on update/delete; self-delete blocked. |
| M-15 TOTP secret plaintext | ❌ | Still plaintext in `user_totp.secret` (migration comment promises app-layer encryption; code stores the Base32 verbatim). |
| M-16 Supply-chain hygiene | ⚠️ | Some tags pinned (`nginx:1.27-alpine`, `radicale:3`, `prometheus:v2`, `grafana:11`); `LICENSE` added; authors fixed. Still floating tags (`rspamd:latest`, `clamav:stable-debian`, `stalwart:v1`), no `.dockerignore`, no `--locked`, apps run as root, no `rust-toolchain.toml`. |
| M-17 HA overlay | ❌ | Still no streaming replica (`postgres-replica` never `pg_basebackup`s; `primary_conninfo` passed as a bare `-c` string); `postfix-2` exposed incorrectly (no ports, no LB). |
| M-18 CI false-green | ✅ (best effort) | `pipefail`, workspace clippy `-D warnings`, real `sqlx migrate run`, `cargo test` without `--offline`, smoke `curl` probes (warn-only). Smoke never starts the full stack (documented as best-effort); no `cargo audit`/`deny`, secret scan, UI build, or image scan. |
| M-19 Docs honesty | ✅ | `README.md` status table + `CHANGELOG-AUDIT.md` added; `.env.example` notes ACME/Redis caveats. |
| M-20 Metrics incomplete | ⚠️ | `inc_api_request` wired via middleware; auth attempt/failure wired. `inc_mail_sent`/`inc_imap_fetch` still never called (send/IMAP are 501). No latency/queue/DB gauges. Grafana datasource + overview dashboard now provisioned, but `postfix-queue` duplicates the watchdog target. |
| M-21 Monitoring empty/duplicate | ⚠️ | Datasource + dashboard files added; passwords/loopback fixed. Duplicate `postfix-queue` job not removed; no Postfix exporter. |
| M-22 Clippy/deprecation | ✅ | Workspace `cargo clippy -- -D warnings` passes; deprecated `get_async_connection` replaced with `get_multiplexed_async_connection` (`readyz`, `redis_invalidate`, `redis_conn` removed). |
| M-23 Data model | ⚠️ | Uniqueness added (`dmarc_reports`, `push_subscriptions`, `mail_search`); `transport` nullable-PK and orphan-user problems not addressed. |
| M-24 Seed reset / hash log | ✅ | `DO NOTHING`, hash-prefix logging replaced with `is_argon2_hash` boolean in `synth-migrate` (M-24 partially: seed password not policy-checked). |

### Low / banding (L-01 … L-13)
Most fixed: L-01 (numeric step + replay), L-02 (`Uuid::parse_str` no longer unwraps in auth paths), L-04 (hand-written RFC4180 parser — correct but untested edges), L-06 (`is_empty` checks), L-08 (release script syntax fixed), L-09 (graceful shutdown in both `main.rs`), L-11 (`active`/`force_password_change` in principal loader), L-12 (UI dead code removed). L-03/L-05/L-07/L-10/L-13 partially or open.

---

## 3. New findings from this re-audit

### N-01 Rate limiter is per-process, not shared [Medium]
`synth-common/src/rate_limit.rs` uses a `Mutex<HashMap<String, Vec<Instant>>>` with sliding-window trimming. State is not stored in Redis, so two control replicas each grant the full budget, and keys with expired timestamps remain in the map as empty vectors (unbounded key growth over the process lifetime). Login lockout is DB-backed, which partially compensates. **Fix:** add TTL/eviction for keys, or move limiting to Redis (`INCR` + `EXPIRE`) for a shared view.

### N-02 Seed admin bypasses password policy [Medium]
`db.rs::seed_admin_inner` hashes whatever `INITIAL_ADMIN_PASSWORD` is provided (or the random fallback) without calling `check_password_allowed`. An operator can set a weak or breached admin password and the server will accept it. **Fix:** run `check_password_allowed` (or at least length + denylist) on the seed password before hashing; fail startup on violation if the password was explicitly provided.

### N-03 Webmail DAV discovery is unauthenticated [Low]
`dav_discovery` (`carddav.rs:176`) returns static XML with no user data, so no information is leaked — but the response is inconsistent with the authenticated JMAP/DAV JSON API. **Fix:** require `authenticate_user` for `/dav`, or document the intentional public discovery behavior.

### N-04 FTS indexer still never scheduled; library `search` still tenant-global [Medium]
`fts_indexer::reindex_batch` has no caller (no spawn in `webmail/main.rs`, no cron/trigger), and `fts_indexer::search` has no `user_id` predicate. The HTTP `search` route is fixed, but any future caller of the library function would leak. **Fix:** schedule the indexer (worker loop or DB trigger) and add the tenant filter + unique index (`mail_search(user_id, message_id)` already added in `20261107000003_audit.sql`).

### N-05 SMTP shim still defaults to `0.0.0.0` with no connection limits [Medium]
`synth-smtp` now documents “trusted networks only”, but the *default bind* is still `0.0.0.0` and there is no max-connections, idle timeout, or per-IP rate limit. An internet-facing deploy of the current default is still an open relay/queue flood. **Fix:** default `--host 127.0.0.1`, or drop the crate from the deployable surface until SASL exists. (It is not in the default compose file, which limits the blast radius.)

### N-06 Postfix `mynetworks` trusts the whole Docker subnet; milters fail-open [Medium]
`main.cf` sets `mynetworks` to `172.16.0.0/12` and `milter_default_action = accept`, so any compromised container on the bridge network can relay as `permit_mynetworks`, and a down OpenDKIM/OpenDMARC silently accepts mail. **Fix:** narrow `mynetworks` to the specific container subnets actually needed, and consider `reject` on milter timeout (or at least alert on milter failure).

### N-07 No config smoke tests in CI [Low–Medium]
Rspamd (`rspamadm configtest`), Postfix (`postmap`/`postfix check`), Dovecot (`doveconf -n`), OpenDKIM, and OpenDMARC configs are not validated in CI. The CI compose smoke only starts `postgres`+`redis`. **Fix:** add config-test steps and start the full stack with `/healthz` assertions.

### N-08 TOTP enrollment disables 2FA before confirmation [Medium]
Enroll sets `enabled=false` + `last_used_step=NULL` and then bumps `token_version`, invalidating the current JWT. Until `totp_verify` succeeds, the account has *no* second factor. If enrollment is abandoned (or the bumped-out session is recovered only via password login), 2FA stays off. **Fix:** store the pending secret in a separate column/row and only activate on verification; do not bump `token_version` until activation.

### N-09 Generated admin password is logged [Low]
`db.rs:34–38` logs the random one-time password at `error` level with `password = %generated`. This is convenient for first boot but permanently writes the credential to logs. **Fix:** print to stdout/stderr only with a prominent “show once” warning, or write to a `0600` file; redact the field in structured logs.

### N-10 Stalwart overlay port collision [Medium]
`docker/compose.stalwart.yaml` publishes `25/465/993/995/4190/443` on all interfaces. If combined with the base stack (Postfix/Dovecot/edge), Docker fails to bind. The overlay assumes a standalone Stalwart install. **Fix:** document `stalwart` as a standalone profile only, or bind to loopback.

### N-11 Sign-release script still produces an empty lockfile [Low]
`scripts/sign-release.sh` ends with `lockfiles="$(mktemp)"` and never writes digests into it. Cosign signing itself is fine. **Fix:** write `docker images --digests` output into the lockfile, or remove the step.

### N-12 `docker/postfix/pgsql` password substitution is fragile [Low]
The entrypoint `sed s|\${POSTGRES_PASSWORD}|${POSTGRES_PASSWORD}|` breaks or silently truncates if the password contains `&`, `;`, or `/`. The password comes from env (operator-controlled), so this is a robustness issue, not a vuln. **Fix:** use `printf`/awk with proper escaping, or pass the password via `PGPASSWORD`/`PG` env to the query.

### N-13 Dovecot auth still plaintext-capable [Low, tied to C-06]
`dovecot.conf` has `ssl = no` and no `ssl = required`, and `auth_mechanisms = plain login` allows credential passthrough over plaintext 143/110 when published. Once TLS is provisioned this closes; until then, require `ssl = required` or disable plaintext listeners.

---

## 4. Prioritized fix list (next round)

**P0 — must close before public exposure:**
- [ ] C-06: add an ACME client + renewal, fail-closed when certs are missing, `ssl = required`, `smtpd_tls_auth_only = yes`, disable plaintext 143/110 or bind them to loopback.
- [ ] H-07: encrypt DKIM private keys (KMS/age) and sync keys + `KeyTable`/`SigningTable` to OpenDKIM; raise `MinimumKeyBits` to 2048.
- [ ] M-15: encrypt TOTP secrets at rest (same KMS/age mechanism).

**P1 — should close before beta:**
- [ ] R-01/N-02: policy-check the seed admin password; keep the compose default misleading value removed; print the generated password once, not to logs.
- [ ] N-08: pending-secret enrollment flow for 2FA.
- [ ] R-04: enforce `force_password_change` in webmail/user self-APIs.
- [ ] N-01/N-02: distributed rate limiting (Redis) + map eviction.
- [ ] N-04: schedule `reindex_batch` and tenant-filter `fts_indexer::search`.
- [ ] N-06: narrow `mynetworks`; make milters fail-closed or alerting.
- [ ] N-05/N-10: SMTP shim loopback default; document Stalwart as standalone.
- [ ] H-11: add a Trunk build + static serving + WASM CI job.
- [ ] H-09: restore rclone/encrypted, checksummed, restore-tested backup.
- [ ] M-17: finish or remove the HA overlay; M-16: pin images/digests, `.dockerignore`, `--locked`, non-root users.

**P2 — hardening:**
- [ ] N-03 auth on `/dav` discovery; N-07 config-test CI; N-11 release lockfile; N-12 sed → safe substitution; N-13 `ssl = required`.
- [ ] DMARC ingestion: parse/validate XML with entity protection.
- [ ] M-20/M-21: add latency/queue gauges, remove the duplicate Prometheus job, add a Postfix exporter.
- [ ] M-23: normalize `users`/`aliases` domain references; deprecate `quota_overrides` or wire it.

---

## 5. Positive observations

- The fix commit (`d8edf83`, merged as PR #1) is well-scoped and its message accurately maps most changes to audit IDs; `CHANGELOG-AUDIT.md` honestly lists residuals (TLS, DKIM sync, TOTP encryption, backup, HA, UI build).
- Authentication is now structural (`Principal` + `authenticate` + `force_password_change` + `token_version`) instead of sprinkled `require_admin` calls, making future routes safer by default.
- Self-documenting decisions (e.g., `dav_discovery` public-static, `banaction = dummy`, SMTP “trusted networks only”) are called out in code comments rather than hidden.
- `cargo clippy --workspace --all-targets -- -D warnings` is now clean and enforced in CI; tests grew 39 → 67.
- `AUDIT.md` (the original report) has not been rewritten, so the before/after can be diffed.

---

## Appendix — files re-examined

`crates/synth-common/src/{auth,config,error,hibp,policy,rate_limit,totp,lib}.rs`, `crates/synth-control/src/{routes.rs,db.rs,app.rs,main.rs}`, all four migrations, `crates/synth-webmail/src/{routes.rs,carddav.rs,fts_indexer.rs,imap.rs,jmap.rs,app.rs,main.rs}`, `crates/synth-smtp/src/main.rs`, `crates/synth-filter/src/lib.rs`, `crates/synth-dkim/src/lib.rs`, `crates/synth-migrate/src/main.rs`, `crates/synth-watchdog/src/main.rs`, `crates/synth-ui/{Cargo.toml,src/lib.rs,Trunk.toml}`, `compose.yaml`, `docker/compose.stalwart.yaml`, `docker/compose.ha.yaml`, `docker/postfix/{Dockerfile,main.cf,master.cf,pgsql/*.cf}`, `docker/dovecot/{Dockerfile,dovecot.conf,sql.conf}`, `docker/rspamd/local.d/*.conf`, `docker/fail2ban/{Dockerfile,entrypoint.sh,jail.local,filter.d/*.conf}`, `docker/opendkim/{Dockerfile,opendkim.conf}`, `docker/edge/{nginx.conf,443.conf.example}`, `docker/monitoring/{prometheus.yml,grafana-*/*}`, all synth Dockerfiles, `scripts/sign-release.sh`, `.github/workflows/ci.yml`, `.env.example`, `CHANGELOG-AUDIT.md`.

*End of re-audit.*
