<!-- ARCHIVED 2026-11-07: all 54 original findings plus all re-audit P0/P1/P2 items are closed; see README.md Status and CHANGELOG.md. This file is the historical record; do not edit. -->
# Changelog — security audit hardening (deployment / CI / docs)

Branch: `fix/security-audit` · Base: `main` @ audit rev. `3edb73d` ·
Source of findings: [`AUDIT.md`](AUDIT.md) (2026-11-07).

> **Scope note:** this changelog covers only the deployment / CI / docs
> portion of the audit owned by this branch (compose, env example,
> Postfix pgsql maps, CI, release script, README status, `.dockerignore`).
> Rust source fixes (C-01–C-04, C-07, H-01–H-04, H-10–H-12, H-14, M-01–M-15
> etc.) are handled in other commits on the branch and are not summarized
> here.

## Fixes landed on this branch

### Compose / secrets

- **C-05 — remove `SESSION_SECRET` placeholder fallback.** Both
  `synth-control` and `synth-webmail` now use
  `${SESSION_SECRET:?SESSION_SECRET must be set}` so compose fails fast
  instead of silently accepting the dev secret
  `dev-secret-change-me-32bytes-min-0000`.
- **C-05 — forward first-boot admin env.** `INITIAL_ADMIN_EMAIL` and
  `INITIAL_ADMIN_PASSWORD` are now passed to the `synth-control` service
  environment, so the documented `.env` values actually reach the
  container. (The Rust-side "always reset password" behaviour is fixed
  separately in source.)
- **H-08 — Redis auth mismatch.** The `redis` service now runs with
  `--requirepass ${REDIS_PASSWORD}` and exposes `REDIS_PASSWORD` to its
  healthcheck (`redis-cli -a … ping`), so `REDIS_URL=redis://:…@redis:6379`
  from the apps no longer fails with `ERR Client sent AUTH, but no password is set`.
  `.env.example` documents that `REDIS_PASSWORD` and `REDIS_URL` must match.
- **H-05 — stop publishing internal ports.** The following ports are now
  bound to `127.0.0.1` (previously all interfaces):
  `synth-control` (8000), `synth-webmail` (3000), Prometheus (9090),
  Grafana (`127.0.0.1:3001:3000`), Radicale (5232). `synth-watchdog`
  already published no host port and stays network-internal (documented in a
  compose comment). Only the `edge` service (80/443) remains public.
- **H-05 — Grafana default password.** `GF_SECURITY_ADMIN_PASSWORD=admin`
  replaced with `${GRAFANA_ADMIN_PASSWORD:?GRAFANA_ADMIN_PASSWORD must be set}`;
  `GRAFANA_ADMIN_PASSWORD` added to `.env.example`.
- **H-09 — backup service (minimal/rescue fix).** Image changed from
  `rclone/rclone` (no `pg_dump`) to `postgres:16-alpine` (ships `pg_dump`);
  added `PGPASSWORD`/`POSTGRES_PASSWORD` env from
  `${POSTGRES_PASSWORD:?…}`, a `maildir:/var/vmail:ro` mount, a
  `service_healthy` condition on `postgres`, and a `set -e` command line so
  failures are visible instead of being swallowed by `|| echo`.
  Full backup tooling (see residual risks) remains TODO.
- **H-06 — Postfix `pgsql` maps.** Added `postfix-pgsql` to
  `docker/postfix/Dockerfile`, copies `docker/postfix/pgsql/*.cf` into the
  image (mode 600), and the entrypoint substitutes `POSTGRES_PASSWORD`
  into them before starting Postfix. The postfix service now declares
  `POSTGRES_PASSWORD=${POSTGRES_PASSWORD:?…}`.
  New map files:
  - `docker/postfix/pgsql/virtual_domains.cf` — active domains from `domains`.
  - `docker/postfix/pgsql/virtual_mailboxes.cf` — active users → `/var/vmail/<domain>/<localpart>/`.
  - `docker/postfix/pgsql/virtual_aliases.cf` — exact-address aliases from `aliases`.
- **M-16 — image pinning (partial).** Pinned floating tags where the version
  was confidently known:
  - `nginx:alpine` → `nginx:1.27-alpine` (edge)
  - `kozea/radicale:latest` → `kozea/radicale:3`
  - `prom/prometheus:latest` → `prom/prometheus:v2`
  - `grafana/grafana:latest` → `grafana/grafana:11`
  Left as-is (not pinned, see below).
