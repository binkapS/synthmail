# SynthMail — Code & Security Audit

**Date:** 2026-11-09 · **Auditor:** Agnes · **Branch:** `main` @ `1c3c33c`

> **Status (day 2):** S-01, S-02, S-03, S-04 remediated in-tree.
> - **S-01 (CORS):** `synth-webmail` now reads `CORS_ORIGINS` env; unset/empty/
>   `*` → legacy `Any` (back-compat), CSV list → restricts to those origins.
>   Router picks the matching layer; `Config` gained `cors_origins` +
>   `cors_origins_list` fields; `from_env` populates them. Regression tests
>   added to `synth-webmail::routes` covering the four parse classes.
> - **S-02 (client-IP trust):** `client_ip()` in both `synth-control` and
>   `synth-webmail` now prefers the nginx-set `X-Real-IP` and falls back to
>   the **right-most** `X-Forwarded-For` entry. An unauthenticated caller
>   can no longer prepend `XFF: 8.8.8.8` and claim that IP; the right-most
>   untrusted hop is the only one read.
> - **S-03 (ntfy exposure):** `synth-watchdog` now supports `NTFY_BASE_URL`
>   so operators can point alerts at a private/self-hosted ntfy;
>   `validate_ntfy_topic()` runs at startup and warns when the topic is
>   < 12 chars or contains characters ntfy rejects. Dispatch failures in
>   both the webhook and ntfy paths are now logged (previously `let _`).
>   `.env.example` documents the new `NTFY_BASE_URL` + the topic-length
>   guidance.
> - **S-04 (SMTP forward pool):** `spawn_forward` now acquires a global
>   8-permit `tokio::sync::Semaphore` before each blocking Postfix relay;
>   on cap overflow the forward is skipped with a log warning, and the
>   Redis queue copy remains the source of truth (no mail lost, no
>   `spawn_blocking` starvation).
>
> **Status (day 1):** B-01, B-02, B-03 remediated in-tree — 6 clippy
> errors in `synth-smtp` fixed (`large_enum_variant` via boxed TLS
> variant, 2× `collapsible_if`, 3× test-file lints), `cargo fmt` applied,
> CI toolchain pinned to `1.99.1` with un-truncated clippy/test logs.
> `cargo clippy --workspace --all-targets -- -D warnings`,
> `cargo fmt --check` and `cargo test --workspace` all pass. Remaining
> items: B-04, D-01…D-03, L-01…L-04.
>
> **Note:** the working tree also carries uncommitted Plan work
> (host-networking §1, fail2ban §2, Docker-socket opt-in §3 — see
> `compose.yaml`, `docker/postfix/`, and the `ENABLE_CONTAINER_STATS`
> changes in `routes.rs`/`ServerPanel.svelte`). Those are the user's
> in-progress edits; this audit does not alter or depend on them, and
> the §3 changes (Docker socket behind an opt-in flag, default-off)
> are *better* than the audited default and partially resolve the
> "injection by any mailnet peer" concern raised in S-04 (the socket is
> no longer mounted by default — the injection surface stays, but the
> blast radius is reduced). Re-verify S-04 after that work lands.

## Executive summary

The codebase is in good shape: it compiles cleanly, all 120+ workspace tests
pass, security-critical paths (JWT + token-version revocation, AES-256-GCM
secrets at rest, TLS-only SASL, bounded queueing, Maildir path scoping, ACME
token validation) are correctly implemented, and secrets are properly kept
out of git. However, the most recently merged commit (`1c3c33c`,
"Make synth-smtp production-ready") was shipped with a **broken CI gate**:
`cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --check` both fail, so the CI workflow that is supposed to deny
warnings does not actually fail on this tree. There are also 4 genuine
security/robustness findings, 4 build/code-quality findings, and a set of
documentation gaps.

