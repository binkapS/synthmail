# SynthMail UI — design + wiring contract

## Architecture

Leptos 0.8 CSR (wasm32). No external component libraries (offline constraint).
Files:
- `crates/synth-ui/src/lib.rs` — router + shell + theme (existing, Agent C owns)
- `crates/synth-ui/src/api.rs` — typed API client (Agent A creates)
- `crates/synth-ui/src/pages.rs` — all pages (Agent B creates)
- `crates/synth-ui/style.css` — design system (Agent C rewrites)

## API contract (already-implemented backend endpoints)

Control plane (base `/api/v1`, proxy via edge → synth-control):
- POST `/auth/login` `{email,password}` → `{token, email, kind}`; then GET `/auth/me` → `{id,email,kind}`
- GET/POST `/domains`; GET/PUT/DELETE `/domains/{id}`; GET `/domains/{id}/dns`; GET `/domains/{id}/verify`
- GET/POST `/users`; GET/PUT/DELETE `/users/{id}`; POST `/users/{id}/password` `{password}`
- GET/POST `/aliases`; GET/PUT/DELETE `/aliases/{id}`
- GET `/quota` → `{quota_mb, used_mb, users:[]}`; GET `/queues`
- POST `/dkim/{domain_id}/generate` → `{domain, selector, key_type, public_key, dns_record, full_dns, keytable_entry, synced_keys, opendkim_note}`; GET `/dkim/{domain_id}`
- GET/PUT `/filters/{user_id}`; GET/PUT `/vacation/{user_id}`
- GET `/admin/audit` (rows), GET `/admin/bans`
- GET `/config/export`; POST `/config/import` (body: full config JSON; users skipped by design)
- GET `/dmarc/reports` (domain, report_id, report_type), GET `/push/subscribe`, POST `/push/subscribe`, DELETE `/push/subscribe/{id}`
- TOTP: POST `/auth/totp/enroll`, POST `/auth/totp/verify` `{code}`, POST `/auth/totp/disable`
- API tokens: GET/POST `/api-tokens`, DELETE `/api-tokens/{id}`

Webmail (base also `/api/v1` but talks to synth-webmail — same edge prefix `/api/v1/mail`, `/api/v1/auth/login` collides! Deploys route `/api/v1/auth/login` + `/api/v1/auth/me` to CONTROL; webmail login at `/api/v1/mail/...` uses webmail for mail only. For mail, call `/api/v1/mail/...` (folders/messages/search/send/drafts). Webmail login POST `/api/v1/auth/login` is control's too (both issue compatible JWTs). UI uses CONTROL login only, then webmail endpoints with the same token (shared SESSION_SECRET, token_version-checked — works today).
- GET `/mail/folders` → `{folders:[..]}`
- GET `/mail/messages?folder=&limit=` → `{messages:[{uid,subject,from,date,preview,seen,message_id}]}`
- GET `/mail/messages/{uid}` → `{uid,subject,from,date,seen,message_id,raw}`
- GET `/mail/search?q=` → `{query, results:[{subject,folder}]}`
- POST `/mail/send` `{from,to,subject,body}` → `{status:"sent"}`
- GET `/mail/drafts` → `{drafts:[{uid,subject,from,date,preview}]}`; POST `/mail/drafts` `{uid}` (delete → moves to Trash)
- JMAP: GET `/jmap/session`, POST `/jmap {using,method_calls}` — implemented methods: Mailbox/query+get, Email/query+get (filter.inMailbox-id mapping), Email/set create/update/destroy; legacy: other methods return errors

## Pages

| Route | Page | Contents |
|---|---|---|
| `/` | Dashboard | stats cards (domains, users, quota, queue), recent audit, quick actions |
| `/login` | Login | email + password + optional TOTP code field; token stored in localStorage `synthmail_token`; redirects to /admin or /mail |
| `/admin` | Admin layout w/ sidebar | nested nav: Overview, Domains, Users, Aliases, Quota, Queues, DKIM, Filters, Vacation, DMARC, Audit, Bans, Tokens, Config |
| `/admin/domains` | Domains | table + create form + row actions (DNS, verify, DKIM generate, delete) |
| `/admin/users` | Users | table + create + edit + reset password + delete |
| `/admin/aliases` | Aliases | table + create + delete |
| `/admin/quota` | Quota | server widget + per-user bars |
| `/admin/queues` | Queues | deferred/active/hold counts |
| `/admin/dkim` | DKIM | select domain → generate → show DNS TXT with copy button |
| `/admin/filters` | Filters | per-user sieve editor (textarea + save) |
| `/admin/vacation` | Vacation | per-user subject/body/dates form |
| `/admin/dmarc` | DMARC reports | table (admin only) |
| `/admin/audit` | Audit log | table |
| `/admin/bans` | Bans | table |
| `/admin/tokens` | API tokens | list + create (name/scopes/expires) + revoke; plaintext shown once |
| `/admin/config` | Config | export (download JSON) + import (file/textarea) |
| `/mail` | Webmail | 3-pane: folders, message list, message view; compose (to/subject/body) sends |
| `/account` | Account | profile (me), change password, TOTP enable/verify/disable, push subscriptions |

All pages require a token (redirect `/login` when absent). Admin pages additionally check `kind` admin/superadmin (from /auth/me; else redirect with notice).
