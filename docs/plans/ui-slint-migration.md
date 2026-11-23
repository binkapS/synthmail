# Plan: Migrate SynthMail Console from Svelte to Slint with Complete UI Redesign

**Status:** Proposed
**Budget:** Low-to-medium. Backend crates are untouched; the new UI is a separate
Rust workspace crate.
**Decision rationale (read first):** Slint is a declarative native UI toolkit for
Rust (stable `1.19.0`, published 2026-11-16, per crates.io). It matches the
Rust-first architecture of SynthMail, removes the browser runtime, and lets the
UI consume the control-plane and webmail APIs through the same Rust stack
(`reqwest`). The current Svelte web UI continues to work during the
transition; the native app replaces it after validation.

---

## 1. Summary

| Area | Change |
|---|---|
| New crate | `crates/synth-ui-app` — Slint 1.19 native app ("SynthMail Desktop") |
| Backends | **Unchanged** (`synth-control`, `synth-webmail`, all other crates) |
| Screens | Login · Dashboard · Admin console (16 tabs, redesigned) · Mail 3-pane client · Account · Error |
| Design | Complete redesign: new "Nocturne Slate" token system (dark + light), ~40 components |
| State/data | `reqwest` client, typed models, Slint models for tables, `keyring` for tokens |
| Deployment | Replace `synth-ui` (nginx+Svelte) service & edge `/admin/` rules with the native binary |
| CI | Replace `ui-build`/`npm audit` jobs with Rust build + tests |
| Tests | `cargo test/clippy/fmt` + API client unit tests (mock server) + Rust model/store tests; manual QA checklist |

---

## 2. Current State (Baseline Inventory)

### 2.1 Routes & screens (Svelte 5 + Tailwind v4, served by nginx via `synth-ui`)

| Screen | Path | Auth | Notes |
|---|---|---|---|
| Sign in | `/login` | guest | email + password (+ optional TOTP code), "Add TOTP code" toggle, DOM-based validation |
| Dashboard | `/` | admin\|superadmin | stats grid (domains, users, deferred, quota), recent audit activity |
| Admin console | `/admin` | admin\|superadmin | sidebar with 16 tabs |
| Webmail | `/mail` | any | 3-pane: folders · messages · reading pane; compose, reply, draft delete |
| Account | `/account` | any | password change, 2FA enroll/verify/disable, push subscriptions |
| Error | `/error` | — | fallback page |

Admin tabs (16): overview · server · domains · users · aliases · quota · queues ·
dkim · filters · vacation · dmarc · audit · bans · tokens · config · account.

### 2.2 Key features per screen

- **Dashboard** — `listDomains`, `listUsers`, `queues`, `quota`, `audit(6)` → 4 stat cards + recent-activity list.
- **Domains** — CRUD, DNS setup modal, DNS verify modal, DKIM generate modal, refresh.
- **Users** — create (email/password/quota/kind user\|admin\|superadmin), toggle active, password reset, refresh.
- **Aliases** — list, search, create, update, delete (confirm), refresh.
- **Tokens** — list (name, expiry, status, revoke with confirm), create (name + expiry).
- **Audit** — list with `limit` (up to 6 on dashboard), columns action/target/actor/time.
- **Mail** — folders from `GET /api/v1/mail/folders`; messages `GET /api/v1/mail/messages?folder=&limit=50`; message `GET /api/v1/mail/messages/{uid}`; search `GET /api/v1/mail/search?q=`; send `POST /api/v1/mail/send {from,to,subject,body}`; draft delete `POST /api/v1/mail/drafts`. Webmail quota via `GET /api/v1/quota`.
- **Account** — `me()`, password change (revokes token → re-login), TOTP enroll (secret + otpauth_url display) / verify / disable, push subscriptions list + delete.

### 2.3 State & data management (Svelte 5)

- Token: `localStorage["synthmail_token"]`, in-memory cache (`client.ts`).
- Auth store: `$state` rune (`auth.svelte.ts`).
- Tables: `$state<any[]>` + `$effect(() => load())` per panel.
- Forms: `$state` fields + imperative `doX()` handlers; validation inline.

### 2.4 Deployment chain

