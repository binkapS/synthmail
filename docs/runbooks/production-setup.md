# SynthMail — Production Setup Guide

Step‑by‑step from a bare host to a running, hardened production
deployment. All commands assume you are in the repository root
(`/opt/synthmail` or wherever you cloned it).

> **Read order:** this guide is the end‑to‑end path. The deep‑dive
> runbooks (`security.md`, `dns.md`, `backup.md`, `monitoring.md`,
> `deliverability.md`) are linked at the end; keep the audit history
> in `docs/history/` as provenance for constraints mentioned here.

---

## Table of contents

0. [Prerequisites](#0-prerequisites)
1. [Clone and inspect](#1-clone-and-inspect)
2. [Configure `.env`](#2-configure-env)
3. [Generate secrets](#3-generate-secrets)
4. [DNS prerequisites](#4-dns-prerequisites)
5. [Firewall](#5-firewall)
6. [Provision TLS (ACME)](#6-provision-tls-acme)
7. [Validate the compose file](#7-validate-the-compose-file)
8. [Start the stack](#8-start-the-stack)
9. [First login and mandatory rotation](#9-first-login-and-mandatory-rotation)
10. [Enrol 2FA (TOTP)](#10-enrol-2fa-totp)
11. [Health verification](#11-health-verification)
12. [Smoke test](#12-smoke-test)
13. [Post-setup hardening](#13-post-setup-hardening)
14. [Optional profiles](#14-optional-profiles)
15. [Ongoing operations](#15-ongoing-operations)
16. [Troubleshooting](#16-troubleshooting)

---

## 0. Prerequisites

| Item | Minimum | Notes |
|---|---|---|
| **OS** | Linux (glibc, e.g. Debian 12, Ubuntu 24.04) | The mail daemons (`postfix`, `dovecot`, `rspamd`, `clamav`) run in `host` network mode by default, so the host kernel sees real client IPs. |
| **CPU / RAM / disk** | 2 vCPU / 4 GB / 20 GB | Postfix + Rspamd + ClamAV are the heaviest pieces. 512 MB extra if you enable Prometheus/Grafana. |
| **Docker** | Docker ≥ 24 + Compose v2 (`docker compose` plugin) | Build happens inside the container; no local Rust or Node toolchain is needed. |
| **Public hostname** | `MAIL_HOST` (e.g. `mail.example.com`) | Must resolve from the internet (A/AAAA) **before** ACME. |
| **Mail domain** | `MAIL_DOMAIN` (e.g. `example.com`) | MX must point to `MAIL_HOST`. See §4. |
| **Ports** | 25, 80, 443, 465, 587, 993, 995, 4190 | In host‑network mode the daemons bind the host directly — the host firewall, not Docker, is the enforcement point. |
| **Port 25 outbound** | Not blocked by your provider | Many VPS providers block port 25. If outbound 25 is blocked, outbound mail will queue; coordinate with the host before cutover. |
| **ACME account email** | A mailbox you can read | `LETSENCRYPT_EMAIL`. You will receive expiry notices. |

---

## 1. Clone and inspect

```bash
git clone https://github.com/synthmail/synthmail.git /opt/synthmail
cd /opt/synthmail
docker compose version
git log --oneline -5   # confirm you are on a released tag
```

Check that the layout matches the architecture in `README.md`:

```
crates/  docker/  docs/  ui/  scripts/  compose.yaml  .env.example
```

---

## 2. Configure `.env`

SynthMail is configured entirely through a `.env` file in the repo
root. There is **no safe default** for the load‑bearing secrets —
`compose.yaml` fails fast (`${VAR:?…}`) if they are unset.

```bash
cp .env.example .env
chmod 600 .env          # secrets file: owner-read only. Never commit it.
```

Then edit `.env` and replace **every** placeholder. The table below is
the production minimum.

### Required identity

| Variable | Example | What it controls |
|---|---|---|
| `MAIL_HOST` | `mail.example.com` | Public hostname. Must match the ACME SAN and the A/AAAA record. |
| `MAIL_DOMAIN` | `example.com` | Hosted mail domain. MX target for inbound mail. |
| `LETSENCRYPT_EMAIL` | `admin@example.com` | ACME account email (expiry notices land here). |
| `ACME_STAGE` | `staging` first, then `prod` | See §6. |
| `CONTROL_URL` / `WEBMAIL_URL` / `ADMIN_URL` | `https://mail.example.com/api` … | Used by the UI for API routing. Set to your public URLs. |

### Fail‑fast secrets (compose asserts these)

| Variable | Length | Generation (see §3) |
|---|---|---|
| `POSTGRES_PASSWORD` | ≥ 24 random chars | `openssl rand -base64 24` |
| `REDIS_PASSWORD` | ≥ 24 random chars | `openssl rand -base64 24` |
| `SESSION_SECRET` | ≥ 32 bytes | `openssl rand -base64 48` |
| `ENCRYPTION_KEY` | **64 hex chars** (32 bytes) | `openssl rand -hex 32` |
| `GRAFANA_ADMIN_PASSWORD` | ≥ 24 random chars | `openssl rand -base64 24` |

These rules are enforced in two places, so a weak value fails loudly in
CI or `docker compose config`:

* **`compose.yaml`** — `${SESSION_SECRET:?…}`, `${ENCRYPTION_KEY:?…}`,
  `${POSTGRES_PASSWORD:?…}`, `${REDIS_PASSWORD:?…}`,
  `${GRAFANA_ADMIN_PASSWORD:?…}`.
* **`synth_common::Config::validate`** — `SESSION_SECRET` must be
  ≥ 32 bytes and not a placeholder; `ENCRYPTION_KEY` must be exactly
  64 hex chars; the placeholder strings in `.env.example` are
  explicitly rejected.

`DATABASE_URL` and `REDIS_URL` must be updated to match — they embed
the passwords above:

```dotenv
POSTGRES_USER=synth
POSTGRES_PASSWORD=<paste generated value>
DATABASE_URL=postgres://synth:<same>@postgres:5432/synthmail

REDIS_PASSWORD=<paste generated value>
REDIS_URL=redis://:<same>@redis:6379
```

> **`REDIS_URL` ↔ `REDIS_PASSWORD` lock‑step (audit H‑08):** the
> `redis` service runs with `--requirepass ${REDIS_PASSWORD:?…}`.
> If the two values disagree, every Redis‑backed limiter and the
> SMTP queue fail at runtime.

### Auth and quotas

| Variable | Default in `.env.example` | Notes |
|---|---|---|
| `MAX_QUOTA_MB` | `5120` | Per‑mailbox quota. |
| `SPAM_THRESHOLD` | `7` | Rspamd spam‑flag threshold. |
| `RATE_LIMIT_PER_MIN` | `30` | Login / API rate limit (Redis‑shared). |
| `HIBP_ENABLED` | `true` | Breached‑password k‑anonymity check on create/reset. |
| `TOTP_ENFORCED_FOR_ADMIN` | `true` | Admins must enrol 2FA before full API access. |
| `INITIAL_ADMIN_EMAIL` | `admin@example.com` | First‑boot admin seed. |
| `INITIAL_ADMIN_PASSWORD` | *(empty)* | **Leave empty for the first boot** — synth‑control generates a one‑time random password, writes it to `/run/secrets/initial-admin-password` (0600), and tells you to rotate it. See §9. If you set it here it must pass the policy (≥12 chars, denylist, HIBP). |

### Edge ports

If you are **not** behind another reverse proxy, bind the edge
directly on 80/443:

```dotenv
HTTP_BIND=0.0.0.0
HTTP_PORT=80
HTTPS_BIND=0.0.0.0
HTTPS_PORT=443
```

Otherwise (an existing Apache/nginx on 80/443) keep
`HTTP_PORT=8080` / `HTTPS_PORT=8443` and front SynthMail with your
own proxy.

### CORS (webmail/JMAP)

`CORS_ORIGINS` defaults to empty (no CORS layer = same‑origin only).
The admin UI ships same‑origin behind the nginx edge, so **leave it
empty** unless your UI lives on a different origin. To open it to
specific origins, set a comma‑separated list:

```dotenv
CORS_ORIGINS=https://mail.example.com
```

`"*"` restores the legacy wildcard (audit S‑01) — do not use in
production.

### Mail‑edge network mode (Plan §1)

```dotenv
MAIL_EDGE_NETWORK=host
MAIL_EDGE=1
```

`host` is the production default: `postfix`/`dovecot`/`rspamd`/
`clamav` bind host ports directly, so Postfix logs the **real
`client=` IP** (not the Docker bridge gateway) and Rspamd/DNSBL
checks receive the original address. `MAIL_EDGE=1` tells the
Postfix/Dovecot entrypoints to tighten `mynetworks` to loopback‑only
in host mode.

Set `MAIL_EDGE_NETWORK=` (empty) only as the bridge‑NAT fallback —
you lose public source‑IP preservation. The `ports:` blocks in
`compose.yaml` for the bridge layout are commented out in host mode;
uncomment them if you fall back.

### Container stats (Plan §3)

```dotenv
ENABLE_CONTAINER_STATS=0
```

Default **off**, so the Docker socket is not mounted. Only set
`ENABLE_CONTAINER_STATS=1` and uncomment the
`/var/run/docker.sock` mount in the `synth-control` service on a
trusted host where you want the per‑container CPU/RAM view in the
admin Server panel.

---

## 3. Generate secrets

Run this **before** editing `.env` (copy the output into the file):

```bash
# SESSION_SECRET — JWT signing secret (≥32 bytes)
openssl rand -base64 48

# ENCRYPTION_KEY — AES‑256 key for DKIM/TOTP at rest (64 hex chars, no
# alternative format is accepted)
openssl rand -hex 32

# POSTGRES_PASSWORD / REDIS_PASSWORD / GRAFANA_ADMIN_PASSWORD
openssl rand -base64 24
openssl rand -base64 24
openssl rand -base64 24
```

Rules to remember:

* `SESSION_SECRET` and `ENCRYPTION_KEY` are **never auto‑generated**.
  Generate them yourself and store them in a secrets manager — losing
  `ENCRYPTION_KEY` means DKIM private keys and TOTP secrets at rest
  cannot be decrypted.
* `POSTGRES_PASSWORD` appears in three places: `POSTGRES_PASSWORD`,
  `DATABASE_URL`, and it must match. Overwrite the placeholder string
  `CHANGE_ME_STRONG_RANDOM` with the generated value in **both**.
* The `.env` file itself is gitignored (`.gitignore` has `.env`).
  `chmod 600 .env` is still required — the gitignore only stops
  commits, not local disclosure.
* `docker compose config` will refuse to render if any `?`-guarded
  secret is missing or still a placeholder, which is intentional.

---

## 4. DNS prerequisites

Do this **before** step 6 (ACME HTTP‑01 needs port 80 reachable) and
before the first inbound mail test.

| Record | Value | Purpose |
|---|---|---|
| `A` / `AAAA` | `MAIL_HOST` → your host's public IP | ACME HTTP‑01 + HTTPS. |
| `MX` | `MAIL_DOMAIN` → `MAIL_HOST` (priority 10) | Inbound mail delivery. |
| `SPF` | `v=spf1 a:<MAIL_HOST> -all` (or `-all` for closed hosts) | Published via the admin UI → Domain DNS panel once running. |
| `DKIM` | `<selector>._domainkey TXT "v=DKIM1; k=rsa; p=…"` | Generate per‑domain in the admin UI (RSA‑2048 / Ed25519, private key AES‑256‑GCM encrypted at rest). |
| `DMARC` | `_dmarc TXT "v=DMARC1; p=…; rua=mailto:dmarc@<MAIL_DOMAIN>; fo=1"` | Published via the same panel. |

The admin UI → domain view builds the exact TXT strings for you
(`docs/runbooks/dns.md`). After publishing:

```bash
dig +short MAIL_HOST A
dig +short MAIL_DOMAIN MX
dig +short MAIL_HOST TXT      # SPF
dig +short _dmarc.MAIL_DOMAIN TXT
# DKIM selector TXT — copy the selector from the admin UI domain view
```

**PTR/rDNS** — ask your provider for a reverse‑DNS entry
(`IP.arpa` → `MAIL_HOST`). Missing PTR is the #1 cause of
rejected outbound mail; it cannot be fixed inside Docker.

---

## 5. Firewall

The daemons are on `network_mode: host` by default, so the **host**
firewall (`nft`/`ufw`/`iptables`) is the enforcement point — Docker
port publishing is not used for the mail ports in host mode.

Open inbound:

```
25/tcp    SMTP (inbound delivery)
80/tcp    HTTP (ACME HTTP‑01 + redirect)
443/tcp   HTTPS (edge)
465/tcp   SMTPS
587/tcp   Submission (Postfix)
993/tcp   IMAPS
995/tcp   POP3S (if you use POP3)
4190/tcp  Sieve (if you use Sieve)
```

For `ufw`:

```bash
sudo ufw allow 25,80,443,465,587,993,995,4190/tcp
sudo ufw status
```

Fail2Ban runs with `network_mode: host` + `NET_ADMIN`, so its
bans reach the host firewall directly (Plan §2). `synth-smtp`
(2525) is `127.0.0.1`-published only — reach it via an SSH tunnel or
a deliberate host‑firewall rule if you want remote submission; see
the `synth-smtp TLS + authenticated submission` note in `README.md`.

---

## 6. Provision TLS (ACME)

The stack is **fail‑closed without certificates** (audit C‑06):
`postfix` (`smtpd_tls_cert_file` missing), `dovecot` (`ssl =
required`), and the edge HTTPS server all refuse to start until
`certs/live/<MAIL_HOST>/fullchain.pem` + `privkey.pem` exist.

HTTP‑01 only (no DNS‑01 automation). The challenge is served by
`synth-watchdog :8081`, proxied through the edge at
`/.well-known/acme-challenge/`.

### 6a. Staging first (safe, no rate limit)

```bash
export ACME_STAGE=staging            # or set it in .env
docker compose --profile acme run --rm acme
```

The output should read: `ACME complete (staging cert issued for …)`.
Verify:

```bash
docker compose run --rm -v certs:/certs alpine \
  sh -c 'ls /certs/live/<MAIL_HOST>/'
# fullchain.pem  privkey.pem  chain.pem  cert.pem
```

### 6b. Production certificate

```bash
export ACME_STAGE=prod               # or ACME_STAGE=prod in .env
docker compose --profile acme run --rm acme
```

Then remove `--staging` permanently — the compose entrypoint already
keys its `--staging` flag off `ACME_STAGE` (Plan §4), so for cutover
just ensure `ACME_STAGE=prod` in `.env` and re‑run the profile.

> If ACME fails with `connection refused` or `403` on the HTTP‑01
> challenge, check: (1) the A/AAAA record resolves from the internet,
> (2) host TCP 80 is reachable externally, (3) the `acme` profile
> service and `synth-watchdog` both start (the challenge is served by
> watchdog, not certbot).

### 6c. Enable HTTPS on the edge

The edge HTTPS server block lives in
`docker/edge/443.conf.example` and is activated **only when**
`certs/live/<MAIL_HOST>/` exists (a custom compose command swaps it
in; nginx aborts at startup on a missing `ssl_certificate`
otherwise). After ACME succeeds, bring the edge up (`docker compose
up -d edge` if it was not started) and confirm:

```bash
curl -fsS https://<MAIL_HOST>/health
# ok
```

`ssl_protocols TLSv1.2 TLSv1.3` and `Strict-Transport-Security` are
already set in the example block.

### 6d. Certificate backup

Add `certs/` to your backup schedule — losing the private key means
a re‑issue and a mail‑delivery interruption window. See
`docs/runbooks/backup.md`.

---

## 7. Validate the compose file

Before the first `up`, render the fully‑interpolated compose file:

```bash
docker compose config        # fails fast if any required secret is unset
```

This also prints the resolved `synth-smtp` command, environment,
and port bindings so you can sanity‑check that `DATABASE_URL` /
`REDIS_URL` carry the real passwords (they will — they are in the
file, so do not paste this output in a ticket) and that
`127.0.0.1:2525:2525` is loopback‑only.

Options: `docker compose config -q` for a quiet syntax check only.

---

## 8. Start the stack

```bash
docker compose up -d --build
```

What starts on first boot, in dependency order:

* `postgres` + `redis` (healthy gates for the app services)
* `synth-control` — runs `sqlx` migrations automatically
  (`crates/synth-control/migrations`), then seeds the first admin
  if the `users` table has no admin (see §9)
* `synth-webmail`, `synth-watchdog`, `synth-smtp`, `synth-ui`
* `postfix`, `dovecot`, `rspamd`, `clamav`, `opendkim`, `opendmarc`,
  `fail2ban` (host‑network mail edge)
* `edge` (nginx)

Watch the bring‑up:

```bash
docker compose ps
docker compose logs -f synth-control
docker compose logs -f postfix | grep client=      # real IPs in host mode
```

The image build takes a few minutes on a cold host (Rust +
Svelte). Subsequent `up -d` is fast.

---

## 9. First login and mandatory rotation

The admin seed logic is in
`crates/synth-control/src/db.rs` (`seed_admin_if_empty`):

1. **`INITIAL_ADMIN_PASSWORD` set in `.env`** → that password is
   hashed (Argon2id) and used. It must pass the policy check
   (`synth_common::validate_password_strength`, ≥ 12 chars,
   denylist, no leading/trailing whitespace, HIBP if enabled). A
   weak seed is **rejected at boot** — synth‑control aborts with
   `INITIAL_ADMIN_PASSWORD rejected: …` rather than seeding a
   weak admin.
2. **`INITIAL_ADMIN_PASSWORD` empty (recommended)** → a one‑time
   random 36‑hex password is generated, written to
   `/run/secrets/initial-admin-password` inside the
   `synth-control` container with mode `0600`, and the password
   itself is **never logged** (audit N‑09). Retrieve it, log in,
   rotate, and delete:

```bash
docker compose exec synth-control cat /run/secrets/initial-admin-password
# log in at https://<MAIL_HOST>/admin/  (or http://localhost:8080/admin/)
# immediately change the password in Account → Password
docker compose exec synth-control rm /run/secrets/initial-admin-password
```

> The file lives inside the container only — a container recreate
> (`docker compose down -v`) destroys it, so copy it out first.

If the admin already exists, `seed_admin_if_empty` is a no‑op and a
`seed_admin` audit‑log row is **not** written again (the password is
unchanged on re‑seed — "password unchanged if already present").

`force_password_change` is checked on every `synth-webmail` API
call (audit R‑04): until the password is rotated, the webmail API
is unusable. That is intentional — it prevents the generated
one‑time credential from lingering.

---

## 10. Enrol 2FA (TOTP)

`TOTP_ENFORCED_FOR_ADMIN=true` blocks full admin API access until
an admin enrols (see `docs/runbooks/security.md`):

```bash
# Enrol with your session token
curl -X POST /api/v1/auth/totp/enroll \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"email":"admin@example.com"}'

# Scan the returned otpauth_url in your authenticator, then verify
curl -X POST /api/v1/auth/totp/verify -H 'Content-Type: application/json' \
  -d '{"email":"admin@example.com","password":"…","totp_code":"123456"}'
```

Secrets are stored encrypted with `ENCRYPTION_KEY` (AES‑256‑GCM,
`enc:` prefix) and replay‑protected (`last_used_step`).
`synth-webmail` enforces the same TOTP step on its login endpoint.

---

## 11. Health verification

```bash
docker compose ps
# every service should be "Up" (and "healthy" where a healthcheck exists)

curl -fsS http://127.0.0.1:8000/healthz     # synth-control
curl -fsS http://127.0.0.1:3000/healthz     # synth-webmail
curl -fsS http://127.0.0.1:8081/healthz     # synth-watchdog (mailnet only)
# Host-side TCP probes (SMTP/IMAP):
exec 3<>/dev/tcp/127.0.0.1/2525 && echo "synth-smtp greeting" && exec 3<&-
exec 3<>/dev/tcp/127.0.0.1/993  && echo "IMAPS"            && exec 3<&-
```

The watchdog publishes a snapshot at `/status` (inside mailnet):

```bash
docker compose exec synth-watchdog wget -qO- http://localhost:8081/status
```

---

## 12. Smoke test

```bash
./scripts/smoke.sh
```

The script brings the stack up (unless `--skip-up`), waits for
`synth-control`/`synth-webmail` health, logs in as the admin, creates
a `smoke.local` domain + `smoke@smoke.local` user, sends a message
to itself via the webmail backend, and asserts it appears in INBOX:

```
SMOKE PASS: control plane + webmail healthy, auth + provisioning + send/read work.
```

Use `--no-build` on repeat runs. Delete the smoke user afterwards
in the admin UI if you want a clean mailbox list.

---

## 13. Post‑setup hardening

Run through `docs/runbooks/security.md` after the first successful
`up`. The production‑relevant items:

* **TLS is fail‑closed** — `dovecot` (`ssl = required`), Postfix
  (SASL only over TLS), edge (HTTPS block only when certs exist).
  Never set `ssl = no` to "make it work" on a public host.
* **CORS** — leave `CORS_ORIGINS` empty for a same‑origin deploy.
  Only set it (CSV allow‑list) if you serve the UI from another
  origin. `*` is the legacy wildcard (audit S‑01) — do not use in
  production.
* **Client‑IP trust** — both `synth-control` and `synth-webmail`
  now prefer the nginx‑set `X-Real-IP` and fall back to the
  *right‑most* `X-Forwarded-For` entry (audit S‑02). A caller can no
  longer spoof `XFF: 8.8.8.8` to bypass per‑IP rate limits or
  pollute `login_attempts`. All traffic should traverse the edge.
* **NTFY alerts (audit S‑03)** — `alert` POSTs to the ntfy topic.
  The public `ntfy.sh` streams are readable by anyone who knows the
  topic. Use a **private/self‑hosted ntfy** in production, or a
  long unguessable topic — `synth-watchdog` warns at startup when
  the topic is < 12 chars. Set `NTFY_BASE_URL` to your private
  instance and `ALERT_WEBHOOK_URL` if you prefer a webhook.
* **`synth-smtp` injection surface (audit S‑04)** — any container on
  `mailnet` that can reach `postfix:25` can inject mail (the
  `builder_dangerous` relay carries no SASL by design; the network
  itself is the trust boundary). Firewall `postfix:25` to the
  mailnet interface at the host level. The connection/queue caps
  (semaphore + Redis queue) are already in place, so a flood
  backpressures instead of losing mail.
* **Docker socket** — leave `ENABLE_CONTAINER_STATS=0` (no socket
  mount) unless you specifically need the per‑container panel view
  on a trusted host (Plan §3).
* **Fail2Ban** — already runs on the host network
  (`network_mode: host` + `NET_ADMIN`), so bans hit the host
  firewall. Verify with
  `docker compose exec fail2ban fail2ban-client status`.
* **ACME renewal ownership** — HTTP‑01 only; keep host TCP 80 open
  and put a calendar reminder for expiry. Back up `certs/`.
  See `docs/runbooks/security.md` "fail2ban host enforcement" and
  `docs/runbooks/backup.md`.

---

## 14. Optional profiles

| Profile | Command | What it brings up |
|---|---|---|
| `acme` | `docker compose --profile acme run --rm acme` | One‑shot certbot (see §6). Not a long‑running service. |
| `backup` | `docker compose --profile backup run --rm backup` | `pg_dump` + Maildir tar + `sha256sum` + retention + optional rclone (`RCLONE_REMOTE`). |
| `backup-test` | `docker compose --profile backup-test run --rm backup-restore-test` | Restore dry‑run (checksums + `pg_restore --list` + `tar list`). Set `RESTORE_EXECUTE=true` for a real restore into a spare target. |
| `dav` | `docker compose --profile dav up -d` | Radicale CardDAV/CalDAV on `127.0.0.1:5232`, proxied at `/dav/` by `synth-webmail`. |
| `monitoring` | `docker compose --profile monitoring up -d` | Prometheus (`127.0.0.1:9090`) + Grafana (`127.0.0.1:3001`). See `docs/runbooks/monitoring.md`. |

---

## 15. Ongoing operations

* **Upgrades:** `docs/runbooks/upgrades.md` — pre‑upgrade backup,
  service update order (Postgres/Redis → control/webmail/watchdog →
  mail edge → UI/edge), and rollback.
* **Backups:** `docker compose --profile backup run --rm backup`
  (schedule with cron). Retention via `BACKUP_RETENTION_DAYS`
  (default 14). See `docs/runbooks/backup.md`.
* **Monitoring:** `synth-watchdog` polls Postgres/Redis/Postfix/
  Dovecot/Rspamd/ClamAV, dedupes alerts (2 min threshold, 30 min
  re‑alert), and dispatches to `ALERT_WEBHOOK_URL` / `NTFY_TOPIC`.
  Prometheus exposes `/metrics` on each Rust service.
* **Image distribution:** `./scripts/build-images.sh [--push]` builds
  all images (control, webmail, smtp, watchdog, UI) for
  multi‑arch publish; sysadmins can then `docker compose pull` with
  a pinned `REGISTRY`/`TAG`.
* **Host‑network mail logs:** confirm Postfix shows the real
  `client=` IP (evidence steps in
  `docs/runbooks/deliverability.md`).

---

## 16. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `docker compose config` fails with `SESSION_SECRET must be set` | Placeholder or missing secret in `.env` | Generate values per §3 and paste them in. |
| `docker compose up` fails with `POSTGRES_PASSWORD must be set` / `REDIS_PASSWORD must be set` | `.env` not in the repo root, or a compose interpolation error | `cp .env.example .env`, fill §2 table, `chmod 600 .env`. |
| `redis` not starting / apps bouncing | `REDIS_URL` password ≠ `REDIS_PASSWORD` | They must match exactly — see §2 H‑08 note. |
| Postfix/Dovecot `ssl` errors, certbot `403` | ACME challenge blocked | Check A/AAAA, host TCP 80, `synth-watchdog` health. See §6. |
| Webmail API 401 after login | `force_password_change=1` on the seed admin | Log in once, rotate, delete
  `/run/secrets/initial-admin-password`. See §9. |
| `POST /api/v1/auth/login` 429 | Rate limited | `RATE_LIMIT_PER_MIN` window (Redis fixed window, 60 s). Wait, or increase it for a bulk import. |
| SMTP `451 4.7.0 Unable to queue message` | Redis queue write failed | Check `docker compose logs synth-smtp` + Redis health. The failure is loud by design (audit: never a silent 250 after a lost enqueue). |
| Outbound mail queued indefinitely | Provider blocks port 25 | Coordinate with the host; check `postfix` logs for `connect: Permission denied`. |
| `client=` shows a bridge IP | Bridge‑NAT fallback | You are in the `MAIL_EDGE_NETWORK=""` fallback; the host‑network mode (`host`) is the fix. See §2. |
| ACME `rate limit exceeded` | You issued in `prod` while testing | Switch `ACME_STAGE=staging` until the pipeline is proven, then cut over (§6). |

---

## Checklist before going public

- [ ] `.env` filled, `chmod 600`, never committed
- [ ] DNS: A/AAAA, MX, SPF, DKIM, DMARC published; PTR requested
- [ ] Firewall: 25/80/443/465/587/993/995/4190 open on the host
- [ ] `ACME_STAGE=staging` pipeline tested → `prod` issued
- [ ] `docker compose config` passes
- [ ] `docker compose up -d --build` — all services healthy
- [ ] Admin password rotated; one‑time secret file deleted
- [ ] TOTP enrolled for admin
- [ ] `./scripts/smoke.sh` → `SMOKE PASS`
- [ ] `CORS_ORIGINS` left empty (same‑origin UI)
- [ ] NTFY: private instance or unguessable topic; webhook optional
- [ ] `ENABLE_CONTAINER_STATS=0`, no Docker socket mounted
- [ ] `synth-smtp` TLS enabled if external submission is needed (see `README.md` “synth‑smtp TLS”)
- [ ] Backup profile run once; `backup-restore-test` dry run passes
- [ ] Prometheus/Grafana profile up (optional) and Grafana password changed from the `.env` value
- [ ] Runbook pass: `docs/runbooks/security.md`
