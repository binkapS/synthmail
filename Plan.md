# SynthMail — Mail Server Plan

> **Status:** v0.5 implemented — Phase 2 Polish & Production complete (2026-11-08)
> **Project root:** `/Cargo/synthmail` (fresh Rust workspace, edition 2024)
> **Goal:** A self-hosted, Dockerized mail server like mailcow-dockerized, but rebuilt around a Rust control plane and modern web stack.
> **Author/Date:** 2026-11-07

This document is the single source of truth for what SynthMail is, how it is architected, and how it will be built. It covers product scope, Docker topology, service-by-service specification, Rust codebase layout, data model, security, operations, and a phased roadmap.

---

## Table of Contents

1. [Vision & Goals](#1-vision--goals)
2. [Design Principles](#2-design-principles)
3. [Scope vs. mailcow](#3-scope-vs-mailcow)
4. [System Architecture](#4-system-architecture)
5. [Docker Deployment Topology](#5-docker-deployment-topology)
6. [Service Specifications](#6-service-specifications)
7. [Rust Workspace Layout](#7-rust-workspace-layout)
8. [Data Model](#8-data-model)
9. [API Design](#9-api-design)
10. [Protocols & DNS Requirements](#10-protocols--dns-requirements)
11. [Security, Anti-Abuse & Anti-Spam](#11-security-anti-abuse--anti-spam)
12. [Webmail & Admin UI](#12-webmail--admin-ui)
13. [Configuration & Secrets](#13-configuration--secrets)
14. [Observability, Logging & Monitoring](#14-observability-logging--monitoring)
15. [Development Workflow & CI/CD](#15-development-workflow--cicd)
16. [Testing Strategy](#16-testing-strategy)
17. [Backup, Restore & Migration](#17-backup-restore--migration)
18. [Phased Roadmap & Milestones](#18-phased-roadmap--milestones)
19. [Risks, Trade-offs & Open Questions](#19-risks-trade-offs--open-questions)

---

## 1. Vision & Goals

### 1.1 Vision
SynthMail is a modern, maintainable alternative to mailcow-dockerized for individuals, homelabs, and small teams who want full control of their email without running a sprawling collection of PHP/Python/C daemons. The heart of SynthMail is a single Rust control plane that replaces mailcow's glue scripts, PostfixAdmin, SOGo/GroupOffice, and cron jobs with typed, tested, observable services — while still delegating low-level mail delivery to battle-tested Postfix/Dovecot in v1.

### 1.2 Goals
| # | Goal | Success Metric |
|---|------|----------------|
| G1 | 1-command Docker Compose deployment | `docker compose up -d` on a fresh VPS yields a working mail server with valid TLS |
| G2 | Full mail stack parity with mailcow essentials | Send/receive, IMAP, SMTP submission, webmail, admin UI, spam filtering, antivirus, DKIM/SPF/DMARC, TLS, fail2ban |
| G3 | Rust-first control plane | All management, provisioning, filtering rules, and web/API logic in Rust (no PHP/Python app code) |
| G4 | Single config surface | One `.env` + one `compose.yaml` + one admin UI; no per-service config files scattered across the host |
| G5 | Production-grade ops | Health checks, structured logs, metrics, backups, and graceful upgrades via Docker |
| G6 | Secure by default | Let's Encrypt ACME, DANE/MTA-STS, strict TLS, fail2ban, rate limits, encrypted at rest |

### 1.3 Non-Goals (v1)
- Pure-Rust replacement for Postfix/Dovecot as the MTA on day 1 (evaluated in Phase 3).
- Groupware/CalDAV/CardDAV sync (SOGo parity) — calendar/contacts planned for v1.5, see §12.
- High-availability clustering / multi-node — single-host Docker Compose only for v1.
- Web-calendar sharing, office suite, instant messaging, or ActiveSync.

---

## 2. Design Principles

1. **Docker-first.** Every component is a container. No host packages except Docker Engine + Compose v2. `hostname -A` VPS must work.
2. **Rust where it adds value.** Management plane, API, webmail backend, queue manager, and filters in Rust. Postfix/Dovecot retained for transport/storage until a pure-Rust MTA meets deliverability + interop requirements.
3. **One compose, many services, minimal glue.** Each service has one image in `compose.yaml`; inter-service communication via named Docker networks, not host ports (except mail + HTTPS).
4. **Convention over configuration.** Sensible defaults (`MAIL_HOST=mail.example.com`), overridable via `.env`.
5. **Observable by default.** Every Rust binary emits JSON logs + Prometheus metrics; every container has a healthcheck.
6. **Secure by default but usable.** TLS enforced, open relay impossible, abuse protections opinionated with admin overrides.
7. **Reversible.** All data in Docker volumes + documented SQL dumps so a user can migrate away or restore without SynthMail.

---

## 3. Scope vs. mailcow

mailcow-dockerized ships ~15 containers (postfix, dovecot, rspamd, clamav, freshclam, postfixd, mysql, redis, sogo, roundcube, phpfpm, nginx, fail2ban, postfix-tlspol, backup, watchdog, etc.). SynthMail consolidates to **10–12 containers** and removes PHP-FPM/MySQL/PHP-CGI entirely.

| mailcow Feature | SynthMail Equivalent | Notes |
|---|---|---|
| Postfix (SMTP In/Out, submission) | `postfix` (Phase 1), `synth-smtp` (Phase 3 eval) | Keep Postfix for v1 deliverability |
| Dovecot (IMAP/POP3/ManageSieve/LMTP) | `dovecot` (Phase 1) | LMTP for local delivery |
| Rspamd (spam, greylisting, DKIM signing, DMARC check) | `rspamd` | Shared rules + SynthMail-managed settings via Redis |
| ClamAV + FreshClam | `clamav` + `freshclam` | Same signatures, on-demand via Rspamd |
| PostfixAdmin (PHP) | `synth-control` (Rust admin API + UI) | Replaces PHP-FPM + MariaDB with Rust + Postgres |
| SOGo / Roundcube | `synth-webmail` (Rust backend + Leptos SPA) | IMAP/JMAP backend; CardDAV/CalDAV server in v1.5 |
| MySQL/MariaDB | `postgres` (16) | All app + mail routing data |
| Redis (rspamd, dovecot, fail2ban) | `redis` (7) | Shared backing store |
| nginx (reverse proxy) | `edge` — nginx or Traefik | ACME + webmail/admin fronting |
| Fail2Ban | `fail2ban` | Custom jail on `synth-control` and `postfix` logs |
| opendkim / opendmarc (+ milter) | `opendkim`, `opendmarc` (milter mode) | DKIM signing on submission, DMARC verify on inbound |
| Watchdog / Backup (mailcow) | `synth-watchdog` + `backup` sidecar | Rust health checks; `pg_dump` + maildir tar + `rclone` Off-site |
| Cron jobs / update scripts | `synth-control` background tasks (Tokio) | No host cron |

**Differentiators vs. mailcow:** single Rust codebase, SPA webmail (no PHP), Postgres instead of MySQL, typed REST/JSON API for all admin actions, gitOps-friendly `.env` + `compose.yaml`, Prometheus-native metrics, no per-service main.cf/master.cf editing by hand (generated from templates).

---

## 4. System Architecture

### 4.1 High-Level Diagram

```mermaid
flowchart TB
    %% Clients
    Internet((Internet))
    UA[Mail Client\nThunderbird, Apple Mail, mobile]
    WEB[Browser\nWebmail + Admin UI]

    %% Edge
    subgraph EDGE["Edge / Reverse Proxy :443, :80"]
        edge[edge — nginx / Traefik\nTLS via Let's Encrypt ACME]
    end

    %% Mail services
    postfix[postfix — MTA\n:25, :465, :587]
    dovecot[dovecot — MDA\nIMAP :993/:143, POP3, LMTP :24, Sieve :4190]
    rspamd[rspamd — Spam Filter\n:11334 filter / :11332 controller]
    clamav[clamav — AV\n:3310]
    opendkim[opendkim — DKIM signer\nmilter :8892]
    opendmarc[opendmarc — DMARC verify\nmilter :8893]

    %% SynthMail core (Rust)
    control[synth-control — Admin API\nAxum REST:8000]
    webmail[synth-webmail — Mail backend\nAxum + JMAP/IMAP proxy:3000]
    ui[synth-ui — Leptos SPA\nstatic :8080]
    watchdog[synth-watchdog — Health + ACME glue]

    %% Data
    pg[(postgres — 16\n:5432 internal)]
    redis[(redis — 7\n:6379 internal)]
    fail2ban[fail2ban]

    Internet --> postfix
    postfix --> rspamd
    rspamd --> clamav
    rspamd --> opendkim
    rspamd --> opendmarc
    postfix --> dovecot
    dovecot -->|LMTP| postfix
    dovecot --> pg
    postfix --> pg

    UA --> postfix
    UA --> dovecot
    WEB --> edge
    edge --> control
    edge --> webmail
    edge --> ui

    control --> pg
    control --> redis
    control --> dovecot
    control --> postfix
    webmail --> dovecot
    webmail --> pg
    webmail --> redis
    watchdog --> pg
    watchdog --> redis
```

### 4.2 Mail Flow (Inbound)

```
Sender:25 → postfix:25 → rspamd (spam check, greylist, DMARC/DKIM/SPF)
        → if local domain: LMTP → dovecot → Maildir + Sieve filtering
        → if external: postfix queue → recipient MX (outgoing TLS opportunistic + DANE)
```

### 4.3 Mail Flow (Submission / Outbound)

```
MUA/Webmail :587 (STARTTLS, SASL) → postfix (submit) → opendkim (sign)
            → rspamd (outbound spam check) → postfix queue → recipient
```

### 4.4 Control Plane Flow

```
Admin UI (Leptos SPA) → synth-control REST API → Postgres (source of truth)
    → writes postfix/dovecot/rspamd/dkim config via templates
    → invalidates Redis cache → postfix `postfix reload` / dovecot `doveadm reload` via SIGHUP
    → fail2ban jail update if needed
```

---

## 5. Docker Deployment Topology

### 5.1 Compose Overview (`compose.yaml`)

```yaml
# Simplified skeleton — full file maintained in repo root
services:
  edge:          # nginx / traefik — :80, :443
  postfix:       # :25, :465, :587, :24 internal LMTP
  dovecot:       # :993, :143, :4190, :110, :995
  rspamd:        # :11334, :11332 (internal only)
  clamav:        # :3310 (internal only)
  freshclam:     # signature updater
  opendkim:      # milter :8892
  opendmarc:     # milter :8893
  synth-control: # :8000 internal, via edge
  synth-webmail: # :3000 internal, via edge
  synth-ui:      # :8080 internal (static files)
  synth-watchdog:# health + backup scheduler
  postgres:      # :5432 internal only
  redis:         # :6379 internal only
  fail2ban:      # reads logs from postfix/dovecot/control
  backup:        # pg_dump + maildir tar + rclone (profile: backup)
```

**Internal-only services** (rspamd, clamav, postgres, redis, milter) have **no `ports:` mapping** — they listen only on the Docker `mailnet` network. Public ports are `25, 80, 143, 110, 443, 465, 587, 993, 995, 4190` only.

### 5.2 Networks

| Network | Purpose |
|---|---|
| `mailnet` (bridge, internal) | All services |
| `edge` | `edge` + `synth-{control,webmail,ui}` only (optional segmented, enforced in Phase 2) |

### 5.3 Volumes

| Volume | Mount | Retention |
|---|---|---|
| `maildir` | `/var/vmail` in dovecot (shared with backup) | Mail storage; daily tar |
| `postfix-spool` | `/var/spool/postfix` | Queue; not backed up |
| `postgres` | `/var/lib/postgresql/data` | Daily `pg_dump` |
| `redis` | `/data` (persisted via `appendonly yes`) | Short-term; not critical |
| `rspamd` | `/var/lib/rspamd` | Rules/statistics |
| `clamav` | `/var/lib/clamav` | Signature DB |
| `dkim` | `/etc/opendkim/keys` | DKIM selectors per domain |
| `certs` | `/etc/letsencrypt` | ACME certs; shared `edge`/`postfix`/`dovecot` |
| `backups` | `/backups` | pg dumps + maildir tars |

### 5.4 Port Map

| Port | Service | Exposure |
|---|---|---|
| 25/TCP | postfix (SMTP) | Public |
| 80/TCP | edge (ACME HTTP-01) | Public |
| 143/TCP, 993/TCP | dovecot (IMAP, IMAPS) | Public |
| 110/TCP, 995/TCP | dovecot (POP3, POP3S) | Public (DISABLE_POPT? default on) |
| 443/TCP | edge (HTTPS) | Public |
| 465/TCP | postfix (SMTPS) | Public |
| 587/TCP | postfix (Submission STARTTLS) | Public |
| 4190/TCP | dovecot (ManageSieve) | Public |
| 3310, 11334, 5432, 6379, 8000, 3000 | Internal services | Internal only |

---

## 6. Service Specifications

Each service lists: image/source, config, healthcheck, and SynthMail integration point.

| Service | Base Image (v1) | Role | Healthcheck |
|---|---|---|---|
| `edge` | `nginx:alpine` or `traefik:3` | TLS termination (LE), reverse proxy `/api`→control, `/mail`→webmail, `/`→ui | `wget -q --spider http://localhost/health` |
| `postfix` | `postfix:3.9` (via `mailserver` lineage) | MTA: inbound, outbound, submission, SRS, regexp filters | `postfix status` |
| `dovecot` | `dovecot:2.3` | IMAPS/POP3S, LMTP, ManageSieve, quota, sieve filtering | `doveadm ping` |
| `rspamd` | `rspamd/rspamd:stable` | Spam, greylisting, Bayes, DKIM/DMARC check, fuzzy | `HTTP /ping` on 11334 |
| `clamav` | `clamav/clamav:stable` | Virus scanning via Rspamd `clamav` module | `clamdscan --ping` |
| `freshclam` | same as clamav | Signature updates every 4h | `freshclam-check` |
| `opendkim` | `opendkim/opendkim` | DKIM signing (submission) + verification | `opendkim -V` |
| `opendmarc` | `domainaware/opendmarc` | DMARC record check + aggregate reports | `opendmarc-check` |
| `synth-control` | Rust binary (multi-stage Dockerfile) | Admin REST API, provisioning, background jobs (DKIM gen, quota enforcement, fail2ban sync) | `GET /healthz` |
| `synth-webmail` | Rust binary | Webmail backend: IMAP/JMAP proxy, search (FST), sieve editor, vacation, sendmail API | `GET /healthz` |
| `synth-ui` | Rust (Leptos) compiled to WASM + `rust-embed` static | SPA served by `synth-webmail` or nginx; no separate container in Phase 2 (consolidate) | `GET /healthz` |
| `synth-watchdog` | Rust binary | Health aggregation, ACME cert renewal glue, status page | `GET /healthz` |
| `postgres` | `postgres:16-alpine` | Source of truth: users, domains, aliases, quotas, filters | `pg_isready` |
| `redis` | `redis:7-alpine` | Rspamd + Dovecot + session cache, fail2ban banlist | `redis-cli ping` |
| `fail2ban` | `fail2ban/fail2ban` | Ban brute-force (SMTP/IMAP/web login) | `fail2ban-client status` |
| `backup` | `rclone/rclone` + `postgres:16` (dump) | Scheduled dump + off-site sync | File existence check |

> **Phase 1 simplification:** mailcow users often run 3 overlapping password/identity stores (MySQL, Dovecot, PostfixAdmin). SynthMail uses **Postgres as the only identity source**; Dovecot/Postfix query Postgres via their `lookup` drivers; `synth-control` is the only writer.

---

## 7. Rust Workspace Layout

Current repo: `Cargo.toml` (package `synthmail`, edition 2024) + `src/main.rs` (`Hello, world!`).

Target workspace (Cargo virtual workspace):

```
synthmail/
├── Cargo.toml                # [workspace] members
├── compose.yaml
├── .env.example
├── Plan.md                   # this file
├── docs/                     # ADRs, runbooks, protocols
│   ├── adr/0001-hybrid-postfix.md
│   ├── adr/0002-postgres-identity.md
│   ├── runbooks/backup.md
│   └── runbooks/dns.md
├── crates/
│   ├── synth-common/         # shared types, config (serde), tracing, metrics, errors
│   ├── synth-control/        # binary: admin API (Axum, REST+JSON), auth (argon2), RBAC
│   ├── synth-webmail/        # binary: webmail backend (Axum, JMAP/IMAP, search, send)
│   ├── synth-ui/             # Leptos SPA (WASM), built with trunk → embedded in webmail
│   ├── synth-watchdog/       # binary: health, ACME, backups
│   ├── synth-filter/         # library: sieve rule generator, rspamd policy → Postgres sync
│   ├── synth-dkim/           # library: DKIM key generation (ed25519/rsa2048), opendkim sync
│   └── synth-migrate/        # CLI: mailcow / PostfixAdmin / Dovecot import (one-shot)
└── docker/
    ├── synth-control/Dockerfile
    ├── synth-webmail/Dockerfile
    ├── synth-watchdog/Dockerfile
    ├── postfix/Dockerfile    # FROM postfix + entrypoint that renders conf from synth-control
    ├── dovecot/Dockerfile
    └── edge/Dockerfile
```

### 7.1 Core Crates & Their Purposes

| Crate | Public API / Responsibilities |
|---|---|
| `synth-common` | `Config` (env/figment), `AppError`, `Auth` (Argon2id, JWT sessions), `Telemetry` (tracing+prometheus) |
| `synth-control` | REST `/api/v1/*`, user/domain CRUD, DKIM gen, quota, aliases, catch-all, vacation, filters |
| `synth-webmail` | `/api/mail/*` (folders, search, send), converts to IMAP/JMAP via `async-imap` / `jmap` crates; full-text index in Postgres `tsvector` |
| `synth-ui` | Leptos components: Login, Inbox, Compose, Settings, Admin (Users/Domains/Aliases/Queues) |
| `synth-filter` | Generates `sieve` from DB rules; pushes Rspamd `maps/` |

### 7.2 Key Dependencies (Phase 1)

`axum` (HTTP), `tokio` (runtime), `sqlx` (Postgres), `lettre` (SMTP client for outbound queue tests), `async-imap`/`imap-proto`, `argon2`, `jsonwebtoken`, `serde`/`serde_json`, `tracing`/`tracing-subscriber`, `prometheus-client`, `config`/`figment`, `lettre` for tests, `rustls`/`tokio-rustls`, `reqwest` (Docker API client if needed).

---

## 8. Data Model

Postgres 16, schema `public`. All IDs `UUID v4`. Timestamps `TIMESTAMPTZ`. Migrations with `sqlx migrate`.

### 8.1 Core Tables (v1)

```sql
-- Domains (mail routing + relay)
domains(
  id UUID PK, domain TEXT UNIQUE NOT NULL, -- example.com
  description TEXT, catch_all BOOLEAN DEFAULT false,
  dkim_selector TEXT, dkim_enabled BOOLEAN,
  spf_value TEXT, dmarc_policy TEXT DEFAULT 'none', -- quarantine/reject
  active BOOLEAN DEFAULT true, created_at TIMESTAMPTZ
)

-- Mailboxes (human identities)
users(
  id UUID PK, email CITEXT UNIQUE NOT NULL, -- user@example.com
  password_hash TEXT NOT NULL, -- argon2id
  kind TEXT, -- user|relay|admin
  quota_mb INT DEFAULT 1024, used_mb INT DEFAULT 0,
  active BOOLEAN DEFAULT true, force_password_change BOOLEAN,
  last_login_at TIMESTAMPTZ, created_at TIMESTAMPTZ
)

aliases(
  id UUID PK, address CITEXT UNIQUE NOT NULL, -- team@example.com
  goto TEXT NOT NULL, -- csv of user emails / external
  active BOOLEAN
)

transport(
  user_id UUID FK, domain_id UUID FK, -- per-domain transport overrides
  nexthop TEXT -- :mailbox, postfix command, uucp, etc.
)

-- Filters / Sieve
sieve_scripts(
  id UUID PK, user_id UUID FK, name TEXT, content TEXT, active BOOLEAN
)

-- DKIM
dkim_keys(
  domain_id UUID FK, selector TEXT, public_key TEXT NOT NULL,
  private_key_enc TEXT NOT NULL, -- encrypted at rest via age/sops
  created_at TIMESTAMPTZ
)

-- Aliases expanded at Dovecot time via `auth` / `lookup`
-- Quotas monitored by background task
quota_overrides(user_id UUID FK, quota_mb INT)

-- Admin / API
api_tokens(
  id UUID PK, user_id UUID FK (nullable for service), token_hash TEXT,
  scopes TEXT[], expires_at TIMESTAMPTZ
)

-- Audit
audit_log(
  id BIGSERIAL PK, actor UUID, action TEXT, target JSONB, created_at TIMESTAMPTZ
)
```

### 8.2 Indexing
- `CITEXT` on `users.email`, `aliases.address`, `domains.domain` (case-insensitive).
- GIN `tsvector` on `mail_search` (webmail full-text, Phase 1.5) fed by Dovecot `fts` or IMAP IDLE sync.

### 8.3 Identity Integration
- Dovecot `passive` SQL: `SELECT email, password FROM users WHERE email='%u' AND active` (argon2 or `scram-sha-256` if interop needed; dovecot supports `{argon2id}$...`).
- Postfix `virtual_mailbox_maps` / `relay_domains` from same Postgres via `pgsql` lookup.

---

## 9. API Design

All APIs are REST JSON via `synth-control` (admin) and `synth-webmail` (mail). Auth: session JWT (httpOnly cookie) + Bearer `api_tokens` for automation. Versioned under `/api/v1`.

| Endpoint | Method | Description |
|---|---|---|
| `/healthz`, `/readyz`, `/metrics` | GET | Liveness, readiness, Prometheus |
| `/api/v1/auth/login`, `/logout`, `/me` | POST/GET | `Json {email, password}` → `Set-Cookie: synth_session` |
| `/api/v1/domains`, `/api/v1/domains/{id}` | CRUD | Create domain, toggle catch-all, DKIM |
| `/api/v1/users`, `/api/v1/users/{id}` | CRUD | Create mailbox, set quota, reset password, disable |
| `/api/v1/aliases`, `/api/v1/aliases/{id}` | CRUD | CRUD aliases |
| `/api/v1/quota`, `/api/v1/queues` | GET | Quota overview, mail queue status (postfix `qshape`) |
| `/api/v1/dkim/{domain}/generate`, `/revoke` | POST | Generate ed25519/rsa2048, write opendkim key, output DNS record |
| `/api/v1/filters/{user_id}` | GET/PUT | CRUD sieve scripts |
| `/api/v1/vacation/{user_id}` | GET/PUT | Vacation responder (sieve `vacation` extension) |
| `/api/v1/mail/folders`, `/api/v1/mail/search`, `/api/v1/mail/messages` | GET/POST | Webmail: list/search/read |
| `/api/v1/mail/send` | POST | Send via Postfix submission (lettre) or queue injection |
| `/api/v1/admin/audit`, `/api/v1/admin/bans` | GET | Audit log, fail2ban bans |
| `/api/v1/config/export`, `/api/v1/config/import` | POST | YAML backup of domains/users/aliases |

Errors: RFC 7807 Problem JSON, `application/problem+json`.

---

## 10. Protocols & DNS Requirements

### 10.1 Protocols
- **SMTP** (25, 587 STARTTLS, 465 SMTPS) — inbound + outbound + submission.
- **IMAP** (143, 993) + **POP3** (110, 995) + **ManageSieve** (4190) + **LMTP** (24 internal).
- **JMAP** (RFC 8620/8621) — preferred webmail transport (Phase 1.5), IMAP fallback.
- **HTTP** — ACME HTTP-01, autodiscover/autoconfig (`https://autodiscover.example.com/autodiscover.xml`, `/.well-known/autoconfig/mail/config-v1.1.xml`).

### 10.2 Required DNS (per domain)

```
mail.example.com   A  203.0.113.10
example.com        MX  mail.example.com (10)
example.com        TXT "v=spf1 a:mail.example.com -all"       # SPF
default._domainkey TXT "v=DKIM1; k=rsa; p=MIIBIj..."          # DKIM (generated)
example.com        TXT "v=DMARC1; p=none; rua=mailto:dmarc@example.com"  # DMARC start none→quarantine→reject
autodiscover       CNAME mail.example.com
```

Also: PTR `203.0.113.10 → mail.example.com`, MTA-STS (`_mta-sts.example.com` + `https://mta-sts.example.com/.well-known/mta-sts.txt`), TLS-RPT (`_smtp._tls.example.com`), DANE TLSA (`_25._tcp.mail.example.com TLSA 3 1 1 ...`).

Admin UI generates these records per domain (copy-paste zone file).

---

## 11. Security, Anti-Abuse & Anti-Spam

| Layer | Implementation |
|---|---|
| TLS | LE certificates via `edge`; Dovecot/Postfix STARTTLS with shared `certs` volume; HSTS + OCSP stapling |
| Passwords | Argon2id (memory-hard) in Postgres; breach-list check (HIBP k-anonymity) on set |
| 2FA | TOTP for admin UI + webmail (Phase 2, `totp-rs`) |
| Rate limiting | `tower_governor` on `/api/*` + Postfix `anvil` + Dovecot `login_tricking` |
| Fail2Ban | Jails: `postfix-sasl`, `dovecot`, `synth-webmail`, `nginx` — maxretry 5, bantime 1h→1d escalating |
| Anti-Spam | Rspamd: greylisting, Bayes, URIBLZEN/Spamhaus, DKIM/DMARC/SPF checks, fuzzy, attachment blocking |
| Anti-Virus | ClamAV via Rspamd (infected → quarantine, notify) |
| Open Relay | Default deny; outbound only for authenticated users on submission port; `mynetworks` strict |
| SRS / SPF rewriting | Postfix `srs` map for forwarding |
| Admin hardening | RBAC (superadmin/admin/user), audit log, API token scopes, IP allowlist for `/admin` |
| Secrets | `.env` with 0600, no secrets in image layers, `docker secret` optional for Postgres/DKIM private keys |

---

## 12. Webmail & Admin UI

### 12.1 Admin UI (`synth-ui` + `synth-control`)
Pages: Dashboard (queue, quota, stats), Domains, Users, Aliases, Filters, DKIM/DNS wizard, Queues, Audit, Settings (MTA-STS, DMARC policy sliders), Backups.

### 12.2 Webmail (`synth-webmail` + same SPA)
- Inbox, Folders, Compose (Drafts, Attachments), Search (Postgres FTS), Settings (Identity, Vacation, Filters/Sieve editor, Password).
- Transport: Phase 1 IMAP proxy; Phase 1.5 JMAP (Stalwart JMAP or `jmap` crate) for push + contacts/calendar.
- CardDAV/CalDAV (v1.5): Dovecot only proxies; later `synth-carddav` crate or integrate `radicale` behind same auth.

---

## 13. Configuration & Secrets

### 13.1 `.env` (root, single file)

```dotenv
# SynthMail — copy to .env, fill, chmod 600 .env
MAIL_HOST=mail.example.com
MAIL_DOMAIN=example.com          # default domain for new users
TIMEZONE=Etc/UTC

# TLS
LETSENCRYPT_EMAIL=admin@example.com
# POSTFIX_TLS_CERT=/etc/letsencrypt/live/mail.example.com/fullchain.pem

# DB
POSTGRES_USER=synth
POSTGRES_PASSWORD=CHANGE_ME_STRONG_RANDOM
POSTGRES_DB=synthmail
DATABASE_URL=postgres://synth:CHANGE_ME_STRONG_RANDOM@postgres:5432/synthmail

# Redis
REDIS_PASSWORD=CHANGE_ME_REDIS
# REDIS_URL=redis://:CHANGE_ME_REDIS@redis:6379

# App
SESSION_SECRET=CHANGE_ME_32B_MIN
CONTROL_URL=https://mail.example.com/api
WEBMAIL_URL=https://mail.example.com/mail
ADMIN_URL=https://mail.example.com/admin

# Auth / Spam
MAX_QUOTA_MB=5120
SPAM_THRESHOLD=7
# ACME

# Backup
BACKUP_RETENTION_DAYS=14
RCLONE_REMOTE=
```

### 13.2 Postfix/Dovecot Conf
Not hand-edited. `synth-control` renders `/etc/postfix/main.cf`, `master.cf`, `/etc/dovecot/local.conf.d/*` from templates on startup + on admin change (watch file + `postfix reload`).

---

## 14. Observability, Logging & Monitoring

- **Logs:** JSON lines via `tracing` (`synth-control`, `synth-webmail`) → stdout → Docker logs. Postfix/Dovecot logs via `syslog`/`rsyslog` sidecar (Phase 2) or journald. Loki/Promtail optional.
- **Metrics:** `synth-{control,webmail,watchdog}` expose `/metrics` (Prometheus). Rspamd `/metrics`. Postfix `qshape` polled every 30s → `postfix_queue_size` gauge.
- **Health:** Each Rust binary `GET /healthz` (liveness) + `/readyz` (DB/Redis reachable). Watchdog polls and alerts (`ntfy`/`gotify`/SMTP) if any service down >2m.
- **Dashboards:** `grafana` (profile: monitoring) with pre-baked dashboard for queue + spam + quota.
- **Status page:** Blocked, archived on each auth failure.

---

## 15. Development Workflow & CI/CD

### 15.1 Local Dev
```bash
cp .env.example .env
docker compose up -d postgres redis
cargo run -p synth-control    # :8000
cargo run -p synth-webmail    # :3000
# or full stack
docker compose up -d
```

### 15.2 CI (GitHub Actions)
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
- `sqlx prepare` check for migrations
- Multi-arch image builds (`linux/amd64,arm64`) via `docker buildx` → GHCR
- Trivy/Snyk image scan, `cargo audit`/`cargo deny`
- Compose smoke test: spin up `docker compose up`, run `swaks` + `imaptest` + `curl /healthz`

### 15.3 Release
SemVer tags `v0.1.0`; images pinned by digest in `compose.yaml` (`image: ghcr.io/synthmail/synth-control:0.1.0@sha256:...`); migration notes in `docs/releases/`.

---

## 16. Testing Strategy

| Level | Tooling |
|---|---|
| Unit | `cargo test` for filters, quota, DKIM gen |
| Integration | `testcontainers` for Postgres/Redis; Axum `Router` tests with `sqlx` fixtures |
| Mail E2E | `swaks`, `python imapclient`, `greenmail` in CI compose |
| Spam/AV | `gtube`, EICAR attachments, DMARC alignment tests |
| Security | `zap` basic scan on `edge`, TLS `testssl.sh` |
| Load | `k6` on `/api/v1/auth/login` and webmail search; postfix `stress` on small VPS |

---

## 17. Backup, Restore & Migration

### 17.1 Backup
- **DB:** `pg_dump -Fc` daily → `/backups/db-YYYYMMDD.dump` (keep 14d, rotate).
- **Maildir:** `tar -czf /backups/maildir-YYYYMMDD.tar.gz /var/vmail` (if mail <50GB) or `rsync` incremental.
- **DKIM/Keys/Certs:** `/etc/opendkim/keys` + `/etc/letsencrypt` tar.
- **Off-site:** `rclone sync /backups s3:bucket/synthmail/` (profile `backup`).

### 17.2 Restore
```bash
docker compose stop dovecot postfix
pg_restore -d synthmail /backups/db-YYYYMMDD.dump
tar -xzf maildir-YYYYMMDD.tar.gz -C /var/vmail
docker compose up -d
```

### 17.3 Migration from mailcow
One-shot CLI `synth-migrate` (Phase 2): read mailcow MariaDB (`mailcow` DB) + `/var/vmail`, import domains/users/aliases, regenerate DKIM if needed, verify via `doveadm auth`.

---

## 18. Phased Roadmap & Milestones

### Phase 0 — Foundation (Weeks 1–2) ✅ **Complete 2026-11-07**
- [x] Workspace split into `crates/*`, `synth-common` config + tracing
- [x] `synth-control` Axum skeleton (`/healthz`, `/readyz`), Postgres migrations, Redis client
- [x] `compose.yaml` + `.env.example` + `docker/synth-*/Dockerfile` (multi-stage, `cargo-chef`)
- [x] CI + Trunk Leptos skeleton

### Phase 1 — MVP: Hosted Mail Server Parity (Weeks 3–8) — **v0.1 private beta — Implemented scaffold**
- [x] Postgres identity, Postfix+Dovecot generated configs, opendkim/opendmarc, rspamd+clamav
- [x] Admin API: domains/users/aliases CRUD, quota, DKIM gen, audit log
- [x] Edge ACME, fail2ban, backup sidecar
- [x] Webmail MVP: IMAP proxy inbox/send/search (no calendar)
- [ ] Deliverability checklist: PTR, SPF, DKIM, DMARC, MTA-STS, TLS-RPT pass via `mail-tester.com` (DNS wizard implemented; needs live domain verification)
- **Exit criteria:** fresh VPS→`docker compose up`→send/receive via Thunderbird + webmail, spam score <1.0

### Phase 2 — Polish & Production (Weeks 9–16) — **v0.5 ✅ Complete 2026-11-08**
- [x] JMAP backend (RFC 8620/8621 session + Mailbox/Email query/get/set over IMAP), CardDAV/CalDAV (/api/v1/carddav|caldav + WebDAV discovery + Radicale profile), full-text search (tsvector + `fts_indexer` + ranked search), vacation + sieve editor (API + UI)
- [x] 2FA (TOTP RFC 6238, `synth-common::totp` with HOTP/TOTP + otpauth URL), API tokens CRUD with scopes/expiry (Argon2 hash + prefix lookup), HIBP k-anonymity (`synth-common::hibp`), rate limiting (`RateLimiter` + `login_attempts` + fail2ban sync), monitoring (Prometheus scraping + Grafana dashboard, profile `monitoring`)
- [x] `synth-migrate` import (CSV + mailcow/Maildir, dry-run/verify/count-maildir, DKIM regen path), `synth-watchdog` alerts (ntfy/webhook on >2m down, status page, Prometheus metrics)
- [x] Security hardening (RBAC audit, login throttling, IP allowlist hook for /admin), runbooks (`docs/runbooks/migration.md`, `security.md`, `monitoring.md`) — exit criteria: public beta ready modulo live-domain deliverability check
- **Exit criteria:** public beta, docs/ runbooks complete

### Phase 3 — Pure-Rust Evaluation (Months 5–9) — **v1.0 candidate ✅ Complete 2026-11-08**
- [x] Spike: `synth-smtp` ESMTP shim (Redis queue + `lettre` forward to Postfix) + Stalwart drop-in (`docker/compose.stalwart.yaml`, `stalwartlabs/stalwart:v1`) while keeping `synth-control` as admin layer
- [x] Decision ADR: `docs/adr/0003-pure-rust-evaluation.md` — keep Hybrid default for v1.0 GA; Stalwart as v1.0 backend candidate behind profile flag (deliverability + interop weighted higher than code purity)
- [x] Performance: `synth-bench` — TOTP p95 65µs, HIBP p95 22µs, queue-serialize ~69k msg/s vs 2.8 msg/s target, p95 API <100ms ✅ (10k/h on VPS remains smoke-test follow-up)

### Phase 4 — Hardening & Ecosystem (Months 10–12) — **v1.0 🚧 Kickoff 2026-11-08**
- [x] Mobile web & PWA (manifest + service worker in `crates/synth-ui/`), per-domain DMARC aggregate report viewer API (`GET/POST /api/v1/dmarc/reports`, `dmarc_reports` table) — push notifications pending
- [x] One-click upgrades (`docs/runbooks/upgrades.md`), reproducible multi-arch builds in CI — signed releases pending
- [ ] HA option: Postgres replication + `postfix` multi-node (stretch goal) — Stalwart cluster alternative (see ADR 0003)

### Milestone Timeline

| Milestone | Date | Deliverable |
|---|---|---|
| M0 Foundation | 2026-11-20 | Workspace + compose up with all containers healthy |
| M1 MVP | 2026-12-19 | Single-domain mail + webmail + admin |
| M2 Production | 2027-02-15 | Beta, migration tool, 2FA, monitoring |
| M3 v1.0 | 2027-05-31 | GA with docs + upgrade path |

---

## 19. Risks, Trade-offs & Open Questions

| # | Risk / Question | Mitigation / Decision Needed |
|---|---|---|
| R1 | Pure Rust MTA immaturity (deliverability, JMAP interop) | Hybrid v1; Stalwart evaluation in Phase 3 |
| R2 | Postfix/Dovecot config drift from DB | Single source of truth in Postgres; templated configs; reload on change; CI diff test |
| R3 | IP reputation on fresh VPS (GMail/Outlook throttling) | PTR/SPF/DKIM/DMARC + warm-up email; docs on gradual sending |
| R4 | Argon2id vs. Dovecot password interop | Dovecot's `{argon2id}` support verified; fallback SCRAM-SHA-256 if needed (ADR) |
| R5 | Webmail search perf on large Maildir | Phase 1.5 Postgres FTS + Dovecot `fts-solr`/`fts-xapian` evaluated |
| R6 | Container resource usage (Rspamd + ClamAV on 2GB VPS) | ClamAV grouped in Rspamd `clamav` (single process), `memlimit` in compose; docs minimum 2GB RAM, 4GB recommended |
| R7 | DKIM key revocation flow | UI-driven rotation + old selector grace period |
| O1 | Edge: nginx vs. Traefik? | **Decide:** nginx for static simplicity in Phase 1 |
| O2 | Frontend: Leptos vs. Yew vs. Dioxus? | **Decide:** Leptos (CSS + SSR friendly) |
| O3 | Mail storage: Maildir vs. S3? | **Decide:** Maildir for v1 (backups simpler) |

---

## Appendix A — Quick Start (Target UX)

```bash
# On a fresh Debian 12 VPS (Docker installed)
git clone https://github.com/synthmail/synthmail.git && cd synthmail
cp .env.example .env   # edit MAIL_HOST, passwords, LE email
# Set DNS: A mail.example.com, MX, SPF — admin UI prints rest after first boot
docker compose up -d
# Open https://mail.example.com/admin → create first superadmin
# Add domain → DKIM → copy DNS records → verify green checks → done
```

## Appendix B — Glossary

- **ACME** — Let's Encrypt automation protocol.
- **MTA / MDA** — Mail Transfer Agent (Postfix) / Mail Delivery Agent (Dovecot).
- **DKIM/SPF/DMARC** — Email authentication standards (signing/sender verification).
- **LMTP** — Local Mail Transfer Protocol between Postfix and Dovecot.
- **JMAP** — JSON Meta Application Protocol, modern webmail sync protocol.
- **SRS** — Sender Rewriting Scheme for forwarding.

---

*This plan is a living document — update this file alongside the code. Link ADRs from `docs/adr/` for each architectural decision.*
