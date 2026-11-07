# Security runbook — Phase 2

## 2FA (TOTP)
```bash
# Enroll (auth required)
curl -X POST /api/v1/auth/totp/enroll -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' -d '{"email":"admin@example.com"}'
# Scan otpauth_url in authenticator, then verify
curl -X POST /api/v1/auth/totp/verify -H 'Content-Type: application/json' -d '{"email":"admin@example.com","password":"...","totp_code":"123456"}'
# Login now requires totp_code
curl -X POST /api/v1/auth/login -H 'Content-Type: application/json' -d '{"email":"...","password":"...","totp_code":"123456"}'
```

## API tokens
```bash
curl -X POST /api/v1/api-tokens -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' -d '{"name":"backups","scopes":["read"],"expires_in_days":90}'
# Use: Authorization: Bearer synth_... (prefix must match stored token_prefix)
curl /api/v1/domains -H "Authorization: Bearer synth_..."
curl -X DELETE /api/v1/api-tokens/$ID -H "Authorization: Bearer $JWT"
```

## Rate limiting & fail2ban
- `synth-common::rate_limit::RateLimiter` (code default 120 req/min/IP, 30 in the shipped `.env.example`, via `RATE_LIMIT_PER_MIN`) enforced on `/api/*` in edge; failures logged to `login_attempts`.
- fail2ban jails (`docker/fail2ban/jail.local`): postfix-sasl, dovecot (maxretry 5, bantime 1h). There is no synth-webmail jail — webmail throttling is enforced in-app via `login_attempts`.
- `/api/v1/admin/bans` lists recent failures.

## HIBP breach check
- `synth-common::hibp::check_breached` (k-anonymity: SHA1 prefix query to api.pwnedpasswords.com). Call on user create / password reset in control plane.

## Audit
- All admin actions → `audit_log` with actor/action/target/ip.

## Test
- `zap` basic scan on `edge`, `testssl.sh` for TLS.
- `cargo test -p synth-common` covers TOTP vectors (RFC 4226), HIBP prefix, rate limiter.