**Checks performed:** `cargo check --workspace --all-targets` (clean),
`cargo test --workspace` (all pass), `cargo clippy -- -D warnings` (FAIL, 3
binary + 3 test errors), `cargo fmt --check` (FAIL, 10 diffs), full read of
all 9 crates, compose/nginx configs, CI, runbooks map, and secret-leak
scans. `cargo audit` was not available offline (see L-04).

---

## Findings

### S-01 (Security, medium) — Webmail CORS is wide open

**File:** `crates/synth-webmail/src/routes.rs:43-48`

The whole webmail/JMAP router is wrapped in:

```rust
.layer(
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any),
)
```

Bearer-token auth is not bypassed by this, but any origin can call the
webmail API and receive CORS-permitted responses; the `Allow` header
discloses the full method surface to every website the user may visit, and
an attacker-controlled origin can inspect responses (including folder
names, subjects, metadata — everything in `list_folders` / `list_messages`
JSON bodies). The control plane (`synth-control`) has no CORS layer at all,
so the webmail router is the odd one out and the `Any` wildcard is almost
certainly vestigial — the real UI is same-origin behind the nginx edge
(`/admin/` → synth-ui; `/api/v1/mail` proxied in the same vhost).

**Remedy:** restrict to configured origins:

```rust
CorsLayer::new()
    .allow_origin(
        vec![s.config.control_url.as_str().into(), s.config.webmail_url.as_str().into()]
            .into_iter()
            .map(|o| o.into())
            .collect(),
    )
    .allow_methods([Method::GET, Method::POST])
    .allow_headers([HeaderName::from_bytes(b"authorization").unwrap()]);
```

and drop `allow_headers(Any)`. At minimum, make origins a `VITE`-style env
list (`CORS_ORIGINS`) and fail closed on an empty list. Add a regression
test asserting the layer echoes no `*`.

---

### S-02 (Security, medium) — `X-Forwarded-For` trusted unconditionally

**Files:** `crates/synth-control/src/routes.rs:107-114`
(`client_ip`), `crates/synth-webmail/src/routes.rs:450-457`
(`client_ip`).

Both helpers take `x-forwarded-for`'s left-most entry with no trust
boundary:

```rust
fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        ...
}
```

Any unauthenticated caller can send
`X-Forwarded-For: 8.8.8.8` and:

1. pollute `login_attempts` rows with a fake IP (audit trail integrity);
2. make the login rate limiter use a different key than the real client
   (rate-limit bypass for the attacker's real IP — each spoofed header
   value is a fresh 60 s window);
3. defeat any per-IP lockout the runbooks document.

nginx sets `X-Real-IP` (`$remote_addr`) on every proxied location, which is
the trustworthy value.

**Remedy:** read `X-Real-IP` first; fall back to `X-Forwarded-For` *only
when the direct peer is the edge proxy* (i.e. keep a
`TRUSTED_PROXY_CIDRS` env and only strip the right-most untrusted hop, the
standard "right-most trusted" rule). Fastest acceptable fix: use
`X-Real-IP` exclusively and document that all traffic must traverse edge.

```rust
fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers.get("x-real-ip").and_then(|v| v.to_str().ok()).map(str::to_string)
}
```

---

### S-03 (Security, low) — NTFY_TOPIC leaks to a public broker

**File:** `crates/synth-watchdog/src/main.rs:186-193`

```rust
if let Ok(topic) = std::env::var("NTFY_TOPIC") {
    let url = format!("https://ntfy.sh/{}/", topic);
    let _ = reqwest::Client::new()
        .post(&url)
        .body(format!("SynthMail: {} — {}", service, detail))
```

Alert payloads (service name + "service down >2m") POST to the
**public** `ntfy.sh` instance. Two issues: (a) unless the operator runs
their own ntfy or the topic is unguessable, *anyone who guesses the topic
can read the alert stream* — and ntfy public topics are not unguessable
when chosen carelessly; (b) the alert body can be read by all subscribers
of that topic, disclosing internal service names. There is also no TLS
pinning/verifiable failure mode: `let _` swallows the error.

