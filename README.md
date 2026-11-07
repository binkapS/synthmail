# SynthMail — Mail Server

Self-hosted mail server with Rust control plane. See `Plan.md` for spec.

## Quick start

```bash
cp .env.example .env   # edit MAIL_HOST, passwords, LE email
docker compose up -d
# Open https://mail.example.com/admin → create first superadmin
# Add domain → DKIM → copy DNS records → verify
```

## Workspace

```
crates/
  synth-common/    # Config, auth (Argon2id/JWT), telemetry, errors
  synth-control/   # Admin REST API (Axum 0.8, :8000) + Postgres migrations
  synth-webmail/   # Webmail backend + IMAP proxy (:3000)
  synth-watchdog/  # Health poller + ACME glue (:8081)
  synth-filter/    # Sieve generator + Rspamd sync
  synth-dkim/      # DKIM RSA-2048 / Ed25519 generation
  synth-migrate/   # mailcow import CLI (Phase 2)
  synth-ui/        # Leptos SPA (Trunk)
```

## Verify

```bash
cargo check --workspace
cargo test --workspace
docker compose config
docker compose --profile monitoring up -d prometheus grafana  # Phase 2 dashboards
docker compose --profile dav up -d radicale                    # CardDAV/CalDAV
```

## Phase 2 (v0.5)

- **2FA**: `POST /api/v1/auth/totp/enroll` → scan otpauth URL; `POST /api/v1/auth/totp/verify`; login requires `totp_code` when enabled (`crates/synth-common/src/totp.rs` RFC 6238).
- **API tokens**: `POST /api/v1/api-tokens` (scoped, expiring), list/revoke; use `Authorization: Bearer synth_...`.
- **JMAP**: `GET /jmap/session`, `POST /jmap` (RFC 8620/8621) over IMAP.
- **FTS**: `GET /api/v1/mail/search?q=` backed by Postgres `tsvector` + `fts_indexer`.
- **CardDAV/CalDAV**: `/api/v1/carddav/*`, `/api/v1/caldav/*`, WebDAV discovery at `/dav` + Radicale (`--profile dav`).
- **Monitoring**: `/metrics` Prometheus (`synth_common::metrics`), Prometheus+Grafana (`--profile monitoring`), watchdog alerts (`ALERT_WEBHOOK_URL`/`NTFY_TOPIC`, >2m down → alert, `/status`).
- **Migration**: `synth-migrate dry-run|import|verify|count-maildir` (CSV + mailcow + Maildir) — see `docs/runbooks/migration.md`.

## Notes

- Phase 0+1: full API, Postgres identity, Postfix/Dovecot/rspamd/opendkim/opendmarc/clamav/fail2ban/edge/backup configs.
- IMAP fetch (`synth-webmail/src/imap.rs`) is stubbed for unit tests; wire to Dovecot :993 for live inbox.
- Run `cargo sqlx migrate run` / binary `--migrate` to apply `crates/synth-control/migrations/` (now `..._init.sql` + `..._phase2.sql`).
- Runbooks: `docs/runbooks/migration.md`, `security.md`, `monitoring.md`, `backup.md`, `dns.md`.
