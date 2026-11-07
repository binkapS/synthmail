# Security-audit coordination note

Two sessions are working `fix/security-audit` in parallel. This file records
who owns what so we don't step on each other.

## Session A (auth/infra, in progress in working tree)

Uncommitted changes in `crates/{synth-common,synth-control,synth-webmail,synth-filter,synth-migrate,synth-ui,synth-watchdog}`,
{}`compose.yaml`, `.env.example`, `docker/postfix/{Dockerfile,pgsql/}`, plus new
`crates/synth-common/src/policy.rs` and migration `20261107000003_audit.sql`.
Covered so far (per AUDIT.md IDs):

- C-01 — central `Principal`/auth extractor in synth-control; auth on read endpoints
- C-02 — TOTP bound to `claims.sub`, replay protection (`last_used_step` → BIGINT),
  replay protection (secret encryption at rest remains open — see CHANGELOG-AUDIT.md)
- C-03/C-04 — webmail `authenticate_user`, `user_id` scoping (partial)
- C-05 — `SESSION_SECRET` fail-fast in compose; `INITIAL_ADMIN_*` forwarded
- H-01 — API-token scopes via `Principal`
- H-03 — `policy.rs`: password policy + HIBP wiring
- H-04 — `token_version` claim + migration; logout/password-reset invalidation
- H-05 — loopback-only ports for control/webmail; watchdog network-internal
- H-06 — `postfix-pgsql` maps under `docker/postfix/pgsql/`
- H-08 — Redis `--requirepass` model (compose + `.env.example`)
- H-12/M-13/M-11 groundwork in `audit.sql`
- M-01 — Sieve generation fixes in `synth-filter`
- M-24/L-04/L-05 — migrate CSV parser + hash-prefix log fix

Left `TODO(H-13)` in compose for the rspamd config split — **handed to Session B**.

## Session B (this note)

Taking, in `docker/rspamd + compose + CI/scripts` only:

- H-13 — split `rspamd.conf` into `local.d/` snippets; remove TODO comment
  (done: `options.conf`, `redis.conf` with `${REDIS_PASSWORD}` UCL expansion,
  `antivirus.conf` with `servers = "clamav:3310"`, `bayes.conf`/`fuzzy.conf`
  on Redis backend; compose mounts the directory and injects REDIS_PASSWORD;
  old single-file config deleted. Validated against rspamd docs; `rspamadm
  configtest` against the live image still pending — no docker daemon here.)

Update: L-08 (`sign-release.sh`) and M-18 (CI pipefail/sqlx) turned out to be
fixed by Session A in the working tree already — not redone here.

Not touching: `crates/**`, `docker/postfix/**`, `compose.yaml` sections owned by
Session A (ports, env, secrets) — only the rspamd service block + TODO comment.
C-06 (ACME) and C-07/H-14 (synth-smtp) remain open for a future pass.
