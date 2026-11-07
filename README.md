# SynthMail

Self-hosted mail server with a Rust control plane. Postfix, Dovecot, and Rspamd move and filter mail; Rust services provide the admin API, webmail backend, DKIM key management, health monitoring, and migration tooling.

> **Status: hardened pre-production (`v0.1.0`).**
> A full security and correctness audit was completed on 2026-11-07 and is checked in as [`AUDIT.md`](AUDIT.md), with a re-audit of the hardening merge as [`AUDIT-RECHECK.md`](AUDIT-RECHECK.md). The P0/P1 findings from the re-audit (TLS fail-closed, DKIM key encryption + OpenDKIM sync, TOTP secret encryption, pending-secret 2FA enrollment, distributed rate limiting, forced password changes, backup tooling, admin UI build/serve, ACME provisioning) are now implemented; see the status table below.
>
> **Do not expose this stack to the public internet** until you have provisioned TLS certificates (see [TLS provisioning](#tls-provisioning)), generated `SESSION_SECRET` and `ENCRYPTION_KEY`, and verified `docs/runbooks/security.md`.

---

## Current status

| Area | Status | Notes |
|---|---|---|
| Workspace build / unit tests | ✅ Compiles, `clippy -D warnings` clean | 72 tests (66 Rust + doctests) |
| PostgreSQL schema + migrations | ✅ Present | `sqlx` migrations incl. `20261107000004_recheck.sql` |
| Admin REST API (`synth-control`) | ✅ Authenticated | Every route gated via `Principal`; admin read/write scopes; transactional writes |
| Auth (Argon2id + JWT) | ✅ | `token_version` revocation on logout/password-reset/2FA/deactivation; `iss`/`aud` validation |
| 2FA (TOTP) | ✅ | Pending-secret enrollment (10-min expiry); replay protection via numeric step; secrets **encrypted at rest** (AES-256-GCM) |
| Password policy / HIBP | ✅ | `synth-common::policy` (length, denylist, whitespace, HIBP k-anonymity) wired to create/reset/change; the seed admin password is policy-checked |
| Postfix / Dovecot integration | ✅ Partial | `postfix-pgsql` package + map files; Dovecot `ssl = yes` **fail-closed**; SASL only over TLS |
| Encrypted secrets at rest | ✅ | `ENCRYPTION_KEY` (64 hex) → AES-256-GCM via `synth-common::secrets`; DKIM private keys + TOTP secrets encrypted |
| DKIM (`synth-dkim`) | ✅ | RSA-2048/Ed25519 (real PKCS#8); keys encrypted in DB **and** synced to the shared OpenDKIM volume (`KeyTable`/`SigningTable`); `MinimumKeyBits 2048` |
| Webmail backend (`synth-webmail`) | ✅ Authenticated mail access | `authenticate_user` enforced everywhere; FTS indexer runs every 60 s; folders/list/get/drafts/delete read the shared Maildir; send via Postfix on mailnet; JMAP Email/query+get |
| Admin UI (`synth-ui`, Leptos) | ✅ Built + served | `docker/synth-ui/Dockerfile` (Trunk WASM build) → `synth-ui` service behind edge `/admin/`; login button posts to the real API |
| SMTP shim (`synth-smtp`) | 🧪 Experimental | Loopback default, max 100 concurrent connections, size/recipient/line limits; still unauthenticated (no AUTH/STARTTLS) — trusted networks only |
| Monitoring / watchdog | ✅ Partial | `/metrics`, health polling, deduped alerts; `queue_depth` gauge; ACME challenge endpoint validates tokens before FS access |
| Backup (`backup` profile) | ✅ | `postgres15-client + tar + rclone` image; `pg_dump` + Maildir tar + `sha256sum` + retention prune + optional `rclone sync` |
| TLS / ACME | ⚠️ Provisioning present, manual step required | `acme` profile (`certbot/certbot:v4.0.0`, webroot via `synth-watchdog`) + fail-closed listeners; no DNS-01 automation |
| HA overlay | ❌ Removed | `docker/compose.ha.yaml` deleted (it did not form a working replica); see audit M-17 |
| Stalwart evaluation | ⚠️ Standalone only | Profile binds to loopback; publish only when it is the sole mail stack |

See [`AUDIT.md`](AUDIT.md) and [`AUDIT-RECHECK.md`](AUDIT-RECHECK.md) for file/line evidence, impact, and remediation steps.

---

## Quick start

```bash
cp .env.example .env

# Generate strong values — do not keep the placeholders.
openssl rand -hex 32   # SESSION_SECRET
openssl rand -hex 32   # ENCRYPTION_KEY (must print 64 hex chars)
openssl rand -hex 24   # POSTGRES_PASSWORD / REDIS_PASSWORD

# Edit .env (MAIL_HOST, POSTGRES_PASSWORD, REDIS_URL password, etc.),
# then validate the compose file.
docker compose config

# 1. Provision TLS (staging cert; remove --staging in docker/compose.acme
#    entrypoint for production). Dovecot/Postfix/edge fail closed without it.
docker compose --profile acme run --rm acme

# 2. Start the core stack.
docker compose up -d
```

Then:

- Admin API: `http://localhost:8000` (health: `/healthz`, metrics: `/metrics`)
- Webmail backend: `http://localhost:3000` (health: `/healthz`)
- Watchdog: `http://localhost:8081` (health: `/healthz`, status: `/status`)
- Edge: `http://localhost` (HTTPS only when certificates are provisioned)
- Admin UI: `http://localhost/admin/` (served by `synth-ui` on `127.0.0.1:3002` behind edge)

Database ports are bound to loopback; mail ports (Postfix 25/465/587) are reachable per their `ports:` binding — keep Dovecot mail ports loopback-bound until TLS is provisioned, as plaintext 143/110 carry credentials.

### First-run admin

On an empty database, `synth-control` seeds an admin from `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD` (forwarded into the control container):

- If `INITIAL_ADMIN_PASSWORD` is set, it must pass the password policy (≥12 chars, not a known default).
- If it is empty/unset, a random 36-hex-char password is generated and written to `/run/secrets/initial-admin-password` (`0600`); it is **not** logged. Read it, log in, rotate, and delete the file.

Existing admins are never reset on subsequent boots (`ON CONFLICT ... DO NOTHING`).

---

## TLS provisioning

1. Set `MAIL_HOST` and `LETSENCRYPT_EMAIL` in `.env` (they must be public).
2. Run `docker compose --profile acme run --rm acme` — the staging cert is issued for `MAIL_HOST` and stored in the shared `certs` volume under `live/<MAIL_HOST>/`.
3. `docker compose up -d` — Dovecot (`ssl = yes` + `ssl = required`), Postfix (SASL only over TLS), and edge (`443.conf.example` when certs exist) pick them up.
4. For production, remove `--staging` from the `acme` service entrypoint in `compose.yaml` and re-run.

> The ACME profile writes `fullchain.pem` + `privkey.pem`; `synth-watchdog` serves HTTP-01 challenges from the webroot directory. There is no DNS-01 automation — use `webroot` for HTTP-01 or provision certs manually.

---

## Workspace

```
crates/
  synth-common/    # Config, Argon2id/JWT auth, TOTP, HIBP, rate limiting
                   # (Redis-shared + bounded local fallback), AES-GCM secrets,
                   # DNS checks, metrics, telemetry, problem+json errors
  synth-control/   # Admin REST API (Axum 0.8, :8000), Postgres migrations
  synth-webmail/   # Webmail backend, IMAP/JMAP proxy, CardDAV/CalDAV (:3000)
  synth-watchdog/  # Health poller, ACME challenge glue, alerting (:8081)
  synth-filter/    # Sieve generation and Rspamd policy helpers
  synth-dkim/      # DKIM RSA-2048 / Ed25519 key generation
  synth-smtp/      # Experimental ESMTP shim with Redis queue (loopback :2525)
  synth-migrate/   # mailcow / PostfixAdmin / CSV import CLI
  synth-ui/        # Leptos CSR SPA (Trunk WASM, served by synth-ui/nginx)
  synth-bench/     # Latency / throughput benchmark harness
```

---

## Verification

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
docker compose config
# With the ACME and Stalwart overlays:
docker compose -f compose.yaml -f docker/compose.stalwart.yaml --profile stalwart config
```

Apply migrations explicitly (they also run on `synth-control` boot):

```bash
cargo sqlx migrate run --source crates/synth-control/migrations
# or
cargo run -p synth-control -- --migrate
```

CI (`.github/workflows/ci.yml`) runs `fmt`, `clippy -D warnings`, `cargo test`, real `sqlx migrate run` against service Postgres, image builds (incl. the Trunk UI), and config smoke tests (`postfix check`, `rspamadm configtest`, `doveconf -n`, WASM `cargo check`).

---

## Configuration

Key environment variables (see `.env.example` for the full list):

| Variable | Purpose |
|---|---|
| `MAIL_HOST` / `MAIL_DOMAIN` | Public mail hostname (MX, TLS, autoconfig) |
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string (must include `REDIS_PASSWORD`) |
| `SESSION_SECRET` | JWT signing secret (≥32 bytes, must be unique, no defaults) |
| `ENCRYPTION_KEY` | AES-256 key (64 hex chars) for DKIM/TOTP secrets at rest |
| `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD` | First admin seed (policy-checked; empty ⇒ generated 0600 file) |
| `LETSENCRYPT_EMAIL` | ACME account email |
| `MAX_QUOTA_MB` / `SPAM_THRESHOLD` | Mailbox quota and spam threshold |
| `ALERT_WEBHOOK_URL` / `NTFY_TOPIC` | Watchdog alert destinations |
| `RATE_LIMIT_PER_MIN` | Per-key request budget (shared via Redis) |
| `HIBP_ENABLED` | Breached-password check on create/reset/change |
| `MIN_PASSWORD_LEN` | Password minimum length (default 12) |
| `RCLONE_REMOTE` / `BACKUP_RETENTION_DAYS` | Backup off-site target / retention |

> `REDIS_URL` and the Redis server must agree on authentication. The example `.env` includes a Redis password, and the `redis` service enforces `--requirepass`; they must match.

---

## Documentation

- [`AUDIT.md`](AUDIT.md) — first audit (2026-11-07, 54 findings)
- [`AUDIT-RECHECK.md`](AUDIT-RECHECK.md) — re-audit of the hardening merge + 13 new findings addressed in this pass
- [`CHANGELOG-AUDIT.md`](CHANGELOG-AUDIT.md) — what each hardening round changed
- [`Plan.md`](Plan.md) — original design specification and phased roadmap
- [`docs/phase3.md`](docs/phase3.md) — pure-Rust SMTP shim / Stalwart evaluation
- [`docs/runbooks/`](docs/runbooks/) — security, monitoring, backup, DNS, migration, upgrades
- [`docs/adr/`](docs/adr/) — architecture decision records

---

## Roadmap

Closed with this hardening pass (see `AUDIT-RECHECK.md` §4 for the full checklist):

1. ✅ TLS fail-closed + ACME provisioning profile; SASL only over TLS; plaintext Dovecot ports loopback-bound
2. ✅ DKIM private-key encryption + OpenDKIM volume sync; `MinimumKeyBits 2048`
3. ✅ TOTP secret encryption; pending-secret enrollment flow
4. ✅ Seed admin password policy; generated passwords written to 0600 files, never logged
5. ✅ Redis-shared rate limiting + in-memory eviction; per-endpoint enforcement
6. ✅ FTS indexer scheduled; library `search` tenant-scoped
7. ✅ Postfix `mynetworks` narrowed; milters fail-closed (`reject`)
8. ✅ SMTP shim loopback default + connection caps; Stalwart documented as standalone
9. ✅ Admin UI built with Trunk and served behind edge `/admin/`
10. ✅ Backup image with `pg_dump` + `tar` + `sha256sum` + retention + `rclone`
11. ✅ HA overlay removed; images pinned; `docker compose config` validated in CI smoke with real secrets
12. ✅ DAV discovery authenticated; `force_password_change` enforced in the webmail API
13. ✅ DMARC ingestion rejects DOCTYPE/ENTITY (XXE-shaped payloads); release script writes a real digest lockfile
14. ✅ Prometheus duplicate job removed; `queue_depth` gauge wired

Still open (documented, not blocking the above):

- JMAP `Email/set` (create/update/destroy) is modeled but not implemented; Email/query+get are real.
- `synth-smtp` has no AUTH/STARTTLS — trusted networks only (sync `docs/runbooks/security.md`).
- ClamAV image has no numeric tag upstream; digest pinning is pending.
- HTTPS edge activation still depends on the manual `--staging` removal for production certificates.

---

## Versioning

The workspace manifests declare `0.1.0`. The `v0.1.0-audit.1` tag marks the audit baseline; this hardening pass is the `0.1.0` development baseline and does **not** yet meet the criteria for a `v1.0.0` or production release name.

---

## License

MIT © 2026 Binkap S. See [`LICENSE`](LICENSE).
