# Migration runbook (mailcow / PostfixAdmin / CSV)

```bash
cp .env.example .env
docker compose up -d postgres redis

# 1) Dry-run from CSV export (mailcow: /admin → Export → mailboxes)
cargo run -p synth-migrate -- dry-run --source mailcow --csv /path/to/export.csv --maildir /var/vmail

# 2) Import
cargo run -p synth-migrate -- import --source mailcow --database-url "$DATABASE_URL" \
  --csv /path/to/export.csv --maildir /var/vmail --force-password-reset --regenerate-dkim

# 3) Verify
cargo run -p synth-migrate -- verify
# Count maildir
cargo run -p synth-migrate -- count-maildir --maildir /var/vmail

# 4) Regenerate DKIM per domain
curl -X POST https://mail.example.com/api/v1/dkim/$DOMAIN_ID/generate \
  -H "Authorization: Bearer $ADMIN_JWT" -H 'Content-Type: application/json' \
  -d '{"key_type":"rsa2048","selector":"default"}'
# Copy DNS record from response (full_dns)

# 5) doveadm auth equivalence: login via IMAP with new password, check hash prefix
cargo run -p synth-migrate -- verify
```

Notes:
- mailcow uses blowfish/sha256_crypt; those hashes cannot be verified by Argon2id — force reset on import (`force_password_reset=true`) and have users set new passwords.
- `import_mailcow` documents MySQL surface (domain/mailbox/alias); CSV path is the fully-working Phase 2 path.