- `synth-ui` service = nginx 1.27 serving static SPA (`docker/synth-ui/Dockerfile`),
  reverse-proxy rules for `/api/`, `/jmap`, `/dav`, `/.well-known/carddav`, `/.well-known/caldav`,
  loopback-only at `127.0.0.1:3002`.
- Edge nginx (`docker/edge/nginx.conf`) proxies `/admin/` and static assets → `synth-ui:80`.
- CI: `ui-build` job (npm ci/build/test), `npm audit` job — to be replaced.

---

## 3. Goals & Non-goals

**Goals**
1. Replicate every screen and interaction of the current console (feature parity).
2. Ship a polished, complete redesign per the "Nocturne Slate" design system (§5).
3. Same visual language as the backend (Rust/Cargo ecosystem, no Node runtime).
4. Testing: Rust unit tests + client mock tests + QA checklist per screen.
5. Migration hygiene: Svelte `ui/` kept working until cutover; no data migration
   (native app reads the same APIs; only tokens persist).

**Non-goals**
1. Backend changes — zero Rust changes outside adding the new crate.
2. Browser-first UI: the native app replaces the browser console; browsers
   remain supported by `synth-webmail` for generic IMAP clients (not the console UI).
3. New features (attachments viewer, spam folder actions, drag & drop) — these
   are evaluated later; parity is the bar.
4. Right-to-left, i18n — plan hooks exist (gettext), English source strings now.

---

## 4. New Architecture

### 4.1 Crate layout

```
crates/synth-ui-app/
  Cargo.toml            # slint 1.19, reqwest (workspace), keyring, tokio, tracing, ...
  build.rs              # slint_build::compile_no_target_prefix("src/app.slint")
  src/
    main.rs             # slint::run_event_loop + app bootstrap
    app.slint           # root Window; holds Screen state (enum) — see §4.3
    api/
      client.rs         # reqwest client: get/post/put/delete, error mapping
      models.rs         # typed DTOs: Domain, User, Alias, TokenRow, AuditRow, MessageRow, ...
    store/
      auth_store.rs     # token persistence via keyring, session transitions
      screen_store.rs   # Screen enum state machine + guards
      toast_store.rs    # notification stack (replacement for inline info bars)
    models/
      tables.rs         # Slint models: Domains, Users, Aliases, Tokens, Audit, Messages, Folders, ...
      sort_filter.rs    # SortModel / FilterModel wrappers for sortable tables
    screens/
      login.slint
      dashboard.slint
      admin.slint       # AdminShell + one file per panel: domains, users, aliases, ...
      mail.slint
      account.slint
      error.slint
    components/         # design-system primitives (§5.2)
```

### 4.2 Slint structure

- **Entry:** `slint_build::compile_no_target_prefix("src/app.slint")` in `build.rs`,
  then `slint::include_modules!()` in `main.rs`; `slint::run_event_loop()` on the main thread.
- **Root window (`app.slint`):**
  - `Screen` property (`SharedString` or enum): `login | dashboard | admin | mail | account | error`.
  - Struct: `{ screen: Screen, ... }`; transitions via callbacks:
    `go-login()`, `go-dashboard()`, `go-admin(tab: AdminTab)`, `go-mail()`, `go-account()`, `go-error()`.
  - The window instantiates the current screen via a big `if screen == ... { <Login /> }` block —
    screens are swapped in place (documented Slint pattern; no global routing needed).
- **Event loop pattern (Rust):**
  - Worker threads: `std::thread::spawn` (or the tokio runtime from the app struct)
    run blocking reqwest calls; on success/failure they call
    `slint::invoke_from_event_loop(move || { ...model.set_row_data(...); store.set_error(...) })`.
  - UI callbacks (`clicked`, form submits) run synchronously in the Slint thread;
    they spawn the fetch workers as above.
  - `slint::quit_event_loop()` on logout/error → transitions to `login`.
- **Models:** tables = `VecModel<Row>` or `#[derive(slint::StructModel)]` structs;
  sortable tables = `slint::SortModel::new(model, less_fn)`; filtered = `slint::FilterModel::new(model, pred)`.
- **Forms:** `slint::TextInput`/`TextArea`/`Checkbox`/`Switch` + custom `Form`/`Field`/`ConfirmDialog`
  (validation in Rust or `Form.validate()` callback; `Form` collects values, submits via callback).
