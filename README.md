# SynthMail

Self-hosted mail server with a Rust control plane. Postfix, Dovecot, and Rspamd move and filter mail; Rust services provide the admin API, webmail backend, DKIM key management, health monitoring, and migration tooling.

> **Status: pre-production / audit baseline (`v0.1.0-audit.1`).**
> This repository is **not production-ready**. A full security and correctness audit was completed on 2026-11-07 and is checked in as [`AUDIT.md`](AUDIT.md). It identifies 7 critical and 14 high-severity findings, including unauthenticated admin read endpoints, a 2FA takeover path, a webmail login that bypasses TOTP, default credentials, missing TLS, and an unauthenticated SMTP shim.
>
> **Do not expose this stack to the public internet** until the P0 items in the audit are resolved.

---

## Current status

| Area | Status | Notes |
|---|---|---|
| Workspace build / unit tests | ✅ Compiles | 39 tests pass; mostly unit/smoke coverage |
| PostgreSQL schema + migrations | ✅ Present | Idempotent `sqlx` migrations, audit log, FTS tables |
| Admin REST API (`synth-control`) | ⚠️ Partial | CRUD, DKIM, TOTP, API tokens; **many `GET` routes are unauthenticated** |
| Auth (Argon2id + JWT) | ⚠️ Partial | Hashing and JWT work; no revocation, rate limiting, or HIBP enforcement |
| 2FA (TOTP) | ⚠️ Unsafe | Enrollment/verification has a takeover path (see audit C-02) |
| Postfix / Dovecot integration | ⚠️ Partial | Configs shipped; Postfix `pgsql` maps and package are missing |
| Webmail backend (`synth-webmail`) | 🚧 Stub | IMAP/JMAP/send/search are stubbed; no authentication |
| Admin UI (`synth-ui`, Leptos) | 🚧 Stub | Demo markup only; not built or served |
| SMTP shim (`synth-smtp`) | 🧪 Experimental | Unauthenticated, no STARTTLS; not in default compose |
| DKIM (`synth-dkim`) | ⚠️ Partial | Key generation works; keys are not synced to OpenDKIM |
| Monitoring / watchdog | ⚠️ Partial | `/metrics`, health polling, alert webhooks; ACME and backup glue incomplete |
| Backup / HA / monitoring stack | ❌ Not working | Backup image, HA overlay, and Grafana provisioning are incomplete |

See [`AUDIT.md`](AUDIT.md) for file/line evidence, impact, and remediation steps.

---

## Quick start (local development only)

```bash
cp .env.example .env

# Generate strong values — do not keep the placeholders.
openssl rand -hex 32   # SESSION_SECRET
openssl rand -hex 24   # POSTGRES_PASSWORD / REDIS_PASSWORD
openssl rand -hex 32   # INITIAL_ADMIN_PASSWORD

# Edit .env, then validate the compose file.
docker compose config

# Start the core stack.
docker compose up -d
```

Then:

- Admin API: `http://localhost:8000` (health: `/healthz`, metrics: `/metrics`)
- Webmail backend: `http://localhost:3000` (health: `/healthz`)
- Watchdog: `http://localhost:8081` (health: `/healthz`, status: `/status`)
- Edge: `http://localhost` (HTTPS only when certificates are provisioned)

> **Security warning:** the default compose file publishes `8000` and `3000` directly, does not enable TLS in Dovecot/Postfix, and does not forward `INITIAL_ADMIN_*` into the control-plane container. Fix these before any shared or public deployment (audit C-05, C-06, H-05).

### First-run admin

On an empty database, `synth-control` seeds an admin from `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD`. If those variables are not present in the container environment, it falls back to `admin@example.com` / `ChangeMe123!Admin`. Treat that as a placeholder only; change it immediately or set the env vars explicitly.

---

## Workspace