**Remedy:** require an operator-supplied `NTFY_BASE_URL`
(default `https://ntfy.sh` but documented), reject topics that look like
short/weak strings (min length + entropy check at startup), log
`POST` failures, and document in `docs/runbooks/monitoring.md` that a
private ntfy is required for public mail hosts.

---

### S-04 (Security, low) — SMTP forward path: unbounded + unauthenticated relay inside mailnet

**Files:** `crates/synth-smtp/src/queue.rs:67-87` (`spawn_forward`),
`crates/synth-smtp/src/main.rs:90-93` (`--forward-to-postfix` **default
true**, `--postfix-host` **default `postfix:25`**).

Two combined properties deserve an operator note:

1. `spawn_forward` fires a **blocking** `lettre` SMTP session to
   `postfix:25` with `spawn_blocking` — no concurrency cap. A busy
   submission client queuing back-to-back 10 MiB messages can pin the
   `spawn_blocking` pool (default `num_cpus * 4`) with slow Postfix
   handshakes, starving other blocking work in the process.
2. Postfix is expected to accept these connections from the mailnet
   without SASL (`lettre::SmtpTransport::builder_dangerous` — no
   credentials). That is the "network-trusted" design documented in
   `docker/edge/nginx.conf` comments, and it is acceptable — but it means
   *any* container on `mailnet` that can reach `postfix:25` can inject
   mail. That already holds today for the shared Maildir spool, but it is
   worth an explicit statement + a Postfix `smtpd_client_restrictions`
   note in `docs/runbooks/security.md`.

**Remedy:** (a) add a semaphore around `spawn_forward` (e.g. 8 in
   flight; on rejection log `451`-style and leave the Redis queue as the
   source of truth — the forward is already best-effort); (b) add the
   "who can inject" paragraph to the security runbook; (c) document that
   `--forward-to-postfix` should stay `true` only on trusted mailnets and
   that the operator must firewall `postfix:25` to the mailnet interface.

---

### B-01 (Build, high) — `synth-smtp` fails `cargo clippy --all-targets -- -D warnings`

`1c3c33c` landed the production SMTP service with 6 clippy errors that are
denied in CI, so **the CI fmt/clippy job is red on the current HEAD**
(CI masks this today only because of B-02). Local reproduction:
`cargo clippy -p synth-smtp --all-targets -- -D warnings`.

Errors:

| Lint | Location | Fix |
|---|---|---|
| `large_enum_variant` | `crates/synth-smtp/src/service.rs:31` `enum Conn` (Tls variant ≈1224 B) | Box the large variant: `Tls(Box<TlsStream<TcpStream>>)` (pinning note at `service.rs:36` must be updated accordingly), or add `#[allow(clippy::large_enum_variant)]` with a comment. Boxing is preferred: `Conn` is per-connection, and the enum is not hot-path sized. |
| `collapsible_if` | `crates/synth-smtp/src/smtp.rs:115` (`extract_envelope_address` inner `if let`) | Merge the two nested `if let`s (`let-else` / `&&` chain). |
| `collapsible_if` | `crates/synth-smtp/src/smtp.rs:330` (`MAIL FROM` SIZE check) | `if let Some(size) = ... && size as usize > MAX_MESSAGE_SIZE_BYTES { ... }` |
| `bool_comparison` | `crates/synth-smtp/src/smtp_tests.rs:402` `assert!(!s.authenticated.is_none() == false, ...)` | `assert!(s.authenticated.is_none(), "no principal leaked");` — the current assertion is a tautological no-op double-negation; also **the test as written cannot fail**, which makes it misleading. |
| `let_and_return` | `crates/synth-smtp/src/service_tests.rs:35-36` `one_read` | `read_line(&mut s, pending).await.expect("read_line")` |
| `manual_contains` | `crates/synth-smtp/src/service_tests.rs:104` | `!pending.contains(&b'x')` |