- **Themes:** default dark; light mode via `slint::Window::set_theme(Theme::Light)` where the API
  exists in 1.19 (verify during integration); if uncertain, keep dark + document.

### 4.3 Screen state machine

```rust
enum Screen { Login, Dashboard, Admin { tab: AdminTab }, Mail, Account, Error(ErrorKind) }

enum AdminTab { Overview, Server, Domains, Users, Aliases, Quota, Queues, Dkim,
                Filters, Vacation, Dmarc, Audit, Bans, Tokens, Config, Account }
enum ErrorKind { Offline, Unauthorized, Generic }
```

**Guards (parity with Svelte):**
- `GuestGuard`: if authenticated → jump to `Dashboard` (admin) or `Mail` (user) on mount.
- `AuthGuard`: if no token → `Login`.
- 401 handling: on `ApiError{status: 401}` from any call → `AuthStore::expire()` → `Login`.

### 4.4 API client (Rust)

```rust
pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
    token: RwLock<Option<String>>,
}
impl ApiClient {
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError>;
    pub async fn post_json<T>(&self, path: &str, body: &impl Serialize) -> Result<T, ApiError>;
    pub async fn put_json / delete / post_empty ...
}
pub struct ApiError { pub status: Option<u16>, pub message: String } // mirrors "HTTP xxx: ..." strings
```

- Error strings keep the old UI's flavor (e.g. `HTTP 401`, backend `detail` messages).
- Base URLs from env at startup: `SYNTH_CONTROL_URL` (default `http://synth-control:8000`),
  `SYNTH_WEBMAIL_URL` (default `http://synth-webmail:3000`); dev overrides documented
  (`http://localhost:8000/3000`).

### 4.5 Token storage

- Primary: `keyring` 3.x OS keychain (`Entry::new("synthmail", "token")`):
  Windows DPAPI, macOS Keychain, Linux Secret Service.
- Fallback if no OS keychain (e.g., headless dev boxes): documented warning state —
  app refuses to persist tokens; runs as guest login only. This is a deliberate
  security posture improvement over the old localStorage.
- On 401/logout/password-change → `clear_credential()` + re-login flow.

### 4.6 Webmail parity (Mail screen)

Same endpoints; models: `Folders: VecModel<SharedString>`, `Messages: VecModel<MessageRow>`
(uid, from, subject, preview, date, seen), `Full: RwLock<Option<MessageFull>>`.
Polling: `slint::Timer` auto-refresh messages every 30 s when screen is `Mail`;
manual refresh buttons for folders/messages/search.

### 4.7 Dev & build workflow

- `cargo run -p synth-ui-app` — dev run with backend URLs (`.env` supported via `config` crate).
- Slint preview: `slint-viewer src/app.slint` for isolated screen iteration.
- Tests: `cargo test -p synth-ui-app` (models, store transitions, API client vs. mock
  axum server in-process).

---

## 5. Complete UI Redesign — "Nocturne Slate"

### 5.1 Design system summary

Dark-first palette, 4-step ambient gradients (subtle, device-like), WCAG 2.1 AA
contrast, info-rich, confident typography. Light variant available (see token table).
Font: Inter with system fallbacks.

### 5.2 Design tokens (CSS-variable style)

```
# Base (dark)
--ink-0: #0A1020      # app background
--ink-1: #121A32      # panel surfaces
--ink-2: #1E2947      # elevated surfaces, table rows, inputs
--ink-3: #2D3D64      # borders, dividers, disabled separators
--ink-4: #3F4E73      # hover bg, scrollbar track, focus inner
--ink-5: #5C6CA0      # muted text, placeholders, secondary labels
--ink-6: #7C8EC4      # dim text, descriptions
--fg:    #F2F5FF      # primary text (contrast > 15:1)
--muted-fg: #93A0D0   # secondary text (≈4.5:1)
--brand: #7C5CFF      # brand accent, primary buttons
--accent: #22D3EE     # cyan — gradient mid-stop, primary icons
--brand-gradient: linear-gradient(135deg, #7C5CFF 0%, #22D3EE 100%)
--success: #34D399    # green (replaces legacy #10B981)
--warning: #FBBF24    # amber
--danger:  #F87171    # red (legacy #F43F5E)
--info:    #60A5FA    # blue

# Light variant
--ink-0: #F7F8FA  --ink-1: #FFFFFF  --ink-2: #F0F2F5  --ink-3: #D8DCE5
--ink-4: #C2C8D9  --ink-5: #667085  --ink-6: #52586E  --fg: #0F1626
--brand: #7C5CFF  --accent: #14B8A6  (success/warning/danger/info light variants)
```

