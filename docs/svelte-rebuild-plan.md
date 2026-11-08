# SynthMail UI — Rebuild Plan (Svelte 5 + Tailwind CSS v4)

> Design system of record: **CyberSlate Premium Dark** (author: Binkap S).
> This plan replaces the existing Leptos/WASM `crates/synth-ui` front-end
> completely. All existing CSS tokens, Leptos component styling and the
> light/dark theme toggle are **discarded**; the new UI uses *only* the
> palette, component classes, utilities and layouts defined in the design
> specification below.

## 0. Goals and non-goals

- Ship an equivalent (and redesigned) admin + webmail SPA under Svelte 5 +
  Tailwind CSS v4+, served exactly as today behind `edge /admin/` and the
  `synth-ui` service on `127.0.0.1:3002`.
- **Do not** change the backend contract, routes, data shapes, or edge proxy
  rules. `synth-control` (`:8000`) and `synth-webmail` (`:3000`) stay as-is.
- Dark-only. No light variant and no theme toggle — only the CyberSlate
  dark palette is defined.
- Preserve auth behavior (JWT in `localStorage`, `Authorization: Bearer`,
  `GET /auth/me` gating) and the PWA shell.

## 1. Target stack

| Concern           | Choice                                                     |
| ----------------- | ---------------------------------------------------------- |
| Framework         | Svelte 5 with **runes** (`$state`, `$derived`, `$effect`)  |
| Routing           | SvelteKit `adapter-static`, `ssr = false` in root layout     |
| Styling           | Tailwind CSS v4+ (CSS-first `@theme`)                       |
| Language          | TypeScript (strict)                                         |
| HTTP client       | native `fetch` + thin wrapper (replaces `gloo-net`)         |
| State             | Svelte runes + small `auth` store (module-level)            |
| Forms             | native + superforms optional (manual validation first pass) |
| Testing           | Vitest + `@testing-library/svelte`; Playwright E2E          |
| Build             | Vite (via SvelteKit)                                        |
| Serving           | nginx (existing) with SPA fallback + proxies                |
| Package manager   | npm (lockfile committed)                                    |

SvelteKit is chosen for file-based routes, layouts and adapter-static output.
`ssr = false` keeps *all* data fetching client-side, matching today's CSR
Leptos app and avoiding auth-token-on-server complexities.

## 2. Repository layout (new)

```
ui/                                # new top-level SPA (replaces crates/synth-ui)
├── package.json
├── svelte.config.js
├── vite.config.ts
├── tailwind.css                   # @import "tailwindcss" + @theme tokens
├── src/
│   ├── app.html
│   ├── routes/
│   │   ├── +layout.svelte         # app shell (TopBar, BottomNav, auth gate)
│   │   ├── +layout.ts             # export const ssr = false;
│   │   ├── +page.svelte           # Home / dashboard
│   │   ├── login/+page.svelte
│   │   ├── admin/+page.svelte     # AdminHome shell + tabs
│   │   ├── mail/+page.svelte      # WebmailHome (3-column)
│   │   └── +error.svelte         # NotFound
│   ├── lib/
│   │   ├── api/                   # port of crates/synth-ui/src/api.rs
│   │   │   ├── client.ts         # request_text / request_json + token storage
│   │   │   ├── models.ts         # TS interfaces (LoginResp, User, ...)
│   │   │   ├── pure.ts           # api_url, encodeQueryComponent, messagesUrl, ...
│   │   │   └── index.ts          # all endpoint functions + compat shims
│   │   ├── stores/
│   │   │   └── auth.svelte.ts
│   │   ├── ui/                   # design-system primitives
│   │   │   ├── Button.svelte
│   │   │   ├── Input.svelte
│   │   │   ├── Textarea.svelte
│   │   │   ├── Select.svelte
│   │   │   ├── Card.svelte
│   │   │   ├── Badge.svelte
│   │   │   ├── Spinner.svelte
│   │   │   ├── EmptyState.svelte
│   │   │   ├── ErrorCard.svelte
│   │   │   ├── PendingButton.svelte
│   │   │   ├── ConfirmButton.svelte
│   │   │   ├── Field.svelte
│   │   │   └── index.ts
│   │   └── components/           # higher-level (TopBar, BottomNav, ...)
│   ├── static/
│   │   ├── manifest.webmanifest
│   │   ├── icon-192.png
│   │   ├── icon-512.png
│   │   └── sw.js
│   └── tests/
├── Dockerfile                     # node build stage
└── README.md
```

