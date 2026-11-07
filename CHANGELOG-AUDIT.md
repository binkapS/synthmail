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