#### Typography

| Role | Size | Weight | Notes |
|---|---|---|---|
| Display | 32–48 px | 700 | Welcome heads, empty states |
| H1 | 24 px | 700 | Screen titles |
| H2 | 18 px | 600 | Panel/section titles |
| Body | 14 px | 400 | Default text |
| Caption | 12 px | 500 | Sidebar labels, metadata, timestamps |
| Mono | 13 px | 400 | Monospace surfaces (mail body, codes, tokens) |

#### Spacing (4 px base grid)

`xs 4 · s 6 · m 8 · l 12 · xl 16 · xxl 20 · 3xl 24 · 4xl 32`

#### Radii

`r-s 4 · r-m 6 · r-l 8 · r-xl 12 · r-2xl 16 · r-3xl 20 · r-4xl 24 · pill 999`

#### Motion

- `ease-out 200 ms` for state changes (buttons, hovers)
- `ease-in-out 300 ms` for screen transitions
- `ease 400 ms` for panels open/close
- Focus-visible: 2 px `--brand` outline, offset 2 px

#### Elevation

```
elev-1: 0 1px 2px rgba(0,0,0,0.40), 0 1px 3px rgba(0,0,0,0.25)   # windows/panels
elev-2: 0 8px 30px rgba(0,0,0,0.35), 0 4px 12px rgba(0,0,0,0.20) # dropdowns, modals
elev-3: 0 20px 50px rgba(0,0,0,0.45), 0 10px 20px rgba(0,0,0,0.25) # toasts
```
(Light variant: `rgba(15,22,38,0.06/0.16/0.32)` equivalents.)

### 5.3 Component inventory (~40 Slint components)

Core form (§4.2 screens rely on these):

1. **`Button.slint`** — variants `primary | secondary | tertiary | destructive | link`;
   states normal/hover/active/disabled; `busy` spinner; icon support.
2. **`IconButton.slint`** — icon-only (copy, refresh, external, chevron); `aria-label`.
3. **`Input.slint`** — text input w/ label, placeholder, validation states
   (`default|focus|valid|invalid|disabled|readonly`), search-suffix variant, masked
   (password) variant.
4. **`SearchInput.slint`** — icon + clear + commit on Enter/click.
5. **`TextArea.slint`** — multiline, label, rows, optional mono font.
6. **`Checkbox.slint`**, **`Switch.slint`** (toggle for config/vacation/quota filters).
7. **`Select.slint`** — custom dropdown overlay (kind filter, audit date range).
8. **`Field.slint`** — label + child + helper text + error line (form building block).
9. **`Form.slint`** — vertical layout, aggregates validation, submit-on-Enter,
   auto-focus first invalid field on submit.

Layout & navigation:

10. **`Card.slint`** — panel container, header slot (title + actions), shadow, rounded corners.
11. **`Panel.slint`** — dashboard grid panel (stat cards, banner lists).
12. **`TopBar.slint`** — brand mark + "SynthMail"; screen title/subtitle; user summary;
    actions: **Refresh**, **DAV** (opens DAV discovery like the old UI), **Logout**;
    status dot on /dashboard.
13. **`Sidebar.slint`** — 16 nav items w/ icons + active state + badge slots (e.g. bans
    count, users count), collapsible on small widths.
14. **`AdminShell.slint`** — `AdminConsole` layout: TopBar + Sidebar + content area.
15. **`Toast.slint`** — animated notification stack (top-right): success/error/info,
    auto-dismiss 5 s, manual dismiss, accessibility announced via `announce`.

Display:

16. **`Table.slint`** — generic: `columns: ModelRc<Column>` (title, width, align, sortable,
    action flags) + `rows: ModelRc<TableRow>`; `TableRow` struct with `cells: ModelRc<TableCell>`;
    `Table` emits `row-selected`, `row-action(row, index, action_id)`; supports empty state.