**Remedy:** apply the six fixes above, re-run
`cargo clippy --workspace --all-targets -- -D warnings` until clean. The
`bool_comparison` fix also removes a dead assertion that looks like it
guards "no principal leaked" but actually asserts nothing.

---

### B-02 (Build, high) — CI clippy job cannot actually fail

**File:** `.github/workflows/ci.yml:15-19`

```yaml
- name: clippy (workspace)
  shell: bash
  run: |
    set -o pipefail
    cargo clippy --workspace --all-targets -- -D warnings 2>&1 | head -n 200
```

With `pipefail` the step *should* fail on a compiler error, but two things
make the gate unreliable:

1. `head -n 200` truncates output, and when a failure happens, the step's
   log shows only the clippy stderr that happened to fit in 200 lines —
   diagnosable but fragile. More importantly:
2. On the *test* job (`ci.yml:26-30`) `cargo test --workspace 2>&1 | tail
   -n 100` **does** fail on pipefail. That is fine. But the clippy
   truncation means a single lint error at line 200+ of the output
   produces a step failure with no visible cause.

And empirically, on this tree, B-01's errors make the clippy job fail —
which means the repo has been green-lighting merges that break its own
gate (see `git log`: `1c3c33c` is merged, `7afc4ca` and earlier are
clippy-clean but the gate was evidently bypassed or the toolchain drifted).

Likely cause: CI runs `dtolnay/rust-toolchain@stable`, so **the clippy
toolchain moved past the one the code was formatted/linted with** and
new lints (all six above are from Rust 1.99-era clippy) started
firing. `cargo fmt --check` would have caught B-03 on the same merge.

**Remedy:**
- Fix B-01 first (gate passes only when the code passes).
- Pin the toolchain: `dtolnay/rust-toolchain@stable` →
  `dtolnay/rust-toolchain@1.91` (or the version in `rust-toolchain.toml`
  resolved at the time of the last green run), so lint drift is
  intentional, not accidental.
- Drop the `head`/`tail` on lint jobs or replace with `if [ $? -ne 0 ];
  then cargo clippy ... > clippy.log; head -n 40 clippy.log; exit 1; fi`
  so failures always show the *error lines*.

---

### B-03 (Quality, medium) — `cargo fmt --check` fails on HEAD

`crates/synth-control/src/routes.rs` has 10 unformatted hunks (line 47
indentation drift after the `admin_server_stats` route was added; lines
1326, 2401, 2448 and others re-wrapping `sqlx::query_as` chains; stray
blank line before `struct DiskStat`). None are semantic.

**Remedy:** `cargo fmt` (single command, no changes needed by hand).
Re-add to the pre-merge checklist — this is the cheapest gate in the
repo.

---

### B-04 (Quality, low) — `synth-smtp` has no Dockerfile or compose service

