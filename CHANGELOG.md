# Changelog

All notable changes to SynthMail are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- `synth-smtp` is now production-ready: STARTTLS (RFC 3207) and SASL
  AUTH PLAIN/LOGIN (RFC 4954) (TLS-only), backed by the Postgres user
  store with Argon2id + timing-parity verification; unauthenticated
  submission restricted to the configured local domain(s) (never an open
  relay); bounded line reads, per-IP rate limits, queue-fail-loud `451`
  semantics, real Postfix relay (verbatim envelope + body), graceful
  shutdown.

### Known gaps

- High availability: the original non-functional overlay was removed.
  A Patroni overlay now adds a streaming replica with manual failover
  only (no floating IP or automatic promotion) — see
  `docs/runbooks/ha.md`; backup plus documented restore remains the
  baseline story.
- ClamAV is pinned to the `stable-debian` channel (upstream publishes no
  numeric tag); digest pinning is pending.

## [1.0.0] - 2026-11-08

First production-ready cut, after two rounds of security hardening. The
stack follows the DA-core model: Postfix, Dovecot, and Rspamd move and
filter mail; Rust services provide the admin API, webmail backend, DKIM
key management, health monitoring, and migration tooling.

### Security

- Authenticated admin and webmail APIs; JWT sessions with `token_version`
  revocation and validated `iss`/`aud`.
- Argon2id hashing with a shared password policy (breach check, length
  and denylist wiring, seed-admin enforcement).
- TOTP two-factor with pending-secret enrollment, replay protection, and
  secrets encrypted at rest (AES-256-GCM).
- DKIM private keys encrypted at rest and synced to the OpenDKIM volume;
  RSA-2048 and Ed25519 via real PKCS#8.
- Redis-shared rate limiting with per-endpoint enforcement.
- ACME provisioning profile; Dovecot and Postfix fail closed without
  certificates; SASL accepted only over TLS.
- Narrowed Postfix relay policy, fail-closed mail filters, and hardened
  DMARC report ingestion.

### Mail stack and webmail

- Postfix with PostgreSQL lookup maps; Dovecot with fail-closed TLS and
  loopback-bound plaintext ports.
- Rspamd (pinned image) for spam filtering; ClamAV for antivirus;
  OpenDKIM signing and OpenDMARC verification as milters.
- Edge reverse proxy (nginx) fronts the admin API, webmail backend, and
  the built admin UI; TLS terminates at the edge once certificates exist.
- Maildir-backed webmail with authenticated folders, search, drafts, send
  via Postfix, and a 60-second full-text indexer.
- JMAP (RFC 8620/8621) session discovery plus Mailbox and Email query/get/set
  (create/update/destroy, Maildir-backed); CardDAV/CalDAV discovery behind
  the webmail backend (Radicale profile).

### Operations

- Prometheus metrics, health polling, and deduped alerts via the watchdog;
  queue-depth gauge and a Grafana dashboard.
- Backup image (`pg_dump`, Maildir tar, checksums, retention, optional
  off-site sync) and migration tooling for mailcow / PostfixAdmin / CSV.
- End-to-end smoke script (`scripts/smoke.sh`) and CI coverage for
  formatting, lints, tests, migrations, image builds, and config checks.

Historical audit detail (not part of this release record) is preserved
under [`docs/history/`](docs/history/).