`crates/synth-ui` is kept untouched until parity; removed from `Cargo.toml`
members in the final cutover phase.

## 3. Tailwind v4 design tokens (`tailwind.css`)

```css
@import "tailwindcss";

@theme {
  --color-appBg: #090d16;
  --color-surface: #0f172a;
  --color-elevated: #1e293b;
  --color-divider: rgba(30, 41, 59, 0.6);
  --color-focusRing: #38bdf8;
  --color-text-primary: #f8fafc;
  --color-text-muted: #94a3b8;
  --color-text-dim: #64748b;
  --color-brand: #6366f1;
  --color-success: #10b981;
  --color-warning: #f59e0b;
  --color-danger: #f43f5e;
  --color-info: #0ea5e9;

  --font-sans: "Inter", system-ui, -apple-system, "Segoe UI", Roboto,
    "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
}

@utility base-transition {
  transition-property: color, background-color, border-color, box-shadow,
    transform, opacity;
  transition-duration: 200ms;
  transition-timing-function: ease;
}
```

Every color utility becomes available automatically (`bg-appBg`, `text-brand`,
`border-divider`, `ring-focusRing`, …). Nested `text.primary` maps to
`text-text-primary`. Body baseline: `bg-appBg text-text-primary font-sans
antialiased`.

## 4. Design-system component classes (verbatim from spec)

Implement these primitives **only** with the classes below; do not invent
variants.

### Inputs

```html
<input class="bg-appBg border border-divider text-text-primary
  placeholder:text-text-dim rounded-xl px-4 py-3 base-transition
  focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing
  focus:shadow-[0_0_15px_rgba(56,189,248,0.15)]" />
```

### Buttons

- Primary: `bg-brand hover:bg-indigo-500 text-text-primary font-medium
  px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98]
  transition-all`
- Secondary: `bg-surface border border-divider hover:bg-elevated
  text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl
  transition-colors`
- Destructive: `bg-danger/10 hover:bg-danger text-danger
  hover:text-text-primary border border-danger/20 px-5 py-2.5 rounded-xl
  transition-colors`

### Cards / rows

- Card: `bg-surface border border-divider rounded-2xl p-6 hover:bg-surface/80
  hover:border-text-dim/30 hover:-translate-y-0.5 shadow-xl transition-all
  duration-200`
- Active row: `bg-brand/10 border-l-4 border-l-brand border-y-divider
  border-r-divider transition-all`

### Badges

- Success: `bg-success/10 text-success text-xs font-semibold px-2.5 py-1
  rounded-full border border-success/20`
- Warning: `bg-warning/10 text-warning text-xs font-semibold px-2.5 py-1
  rounded-full border border-warning/20`
- Danger: `bg-danger/10 text-danger text-xs font-semibold px-2.5 py-1
  rounded-full border border-danger/20`

### KPI card

`bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2`.

## 5. Layouts

### Admin

- Wrapper (dashboard KPI grid): `grid grid-cols-1 md:grid-cols-3 gap-6 p-6
  bg-appBg min-h-screen text-text-primary`
- Sidebar: `w-64 bg-surface border-r border-divider h-screen text-text-muted
  flex flex-col p-4`
- KPI card: see §4.

Admin shell on small screens collapses sidebar into a horizontally
scrollable chip nav (retain mobile usability; the provided sidebar keeps
`w-64` at large widths). Note: a fixed `w-64` sidebar + the 3-column KPI
grid are **complementary**, not nested — dashboard cards use the grid;
the shell sidebar is outside it.

### Webmail (3-column)

```
<div class="flex h-screen w-screen overflow-hidden bg-appBg text-text-primary">
  <nav  class="w-[18%] bg-surface border-r border-divider flex flex-col p-4 gap-4" />
  <section class="w-[32%] bg-appBg border-r border-divider flex flex-col
                  overflow-y-auto scrollbar-thin" />
  <main class="flex-1 bg-surface flex flex-col p-6 overflow-y-auto" />
</div>
```

On `<md`, stack to `flex-col` with folder nav becoming a tab bar and list
pane taking precedence; reading pane scrolls below. Keep original fixed
percentages only at `md:` and up to preserve the desktop 3-pane design.

## 6. Routing and auth

