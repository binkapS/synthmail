# SynthMail

Self-hosted mail server with a Rust control plane. Postfix, Dovecot and Rspamd
move and filter mail; Rust services provide the admin API, webmail backend, DKIM
key management, health monitoring and migration tooling. The admin console is a
Svelte 5 + Tailwind CSS v4 single-page app.

> **Status:** production-ready, pending final Docker-host validation. All security
> audits are closed (see `docs/history/`). Complete TLS provisioning and run
> `docs/runbooks/security.md` before exposing the stack publicly.

## Table of contents

1. [Features](#features)
2. [Architecture](#architecture)
3. [Prerequisites](#prerequisites)
4. [Quick start](#quick-start)
5. [Configuration](#configuration)
6. [Services and ports](#services-and-ports)
7. [TLS and ACME](#tls-and-acme)
8. [Admin UI](#admin-ui)
9. [API overview](#api-overview)
10. [Security notes](#security-notes)
11. [Backup and monitoring](#backup-and-monitoring)
12. [Development](#development)
13. [Testing](#testing)
14. [Docker images](#docker-images)
15. [Documentation map](#documentation-map)

## Features

- Authenticated admin REST API (JWT, Argon2id, TOTP 2FA, token-version revocation)
- Domain, user, alias, quota and queue management
- DKIM RSA-2048 / Ed25519 key generation with encrypted-at-rest storage
- Webmail backend: folders, messages, search, send, drafts, JMAP session
- Sieve filters and vacation responders
- DMARC aggregate report ingestion
- CardDAV / CalDAV via Radicale
- Health monitoring and alerting (watchdog + Prometheus metrics)
- Backup profile: `pg_dump` + Maildir tar + checksums + retention + rclone
- Responsive admin & webmail UI with PWA shell

## Architecture

```
crates/
  synth-common/     # config, auth, TOTP, HIBP, rate limiting, AES-GCM secrets, errors
  synth-control/    # admin REST API (Axum 0.8, :8000), Postgres migrations
  synth-webmail/     # webmail backend, IMAP/JMAP proxy, DAV discovery (:3000)
  synth-watchdog/    # health poller, ACME glue, alerting (:8081)
  synth-filter/      # Sieve generation and Rspamd policy helpers
  synth-dkim/        # DKIM key generation
  synth-smtp/        # ESMTP submission service: STARTTLS + AUTH + Postfix relay (:2525)
  synth-migrate/     # mailcow / PostfixAdmin / CSV import CLI
  synth-bench/       # latency / throughput harness
ui/                 # Svelte 5 + Tailwind v4 SPA (served by synth-ui/nginx)
docker/             # service Dockerfiles and configuration templates
```

Edge (nginx) fronts HTTP/HTTPS and routes `/api/*` to `synth-control`,
`/api/v1/mail/*` and `/jmap` to `synth-webmail`, `/admin/` to `synth-ui`,
and `/dav/` to Radicale.

Mail flow: Postfix receives and delivers to per-user Maildir; Dovecot serves
IMAP; Rspamd filters; `synth-webmail` reads the shared Maildir and sends via
Postfix on `mailnet`.

## Prerequisites

- Docker + Docker Compose
- (Optional, for host development) Rust stable toolchain and Node 22
- A public hostname for `MAIL_HOST` if issuing real TLS certificates

## Quick start

```bash
cp .env.example .env
# Edit .env: set MAIL_HOST, MAIL_DOMAIN, POSTGRES_PASSWORD, REDIS_PASSWORD,
# SESSION_SECRET, ENCRYPTION_KEY, INITIAL_ADMIN_* etc.

docker compose config
docker compose --profile acme run --rm acme   # staging certificate
docker compose up -d
./scripts/smoke.sh
```

Then:

- Edge: `http://localhost:${HTTP_PORT:-8080}` (HTTPS on `${HTTPS_PORT:-8443}`)
- Admin UI: `http://localhost:${HTTP_PORT:-8080}/admin/`
- Health: `synth-control` `:8000/healthz`, `synth-webmail` `:3000/healthz`

## Configuration

Key environment variables (full list in `.env.example`):

| Variable | Purpose |
|---|---|
| `MAIL_HOST`, `MAIL_DOMAIN` | Public mail hostname (MX, TLS, autoconfig) |
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string (must carry `REDIS_PASSWORD`) |
| `SESSION_SECRET` | JWT signing secret (≥32 bytes, must be unique) |
| `ENCRYPTION_KEY` | AES-256 key (64 hex chars) for DKIM/TOTP at rest |
| `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD` | First admin seed (policy-checked) |
| `LETSENCRYPT_EMAIL` | ACME account email |
| `MAX_QUOTA_MB` / `SPAM_THRESHOLD` | Mailbox quota and spam threshold |
| `RATE_LIMIT_PER_MIN` / `HIBP_ENABLED` / `MIN_PASSWORD_LEN` | Auth hardening |
| `ALERT_WEBHOOK_URL` / `NTFY_TOPIC` | Watchdog alert destinations |
| `BACKUP_RETENTION_DAYS` / `RCLONE_REMOTE` | Backup tuning |

`REDIS_URL` and the Redis server must agree on authentication.

## Services and ports

| Service | Port | Notes |
|---|---|---|
| edge (nginx) | `HTTP_PORT` (8080) / `HTTPS_PORT` (8443) | public entrypoint |
| synth-control | 8000 | admin REST API (`/api/v1`) |
| synth-webmail | 3000 | webmail + JMAP (`/api/v1/mail`, `/jmap`) |
| synth-ui (nginx) | 127.0.0.1:3002 | admin SPA, proxied by edge `/admin/` |
| synth-watchdog | 8081 (internal) | health polling, ACME challenge glue |
| postgres | 5432 (loopback) | |
| redis | 6379 (loopback) | |

## TLS and ACME

```bash
# staging cert
docker compose --profile acme run --rm acme
docker compose up -d
```

Dovecot (`ssl = required`), Postfix (SASL only over TLS) and edge fail closed
without certificates. For production, remove `--staging` from the ACME
entrypoint and re-run. There is no DNS-01 automation; HTTP-01 only.

## Admin UI

The admin console (`ui/`) is Svelte 5 + Tailwind CSS v4 using the **CyberSlate
Premium Dark** design system. Auth-gated routes:

- `/` — dashboard
- `/login` — sign-in (guest-only)
- `/admin` — admin console (requires admin/superadmin, enforced by load
  function + server-side API)
- `/mail` — webmail 3-pane client

Commands:

```bash
cd ui
npm ci
npm run check       # svelte-check + typecheck
npm test            # Vitest (pure helpers)
npm run build       # production bundle
```

## API overview

Control plane is under `/api/v1` (synth-control); webmail under `/api/v1/mail`;
JMAP at `/jmap/session` and `/jmap`. All requests accept
`Authorization: Bearer <token>`. Responses are JSON; errors follow
`problem+json` with a `detail` field, surfaced by the UI.

Primary resources: auth, domains, users, aliases, quota, queues, DKIM, filters,
vacation, audit, bans, config, DMARC, push, TOTP, API tokens, mail (folders,
messages, search, send, drafts).

## Security notes

- TLS fail-closed: Dovecot requires TLS, Postfix SASL only over TLS.
- Secrets at rest: DKIM private keys and TOTP secrets encrypted with
  `ENCRYPTION_KEY` (AES-256-GCM).
- `token_version` revocation on logout / password reset / 2FA / deactivation.
- Password policy enforced (length, denylist, HIBP k-anonymity).
- 2FA uses pending-secret enrollment with replay protection.
- Redis-backed rate limiting with bounded local fallback.
- ACME challenge tokens validated before filesystem access.

See `docs/runbooks/security.md` for the full hardening checklist.

## Backup and monitoring

- Backup profile: `docker compose --profile backup run --rm backup`
  (`pg_dump` + Maildir tar + `sha256sum` + retention prune + optional rclone).
- Health endpoints: `/healthz` on control and webmail.
- Watchdog polls services, dedupes alerts, exposes `/status`.
- Prometheus metrics at `/metrics` (includes `queue_depth` gauge).
- Grafana dashboard container included (see `docker/monitoring`).

## Development

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings

# UI dev server (proxying to local backends)
cd ui && npm run dev
```

Apply migrations explicitly (also run on `synth-control` boot):

```bash
cargo sqlx migrate run --source crates/synth-control/migrations
# or
cargo run -p synth-control -- --migrate
```

## Testing

- Rust: `cargo test --workspace` (unit + integration, incl. SQLx migrate smoke)
- UI: `cd ui && npm test` (Vitest pure helpers), `npm run check`
- Compose: `docker compose config`
- Mail configs: `postfix check`, `rspamadm configtest`, `doveconf -n`

CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests, migrations, image
builds (control, webmail, UI) and config smoke.

## Docker images

Images are built from `docker/<service>/Dockerfile`. The UI image installs
dependencies in a Node 22 stage and serves the static bundle via nginx 1.27 with
SPA fallback and reverse proxy rules for `/api/` and `/jmap`.

Build and publish all images (no Rust/Node toolchain required for the
consuming host) with:

```bash
# local single-arch build
./scripts/build-images.sh

# multi-arch publish
REGISTRY=ghcr.io/yourorg TAG=1.0.0 ./scripts/build-images.sh --push
```

System administrators can then deploy with a pinned tag and pull only:

```bash
REGISTRY=ghcr.io/yourorg TAG=1.0.0 docker compose pull
docker compose up -d
```

```bash
docker compose build   # per-service local builds
docker compose config # validate compose
```

## Documentation map

- `docs/runbooks/` — security, monitoring, backup, DNS, migration, upgrades, HA
- `docs/adr/` — architecture decision records
- `docs/phase3.md` — pure-Rust SMTP shim / Stalwart evaluation
- `docs/releases/v1.0.0.md` — release notes
- `docs/history/` — audit records (AUDIT, AUDIT-RECHECK)
- `docs/svelte-rebuild-plan.md` — front-end rebuild plan
- `CHANGELOG.md` — curated release record

## License

MIT © 2026 Binkap S. See `LICENSE`.