`crates/synth-smtp` is a production service, yet `docker/` has no
`synth-smtp/` directory and `compose.yaml` does not run it. Its
`--postfix-host` default is `postfix:25` (docker-network naming), so it is
obviously meant for the compose stack, but it currently has no deployment
path, no healthcheck wiring, and no CI image build (CI builds only
control/webmail/UI). A sysadmin reading the README (line 53: "ESMTP
submission service … (:2525)") would look for it in compose and not find
it.

**Remedy:** add `docker/synth-smtp/Dockerfile` (copy the
synth-control pattern), add a compose service on `mailnet` with
`2525:2525` **loopback-published** per the H-05 convention, wire
`SESSION_SECRET` / `ENCRYPTION_KEY` / `DATABASE_URL` / `REDIS_URL`
envs, add to CI `build` matrix, and update the README's "Services and
ports" table.

---

### D-01 (Docs, medium) — README references `docs/history/` audit records that do not exist

`README.md:256` ("Documentation map") lists
"`docs/history/` — audit records (AUDIT, AUDIT-RECHECK)", but
`docs/history/` **is an empty directory**. Every prior audit note in the
codebase ("audit C-06", "audit H-08", "audit M-15", "audit N-01", etc.)
points at this missing record, so the cross-reference layer of the
security story is broken. This Audit.md is a natural first entry there.

**Remedy:** move (or leave) this file at the repo root, *and* populate
`docs/history/AUDIT.md` + `AUDIT-RECHECK.md` with the closed findings
the code comments already reference (C-05, C-06, H-04, H-05, H-07, H-08,
H-10, M-15, N-01, N-04, R-04 …) so future readers can trace the
provenance. At minimum, add this file as
`docs/history/2026-11-09-audit.md` and fix the README line to point to
the real path.

---

### D-02 (Docs, low) — `plan.md` at the repo root is untracked and stale

`git status` shows `?? plan.md` (untracked since the initial commit
window). It is a work-in-progress hardening plan (host-networking,
fail2ban) with no status markers, and it sits at the root rather than
under `docs/`.

**Remedy:** either commit it to `docs/plans/network-harden.md` with a
"Status: open" header, or delete it if the work is captured in
`docs/runbooks/`. Do not let it drift untracked.

---

### D-03 (Docs, low) — nginx comments still reference the removed Leptos UI

`docker/edge/nginx.conf:38` says
"Built UI (Leptos SPA) — served by the synth-ui container." and
`nginx.conf:44` says "Leptos/WASM build artifacts from synth-ui." The
Leptos crate was removed in `830404f`; the UI is now a Svelte 5 SPA
(served by nginx inside the synth-ui image, static bundle).

**Remedy:** update both comment lines to "Svelte SPA — served by
synth-ui".

---

### L-01 (Lint, low) — `synth-webmail` uses a crate-wide `#[allow(dead_code)]`

**File:** `crates/synth-webmail/src/main.rs:4`

`#![allow(dead_code)]` hides genuinely unreachable code in a
security-sensitive crate (Maildir access, JMAP dispatch). It masks
refactoring debt and would let dead, possibly out-of-date auth helpers
accumulate.

**Remedy:** remove the attribute; add targeted `#[allow(dead_code)]`
with a comment on anything that is intentionally unused (e.g. a CLI flag
for a future build).

---

### L-02 (Lint, low) — Dead-code smell in `db.rs::redis_invalidate`

**File:** `crates/synth-control/src/db.rs:115-124`

```rust
pub async fn redis_invalidate(pool: &PgPool, redis_client: &redis::Client) -> anyhow::Result<()> {
    if let Ok(mut c) = redis_client.get_multiplexed_async_connection().await {
        let _: Option<String> = redis::AsyncCommands::publish(&mut c, "synth:config:invalidate", "1").await.ok();
    }
    let _ = pool;
    Ok(())
}
```

The `pool` parameter is unused (silenced with `let _ = pool;`), which is
a sign the API contract changed (DB row bumps, Redis publish only).
Callers pass `&s.pool` at 9+ sites, paying a borrow they do not need.

**Remedy:** drop the `pool` parameter; update the 9 call sites. This
also removes one line of the "audit-compliant but confused" surface.

---

### L-03 (Lint, low) — `config.rs` has a crate-level `#[allow(dead_code)]` on a public struct

**File:** `crates/synth-common/src/config.rs:4`

`#[allow(dead_code)]` on `pub struct Config` is harmless (it *is*
public, the allow is just noise) but it hides whether fields like
`letsencrypt_email` / `timezone` are actually consumed anywhere.

**Remedy:** run `cargo clippy` with
`-W dead-code` on the workspace after the `synth-webmail` allow is
removed (L-01), then either delete unused fields or add usages.

---

### L-04 (Process, low) — `cargo audit` is not part of the repo's checks

No dependency advisories are checked anywhere: not in CI, not in a
Makefile, not in the runbooks. `Cargo.lock` pins 398 packages; a future
advisory (e.g. in `openssl`-adjacent deps, `sqlx`, or `lettre`) would
land silently.

**Remedy:** add to CI (new job):

```yaml
audit:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: rustsec/audit-check@v2
      with:
        token: ${{ secrets.GITHUB_TOKEN }}
```

and a local `cargo audit` alias in the Development section of the
README. `npm audit` (the UI has `package-lock.json`) gets the same
treatment in the `ui-build` job.

---

## What's solid (no action)

Recorded so the next audit does not re-flag these:

- **JWT + token-version revocation** (`synth_common::auth`,
  `synth-control/src/routes.rs:184-215`,
  `synth-webmail/src/routes.rs:66-104`): claims carry `tv`, decoded
  against the DB row, `iss`/`aud` validated, inactive users rejected,
  `force_password_change` blocks the whole webmail surface.
- **At-rest encryption** (`synth_common::secrets.rs`): AES-256-GCM,
  64-hex-char key enforced in `Config::validate`, `enc:` prefix with
  legacy-plaintext fallback (documented rollout), 12-byte random nonce
  per blob.
- **Sieve / vacation / alias generators** (`synth-filter`): escape
  function is correct (`\`, `"`, CR/LF), capability set is validated,
  `":address"` patterns rejected, tests cover the generated output.
- **DKIM key generation** (`synth-dkim`): RSA-2048 + Ed25519, Ed25519
  emits a real PKCS#8 document (the earlier bare-seed bug is fixed and
  regression-tested against `from_pkcs8_pem`), round-trips through the
  `rsa` / `ed25519-dalek` parsers.
- **SMTP state machine** (`synth-smtp/src/smtp.rs`): correct RFC 5321 /
  3207 / 4954 / 1870 handling — STARTTLS mid-transaction rejected,
  AUTH only over TLS, `SIZE` parameter parsed, unauth relay restricted
  to `relay_domains` by default, dot-stuffing handled, `RCPT` syntax
  validated, open-relay only via an explicit flag that logs loudly.
- **Bounded I/O** (`synth-smtp/src/service.rs`): the capped line reader
  really does bound memory (the drain path in `read_line` drops the
  overlong prefix and continues reading), session wall-clock, idle
  timeouts, and queue backpressure via `451` on Redis failure are all
  in place.
- **Maildir access** (`synth-webmail/src/imap.rs`): `mailbox_dir`
  rejects traversal (`/`, `\`, empty local/domain), `valid_folder`
  rejects `.` / `..` / separators / NUL, `create_message` writes to
  `tmp/` first with mode `0600` and `create_new`, folder scoping
  is consistent across `fetch` / `move_to_trash` / `create`.
- **ACME token validation** (`synth-watchdog`): `is_valid_acme_token`
  rejects traversal, tested.
- **Secrets hygiene:** `.env` is gitignored (`git check-ignore` confirms),
  `.env` on this host contains real local secrets (not committed),
  `compose.yaml` enforces `:?` fail-fast on `SESSION_SECRET`,
  `ENCRYPTION_KEY`, `POSTGRES_PASSWORD`, `REDIS_PASSWORD`,
  `GRAFANA_ADMIN_PASSWORD`; the postgres default `synth_dev` is a
  documented dev-only fallback guarded by the compose `:?`.

## Suggested action order

1. **B-01 + B-02 + B-03** — make the CI gate actually green and actually
   meaningful (30 min: 6 clippy fixes + `cargo fmt` + pin toolchain +
   untruncate logs).
2. **S-01 + S-02** — CORS and client-IP trust (both are one-file changes
   with a test each; half a day).
3. **S-04 + B-04** — bound the SMTP forward pool and give `synth-smtp` a
   Dockerfile + compose service + CI build.
4. **D-01 + D-02 + D-03** — close out the documentation debt (populate
   `docs/history/`, commit-or-delete `plan.md`, fix nginx comments).
5. **L-01..L-04 + S-03** — lint cleanups and the ntfy hardening
   (one focused pass).
6. Re-run `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo fmt --check`, `cargo test --workspace`, and add `cargo audit`
   to CI (L-04).