```
crates/
  synth-common/    # Config, Argon2id/JWT auth, TOTP, HIBP, rate limiting,
                   # DNS checks, metrics, telemetry, problem+json errors
  synth-control/   # Admin REST API (Axum 0.8, :8000), Postgres migrations
  synth-webmail/   # Webmail backend, IMAP/JMAP proxy, CardDAV/CalDAV (:3000)
  synth-watchdog/  # Health poller, ACME challenge glue, alerting (:8081)
  synth-filter/    # Sieve generation and Rspamd policy helpers
  synth-dkim/      # DKIM RSA-2048 / Ed25519 key generation
  synth-smtp/      # Experimental ESMTP shim with Redis queue (:2525)
  synth-migrate/   # mailcow / PostfixAdmin / CSV import CLI
  synth-ui/        # Leptos CSR SPA (Trunk)
  synth-bench/     # Latency / throughput benchmark harness
```

---

## Verification

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --check
cargo clippy -p synth-common -p synth-filter -p synth-dkim -- -D warnings
docker compose config
```

Apply migrations explicitly (they also run on `synth-control` boot):

```bash
cargo sqlx migrate run --source crates/synth-control/migrations
# or
cargo run -p synth-control -- --migrate
```

The audit reports that the CI migration check is currently a no-op and that clippy failures for most crates are masked; see audit M-18 before trusting a green build.

---

## Configuration

Key environment variables (see `.env.example` for the full list):

| Variable | Purpose |
|---|---|
| `MAIL_HOST` | Public mail hostname (MX, TLS, autoconfig) |
| `MAIL_DOMAIN` | Default mail domain |
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string |
| `SESSION_SECRET` | JWT signing secret (≥32 bytes, must be unique) |
| `INITIAL_ADMIN_EMAIL` / `INITIAL_ADMIN_PASSWORD` | First admin seed (must be passed into the control container) |
| `LETSENCRYPT_EMAIL` | ACME account email (ACME client not yet implemented) |
| `MAX_QUOTA_MB` / `SPAM_THRESHOLD` | Mailbox quota and spam threshold |
| `ALERT_WEBHOOK_URL` / `NTFY_TOPIC` | Watchdog alert destinations |
| `RATE_LIMIT_PER_MIN` / `HIBP_ENABLED` | Documented but **not currently enforced** |

> `REDIS_URL` and the Redis server must agree on authentication. The example `.env` includes a Redis password, but the compose service does not enable `requirepass`; see audit H-08.

---

## Documentation

- [`AUDIT.md`](AUDIT.md) — full audit findings and prioritized remediation plan
- [`Plan.md`](Plan.md) — original design specification and phased roadmap
- [`docs/phase3.md`](docs/phase3.md) — pure-Rust SMTP shim / Stalwart evaluation
- [`docs/runbooks/`](docs/runbooks/) — security, monitoring, backup, DNS, migration, upgrades
- [`docs/adr/`](docs/adr/) — architecture decision records
- [`docs/releases/v1.0.0.md`](docs/releases/v1.0.0.md) — historical release notes
  (the audit found that the “GA” claims do not match the implementation)

---

## Roadmap

The immediate priority is the P0 list from the audit:

1. Authenticate and authorize every admin read endpoint.
2. Fix TOTP enrollment/verification and enforce TOTP in webmail login.
3. Add `user_id` scoping to all webmail, JMAP, CardDAV/CalDAV, search, and push queries.
4. Remove default secrets and default admin seeding; fail startup on placeholders.
5. Implement TLS end-to-end (ACME client + fail-closed listeners).
6. Authenticate/restrict the SMTP shim or remove it from the deployable surface.
7. Enforce API token expiry and scopes.

Then close the P1 items (rate limiting, password policy/HIBP, session revocation, Postfix `pgsql` maps, DKIM→OpenDKIM sync, Redis auth, working backup, real webmail/UI, Rspamd config validation, CI hardening, and accurate docs).

---

## Versioning

This audit baseline is tagged **`v0.1.0-audit.1`**. The workspace manifests declare `0.1.0`. The repository does **not** yet meet the criteria for a `v1.0.0` or production release.

---

## License

MIT © 2026 Binkap S. See [`LICENSE`](LICENSE).