Routes (SvelteKit):
- `/` — Home dashboard (requires auth; redirects unauthenticated → `/login`)
- `/login` — LoginPage (guest-only: authenticated users bounced to
  `/admin` or `/mail` via `/auth/me`)
- `/admin` — AdminHome (requires auth + admin kind via `/auth/me`)
- `/mail` — WebmailHome (requires auth)
- `+error.svelte` — NotFound (404)

Auth store mirrors `api.get_token` / `set_token` / `clear_token` using
`window.localStorage` with an in-memory fallback (for sand-boxed embeds),
exactly like `crates/synth-ui/src/api.rs`.

Guards run in `+layout.ts`/page `onMount` and redirect; never SSR.

## 7. API client port (TS)

Mirror `crates/synth-ui/src/api.rs` verbatim in semantics:

- `API_BASE = "/api/v1"`; `TOKEN_KEY = "synthmail_token"`.
- Pure helpers (unit-tested in Vitest, mirroring the Rust `#[cfg(test)]`):
  `apiUrl`, `encodeQueryComponent`, `messagesUrl`, `mailSearchUrl`, `auditUrl`,
  `httpErrorMessage`, `jmapRequestBody`, `parseQuota`.
- Interfaces: `LoginResp, MeResp, User, Domain, Alias, QuotaResp/QuotaUser,
  QueueResp/QueueCounts, DkimGenResp/DkimKey, DmarcRow, TokenRow, CreatedToken,
  PushSub, TotpEnrollResp, MessageHeader, FoldersResp, MessagesResp, MessageFull,
  SearchResp, SearchResult, DraftsResp, SendResp, Create*Req, Update*Req`.
- Functions: every `api::*` (`login`, `logout`, `me`, `change_password`,
  domains/users/aliases/quota/queues/dkim/filters/vacation/audit/bans/config/
  dmarc/push/totp/tokens/mail/jmap) plus compatibility shims (`tokens_list`,
  `mail_messages`, etc.).
- Typed errors surface server `detail` first, then `message/error/title`,
  then `HTTP {status}` (matches `http_error_message`).
- JMAP paths intentionally bypass `/api/v1` (`/jmap/session`, `/jmap`).

## 8. Screen specification (what each route renders)

### Home (`/`)
- Card "Dashboard"; `DashboardWidgets` KPI grid uses `adminGrid.wrapper`
  + three+ `adminGrid.kpiCard`s: Domains, Users, Quota (used/total), Deferred
  queue. Show `Spinner` while loading, soft "Some widgets unavailable" note on
  partial failure (admin-gated).
- "Quick links" card with primary/secondary buttons.
- "Recent audit" card: table of last 5 audit rows (Actor/Action/Target/Time).

### Login (`/login`)
- Centered card `max-w-md` with `auth-head`, email/password/TOTP fields,
  "Add TOTP code" ghost toggle, primary submit "Sign in", `ErrorCard`.

### Admin (`/admin`)
- Shell: `aside` sidebar + `section` content (tab-based exactly like
  `AdminHome`: overview, domains, users, aliases, quota, queues, dkim,
  filters, vacation, dmarc, audit, bans, tokens, config, account).
- Sidebar nav uses secondary button styling on active (brand-tinted active row:
  `bg-brand/10 border-l-4 border-l-brand`).
- Each panel: refresh button (secondary), form with inputs, table with actions,
  `ConfirmButton` for destructive, `PendingButton`/`PendingSubmitButton` for
  async, `Spinner`/`EmptyState`/`ErrorCard`.
- Admin gating: `require_admin` → `Some(false)` renders "Admin access
  required" card.

### Webmail (`/mail`)
- 3-column layout per §5 (navigation: folders + compose + quota meter; list:
  search + message rows; reading: subject/sender/meta/body + reply box).
- Message row selected state: card active row (`bg-brand/10 border-l-4 ...`).
  Unread dot uses `brand`.
- Badges for folder counts use `bg-brand/10 text-brand` pill.

### NotFound
- Simple centered card, "Page not found", link home.

## 9. Deliverables checklist (per-screen components)

Admin panels to port (1:1 functional parity):
`OverviewPanel, DomainsPanel, UsersPanel, AliasesPanel, QuotaPanel,
QueuesPanel, DkimPanel, FiltersPanel, VacationPanel, DmarcPanel, AuditPanel,
BansPanel, TokensPanel, ConfigPanel, AccountPanel` plus shared
`DashboardWidgets`.