17. **`Badge.slint`** — status chips: Active/Superadmin/Enabled/Pending/Deferred/Running with
    token colors.
18. **`Spinner.slint`** — SVG spinner; `busy` variant.
19. **`Skeleton.slint`** — loading shimmer rows for tables/stats.
20. **`EmptyState.slint`** — icon + message + optional CTA (all screens use it).
21. **`ErrorBanner.slint`** — red alert box (`ErrorCard` parity).
22. **`ConfirmDialog.slint`** — destructive-intent confirm (used by every Delete).
23. **`Modal.slint`** — overlay + focus trap + Escape close.

Per-screen components (screens/):

24. **`LoginWindow.slint`** — brand + 2-field form + TOTP toggle + error + spinner.
25. **`DashboardWindow.slint`** — hero card + stats grid (4) + recent activity list.
26. **`DomainsPanel`, `UsersPanel`, `AliasesPanel`, TokensPanel, DkimPanel, FiltersPanel`,
    `VacationPanel`, DmarcPanel`, AuditPanel`, BansPanel`, ConfigPanel`, ServerPanel,
    QuotaPanel`, QueuesPanel, OverviewPanel` — redesigned admin panels (all features of §2.2).
27. **`AccountPanel.slint`** — profile card + password change + TOTP enroll/verify/disable +
    push subscriptions list.
28. **`Mail3Pane.slint`** — folders (unread counts, search input, refresh), messages list
    (avatar initials, from, subject, preview, date, unread dot), reading pane
    (header, body, reply form), draft-delete for Drafts folder.
29. **`ComposeForm.slint`** — To/Subject/Body/Send/Cancel; **`ReplyBox.slint`**.
30. **`ErrorWindow.slint`** — offline banner + retry (and forced-password-change card on Mail).

### 5.4 Screens — redesign specs (parity + new polish)

**Login** — centered card on ambient gradient; brand mark + headline; two masked/text
inputs; email validation client-side (regex, but server is source of truth); TOTP
conditional; error banner top; success spinner during request; keyboard flow (Enter
submits). Screen-level "accessible" focus management.

**Dashboard** — ambient light leaks (records the "radial-gradient" motif of the old
design but subtle); welcome hero card; 4 stat cards with accent underline bars
(the old design's `--top-bar`), tabular numerics, subtitle lines; recent-activity list
with icons (✦ actor chip, bold action, muted target, right-aligned time); "Open admin
console" / "Open webmail" CTA buttons; if any widget fails, neutral subtitle line.

**Admin console** — TopBar + Sidebar (16 tabs, active highlight, logout button at bottom);
content area switches per tab; every panel supports: auto-refresh on mount, manual
refresh button (TopBar "Refresh" → current panel callback), empty states, error banner,
Toast on mutations.

**Admin panels (key redesign choices):**
- **Domains:** searchable list with rows (name, status, verified, catch-all); create form
  (name, description); per-row actions: DNS (copy records), verify, DKIM generate, delete;
  refresh; `Count + N` footer.
- **Users:** table w/ avatar initials + badge (Active/Inactive), role badge, quota bar;
  create form (email, password+strength indicator, quota, kind select); row actions:
  toggle active, reset password, delete; refresh.
- **Aliases:** table (address → target, status); create form + search filter; row actions:
  update, delete; refresh.
- **Tokens:** table (name, expiry, status badge, hash prefix, created); create form
  (name, expiry select); display new secret once in Modal; revoke with ConfirmDialog; refresh.
- **Audit:** table (time, actor, action, target, detail) with date range + actor filter;
  `limit` display ("Showing 6 of N"); refresh.
- **Mail:** folders sidebar with per-folder unread counts, search (with live commit),
  messages list w/ avatar + seen/unread states, reading pane with body, reply text area,
  draft delete on Drafts; status line for quota; auto-poll 30 s.
- **Account:** password change w/ strength indicator; forced-change warning banner;
  TOTP: enroll (secret displayed, copy button), verify (confirm), disable; push
  subscriptions list; logout moved to TopBar (visible on all screens).
- **Config/Server/Quota/Queues/Vacation/Filters/Bans/DKIM/DMARC/Overview:** same feature
  sets; new presentation: stat cards with trend lines where meaningful (e.g. Queues:
  deferred/retry/held gauges; Quota: per-domain bars), information density stays high.

### 5.5 Responsive behavior (native app)

- Desktop-first design. Layout adapts at widths: sidebar → collapsible; TopBar stacks
  vertically below 800 px; on ≤480 px, a bottom navigation (`BottomNav.slint`, port of
  the Svelte one: sign in / home / mail / account / admin) appears, toggling a
  mobile sidebar drawer.

### 5.6 Accessibility

- Keyboard navigation everywhere (Tab/Enter/Escape); focus traps in Modals/Dialogs;
  `aria-*` labels on icon-only controls; color-contrast ≥ 4.5:1 minimum (≥ 7:1 preferred);
  screen-reader text on avatars; text alternatives on icons; `announce` for async
  toasts; focus indicators visible; scalable fonts (no fixed px clamping below 12 px).
- Slint 1.19 ships with accessibility APIs (the crate advertises "accessibility"
  support; `system-testing` feature available for integration tests).

### 5.7 Localization

- Strings in `src/strings.slint` (or `gettext` po files) with English source; no LTR
  layout changes needed yet; document future steps (translation, font hinting, IME).

---

## 6. Migration Phases

Timeline: **~14 weeks** (≈ 3½ months) from first line of Slint code. Svelte UI remains
the fallback until cutover.

### Phase 0 — Discovery & Design System (1 week, overlaps with P1)
- Finalize token tables (§5.1), radii, motion, elevations; write `docs/design-tokens.md`.
- Create crate skeleton (`crates/synth-ui-app/`): Cargo.toml (slint 1.19, reqwest,
  keyring 3, tokio, tracing, clap), build.rs, main.rs smoke (compiles, runs headless?
  → dev only), `src/app.slint` root with a "SynthMail Desktop" placeholder window.
- Slint spike: render `Button`+`Input` in root window; run `cargo run -p synth-ui-app`
  on the dev machine; verify winit event loop + software renderer fallback.
- Define `Screen`/`AdminTab`/`ErrorKind` enums + `ScreenStore` skeleton.

### Phase 1 — API Client, Token Store, Auth & Login Screen (1.5 weeks)
- `ApiClient` (reqwest) + `ApiError` + mock-server unit tests (`mockito` or hand-rolled
  axum in tests; prefer `mockito`-style; if network restricted use in-crate `mockito`).
  - Endpoint inventory from §2.1/2.2 (mail endpoints included).
- `TokenStore` (keyring) + unit tests (mockless: save/read/clear).
- `AuthStore`: token→screen transitions, `expired()` hook, TopBar-ready user summary.
- **LoginScreen** (`login.slint` + `LoginWindow.slint`): email/password (+ TOTP),
  client validation, async spawn + `invoke_from_event_loop`, error banner, spinner.
- Guard parity: `GuestGuard` (redirect if authenticated).
- CI: `app-build` job — `cargo build` linux/mac/windows matrix, `cargo test -p synth-ui-app`.
- Manual QA: login happy path against local dev backends; wrong-credential error path.

### Phase 2 — App Shell & Admin Console (2 weeks)
- `TopBar.slint`, `Sidebar.slint` (16 tabs, icons), `AdminShell.slint`, `AdminTab` mapping.
- `AuthGuard` parity; `AdminStore` (active tab, sidebar state); refresh callbacks.
- Guest-redirect + admin-check logic (`/admin` requires admin/superadmin, parity).
- Tests: tab switching transitions, guard paths.
- Manual QA: shell chrome + keyboard nav of tabs.

### Phase 3 — Admin Panels (6 weeks)
Build components shared across panels (§5.3): `Button, IconButton, Input, SearchInput,
TextArea, Checkbox, Switch, Select, Field, Form, Card, Panel, Table, Badge, Spinner,
Skeleton, EmptyState, ErrorBanner, ConfirmDialog, Modal, Toast` (§5.3).

Per-panel work (each: Slint component → models → behaviors → QA checklist items):

| Week | Panels | Key features |
|---|---|---|
| 3.1 | Overview, Server, Queues, Quota | stat cards, trend lines, status badges |
| 3.2 | Domains, Aliases | search, create, update, delete, DNS/DKIM modals |
| 3.3 | Users, Tokens, Bans | tables w/ badges, create/reset/revoke/unban forms |
| 3.4 | DKIM, Config | generate dialog, key rows w/ copy, config export/import forms |
| 3.5 | Filters, Vacation, DMARC, Audit | filters, editor dialogs, reports table |
| 3.6 | Account, Audit (limits) | password, TOTP, push subs; refresh-all callbacks |

Per-panel flow: `fetch on mount → Skeleton → Skeleton→Table/Card → mutate+Toast →
refresh`; confirmations for destructive actions; accessible focus management.

### Phase 4 — Mail Screen (4 weeks)
- `Mail3Pane.slint` + `Folders`, `Messages`, `Full` models; message list states
  (seen, unread dot, preview, time format), reading pane (monospace body, headers),
  reply box (parity: `Re:` prefix), send + draft-delete flows, To/Subject/Body forms
  with validation, "Select a message to read" empty state.
- Webmail quota bar at top of sidebar.
- Auto-poll every 30 s (`slint::Timer`); manual refresh.
- QA: folders switching, search commit, list → read transitions, draft delete.

### Phase 5 — Dashboard (2 weeks)
- `DashboardWindow.slint`: hero, 4 stat cards, activity list, buttons to Admin/Webmail;
  redirects: admin → `/admin`, user → `/mail` (parity); error state handling.
- QA: data load, refresh, non-admin bounce.

### Phase 6 — QA & Cutover (2 weeks)
- **Manual QA checklist** per screen (design-spec conformance + interactions list in §6.7).
- Cross-platform: build macOS + Windows release binaries in CI; verify basic run;
  document platform notes (Windows: DPAPI via keyring; macOS: Keychain).
- **Cutover checklist** (§7.1): remove `ui/`, `docker/synth-ui`, edge rules, CI legacy jobs.
- Update README/CHANGELOG (§7.2).

### Phase 7 — Post-Migration Cleanup (1 week)
- Archive `docs/svelte-rebuild-plan.md` → `docs/history/svelte-rebuild-plan-v1.md`.
- Workspace gate: `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`.
- Repo-wide grep: no remaining `ui/`, `synth-ui`, `npm`, `svelte`, `vite` references
  (README, docs, compose, CI, docker).
- Open questions resolved; final review of the plan (this doc).

**Total: ≈ 14 weeks.** Risk-adjusted (slint API spike, font/rendering polish):
+3 weeks; milestone: Phase 1 end (login working) and Phase 3.2-mid (domains+aliases)
are the primary validation gates.

### 6.7 Manual QA checklist (per screen, run against dev backends)

- [ ] Login: empty fields → client error; wrong creds → backend error text; TOTP field
      appears on toggle; 401 → forced re-login.
- [ ] Dashboard: admin sees 4 stats; user bounces to Mail; activity list; "Open admin/webmail".
- [ ] Admin console: 16 tabs; guard bounce for non-admin; logout works (TopBar).
- [ ] Domains/Aliases/Users/Tokens/Audit: create/update/delete flows incl. confirms;
      empty states; search; refresh.
- [ ] DKIM/Config/Filters/Vacation/DMARC/Bans/Quota/Queues/Server/Overview: parity vs
      old UI (columns, actions, error paths).
- [ ] Account: password change → re-login (parity); TOTP enroll/verify/disable.
- [ ] Mail: folders list w/ counts; search; select message → read; reply; compose;
      draft delete; empty states; polling.
- [ ] Error: offline banner + retry.
- [ ] Theme: light mode renders correctly (if supported); desktop resizing breakpoints.
- [ ] Keyboard: Tab order, Enter submit, Escape close dialogs.

---

## 7. Backend / Deployment Changes

### 7.1 Cutover checklist

1. Validate parity per screen (Phase 6).
2. Delete `ui/` (Svelte, SvelteKit, Tailwind).
3. Delete `docker/synth-ui/Dockerfile`; remove `synth-ui` service from `compose.yaml`.
4. Remove edge rules `/admin/`, `/admin/index`, static → `synth-ui` from `docker/edge/nginx.conf`.
   Keep `/api/`, `/jmap/`, `/dav/`, `/.well-known/carddav|caldav` (used by DAV/CalDAV clients).
5. Remove `ui-build` + `npm audit` jobs from `.github/workflows/ci.yml`; add `app-build`
   (cargo matrix) + `cargo test/clippy` on synth-ui-app (already workspace-covered).
6. Release process: tagged binaries (Linux/macOS/Windows) via CI + `./scripts/build-images.sh`
   (document; images already exist for services).
7. Docs sweep (README, CHANGELOG, docs/plans/ui-slint-migration.md → final state).

### 7.2 Docs updates (README)

- "Admin UI" section: rewrite for the native app (launch `synth-ui-app` / distro package),
  update architecture diagram (`ui/` → `crates/synth-ui-app/`), remove Node 22 prerequisite
  note, update Development commands (`cargo run -p synth-ui-app`).
- Add "Licensing note" for Slint (see §8).
- `docs/plans/ui-slint-migration.md` becomes the living plan; `docs/svelte-rebuild-plan.md` archived.

---

## 8. Legal / Licensing Note (Slint)

Slint 1.19 is dual-licensed: **GPL-3.0-only** (default open source) or a commercial
"royalty-free" license (`LicenseRef-Slint-Royalty-free-2.0` / `LicenseRef-Slint-Software-3.0`
per crates.io). SynthMail is AGPL-3.0 — linking GPL-3.0 code is permitted (AGPL-3.0 is
compatible with GPL-3.0). Plan default: **GPL-3.0** (no action); commercial option available
if the project ever wants permissive terms. Legal review recommended at plan approval
(own choice — this plan does not impose a license).

---

## 9. Risks & Mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| Slint API drift vs. plan | Medium | Pin `slint = "1.19"`; spike in P0; CI compile gate |
| Font/rendering differences (native vs. browser) | Medium | Design tokens with same contrast math; visual QA per screen; option to bundle fonts via `fontique` later |
| OS keychain unavailable on some deployments | Low-Medium | Guest-login fallback + warning state (never insecure) |
| Scope creep in redesign | High | Phase-gated: parity first (all screens working), polish after |
| Windows/macOS window manager differences | Medium | CI builds; manual spot-checks on dev machines; document platform notes |
| reqwest TLS/platform issues (esp. Windows Schannel) | Low | rustls + native roots (verify) or vendored rustls |
| Compile times with slint | Low | sccache in CI |
| Mobile experience (bottom nav) | Low | Optional drawer; desktop-first |
| Localization future | Low | gettext hook planned (unlisted in Phase 0) |
| Feature parity misses (admin panels) | Medium | QA checklist §6.7 against old UI side-by-side; timeboxed per panel |
| Cutover risk (users on browser console) | Medium | Keep Svelte until Phase 6; announce cutover; old UI remains for self-hosters until released |

---

## 10. Success Criteria (Definition of Done)

1. `crates/synth-ui-app` builds with `cargo build` (no warnings); CI green.
2. `cargo test -p synth-ui-app`: ≥ 80% coverage of models/store/API client; integration test
   with in-crate mock server for ≥ 10 endpoints (incl. mail endpoints).
3. All 6 screens implemented per §5 (Login, Dashboard, Admin console w/ 16 redesigned
   panels, Mail 3-pane, Account, Error) — verified by QA checklist §6.7 (≥ 90% pass rate).
4. Token persistence verified on dev machine: save → restart → still logged in.
5. Release binaries build for ≥ 3 platforms (Linux x64/arm64, macOS, Windows); runnable smoke on one machine.
6. Zero Rust changes needed in `crates/synth-control`, `crates/synth-webmail` (diff shows only crate additions/docs).
7. `ui/` removed; no `synth-ui` service; edge `/admin/` rules removed; docs coherent
   (README, CHANGELOG, architecture diagram).
8. Contrast ratio of design tokens ≥ 4.5:1 on dark (≥ 3:1 on light) per WCAG A.
9. Licensing: decision recorded (default GPL-3.0).

---

## 11. Owners & Staging (review)

- **Plan author/owner:** [to be filled: person responsible for the migration]
- **Staging:** after Phase 1 (Login works) → ship "Draft" milestone; after Phase 3
  (Admin complete) → ship "v0.5 of new console"; after Phase 5+6 → ship "new console"
  cutover. Each milestone with a repo tag + release notes.
- **Rollback if needed:** before cutover, revert to `main` Svelte build + `synth-ui` service;
  plan Phase 6 keeps Svelte intact until acceptance.