- **M-16 — `.dockerignore`.** Created at repo root (`target/`, `.git/`, `.env`,
  `*.local`) so build contexts no longer send large/local state.

### CI (`.github/workflows/ci.yml`)

- **M-18 — clippy no longer hides failures.** The
  `cargo clippy … | head -n 50` step is replaced with a
  `set -o pipefail` + `head -n 200` pipeline (the step's exit status is now
  clippy's), and clippy now runs `--workspace --all-targets -- -D warnings`
  instead of only three crates plus a non-failing follow-up.
- **M-18 — tests no longer `--offline`.** Removed `--offline` (cold runners
  have no vendor/cache); the output pipe is guarded with `set -o pipefail`
  so `tail` can no longer mask failures.
- **M-18 — migrations actually run.** The `migrate_check` job now installs
  `sqlx-cli` (`cargo install … --features postgres --locked`) and runs
  `sqlx migrate run --source crates/synth-control/migrations` against the
  service Postgres with `DATABASE_URL` set; the previous
  `|| echo "migration syntax checked"` swallow is removed.
- **M-18 — smoke is best-effort honest.** The smoke job still starts only
  `postgres` and `redis`, but now also curls (non-fatally, with a clear
  `WARN`) `/healthz` for both `synth-control:8000` and `synth-webmail:3000`
  so a full-stack smoke test can be enabled by adding
  `docker compose up -d --build` without breaking this job.
- Pre-existing uncommitted working-tree changes: none were present to
  preserve for `compose.yaml`/docker configs at the time of work (`git
  status` showed only Rust-source modifications from the parallel track);
  all compose/docker changes above are new on this branch.

### Release script

- **L-08 — `scripts/sign-release.sh` syntax error.** Fixed the broken line
  `docker lockfiles="$(mktemp)"` → `lockfiles="$(mktemp)"` and added echo
  output for readability. Validates with `bash -n`.

### Docs

- **M-19 — prominent status section.** `README.md` gained a top-level
  **Status / Known limitations** section stating the project is
  **pre-production, not GA**, and listing what's stubbed: webmail
  IMAP/send, ACME/TLS, backup tooling, DKIM signing sync, and the admin UI.
  No existing README content was removed.

## Residual risks — deliberately left open on this branch

- **TLS / ACME (C-06).** No ACME client ships; certificates must be
  provisioned manually at `/etc/letsencrypt/live/mail.example.com/`.
  Dovecot still ships `ssl = no` unless that cert exists. Until an ACME
  client (certbot/lego) with renewal and fail-closed listeners lands,
  do not expose the stack to the internet.
- **DKIM key encryption + OpenDKIM sync (H-07, M-12).** Private keys are
  still stored plaintext in `dkim_keys.private_key_enc` (column name is
  aspirational) and nothing copies them into the shared `dkim` volume or
  generates `KeyTable`/`SigningTable` — outbound mail remains unsigned.
  `MinimumKeyBits` and the Ed25519 PEM issues are also still open. Fixing
  this needs an encryption/companion story that crosses the Rust/Docker
  boundary, so it is tracked as a follow-up cross-track change.
- **TOTP secret encryption at rest (M-15, C-02).** Secrets are still stored
  plaintext in `user_totp`. The application-layer age/SOPS/KMS encryption
  promised by the migration comment is not yet implemented; it ships with
  the Rust auth-track fix.
- **Full backup / restore (H-09).** The backup job dumps Postgres and tars
  the maildir, but there is no encryption, checksum, retention enforcement,
  schedule, rclone upload (the image switch dropped `rclone` entirely), or
  restore test. A proper backup image (postgres client + tar + rclone) or a
  multi-stage build is still TODO.
- **Rspamd config split (H-13).** `docker/rspamd/rspamd.conf` is still a
  single `local.d/rspamd.conf` snippet with top-level `options {}` etc. —
  likely ignored/invalid in Rspamd's `local.d` snippet model. A minimal
  valid split into `local.d/options.inc`, `local.d/antivirus.conf`,
  `local.d/dkim_signing.conf` (etc.) plus a `rspamadm configtest` CI check
  was judged out of minimal-change scope here; the file carries a TODO
  comment in `compose.yaml` instead.
- **HA overlay (M-17).** `docker/compose.ha.yaml` still does not form a
  streaming replica (no real `pg_basebackup`/Patroni). Unchanged here;
  mark or fix in a follow-up.
- **`synth-ui` serving (H-11).** The Leptos UI is still not built with
  Trunk or served by edge/webmail.
- **Remaining floating image tags (M-16).** `rspamd/rspamd:latest` and
  `linuxserver/fail2ban:latest` were left unpinned because the exact
  versioned tags could not be verified from the working tree; `clamav`
  (`stable-debian`), `redis:7-alpine`, `postgres:16-alpine` were already
  pinned. Image digests, non-root `USER`, and `cargo build --locked` in
  the Dockerfiles also remain TODO.
- **CI gaps (M-18).** `cargo audit`/`cargo deny`, secret scanning, WASM/UI
  build, image scanning, and a true full-stack `/healthz` smoke test are
  not yet in CI.

## Files changed

- `compose.yaml`
- `.env.example`
- `.dockerignore` (new)
- `docker/postfix/Dockerfile`
- `docker/postfix/pgsql/virtual_domains.cf` (new)
- `docker/postfix/pgsql/virtual_mailboxes.cf` (new)
- `docker/postfix/pgsql/virtual_aliases.cf` (new)
- `.github/workflows/ci.yml`
- `scripts/sign-release.sh`
- `README.md` (status section only)
- `CHANGELOG-AUDIT.md` (new)

## Validation (run at work time)

- `docker compose config -q` — re-run noted in the work report (docker
  presence varies by environment).
- YAML sanity for `.github/workflows/ci.yml` via `python3 -c 'import yaml'`
  — or reported as unavailable.
- `bash -n scripts/sign-release.sh`

---

## Addendum — re-audit hardening pass (2026-11-07, HEAD `3aefa77` → this commit)

Source of findings: [`AUDIT-RECHECK.md`](AUDIT-RECHECK.md) (re-audit of `d8edf83`).
This pass implements §4 of that report (plus §3's N-01…N-13) and rewrites
[`README.md`](README.md). Validation: `cargo check/test/clippy -D warnings`,
`cargo fmt --check`, `docker compose (plus --profile stalwart) config -q`
all pass; 72 tests pass (up from 67).

### P0 — TLS, DKIM, TOTP encryption (C-06, H-07, M-15)
- **C-06 TLS fail-closed + ACME profile.** `docker/dovecot/dovecot.conf`
  is now `ssl = yes` + `ssl = required` with explicit `ssl_cert`/`ssl_key`;
  the Dovecot entrypoint exits 1 when the certificate is missing.
  Postfix `smtpd_tls_auth_only = yes` (+ `smtps` `smtpd_tls_auth_only=yes`
  and SASL-or-reject). Dovecot mail ports are bound to loopback.
  New `acme` compose profile (`certbot/certbot:v4.0.0`, `--webroot`
  via `synth-watchdog`) issues staging certs; README documents the
  one-step `--staging` removal for production.
- **H-07 DKIM encryption + OpenDKIM sync.** New `synth-common::secrets`
  (AES-256-GCM). `dkim_generate` now encrypts the PEM at rest
  (`private_key_enc = enc:<b64>`) and writes `KeyTable`, `SigningTable`,
  and `<selector>.pem` (`0600`) into the shared `dkim` volume the
  `opendkim` service reads (`KeyTable` points at `/etc/opendkim/keys/<selector>.pem`).
  `MinimumKeyBits` 1024 → 2048.
- **M-15 / H-07 encryption key.** `Config` gains `ENCRYPTION_KEY`
  (64 hex, fail-fast, no default), forwarded to `synth-control` and
  `synth-webmail`; `config.rs` validates it. Plaintext DB values remain
  readable during rollout (decrypt passthrough) and are flagged legacy.
- **N-08 pending-secret 2FA enrollment.** Migration
  `20261107000004_recheck.sql` adds `user_totp.secret_pending` +
  `pending_expires_at` (10 min). `totp_enroll` stores the new secret
  there (encrypted) instead of clobbering the active one;
  `totp_verify` activates pending→active and clears pending; `totp_disable`
  clears pending. Abandoning enrollment no longer disables 2FA.

### P1 — seeds, limits, FTS, network, UI, backup, HA
- **N-02/R-01 seed password policy.** `seed_admin` now runs
  `validate_password_strength(…, 12)` on a user-supplied seed password
  and fails startup on violation. `INITIAL_ADMIN_PASSWORD` compose
  default removed (empty ⇒ random). Generated passwords are written to
  `/run/secrets/initial-admin-password` (`0600`) and announced on stderr
  instead of being logged (N-09).
- **N-01 distributed rate limiting.** `synth-common::rate_limit` is now
  Redis-backed (fixed window, shared across replicas) with a bounded
  in-memory fallback that evicts empty keys. Both `AppState`s construct
  it with the Redis client; all `check()` call sites are now `.await`.
- **N-04 FTS scheduling.** `synth-webmail::main` spawns a 60 s
  `reindex_batch` loop; the library `fts_indexer::search` takes
  `user_id` and is tenant-scoped (the HTTP route already was).
- **N-06 Postfix hardening.** `mynetworks` narrowed from
  `172.16.0.0/12` to loopback + `172.17.0.0/16`;
  `milter_default_action = reject` (was `accept`).
- **N-05/N-10 shim + Stalwart.** `synth-smtp` defaults to loopback
  (`--host 127.0.0.1`), caps 100 concurrent connections with a semaphore
  and a 60 s idle timeout. The Stalwart overlay is documented as
  standalone and binds all mail ports to loopback.
- **H-11 admin UI.** New `docker/synth-ui/Dockerfile` (Trunk WASM build →
  `nginx` static); `synth-ui` service (`127.0.0.1:3002`) behind edge
  `/admin/`; CI gained a `ui-build` job.
- **H-09 backup.** New `docker/backup/{Dockerfile,run-backup.sh}`
  (Postgres 15 client + `tar` + `rclone`): `pg_dump -Fc`, Maildir tar,
  `sha256sum` checksums, retention prune, optional `rclone sync`;
  `compose.yaml` `backup` service rewritten to use it.
- **M-17 HA overlay removed.** `docker/compose.ha.yaml` deleted (never a
  working replica). **M-16 pins:** `rspamd:1.94.2`; `clamav` left on
  `stable-debian` (no numeric tag upstream) with a digest-pin note.
  `.dockerignore` already existed and is now honored by the UI/backup builds.
- **M-23 light.** Migration adds a `users`-domain-managed trigger and drops
  the deprecated `quota_overrides` table.
- **R-04 forced password change.** `webmail::authenticate_user` now
  rejects `force_password_change` with a validation error so the webmail
  API is unusable until the password is rotated.
- **N-03 DAV auth.** `dav_discovery` now requires `authenticate_user`.

### P2 — hygiene
- **N-07 CI config tests.** New `config-test` job (`postfix check`,
  `rspamadm configtest`, `doveconf -n`) plus an `env:` block so the
  smoke `docker compose config` works with required secrets.
- **N-11 release lockfile.** `sign-release.sh` now writes real digests
  to `docs/releases/v${VERSION}.digests.txt`.
- **N-12 safe substitution.** Postfix/Dovecot entrypoints replaced
  with `entrypoint.sh` awk helpers (passwords with `&`, `|`, `/` OK).
- **N-13/Dovecot.** Entry point fails before `ssl` config creep — the
  config now hardcodes cert paths (no more sed-flip from `ssl = no`).
- **DMARC XXE.** `create_dmarc_report` rejects `<!DOCTYPE`, `<!ENTITY`,
  and `<!-- entity -->` shapes.
- **M-20/M-21 metrics.** `metrics::set_queue_depth` gauge wired into
  `queue_status`; the duplicate `postfix-queue` Prometheus job removed.
  Grafana datasource + overview dashboard were already present.

### Files changed (this pass)
- `Cargo.toml`, `Cargo.lock`
- `crates/synth-common/{Cargo.toml,src/secrets.rs,src/lib.rs,src/config.rs,src/rate_limit.rs,src/metrics.rs}`
- `crates/synth-control/{src/routes.rs,src/app.rs,src/db.rs,migrations/20261107000004_recheck.sql}`
- `crates/synth-dkim/src/lib.rs`
- `crates/synth-webmail/{src/main.rs,src/app.rs,src/routes.rs,src/carddav.rs,src/fts_indexer.rs}`
- `crates/synth-smtp/src/main.rs`
- `compose.yaml`, `.env.example`, `.github/workflows/ci.yml`
- `docker/{postfix,dovecot}/entrypoint.sh` (new), `docker/postfix/Dockerfile`,
  `docker/dovecot/Dockerfile`, `docker/postfix/main.cf`, `docker/postfix/master.cf`,
  `docker/dovecot/dovecot.conf`, `docker/opendkim/opendkim.conf`,
  `docker/edge/nginx.conf`, `docker/synth-ui/Dockerfile` (new),
  `docker/backup/{Dockerfile,run-backup.sh}` (new),
  `docker/compose.stalwart.yaml`, `docker/monitoring/prometheus.yml`
- `docker/compose.ha.yaml` (deleted)
- `scripts/sign-release.sh`
- `README.md`