Shared primitives: `Spinner, Loading, ErrorCard, EmptyState, ConfirmButton,
Field, TextAreaField, PendingButton, PendingSubmitButton, LogoutButton, TopBar,
BottomNav, LogoutNavButton`.

## 10. Migration phases and gates

| Phase | Work                                                                                 | Gate                                                    |
| ----- | ------------------------------------------------------------------------------------ | ------------------------------------------------------- |
| 0     | Scaffold `ui/`, SvelteKit+TS+Vite, Tailwind v4 tokens, design-system primitives     | `npm run check` clean; token CSS variables present      |
| 1     | API client port + pure helpers tests; auth store + guards                            | Vitest pure helpers green; login smoke against backend   |
| 2     | App shell, routing, TopBar/BottomNav, Login, Home                                     | E2E login + dashboard renders                           |
| 3     | Admin shell + all 15 panels with full parity                                         | Each panel exercises list/create/delete against control  |
| 4     | Webmail 3-column, folders, messages, search, compose, drafts, quota meter             | Send+read round-trip + search                           |
| 5     | PWA (manifest, icons, sw), a11y (axe), responsive QA, loading/empty/error states      | axe clean on each route; lighthouse perf/a11y targets   |
| 6     | Dockerfile (node build) + nginx config; CI `ui-build` → npm; remove Leptos crate      | `docker compose config`; image build; smoke.sh passes   |

## 11. Docker / CI cutover

- Replace `docker/synth-ui/Dockerfile` build stage with Node:
  1. `node:20-alpine` (or 22) builder: `npm ci && npm run build`
  2. Copy output dir (`.svelte-kit`/`build` with adapter-static) to nginx image.
  nginx SPA fallback + proxy rules stay unchanged (`/api/`, `/jmap`, mail).
- `Trunk.toml`, `Cargo.toml` member entry and `wasm32` target check removed
  only in Phase 6 after parity confirmed.
- CI `ui-build` job: Node setup + `npm ci && npm run check && npm test`;
  keep `docker buildx` of `docker/synth-ui/Dockerfile`.
- Update README workspace text (`synth-ui` → Svelte) and status table in final.

## 12. Testing matrix

- Unit (Vitest): all pure helpers (`apiUrl`, `messagesUrl`, `httpErrorMessage`,
  `parseQuota`, `jmapRequestBody`) with the exact parity tests from Rust.
- Component (Testing Library): Button/Card/Input variants render expected
  class strings; PendingButton disabled state.
- E2E (Playwright): login, admin user/domain CRUD, webmail send/receive.
- Accessibility: `@axe-core/playwright` on each route; keyboard nav for tabs,
  bottom nav, confirm dialogs.
- Visual: optional Percy/Chromatic against CyberSlate dark screenshots.
- Performance budget: gzipped JS < 120 KB initial; Lighthouse perf ≥ 90.

## 13. Risks and mitigations

- **Tailwind v4 CSS-first config** differs from v3; ensure `@theme` token names
  produce expected utilities (`text-text-primary`, `bg-appBg`). Vitest
  snapshot of rendered class names guards regressions.
- **Runes learning curve / reactivity parity**: keep a thin abstraction for
  pending flags to match Leptos `use_pending` semantics (signals copy across
  closures).
- **No light theme**: ensure OS dark/light setting doesn't break contrast;
  force `color-scheme: dark` on `:root`.
- **Service worker stale cache** after cutover: bump cache name
  `synthmail-shell-v2` and add precache invalidation.
- **Leptos removal** may break workspace `cargo test --workspace`: remove member
  entry or gate with `default-members` before deletion.

## 14. Acceptance criteria (definition of done)

1. `docker compose build synth-ui` succeeds; `smoke.sh` passes end-to-end.
2. All 15 admin panels functionally equivalent (list/create/update/delete).
3. Webmail 3-column desktop layout matches spec widths (18/32/flex-1) and
   collapses on mobile without horizontal scroll.
4. `npm run check`, `npm test`, `npm run build` all green in CI.
5. `cargo test --workspace` green with `synth-ui` removed from members.
6. Every interactive control uses the provided component classes only; no
   ad-hoc color values outside tokens.
7. Accessibility: no axe violations on `/`, `/login`, `/admin`, `/mail`.

---
*Plan of record for the Svelte rebuild. Update in place; do not append new
plans for the same migration.*
