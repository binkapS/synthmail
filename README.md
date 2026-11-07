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
cargo check -p synth-common -p synth-control -p synth-webmail -p synth-watchdog -p synth-filter -p synth-dkim
cargo test -p synth-common -p synth-filter -p synth-dkim
docker compose config
```

## Notes

- Phase 1 MVP scaffold: full API, Postgres identity, Postfix/Dovecot/rspamd/opendkim/opendmarc/clamav/fail2ban/edge/backup configs.
- IMAP fetch (`synth-webmail/src/imap.rs`) is stubbed for unit tests; wire to Dovecot :993 for live inbox.
- Run `cargo sqlx migrate run` / binary `--migrate` to apply `crates/synth-control/migrations/`.
