# ADR 0003 — Pure-Rust MTA evaluation (Phase 3)

Date: 2026-11-07-08
Status: Accepted (Phase 3 deliverable)

## Context

Phase 3 asks whether SynthMail should replace Postfix+Dovecot with:
1. **Hybrid (v1 status quo):** Postfix MTA + Dovecot MDA + Rust control plane (`synth-control`).
2. **Pure-Rust shim:** hand-rolled `smtp-server` + `lettre` queue (new `synth-smtp` crate).
3. **Stalwart:** `stalwartlabs/stalwart:v1` (Rust, JMAP/IMAP/POP3/SMTP/CalDAV/CardDAV/WebDAV) as drop-in backend, keeping `synth-control` as admin layer.

## Decision factors (weighted)

| Factor | Weight | Hybrid (Postfix) | synth-smtp (shim) | Stalwart |
|---|---|---|---|---|
| Deliverability (SPF/DKIM/DMARC, PTR, queues, SRS, DANE) | High | Excellent (mature MTA) | Unknown — must reimplement queues, retry, TLS, reputation | Good — full MTA with DKIM/SPF/DMARC, DANE, queues |
| Interop (Thunderbird/Apple/Mobile, autoconfig, ManageSieve) | High | Excellent | Poor — no IMAP/POP3/ManageSieve yet | Excellent — JMAP + IMAP/POP3/ManageSieve native |
| Harder deliverability risks (greylist, Rspamd/ClamAV integration) | Medium | Rspamd+ClamAV proven | Must build spam/AV from scratch | Built-in filter + Rspamd optional |
| Ops (single Rust codebase) | Medium | 12+ containers | 2 containers (control+shim) | 2-3 containers (control+stalwart+postgres) |
| Performance (10k msg/h, p95 API <100ms) | Medium | Proven | TBD (bench required) | To benchmark (RocksDB/Postgres) |
| Migration (mailcow import, failover) | Medium | `synth-migrate` ready | Must rewrite maildir ingest | Native Maildir import + `synth-migrate` can target Stalwart API |

## Spike results (this repo)

### `synth-smtp` spike

- Minimal ESMTP server (EHLO/MAIL/RCPT/DATA/QUIT/RSET) on Tokio, `crates/synth-smtp`.
- Stores queued messages to Redis (`synth:queue:{id}` JSON) + forwards to Postfix `:25` via `lettre` when `FORWARD_TO_POSTFIX=true` (hybrid fallback) — proves deliverability without reimplementing queues.
- **Verdict:** usable as submission-edge shim, **not** a replacement for Postfix queues (no retry/backoff/DSN/SRS yet).

### Stalwart drop-in

- Added `docker/compose.stalwart.yaml` (profile `stalwart`) with `stalwartlabs/stalwart:v1` behind same `edge` + `synth-control` linking (see below).
- Image verified: `stalwartlabs/stalwart:v1` (latest `v1-alpine`/`latest-alpine`, pushed Nov 2026, multi-arch).
- Endpoints align with Phase 2 JMAP (`/jmap/session`, `/jmap`) — `synth-webmail` can proxy to Stalwart instead of Dovecot with config flag `IMAP_BACKEND=stalwart`.

## Recommendation

**Keep Hybrid for v0.5 and v1.0 GA; evaluate Stalwart as the v1.0 backend candidate behind a feature flag.** Pure hand-rolled MTA is not recommended — deliverability + interop outweigh code purity (R1).

- Default compose stays Postfix+Dovecot.
- Opt-in: `docker compose --profile stalwart up -d` for Stalwart evaluation (needs live-domain DNS + `mail-tester.com` pass).
- `synth-smtp` stays as submission shim / queue forwarder, not inbound MTA.

## Follow-ups

- Run `cargo test --workspace` + load: `synth-bench` p95 API <100ms, 10k msg/h queue throughput (needs VPS smoke).
- Promote Stalwart to default if it passes mail-tester + Thunderbird + Webmail JMAP integration for 4 weeks.
