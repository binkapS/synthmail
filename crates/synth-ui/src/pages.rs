// SynthMail Leptos 0.8 UI pages — fully wired against the `crate::api` client.
//
// ASSUMED `crate::api` CONTRACT (sibling module; see docs/ui-design.md):
//   All functions are `pub async fn` returning `Result<T, String>`:
//   login(email, password, totp: Option<&str>), me(),
//   list_domains(), create_domain(name), get_domain(id), delete_domain(id),
//   domain_dns(id), domain_verify(id),
//   list_users(), create_user(email, password, quota: i32, kind),
//   update_user(id, Value), delete_user(id), reset_password(id, password),
//   list_aliases(), create_alias(address, goto), delete_alias(id),
//   quota(), queues(), dkim_generate(domain_id), dkim_list(domain_id),
//   filters_get(user_id) -> String sieve, filters_put(user_id, sieve),
//   vacation_get(user_id), vacation_put(user_id, Value),
//   audit(), bans(), config_export(), config_import(Value),
//   dmarc_list(),
//   push_list(), push_create(endpoint, p256dh, auth), push_delete(id),
//   totp_enroll(password, Option<code>), totp_verify(code), totp_disable(password),
//   tokens_list(), tokens_create(name, scopes: Vec<String>, expires_days: i64),
//   tokens_revoke(id),
//   mail_folders(), mail_messages(folder, limit: u32), mail_message(uid: u32),
//   mail_search(q), mail_send(from, to, subject, body),
//   mail_drafts(), mail_draft_delete(uid: u32), jmap_request(Value),
//   set_token(&str) — persists token to localStorage (`synthmail_token`).
//   Ids are strings (backend uses Uuid; synth-ui has no uuid dependency).
//
// Rendering strategy: every API result is converted with `serde_json::to_value`
// and read through the `s`/`pick`/`rows_of` helpers, so this file does not
// depend on the exact field list of the sibling's response structs — only on
// the function signatures above (which must be reconciled at integration).
//
// NOTE on clipboard: `web-sys` in this dependency tree has no `Clipboard`
// feature, so `navigator.clipboard` is unavailable. The DKIM DNS record is
// shown in a readonly <textarea> (select-all + copy manually) instead.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use serde_json::{Value, json};

/// Token context provided by `lib.rs` (`provide_context`d `RwSignal<Option<String>>`).
pub type AuthState = RwSignal<Option<String>>;
/// Theme context provided by `lib.rs` (`RwSignal<String>`).
pub type ThemeSignal = RwSignal<String>;

// ---------------------------------------------------------------------------
// Value helpers (indifferent to the sibling's exact response struct shapes)
// ---------------------------------------------------------------------------

fn val<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn b(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

fn pick(v: &Value, keys: &[&str]) -> String {
    for k in keys {
        let x = s(v, k);
        if !x.is_empty() {
            return x;
        }
    }
    String::new()
}

/// Extract a list of rows from an API result that may be a bare array or an
/// envelope object (`{"rows":[...]}`, `{"folders":[...]}`, ...).
fn rows_of(v: &Value) -> Vec<Value> {
    if let Some(a) = v.as_array() {
        return a.clone();
    }
    if let Some(o) = v.as_object() {
        for k in [
            "rows",
            "items",
            "data",
            "users",
            "domains",
            "aliases",
            "messages",
            "results",
            "drafts",
            "folders",
            "reports",
            "tokens",
            "subscriptions",
            "bans",
            "audit",
            "keys",
            "list",
        ] {
            if let Some(a) = o.get(k).and_then(Value::as_array) {
                return a.clone();
            }
        }
        return vec![v.clone()];
    }
    Vec::new()
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

fn parse_i32(t: &str, d: i32) -> i32 {
    t.trim().parse::<i32>().unwrap_or(d)
}

fn parse_i64(t: &str, d: i64) -> i64 {
    t.trim().parse::<i64>().unwrap_or(d)
}

fn percent_encode(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for byte in t.as_bytes() {
        let c = *byte as char;
        if c.is_ascii_alphanumeric()
            || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
        {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn pct(used: f64, total: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    (used / total * 100.0).clamp(0.0, 100.0)
}

// ---------------------------------------------------------------------------
// Auth guards
// ---------------------------------------------------------------------------

/// Returns the token signal when signed in; otherwise redirects to /login.
pub fn require_auth() -> Option<AuthState> {
    match use_context::<AuthState>() {
        Some(t) if t.get_untracked().is_some() => Some(t),
        _ => {
            let nav = use_navigate();
            Effect::new(move |_| {
                nav("/login", Default::default());
            });
            None
        }
    }
}

/// Resolves `None` while checking, `Some(true)` for admin/superadmin,
/// `Some(false)` otherwise. On a failed /auth/me the panel stays closed.
pub fn require_admin() -> ReadSignal<Option<bool>> {
    let (state, set_state) = signal(None::<bool>);
    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::me().await {
                Ok(r) => {
                    let v = val(&r);
                    let k = s(&v, "kind");
                    set_state.set(Some(k == "admin" || k == "superadmin"));
                }
                Err(_) => set_state.set(Some(false)),
            }
        });
    });
    state
}

// ---------------------------------------------------------------------------
// Shared bits
// ---------------------------------------------------------------------------

#[component]
pub fn Spinner() -> impl IntoView {
    view! {
        <div class="spinner" role="status" aria-label="Loading">
            <span class="spinner-dot" aria-hidden="true"></span>
            <span class="muted">"Loading…"</span>
        </div>
    }
}

#[component]
pub fn Loading() -> impl IntoView {
    view! { <Spinner/> }
}

#[component]
pub fn ErrorCard(error: String) -> impl IntoView {
    view! {
        <div class="card error-card" role="alert">
            <p>{error}</p>
        </div>
    }
}

#[component]
pub fn EmptyState(text: String) -> impl IntoView {
    view! {
        <div class="card empty">
            <p class="muted">{text}</p>
        </div>
    }
}

/// Two-step confirm button: first click arms it, second click runs `on_confirm`.
/// When `pending` is supplied the button is disabled with a spinner label
/// while the async action is in flight.
#[component]
pub fn ConfirmButton(
    label: String,
    confirm_label: String,
    #[prop(optional)] danger: bool,
    #[prop(optional)] pending: Option<ReadSignal<bool>>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let (armed, _set_armed_unused) = signal(false);
    let set_armed = _set_armed_unused;
    let is_pending = move || pending.map(|p| p.get()).unwrap_or(false);
    let class = if danger {
        "btn btn-danger"
    } else {
        "btn btn-primary"
    };
    let label_cb = label.clone();
    let confirm_cb = confirm_label.clone();
    view! {
        <span class="toolbar">
            <button
                type="button"
                class=class
                class:armed=move || armed.get()
                class:pending=move || is_pending()
                disabled=move || pending.map(|p| p.get()).unwrap_or(false)
                on:click=move |_| {
                    if is_pending() {
                        return;
                    }
                    if armed.get() {
                        set_armed.set(false);
                        on_confirm.run(());
                    } else {
                        set_armed.set(true);
                    }
                }
            >
                {move || {
                    if is_pending() {
                        format!("{} …", label_cb)
                    } else if armed.get() {
                        confirm_cb.clone()
                    } else {
                        label_cb.clone()
                    }
                }}
            </button>
            {move || armed.get().then(|| view! {
                <button
                    type="button"
                    class="btn btn-ghost"
                    on:click=move |_| set_armed.set(false)
                >
                    "Cancel"
                </button>
            })}
        </span>
    }
}

#[component]
pub fn Field(
    label: String,
    value: RwSignal<String>,
    #[prop(optional)] placeholder: String,
    #[prop(optional, default = "text".to_string())] input_type: String,
) -> impl IntoView {
    view! {
        <label class="field">
            <span>{label}</span>
            <input
                class="input"
                type=input_type
                placeholder=placeholder
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            />
        </label>
    }
}

#[component]
pub fn TextAreaField(
    label: String,
    value: RwSignal<String>,
    #[prop(optional)] placeholder: String,
    #[prop(optional, default = 5usize)] rows: usize,
) -> impl IntoView {
    view! {
        <label class="field">
            <span>{label}</span>
            <textarea
                class="input"
                rows=rows
                placeholder=placeholder
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            ></textarea>
        </label>
    }
}

// ---------------------------------------------------------------------------
// Per-action pending state (P0: every action button gets a loading state)
// ---------------------------------------------------------------------------

/// Shared per-action pending hook: returns a pending flag + its setter.
/// Usage: `let (pending, set_pending) = use_pending();` — set `pending` to
/// `true` before the async call and `false` when it resolves, and pass the
/// flag to [`PendingButton`]. Signals are `Copy`, so closures spawn freely.
pub fn use_pending() -> (ReadSignal<bool>, WriteSignal<bool>) {
    signal(false)
}

/// Action button with a built-in loading state: while `pending` is true the
/// button is disabled and its label is replaced with a `"…"` spinner text,
/// then restores. `on_click` runs only when not pending.
///
/// For form submits pass `btn_type="submit"` and wire the handler via the
/// form's `on:submit` (the button then only reflects pending; this avoids
/// double-firing click + submit). The `on_click` callback is still required
/// for the component signature — pass `Callback::new(|_: ()| {})` in submit
/// mode.
#[component]
pub fn PendingButton(
    label: String,
    pending: ReadSignal<bool>,
    on_click: Callback<()>,
    #[prop(optional, default = "btn btn-primary".to_string())] class: String,
    #[prop(optional, default = "button".to_string())] btn_type: String,
    #[prop(optional, default = "".to_string())] busy_label: String,
) -> impl IntoView {
    let label_click = label.clone();
    let busy_click = busy_label.clone();
    let btn_type_click = btn_type.clone();
    view! {
        <button
            type=btn_type.clone()
            class=class.clone()
            disabled=move || pending.get()
            class:pending=move || pending.get()
            on:click=move |_| {
                if btn_type_click != "submit" && !pending.get_untracked() {
                    on_click.run(());
                }
            }
        >
            {move || {
                if pending.get() {
                    if !busy_click.is_empty() {
                        busy_click.clone()
                    } else {
                        format!("{} …", label_click)
                    }
                } else {
                    label_click.clone()
                }
            }}
            {move || pending.get().then(|| view! { <span class="spinner-dot" aria-hidden="true"></span> })}
        </button>
    }
}

/// Form-submit variant: a `type="submit"` button whose label/disabled state
/// reflect `pending`. The handler runs from the form's `on:submit` — never
/// `on:click` on an anchor — so this button intentionally has no click
/// handler. Always pair with a form that calls `ev.prevent_default()`.
#[component]
pub fn PendingSubmitButton(
    label: String,
    pending: ReadSignal<bool>,
    #[prop(optional, default = "btn btn-primary".to_string())] class: String,
    #[prop(optional, default = "".to_string())] busy_label: String,
) -> impl IntoView {
    let label_submit = label.clone();
    let busy_submit = busy_label.clone();
    view! {
        <button
            type="submit"
            class=class.clone()
            disabled=move || pending.get()
            class:pending=move || pending.get()
        >
            {move || {
                if pending.get() {
                    if !busy_submit.is_empty() {
                        busy_submit.clone()
                    } else {
                        format!("{} …", label_submit)
                    }
                } else {
                    label_submit.clone()
                }
            }}
            {move || pending.get().then(|| view! { <span class="spinner-dot" aria-hidden="true"></span> })}
        </button>
    }
}

/// Visible logout button for page shells (the topbar lives in `lib.rs`,
/// which this file cannot edit — pages therefore carry their own affordance).
/// Posts `api::logout()` (which clears localStorage + in-memory token on
/// success), unconditionally calls `api::clear_token()`, clears the
/// `AuthState` context signal, then navigates to `/login`. The button shows
/// a pending/disabled state while the request is in flight.
#[component]
pub fn LogoutButton(
    #[prop(optional, default = "btn btn-ghost".to_string())] class: String,
) -> impl IntoView {
    let (pending, set_pending) = use_pending();
    let auth = use_context::<AuthState>();
    let nav = use_navigate();
    let do_logout = Callback::new(move |_: ()| {
        if pending.get_untracked() {
            return;
        }
        set_pending.set(true);
        let auth = auth;
        let nav = nav.clone();
        spawn_local(async move {
            let _ = crate::api::logout().await;
            crate::api::clear_token();
            if let Some(a) = auth {
                a.set(None);
            }
            set_pending.set(false);
            nav("/login", Default::default());
        });
    });
    view! {
        <PendingButton
            label="Logout".to_string()
            pending=pending
            on_click=do_logout
            class=class
            busy_label="Logging out…".to_string()
        />
    }
}

// ---------------------------------------------------------------------------
// Dashboard (Home + Admin Overview)
// ---------------------------------------------------------------------------

#[component]
fn DashboardWidgets() -> impl IntoView {
    let (domains, set_domains) = signal(0u32);
    let (users, set_users) = signal(0u32);
    let (quota_used, set_quota_used) = signal(String::new());
    let (quota_total, set_quota_total) = signal(String::new());
    let (deferred, set_deferred) = signal(0u32);
    let (loading, set_loading) = signal(true);
    let (failed, set_failed) = signal(false);

    Effect::new(move |_| {
        spawn_local(async move {
            let mut any_fail = false;
            match crate::api::list_domains().await {
                Ok(r) => set_domains.set(rows_of(&val(&r)).len() as u32),
                Err(_) => any_fail = true,
            }
            match crate::api::list_users(None).await {
                Ok(r) => set_users.set(rows_of(&val(&r)).len() as u32),
                Err(_) => any_fail = true,
            }
            match crate::api::quota().await {
                Ok(r) => {
                    let v = val(&r);
                    set_quota_used.set(s(&v, "used_mb"));
                    set_quota_total.set(s(&v, "quota_mb"));
                }
                Err(_) => any_fail = true,
            }
            match crate::api::queues().await {
                Ok(r) => {
                    let v = val(&r);
                    set_deferred.set(f(&v, "deferred") as u32);
                }
                Err(_) => any_fail = true,
            }
            set_failed.set(any_fail);
            set_loading.set(false);
        });
    });

    view! {
        <div>
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get()).then(|| view! {
                <div>
                    <div class="grid">
                        <article class="stat-card card">
                            <span class="stat-label">"Domains"</span>
                            <span class="stat-value">{move || domains.get().to_string()}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Users"</span>
                            <span class="stat-value">{move || users.get().to_string()}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Quota (used / total MB)"</span>
                            <span class="stat-value">
                                {move || format!("{} / {}", quota_used.get(), quota_total.get())}
                            </span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Deferred queue"</span>
                            <span class="stat-value">{move || deferred.get().to_string()}</span>
                        </article>
                    </div>
                    {move || failed.get().then(|| view! {
                        <p class="muted">"Some widgets are unavailable (admin access required)."</p>
                    })}
                </div>
            })}
        </div>
    }
}

#[component]
pub fn Home() -> impl IntoView {
    let _ = require_auth();
    let (audit_rows, set_audit_rows) = signal(Vec::<Value>::new());

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(r) = crate::api::audit(None).await {
                let rows = rows_of(&val(&r));
                set_audit_rows.set(rows.into_iter().take(5).collect());
            }
        });
    });

    view! {
        <section class="card">
            <h1>"Dashboard"</h1>
            <p class="muted">"Mail platform overview."</p>
        </section>
        <DashboardWidgets/>
        <section class="card">
            <h2>"Quick links"</h2>
            <div class="toolbar">
                <a href="/admin" class="btn btn-primary">"Admin (domains, users, DKIM…)"</a>
                <a href="/admin" class="btn">"Config & tokens"</a>
                <a href="/mail" class="btn">"Webmail"</a>
                <a href="/login" class="btn btn-ghost">"Sign in"</a>
            </div>
        </section>
        <section class="card">
            <h2>"Recent audit"</h2>
            {move || if audit_rows.get().is_empty() {
                view! { <EmptyState text="No audit entries (or admin access required).".to_string()/> }.into_any()
            } else {
                view! {
                    <div class="table-wrap">
                        <table class="table">
                            <thead><tr><th>"Actor"</th><th>"Action"</th><th>"Target"</th><th>"Time"</th></tr></thead>
                            <tbody>
                                <For
                                    each=move || audit_rows.get()
                                    key=|v| pretty(v)
                                    children=move |v| {
                                        let actor = pick(&v, &["actor", "user", "email"]);
                                        let action = pick(&v, &["action", "event"]);
                                        let target = pick(&v, &["target", "object", "id"]);
                                        let when = pick(&v, &["time", "created_at", "when"]);
                                        view! {
                                            <tr>
                                                <td>{actor}</td>
                                                <td>{action}</td>
                                                <td>{target}</td>
                                                <td>{when}</td>
                                            </tr>
                                        }
                                    }
                                />
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </section>
    }
}

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

#[component]
pub fn LoginPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let totp = RwSignal::new(String::new());
    let (show_totp, set_show_totp) = signal(false);
    let (loading, set_loading) = signal(false);
    let (err, set_err) = signal(String::new());
    let auth = use_context::<AuthState>();
    let nav = use_navigate();

    let do_login = move || {
        let email_s = email.get().trim().to_string();
        let password_s = password.get();
        let totp_s = totp.get().trim().to_string();
        if email_s.is_empty() || password_s.is_empty() {
            set_err.set("Enter email and password.".to_string());
            return;
        }
        set_err.set(String::new());
        set_loading.set(true);
        let nav = nav.clone();
        let auth = auth;
        spawn_local(async move {
            let totp_opt = if totp_s.is_empty() {
                None
            } else {
                Some(totp_s.as_str())
            };
            match crate::api::login(&email_s, &password_s, totp_opt).await {
                Ok(r) => {
                    let v = val(&r);
                    let token = s(&v, "token");
                    let kind = s(&v, "kind");
                    if !token.is_empty() {
                        if let Some(a) = auth {
                            a.set(Some(token.clone()));
                        }
                        crate::api::set_token(&token);
                    }
                    set_loading.set(false);
                    if kind == "admin" || kind == "superadmin" {
                        nav("/admin", Default::default());
                    } else {
                        nav("/mail", Default::default());
                    }
                }
                Err(e) => {
                    set_loading.set(false);
                    set_err.set(e);
                }
            }
        });
    };

    view! {
        <section class="auth-wrap">
            <div class="card auth-card">
                <div class="auth-head">
                    <h2>"Sign in"</h2>
                    <p class="muted">"Access admin & webmail with your mail account (TOTP supported)."</p>
                </div>
                <form class="form" on:submit=move |ev| {
                    ev.prevent_default();
                    do_login();
                }>
                    <Field label="Email".to_string() value=email placeholder="you@example.com".to_string() input_type="email".to_string()/>
                    <Field label="Password".to_string() value=password placeholder="••••••••".to_string() input_type="password".to_string()/>
                    {move || show_totp.get().then(|| view! {
                        <Field label="TOTP code".to_string() value=totp placeholder="123456".to_string() input_type="text".to_string()/>
                    })}
                    <div class="form-row">
                        <button
                            type="button"
                            class="btn btn-ghost"
                            on:click=move |_| set_show_totp.set(!show_totp.get())
                        >
                            {move || if show_totp.get() { "Hide TOTP" } else { "Add TOTP code" }}
                        </button>
                        <button class="btn btn-primary" type="submit" disabled=move || loading.get()>
                            {move || if loading.get() { "Signing in…" } else { "Sign in" }}
                        </button>
                    </div>
                </form>
                {move || (!err.get().is_empty()).then(|| view! {
                    <ErrorCard error=err.get()/>
                })}
            </div>
        </section>
    }
}

// ---------------------------------------------------------------------------
// Admin shell
// ---------------------------------------------------------------------------

#[component]
pub fn AdminHome() -> impl IntoView {
    let _ = require_auth();
    let admin = require_admin();
    let (active, set_active) = signal("overview".to_string());

    let tabs: Vec<(&str, &str)> = vec![
        ("overview", "Overview"),
        ("domains", "Domains"),
        ("users", "Users"),
        ("aliases", "Aliases"),
        ("quota", "Quota"),
        ("queues", "Queues"),
        ("dkim", "DKIM"),
        ("filters", "Filters"),
        ("vacation", "Vacation"),
        ("dmarc", "DMARC"),
        ("audit", "Audit"),
        ("bans", "Bans"),
        ("tokens", "Tokens"),
        ("config", "Config"),
        ("account", "Account"),
    ];

    view! {
        <div class="admin-layout">
            <aside class="sidebar card">
                <h3>"Admin"</h3>
                <nav aria-label="Admin sections">
                    <For
                        each=move || tabs.clone()
                        key=|t| t.0.to_string()
                        children=move |(key, label)| {
                            let kc = key.to_string();
                            let kc_active = kc.clone();
                            view! {
                                <button
                                    type="button"
                                    class="side-link"
                                    class:active=move || active.get() == kc_active
                                    on:click=move |_| set_active.set(kc.clone())
                                >
                                    {label}
                                </button>
                            }
                        }
                    />
                </nav>
                <div class="sidebar-footer">
                    <LogoutButton class="btn btn-ghost side-logout".to_string()/>
                </div>
            </aside>
            <section class="content">
                {move || match admin.get() {
                    None => view! { <Spinner/> }.into_any(),
                    Some(false) => view! {
                        <div class="card empty">
                            <h2>"Admin access required"</h2>
                            <p class="muted">"Your account does not have the admin role. Sign in with an admin account to use these tools."</p>
                            <div class="toolbar">
                                <a href="/login" class="btn btn-primary">"Sign in"</a>
                                <a href="/mail" class="btn">"Open webmail"</a>
                            </div>
                        </div>
                    }.into_any(),
                    Some(true) => match active.get().as_str() {
                        "domains" => view! { <DomainsPanel/> }.into_any(),
                        "users" => view! { <UsersPanel/> }.into_any(),
                        "aliases" => view! { <AliasesPanel/> }.into_any(),
                        "quota" => view! { <QuotaPanel/> }.into_any(),
                        "queues" => view! { <QueuesPanel/> }.into_any(),
                        "dkim" => view! { <DkimPanel/> }.into_any(),
                        "filters" => view! { <FiltersPanel/> }.into_any(),
                        "vacation" => view! { <VacationPanel/> }.into_any(),
                        "dmarc" => view! { <DmarcPanel/> }.into_any(),
                        "audit" => view! { <AuditPanel/> }.into_any(),
                        "bans" => view! { <BansPanel/> }.into_any(),
                        "tokens" => view! { <TokensPanel/> }.into_any(),
                        "config" => view! { <ConfigPanel/> }.into_any(),
                        "account" => view! { <AccountPanel/> }.into_any(),
                        _ => view! { <OverviewPanel/> }.into_any(),
                    },
                }}
            </section>
        </div>
    }
}

#[component]
fn OverviewPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"Overview"</h2>
            <DashboardWidgets/>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Domains
// ---------------------------------------------------------------------------

#[component]
fn DomainsPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());
    let new_name = RwSignal::new(String::new());
    let new_desc = RwSignal::new(String::new());
    let (detail_title, set_detail_title) = signal(String::new());
    let (detail, set_detail) = signal(String::new());
    let (create_pending, set_create_pending) = use_pending();
    let (row_pending, set_row_pending) = use_pending();
    let (refresh_pending, set_refresh_pending) = use_pending();

    let load = move || {
        set_loading.set(true);
        set_refresh_pending.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::list_domains().await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_refresh_pending.set(false);
            set_loading.set(false);
        });
    };
    let load2 = load;
    Effect::new(move |_| load());

    let create = move || {
        if create_pending.get_untracked() {
            return;
        }
        let name = new_name.get().trim().to_string();
        if name.is_empty() {
            set_err.set("Domain name is required.".to_string());
            return;
        }
        let _desc = new_desc.get().trim().to_string(); // assumed create_domain(name) has no description param — not sent
        set_create_pending.set(true);
        spawn_local(async move {
            match crate::api::create_domain(&crate::api::CreateDomainReq {
                domain: name,
                description: None,
                catch_all: None,
            })
            .await
            {
                Ok(_) => {
                    new_name.set(String::new());
                    new_desc.set(String::new());
                    load2();
                }
                Err(e) => set_err.set(e),
            }
            set_create_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <div class="panel-head">
                <h2>"Domains"</h2>
                <PendingButton
                    label="Refresh".to_string()
                    pending=refresh_pending
                    class="btn btn-ghost".to_string()
                    on_click=Callback::new(move |_: ()| load2())
                />
            </div>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            <form class="form" on:submit=move |ev| { ev.prevent_default(); create(); }>
                <div class="form-row">
                    <Field label="Domain".to_string() value=new_name placeholder="example.com".to_string()/>
                    <Field label="Description (shown, not sent — api takes name only)".to_string() value=new_desc placeholder="".to_string()/>
                </div>
                <div class="form-actions">
                    <PendingSubmitButton label="Add domain".to_string() pending=create_pending busy_label="Adding…".to_string()/>
                </div>
            </form>
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No domains yet.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead>
                            <tr><th>"Domain"</th><th>"DKIM"</th><th>"Active"</th><th>"Actions"</th></tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| {
                                    let id = pick(&v, &["id", "uuid"]);
                                    let name = pick(&v, &["domain", "name"]);
                                    let dkim = if b(&v, "dkim_enabled") { "enabled" } else { "—" };
                                    let active = b(&v, "active");
                                    let reload = load2;
                                    let id_dns = id.clone();
                                    let id_verify = id.clone();
                                    let id_dkim = id.clone();
                                    let id_del = id.clone();
                                    view! {
                                        <tr>
                                            <td>{name.clone()}</td>
                                            <td>
                                                {move || if b(&v, "dkim_enabled") {
                                                    view! { <span class="badge ok">{dkim}</span> }.into_any()
                                                } else {
                                                    view! { <span>{dkim}</span> }.into_any()
                                                }}
                                            </td>
                                            <td>{move || if active { "active" } else { "inactive" }}</td>
                                            <td>
                                                <div class="toolbar">
                                                    <PendingButton
                                                        label="DNS".to_string()
                                                        pending=row_pending
                                                        class="btn small".to_string()
                                                        on_click=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_dns.clone();
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::domain_dns(&id_h).await {
                                                                    Ok(r) => {
                                                                        set_detail_title.set("DNS records".to_string());
                                                                        set_detail.set(pretty(&val(&r)));
                                                                    }
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                    <PendingButton
                                                        label="Verify".to_string()
                                                        pending=row_pending
                                                        class="btn small".to_string()
                                                        on_click=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_verify.clone();
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::domain_verify(&id_h).await {
                                                                    Ok(r) => {
                                                                        set_detail_title.set("Verify result".to_string());
                                                                        set_detail.set(pretty(&val(&r)));
                                                                    }
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                    <PendingButton
                                                        label="DKIM".to_string()
                                                        pending=row_pending
                                                        class="btn small".to_string()
                                                        on_click=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_dkim.clone();
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::dkim_generate(&id_h, &crate::api::DkimGenReq::default()).await {
                                                                    Ok(r) => {
                                                                        set_detail_title.set("DKIM generated".to_string());
                                                                        set_detail.set(pretty(&val(&r)));
                                                                    }
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                    <ConfirmButton
                                                        label="Delete".to_string()
                                                        confirm_label="Confirm delete".to_string()
                                                        danger=true
                                                        pending=row_pending
                                                        on_confirm=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_del.clone();
                                                            let reload = reload;
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::delete_domain(&id_h).await {
                                                                    Ok(_) => reload(),
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                </div>
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
            {move || (!detail.get().is_empty()).then(|| view! {
                <div class="card detail-box">
                    <div class="panel-head">
                        <h3>{move || detail_title.get()}</h3>
                        <button class="btn btn-ghost small" type="button" on:click=move |_| set_detail.set(String::new())>"Close"</button>
                    </div>
                    <pre class="code">{move || detail.get()}</pre>
                </div>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[component]
fn UsersPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());
    let (info, set_info) = signal(String::new());
    let new_email = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let new_quota = RwSignal::new("1024".to_string());
    let new_kind = RwSignal::new("user".to_string());
    let (reset_for, set_reset_for) = signal(Option::<String>::None);
    let reset_pw = RwSignal::new(String::new());
    let (create_pending, set_create_pending) = use_pending();
    let (row_pending, set_row_pending) = use_pending();
    let (refresh_pending, set_refresh_pending) = use_pending();

    let load = move || {
        set_loading.set(true);
        set_refresh_pending.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::list_users(None).await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_refresh_pending.set(false);
            set_loading.set(false);
        });
    };
    let load2 = load;
    Effect::new(move |_| load());

    let create = move || {
        if create_pending.get_untracked() {
            return;
        }
        let email = new_email.get().trim().to_string();
        let password = new_password.get();
        let quota = parse_i32(&new_quota.get(), 0);
        let kind = new_kind.get();
        if email.is_empty() || password.is_empty() {
            set_err.set("Email and password are required.".to_string());
            return;
        }
        set_create_pending.set(true);
        spawn_local(async move {
            match crate::api::create_user(&crate::api::CreateUserReq {
                email: email.clone(),
                password: password.clone(),
                kind: Some(kind.clone()),
                quota_mb: Some(quota as i64),
            })
            .await
            {
                Ok(_) => {
                    new_email.set(String::new());
                    new_password.set(String::new());
                    load2();
                }
                Err(e) => set_err.set(e),
            }
            set_create_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <div class="panel-head">
                <h2>"Users"</h2>
                <PendingButton
                    label="Refresh".to_string()
                    pending=refresh_pending
                    class="btn btn-ghost".to_string()
                    on_click=Callback::new(move |_: ()| load2())
                />
            </div>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || (!info.get().is_empty()).then(|| view! {
                <div class="card"><p class="muted">{move || info.get()}</p></div>
            })}
            <form class="form" on:submit=move |ev| { ev.prevent_default(); create(); }>
                <div class="form-row">
                    <Field label="Email".to_string() value=new_email placeholder="user@example.com".to_string()/>
                    <Field label="Password".to_string() value=new_password placeholder="••••••••".to_string() input_type="password".to_string()/>
                    <Field label="Quota (MB)".to_string() value=new_quota placeholder="1024".to_string()/>
                </div>
                <label class="field">
                    <span>"Kind"</span>
                    <select class="input" on:change=move |ev| new_kind.set(event_target_value(&ev))>
                        <option value="user">"user"</option>
                        <option value="relay">"relay"</option>
                        <option value="admin">"admin"</option>
                    </select>
                </label>
                <div class="form-actions">
                    <PendingSubmitButton label="Create user".to_string() pending=create_pending busy_label="Creating…".to_string()/>
                </div>
            </form>
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No users yet.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead>
                            <tr><th>"Email"</th><th>"Kind"</th><th>"Quota used"</th><th>"Active"</th><th>"Actions"</th></tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| {
                                    let id = pick(&v, &["id", "uuid"]);
                                    let email = pick(&v, &["email"]);
                                    let kind = pick(&v, &["kind"]);
                                    let used = f(&v, "used_mb");
                                    let total = f(&v, "quota_mb");
                                    let active = b(&v, "active");
                                    let pct_val = pct(used, total);
                                    let reload = load2;
                                    let id_toggle = id.clone();
                                    let id_reset_toggle = id.clone();
                                    let id_label = id.clone();
                                    let id_del = id.clone();
                                    let id_reset_row = id.clone();
                                    view! {
                                        <tr>
                                            <td>{email.clone()}</td>
                                            <td>{kind}</td>
                                            <td>
                                                {move || format!("{:.1} / {:.0} MB", used, total)}
                                                <progress class="progress" value=used.to_string() max=total.to_string()></progress>
                                                {move || format!("{:.0}%", pct_val)}
                                            </td>
                                            <td>
                                                {move || if active {
                                                    view! { <span class="badge ok">"active"</span> }.into_any()
                                                } else {
                                                    view! { <span class="badge warn">"disabled"</span> }.into_any()
                                                }}
                                            </td>
                                            <td>
                                                <div class="toolbar">
                                                    <PendingButton
                                                        label=if active { "Disable".to_string() } else { "Enable".to_string() }
                                                        pending=row_pending
                                                        class="btn small".to_string()
                                                        on_click=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_toggle.clone();
                                                            let reload = reload;
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::update_user(&id_h, &crate::api::UpdateUserReq { kind: None, quota_mb: None, active: Some(!(active)) }).await {
                                                                    Ok(_) => reload(),
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                    <PendingButton
                                                        label=if reset_for.get_untracked().as_deref() == Some(id_label.as_str()) { "Cancel reset".to_string() } else { "Reset password".to_string() }
                                                        pending=row_pending
                                                        class="btn small".to_string()
                                                        on_click=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            if reset_for.get().as_deref() == Some(id_reset_toggle.as_str()) {
                                                                set_reset_for.set(None);
                                                            } else {
                                                                reset_pw.set(String::new());
                                                                set_reset_for.set(Some(id_reset_toggle.clone()));
                                                            }
                                                        })
                                                    />
                                                    <ConfirmButton
                                                        label="Delete".to_string()
                                                        confirm_label="Confirm delete".to_string()
                                                        danger=true
                                                        pending=row_pending
                                                        on_confirm=Callback::new(move |_: ()| {
                                                            if row_pending.get_untracked() { return; }
                                                            let id_h = id_del.clone();
                                                            let reload = reload;
                                                            set_row_pending.set(true);
                                                            spawn_local(async move {
                                                                match crate::api::delete_user(&id_h).await {
                                                                    Ok(_) => reload(),
                                                                    Err(e) => set_err.set(e),
                                                                }
                                                                set_row_pending.set(false);
                                                            });
                                                        })
                                                    />
                                                </div>
                                                {move || {
                                                    let handler_id = id_reset_row.clone();
                                                    if reset_for.get().as_deref() == Some(handler_id.as_str()) {
                                                        view! {
                                                            <span class="toolbar">
                                                                <Field label="New password".to_string() value=reset_pw placeholder="".to_string() input_type="password".to_string()/>
                                                                <PendingButton
                                                                    label="Apply".to_string()
                                                                    pending=row_pending
                                                                    class="btn small btn-primary".to_string()
                                                                    on_click=Callback::new(move |_: ()| {
                                                                        if row_pending.get_untracked() { return; }
                                                                        let id_h = handler_id.clone();
                                                                        let pw = reset_pw.get();
                                                                        let reload = reload;
                                                                        set_row_pending.set(true);
                                                                        spawn_local(async move {
                                                                            match crate::api::reset_password(&id_h, &pw).await {
                                                                                Ok(_) => {
                                                                                    set_reset_for.set(None);
                                                                                    set_info.set("Password reset.".to_string());
                                                                                    reload();
                                                                                }
                                                                                Err(e) => set_err.set(e),
                                                                            }
                                                                            set_row_pending.set(false);
                                                                        });
                                                                    })
                                                                />
                                                            </span>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }
                                                }}
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Aliases
// ---------------------------------------------------------------------------

#[component]
fn AliasesPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());
    let addr = RwSignal::new(String::new());
    let goto = RwSignal::new(String::new());
    let (create_pending, set_create_pending) = use_pending();
    let (row_pending, set_row_pending) = use_pending();
    let (refresh_pending, set_refresh_pending) = use_pending();

    let load = move || {
        set_loading.set(true);
        set_refresh_pending.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::list_aliases().await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_refresh_pending.set(false);
            set_loading.set(false);
        });
    };
    let load2 = load;
    Effect::new(move |_| load());

    let create = move || {
        if create_pending.get_untracked() {
            return;
        }
        let a = addr.get().trim().to_string();
        let g = goto.get().trim().to_string();
        if a.is_empty() || g.is_empty() {
            set_err.set("Both address and goto are required.".to_string());
            return;
        }
        set_create_pending.set(true);
        spawn_local(async move {
            match crate::api::create_alias(&crate::api::CreateAliasReq {
                address: a.clone(),
                goto: g.clone(),
            })
            .await
            {
                Ok(_) => {
                    addr.set(String::new());
                    goto.set(String::new());
                    load2();
                }
                Err(e) => set_err.set(e),
            }
            set_create_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <div class="panel-head">
                <h2>"Aliases"</h2>
                <PendingButton
                    label="Refresh".to_string()
                    pending=refresh_pending
                    class="btn btn-ghost".to_string()
                    on_click=Callback::new(move |_: ()| load2())
                />
            </div>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            <form class="form" on:submit=move |ev| { ev.prevent_default(); create(); }>
                <div class="form-row">
                    <Field label="Address".to_string() value=addr placeholder="alias@example.com".to_string()/>
                    <Field label="Goto (comma separated)".to_string() value=goto placeholder="a@x, b@x".to_string()/>
                </div>
                <div class="form-actions">
                    <PendingSubmitButton label="Create alias".to_string() pending=create_pending busy_label="Creating…".to_string()/>
                </div>
            </form>
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No aliases yet.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"Address"</th><th>"Goto"</th><th>"Active"</th><th>"Actions"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| {
                                    let id = pick(&v, &["id", "uuid"]);
                                    let reload = load2;
                                    view! {
                                        <tr>
                                            <td>{pick(&v, &["address", "alias"])}</td>
                                            <td>{pick(&v, &["goto", "targets"])}</td>
                                            <td>{move || if b(&v, "active") { "active" } else { "inactive" }}</td>
                                            <td>
                                                <ConfirmButton
                                                    label="Delete".to_string()
                                                    confirm_label="Confirm delete".to_string()
                                                    danger=true
                                                    pending=row_pending
                                                    on_confirm=Callback::new(move |_: ()| {
                                                        if row_pending.get_untracked() { return; }
                                                        let id = id.clone();
                                                        let reload = reload;
                                                        set_row_pending.set(true);
                                                        spawn_local(async move {
                                                            match crate::api::delete_alias(&id).await {
                                                                Ok(_) => reload(),
                                                                Err(e) => set_err.set(e),
                                                            }
                                                            set_row_pending.set(false);
                                                        });
                                                    })
                                                />
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Quota / Queues
// ---------------------------------------------------------------------------

#[component]
fn QuotaPanel() -> impl IntoView {
    let (summary, set_summary) = signal(Value::Null);
    let (users, set_users) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::quota().await {
                Ok(r) => {
                    let v = val(&r);
                    set_users.set(rows_of(&v.clone()));
                    set_summary.set(v);
                }
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div class="panel">
            <h2>"Quota"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get()).then(|| view! {
                <div>
                    <div class="grid">
                        <article class="stat-card card">
                            <span class="stat-label">"Used (MB)"</span>
                            <span class="stat-value">{move || s(&summary.get(), "used_mb")}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Total (MB)"</span>
                            <span class="stat-value">{move || s(&summary.get(), "quota_mb")}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Mailboxes"</span>
                            <span class="stat-value">{move || users.get().len().to_string()}</span>
                        </article>
                    </div>
                    {move || if users.get().is_empty() {
                        view! { <EmptyState text="No per-user quota data.".to_string()/> }.into_any()
                    } else {
                        view! {
                            <div class="table-wrap">
                                <table class="table">
                                    <thead><tr><th>"User"</th><th>"Used / quota"</th><th>"Usage"</th></tr></thead>
                                    <tbody>
                                        <For
                                            each=move || users.get()
                                            key=|v| pretty(v)
                                            children=move |v| {
                                                let used = f(&v, "used_mb");
                                                let total = f(&v, "quota_mb");
                                                let p = pct(used, total);
                                                view! {
                                                    <tr>
                                                        <td>{pick(&v, &["email", "user", "address"])}</td>
                                                        <td>{move || format!("{:.1} / {:.0} MB", used, total)}</td>
                                                        <td>
                                                            <progress class="progress" value=used.to_string() max=total.to_string()></progress>
                                                            {move || format!("{:.0}%", p)}
                                                        </td>
                                                    </tr>
                                                }
                                            }
                                        />
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }}
                </div>
            })}
        </div>
    }
}

#[component]
fn QueuesPanel() -> impl IntoView {
    let (summary, set_summary) = signal(Value::Null);
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::queues().await {
                Ok(r) => set_summary.set(val(&r)),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div class="panel">
            <h2>"Queues"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get()).then(|| view! {
                <div>
                    <div class="grid">
                        <article class="stat-card card">
                            <span class="stat-label">"Deferred"</span>
                            <span class="stat-value">{move || pick(&summary.get(), &["deferred"]).if_empty("0")}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Active"</span>
                            <span class="stat-value">{move || pick(&summary.get(), &["active"]).if_empty("0")}</span>
                        </article>
                        <article class="stat-card card">
                            <span class="stat-label">"Hold"</span>
                            <span class="stat-value">{move || pick(&summary.get(), &["hold"]).if_empty("0")}</span>
                        </article>
                    </div>
                    <div class="card">
                        <h3>"Activity"</h3>
                        <pre class="code">{move || {
                            let v = summary.get();
                            let extra = pretty(&v);
                            if extra == "{}" { "No queue data.".to_string() } else { extra }
                        }}</pre>
                    </div>
                </div>
            })}
        </div>
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

// ---------------------------------------------------------------------------
// DKIM
// ---------------------------------------------------------------------------

#[component]
fn DkimPanel() -> impl IntoView {
    let (domains, set_domains) = signal(Vec::<Value>::new());
    let (selected, set_selected) = signal(String::new());
    let (out, set_out) = signal(String::new());
    let (existing, set_existing) = signal(Vec::<Value>::new());
    let (err, set_err) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let (gen_pending, set_gen_pending) = use_pending();
    let (keys_pending, set_keys_pending) = use_pending();

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::list_domains().await {
                Ok(r) => {
                    let rows = rows_of(&val(&r));
                    if let Some(first) = rows.first() {
                        set_selected.set(pick(first, &["id", "uuid"]));
                    }
                    set_domains.set(rows);
                }
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    let generate = move || {
        if gen_pending.get_untracked() {
            return;
        }
        let id = selected.get().trim().to_string();
        if id.is_empty() {
            set_err.set("Select a domain first.".to_string());
            return;
        }
        set_gen_pending.set(true);
        spawn_local(async move {
            match crate::api::dkim_generate(&id, &crate::api::DkimGenReq::default()).await {
                Ok(r) => {
                    set_out.set(pretty(&val(&r)));
                }
                Err(e) => set_err.set(e),
            }
            set_gen_pending.set(false);
        });
    };

    let load_keys = move || {
        if keys_pending.get_untracked() {
            return;
        }
        let id = selected.get().trim().to_string();
        if id.is_empty() {
            return;
        }
        set_keys_pending.set(true);
        spawn_local(async move {
            match crate::api::dkim_list(&id).await {
                Ok(r) => set_existing.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_keys_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <h2>"DKIM"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            <div class="form-row">
                <label class="field">
                    <span>"Domain"</span>
                    <select
                        class="input"
                        on:change=move |ev| set_selected.set(event_target_value(&ev))
                    >
                        <For
                            each=move || domains.get()
                            key=|v| pretty(v)
                            children=move |v| {
                                let id = pick(&v, &["id", "uuid"]);
                                let name = pick(&v, &["domain", "name"]);
                                view! {
                                    <option value=id.clone() selected=move || selected.get() == id>
                                        {name}
                                    </option>
                                }
                            }
                        />
                    </select>
                </label>
                <div class="form-actions">
                    <PendingButton
                        label="Generate key".to_string()
                        pending=gen_pending
                        on_click=Callback::new(move |_: ()| generate())
                    />
                    <PendingButton
                        label="Existing keys".to_string()
                        pending=keys_pending
                        class="btn".to_string()
                        on_click=Callback::new(move |_: ()| load_keys())
                    />
                </div>
            </div>
            {move || (!out.get().is_empty()).then(|| view! {
                <div class="card detail-box">
                    <h3>"Generated record"</h3>
                    <p class="muted">"Copy this DNS TXT record into your zone (select the text and press Ctrl/Cmd+C — the clipboard API is unavailable in this build)."</p>
                    <textarea class="input code" rows="8" readonly=true prop:value=move || out.get()></textarea>
                </div>
            })}
            {move || (!existing.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"Selector"</th><th>"Key type"</th><th>"DNS record"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || existing.get()
                                key=|v| pretty(v)
                                children=move |v| view! {
                                    <tr>
                                        <td>{pick(&v, &["selector", "name"])}</td>
                                        <td>{pick(&v, &["key_type", "type"])}</td>
                                        <td class="code">{pick(&v, &["dns_value", "dns_record", "public_key"])}</td>
                                    </tr>
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Filters / Vacation
// ---------------------------------------------------------------------------

#[component]
fn FiltersPanel() -> impl IntoView {
    let (users, set_users) = signal(Vec::<Value>::new());
    let (selected, set_selected) = signal(String::new());
    let sieve = RwSignal::new(String::new());
    let (err, set_err) = signal(String::new());
    let (info, set_info) = signal(String::new());
    let (save_pending, set_save_pending) = use_pending();

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::list_users(None).await {
                Ok(r) => {
                    let rows = rows_of(&val(&r));
                    if let Some(first) = rows.first() {
                        set_selected.set(pick(first, &["id", "uuid"]));
                    }
                    set_users.set(rows);
                }
                Err(e) => set_err.set(e),
            }
        });
    });

    // Auto-load sieve when user changes.
    Effect::new(move |_| {
        let id = selected.get();
        if id.is_empty() {
            return;
        }
        spawn_local(async move {
            match crate::api::filters_get(&id).await {
                Ok(r) => {
                    let v = val(&r);
                    if let Some(t) = v.as_str() {
                        sieve.set(t.to_string());
                    } else {
                        let text = pick(&v, &["sieve", "script", "content", "filter"]);
                        sieve.set(if text.is_empty() { pretty(&v) } else { text });
                    }
                }
                Err(e) => set_err.set(e),
            }
        });
    });

    let save = move || {
        if save_pending.get_untracked() {
            return;
        }
        let id = selected.get().trim().to_string();
        let text = sieve.get();
        if id.is_empty() {
            return;
        }
        set_save_pending.set(true);
        spawn_local(async move {
            match crate::api::filters_put(&id, &text).await {
                Ok(r) => {
                    set_info.set(format!("Saved ({})", pretty(&val(&r))));
                    set_err.set(String::new());
                }
                Err(e) => set_err.set(e),
            }
            set_save_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <h2>"Sieve filters"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || (!info.get().is_empty()).then(|| view! {
                <div class="card"><p class="muted">{move || info.get()}</p></div>
            })}
            <div class="form-row">
                <label class="field">
                    <span>"User"</span>
                    <select class="input" on:change=move |ev| set_selected.set(event_target_value(&ev))>
                        <For
                            each=move || users.get()
                            key=|v| pretty(v)
                            children=move |v| {
                                let id = pick(&v, &["id", "uuid"]);
                                let name = pick(&v, &["email"]);
                                view! {
                                    <option value=id.clone() selected=move || selected.get() == id>
                                        {name}
                                    </option>
                                }
                            }
                        />
                    </select>
                </label>
            </div>
            <TextAreaField label="Sieve script".to_string() value=sieve placeholder="require [\"fileinto\"]; …".to_string() rows=12/>
            <div class="form-actions">
                <PendingButton
                    label="Save filters".to_string()
                    pending=save_pending
                    on_click=Callback::new(move |_: ()| save())
                />
            </div>
        </div>
    }
}

#[component]
fn VacationPanel() -> impl IntoView {
    let (users, set_users) = signal(Vec::<Value>::new());
    let (selected, set_selected) = signal(String::new());
    let subject = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let start = RwSignal::new(String::new());
    let end = RwSignal::new(String::new());
    let (enabled, set_enabled) = signal(false);
    let (err, set_err) = signal(String::new());
    let (info, set_info) = signal(String::new());
    let (save_pending, set_save_pending) = use_pending();

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::list_users(None).await {
                Ok(r) => {
                    let rows = rows_of(&val(&r));
                    if let Some(first) = rows.first() {
                        set_selected.set(pick(first, &["id", "uuid"]));
                    }
                    set_users.set(rows);
                }
                Err(e) => set_err.set(e),
            }
        });
    });

    Effect::new(move |_| {
        let id = selected.get();
        if id.is_empty() {
            return;
        }
        spawn_local(async move {
            match crate::api::vacation_get(&id).await {
                Ok(r) => {
                    let v = val(&r);
                    set_enabled.set(b(&v, "enabled"));
                    subject.set(pick(&v, &["subject"]));
                    body.set(pick(&v, &["body", "message"]));
                    start.set(pick(&v, &["start", "start_date", "from"]));
                    end.set(pick(&v, &["end", "end_date", "to"]));
                }
                Err(e) => set_err.set(e),
            }
        });
    });

    let save = move || {
        if save_pending.get_untracked() {
            return;
        }
        let id = selected.get().trim().to_string();
        if id.is_empty() {
            return;
        }
        let payload = json!({
            "enabled": enabled.get(),
            "subject": subject.get(),
            "body": body.get(),
            "start": start.get(),
            "end": end.get(),
        });
        set_save_pending.set(true);
        spawn_local(async move {
            match crate::api::vacation_put(&id, payload).await {
                Ok(r) => {
                    set_info.set(format!("Saved ({})", pretty(&val(&r))));
                    set_err.set(String::new());
                }
                Err(e) => set_err.set(e),
            }
            set_save_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <h2>"Vacation responder"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || (!info.get().is_empty()).then(|| view! {
                <div class="card"><p class="muted">{move || info.get()}</p></div>
            })}
            <div class="form-row">
                <label class="field">
                    <span>"User"</span>
                    <select class="input" on:change=move |ev| set_selected.set(event_target_value(&ev))>
                        <For
                            each=move || users.get()
                            key=|v| pretty(v)
                            children=move |v| {
                                let id = pick(&v, &["id", "uuid"]);
                                let name = pick(&v, &["email"]);
                                view! {
                                    <option value=id.clone() selected=move || selected.get() == id>
                                        {name}
                                    </option>
                                }
                            }
                        />
                    </select>
                </label>
                <label class="field">
                    <span>"Enabled"</span>
                    <input
                        class="input"
                        type="checkbox"
                        prop:checked=move || enabled.get()
                        on:click=move |_| set_enabled.set(!enabled.get())
                    />
                </label>
            </div>
            <div class="form-row">
                <Field label="Subject".to_string() value=subject placeholder="On vacation".to_string()/>
                <Field label="Start (YYYY-MM-DD)".to_string() value=start placeholder="".to_string()/>
                <Field label="End (YYYY-MM-DD)".to_string() value=end placeholder="".to_string()/>
            </div>
            <TextAreaField label="Body".to_string() value=body placeholder="I am on vacation and will reply…".to_string() rows=6/>
            <div class="form-actions">
                <PendingButton
                    label="Save vacation".to_string()
                    pending=save_pending
                    on_click=Callback::new(move |_: ()| save())
                />
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// DMARC / Audit / Bans
// ---------------------------------------------------------------------------

#[component]
fn DmarcPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::dmarc_list().await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div class="panel">
            <h2>"DMARC reports"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No DMARC reports.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"Domain"</th><th>"Report ID"</th><th>"Type"</th><th>"Details"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| view! {
                                    <tr>
                                        <td>{pick(&v, &["domain"])}</td>
                                        <td>{pick(&v, &["report_id", "id"])}</td>
                                        <td>{pick(&v, &["report_type", "type"])}</td>
                                        <td class="code">{pretty(&v)}</td>
                                    </tr>
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

#[component]
fn AuditPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::audit(None).await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div class="panel">
            <h2>"Audit log"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No audit entries.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"Actor"</th><th>"Action"</th><th>"Target"</th><th>"Time"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| view! {
                                    <tr>
                                        <td>{pick(&v, &["actor", "user", "email"])}</td>
                                        <td>{pick(&v, &["action", "event"])}</td>
                                        <td>{pick(&v, &["target", "object", "id"])}</td>
                                        <td>{pick(&v, &["time", "created_at", "when"])}</td>
                                    </tr>
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

#[component]
fn BansPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            match crate::api::bans().await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    });

    view! {
        <div class="panel">
            <h2>"Bans"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No bans.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"IP / subject"</th><th>"Reason"</th><th>"Until"</th><th>"Details"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| view! {
                                    <tr>
                                        <td>{pick(&v, &["ip", "subject", "email", "id"])}</td>
                                        <td>{pick(&v, &["reason"])}</td>
                                        <td>{pick(&v, &["until", "expires_at", "created_at"])}</td>
                                        <td class="code">{pretty(&v)}</td>
                                    </tr>
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

// ---------------------------------------------------------------------------
// API tokens / Config
// ---------------------------------------------------------------------------

#[component]
fn TokensPanel() -> impl IntoView {
    let (rows, set_rows) = signal(Vec::<Value>::new());
    let (loading, set_loading) = signal(true);
    let (err, set_err) = signal(String::new());
    let name = RwSignal::new(String::new());
    let scopes = RwSignal::new(String::new());
    let expires = RwSignal::new("30".to_string());
    let (created, set_created) = signal(String::new());
    let (create_pending, set_create_pending) = use_pending();
    let (row_pending, set_row_pending) = use_pending();
    let (refresh_pending, set_refresh_pending) = use_pending();

    let load = move || {
        set_loading.set(true);
        set_refresh_pending.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::tokens_list().await {
                Ok(r) => set_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_refresh_pending.set(false);
            set_loading.set(false);
        });
    };
    let load2 = load;
    Effect::new(move |_| load());

    let create = move || {
        if create_pending.get_untracked() {
            return;
        }
        let nm = name.get().trim().to_string();
        let scope_list: Vec<String> = scopes
            .get()
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
        let days = parse_i64(&expires.get(), 30);
        set_create_pending.set(true);
        spawn_local(async move {
            match crate::api::tokens_create(&nm, scope_list, days).await {
                Ok(r) => {
                    set_created.set(pretty(&val(&r)));
                    name.set(String::new());
                    scopes.set(String::new());
                    load2();
                }
                Err(e) => set_err.set(e),
            }
            set_create_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <div class="panel-head">
                <h2>"API tokens"</h2>
                <PendingButton
                    label="Refresh".to_string()
                    pending=refresh_pending
                    class="btn btn-ghost".to_string()
                    on_click=Callback::new(move |_: ()| load2())
                />
            </div>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            <form class="form" on:submit=move |ev| { ev.prevent_default(); create(); }>
                <div class="form-row">
                    <Field label="Name".to_string() value=name placeholder="ci deploy".to_string()/>
                    <Field label="Scopes (comma separated)".to_string() value=scopes placeholder="mail:read, mail:send".to_string()/>
                    <Field label="Expires (days)".to_string() value=expires placeholder="30".to_string()/>
                </div>
                <div class="form-actions">
                    <PendingSubmitButton label="Create token".to_string() pending=create_pending busy_label="Creating…".to_string()/>
                </div>
            </form>
            {move || (!created.get().is_empty()).then(|| view! {
                <div class="card detail-box">
                    <h3>"Created — copy the plaintext token now"</h3>
                    <textarea class="input code" rows="4" readonly=true prop:value=move || created.get()></textarea>
                </div>
            })}
            {move || loading.get().then(|| view! { <Spinner/> })}
            {move || (!loading.get() && rows.get().is_empty()).then(|| view! {
                <EmptyState text="No tokens.".to_string()/>
            })}
            {move || (!rows.get().is_empty()).then(|| view! {
                <div class="table-wrap">
                    <table class="table">
                        <thead><tr><th>"Name"</th><th>"Scopes"</th><th>"Expires"</th><th>"Actions"</th></tr></thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=|v| pretty(v)
                                children=move |v| {
                                    let id = pick(&v, &["id", "uuid"]);
                                    let reload = load2;
                                    view! {
                                        <tr>
                                            <td>{pick(&v, &["name", "prefix"]).if_empty("—")}</td>
                                            <td>{pick(&v, &["scopes"])}</td>
                                            <td>{pick(&v, &["expires_at", "expires_in_days"])}</td>
                                            <td>
                                                <ConfirmButton
                                                    label="Revoke".to_string()
                                                    confirm_label="Confirm revoke".to_string()
                                                    danger=true
                                                    pending=row_pending
                                                    on_confirm=Callback::new(move |_: ()| {
                                                        if row_pending.get_untracked() { return; }
                                                        let id = id.clone();
                                                        let reload = reload;
                                                        set_row_pending.set(true);
                                                        spawn_local(async move {
                                                            match crate::api::tokens_revoke(&id).await {
                                                                Ok(_) => reload(),
                                                                Err(e) => set_err.set(e),
                                                            }
                                                            set_row_pending.set(false);
                                                        });
                                                    })
                                                />
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            })}
        </div>
    }
}

#[component]
fn ConfigPanel() -> impl IntoView {
    let (err, set_err) = signal(String::new());
    let (info, set_info) = signal(String::new());
    let (download_uri, set_download_uri) = signal(String::new());
    let import_text = RwSignal::new(String::new());
    let (export_pending, set_export_pending) = use_pending();
    let (import_pending, set_import_pending) = use_pending();

    let export = move || {
        if export_pending.get_untracked() {
            return;
        }
        set_export_pending.set(true);
        spawn_local(async move {
            match crate::api::config_export().await {
                Ok(text) => {
                    set_download_uri.set(format!(
                        "data:application/json;charset=utf-8,{}",
                        percent_encode(&text)
                    ));
                    import_text.set(text);
                    set_info
                        .set("Exported. Use the download link or the textarea below.".to_string());
                }
                Err(e) => set_err.set(e),
            }
            set_export_pending.set(false);
        });
    };

    let import = move || {
        if import_pending.get_untracked() {
            return;
        }
        let text = import_text.get();
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => {
                set_import_pending.set(true);
                spawn_local(async move {
                    match crate::api::config_import(v).await {
                        Ok(r) => set_info.set(format!("Imported: {}", pretty(&val(&r)))),
                        Err(e) => set_err.set(e),
                    }
                    set_import_pending.set(false);
                });
            }
            Err(e) => set_err.set(format!("Invalid JSON: {}", e)),
        }
    };

    view! {
        <div class="panel">
            <h2>"Config"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || (!info.get().is_empty()).then(|| view! {
                <div class="card"><p class="muted">{move || info.get()}</p></div>
            })}
            <div class="card">
                <h3>"Export"</h3>
                <p class="muted">"Download the full server config as JSON (users are skipped by design)."</p>
                <div class="toolbar">
                    <PendingButton
                        label="Export config".to_string()
                        pending=export_pending
                        on_click=Callback::new(move |_: ()| export())
                    />
                    {move || (!download_uri.get().is_empty()).then(|| view! {
                        <a href=move || download_uri.get() download="synthmail-config.json" class="btn">"Download JSON"</a>
                    })}
                </div>
            </div>
            <div class="card">
                <h3>"Import"</h3>
                <p class="muted">"Paste a previously exported config JSON and apply it."</p>
                <TextAreaField label="Config JSON".to_string() value=import_text placeholder="{ … }".to_string() rows=10/>
                <div class="form-actions">
                    <PendingButton
                        label="Import config".to_string()
                        pending=import_pending
                        on_click=Callback::new(move |_: ()| import())
                    />
                </div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Account (profile, password, TOTP, push)
// ---------------------------------------------------------------------------

#[component]
pub fn Account() -> impl IntoView {
    view! { <AccountPanel/> }
}

#[component]
fn AccountPanel() -> impl IntoView {
    let theme = use_context::<ThemeSignal>();
    let (me, set_me) = signal(Value::Null);
    let (err, set_err) = signal(String::new());
    let (info, set_info) = signal(String::new());
    let new_pw = RwSignal::new(String::new());
    let totp_pw = RwSignal::new(String::new());
    let totp_code = RwSignal::new(String::new());
    let disable_pw = RwSignal::new(String::new());
    let (totp_out, set_totp_out) = signal(String::new());
    let ep = RwSignal::new(String::new());
    let p256dh = RwSignal::new(String::new());
    let push_auth = RwSignal::new(String::new());
    let (push_rows, set_push_rows) = signal(Vec::<Value>::new());
    let (pw_pending, set_pw_pending) = use_pending();
    let (totp_pending, set_totp_pending) = use_pending();
    let (push_pending, set_push_pending) = use_pending();
    let (push_row_pending, set_push_row_pending) = use_pending();

    let load_me = move || {
        spawn_local(async move {
            match crate::api::me().await {
                Ok(r) => set_me.set(val(&r)),
                Err(e) => set_err.set(e),
            }
        });
    };
    let load_me2 = load_me;
    Effect::new(move |_| load_me());

    let load_push = move || {
        spawn_local(async move {
            match crate::api::push_list().await {
                Ok(r) => set_push_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
        });
    };
    let load_push2 = load_push;
    Effect::new(move |_| load_push());

    let change_pw = move || {
        if pw_pending.get_untracked() {
            return;
        }
        let id = pick(&me.get(), &["id", "uuid"]);
        let pw = new_pw.get();
        if id.is_empty() || pw.is_empty() {
            set_err.set("New password is required.".to_string());
            return;
        }
        set_pw_pending.set(true);
        spawn_local(async move {
            match crate::api::reset_password(&id, &pw).await {
                Ok(_) => {
                    new_pw.set(String::new());
                    set_info.set("Password changed.".to_string());
                }
                Err(e) => set_err.set(e),
            }
            set_pw_pending.set(false);
        });
    };

    view! {
        <div class="panel">
            <h2>"Account"</h2>
            {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
            {move || (!info.get().is_empty()).then(|| view! {
                <div class="card"><p class="muted">{move || info.get()}</p></div>
            })}
            <div class="card">
                <h3>"Profile"</h3>
                <p class="muted">{move || format!("{} · kind: {}", pick(&me.get(), &["email"]), pick(&me.get(), &["kind"]))}</p>
                <LevelHelp/>
            </div>
            <div class="card">
                <h3>"Change password"</h3>
                <form class="form" on:submit=move |ev| { ev.prevent_default(); change_pw(); }>
                    <Field label="New password".to_string() value=new_pw placeholder="••••••••".to_string() input_type="password".to_string()/>
                    <div class="form-actions">
                        <PendingSubmitButton label="Change password".to_string() pending=pw_pending busy_label="Changing…".to_string()/>
                    </div>
                </form>
                <div class="form-actions">
                    <LogoutButton/>
                </div>
            </div>
            <div class="card">
                <h3>"TOTP two-factor"</h3>
                <div class="form-row">
                    <Field label="Password".to_string() value=totp_pw placeholder="current password".to_string() input_type="password".to_string()/>
                    <Field label="Code (optional at enroll)".to_string() value=totp_code placeholder="123456".to_string()/>
                </div>
                <div class="form-actions">
                    <PendingButton
                        label="Enroll / show secret".to_string()
                        pending=totp_pending
                        on_click=Callback::new(move |_: ()| {
                            if totp_pending.get_untracked() { return; }
                            let pw = totp_pw.get();
                            let code_s = totp_code.get().trim().to_string();
                            set_totp_pending.set(true);
                            spawn_local(async move {
                                let code_opt = if code_s.is_empty() { None } else { Some(code_s.as_str()) };
                                match crate::api::totp_enroll(&pw, code_opt).await {
                                    Ok(r) => set_totp_out.set(pretty(&val(&r))),
                                    Err(e) => set_err.set(e),
                                }
                                set_totp_pending.set(false);
                            });
                        })
                    />
                    <PendingButton
                        label="Verify code".to_string()
                        pending=totp_pending
                        class="btn".to_string()
                        on_click=Callback::new(move |_: ()| {
                            if totp_pending.get_untracked() { return; }
                            let code = totp_code.get().trim().to_string();
                            set_totp_pending.set(true);
                            spawn_local(async move {
                                match crate::api::totp_verify(&code).await {
                                    Ok(r) => set_totp_out.set(pretty(&val(&r))),
                                    Err(e) => set_err.set(e),
                                }
                                set_totp_pending.set(false);
                            });
                        })
                    />
                </div>
                {move || (!totp_out.get().is_empty()).then(|| view! {
                    <textarea class="input code" rows="6" readonly=true prop:value=move || totp_out.get()></textarea>
                })}
                <div class="form-row">
                    <Field label="Password".to_string() value=disable_pw placeholder="current password".to_string() input_type="password".to_string()/>
                    <PendingButton
                        label="Disable TOTP".to_string()
                        pending=totp_pending
                        class="btn btn-danger".to_string()
                        on_click=Callback::new(move |_: ()| {
                            if totp_pending.get_untracked() { return; }
                            let pw = disable_pw.get();
                            set_totp_pending.set(true);
                            spawn_local(async move {
                                match crate::api::totp_disable(&pw).await {
                                    Ok(r) => set_totp_out.set(pretty(&val(&r))),
                                    Err(e) => set_err.set(e),
                                }
                                set_totp_pending.set(false);
                            });
                        })
                    />
                </div>
            </div>
            <div class="card">
                <h3>"Push subscriptions"</h3>
                <div class="form-row">
                    <Field label="Endpoint".to_string() value=ep placeholder="https://…/push/…".to_string()/>
                    <Field label="p256dh".to_string() value=p256dh placeholder="".to_string()/>
                    <Field label="auth".to_string() value=push_auth placeholder="".to_string()/>
                </div>
                <div class="form-actions">
                    <PendingButton
                        label="Subscribe".to_string()
                        pending=push_pending
                        on_click=Callback::new(move |_: ()| {
                            if push_pending.get_untracked() { return; }
                            let e = ep.get().trim().to_string();
                            let k = p256dh.get().trim().to_string();
                            let a = push_auth.get().trim().to_string();
                            let reload = load_push2;
                            set_push_pending.set(true);
                            spawn_local(async move {
                                match crate::api::push_create(&e, &k, &a).await {
                                    Ok(_) => {
                                        ep.set(String::new());
                                        p256dh.set(String::new());
                                        push_auth.set(String::new());
                                        reload();
                                    }
                                    Err(e) => set_err.set(e),
                                }
                                set_push_pending.set(false);
                            });
                        })
                    />
                </div>
                {move || if push_rows.get().is_empty() {
                    view! { <EmptyState text="No push subscriptions.".to_string()/> }.into_any()
                } else {
                    view! {
                        <div class="table-wrap">
                            <table class="table">
                                <thead><tr><th>"Endpoint"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    <For
                                        each=move || push_rows.get()
                                        key=|v| pretty(v)
                                        children=move |v| {
                                            let id = pick(&v, &["id", "uuid"]);
                                            let reload = load_push2;
                                            view! {
                                                <tr>
                                                    <td class="code">{pick(&v, &["endpoint"])}</td>
                                                    <td>
                                                        <ConfirmButton
                                                            label="Delete".to_string()
                                                            confirm_label="Confirm delete".to_string()
                                                            danger=true
                                                            pending=push_row_pending
                                                            on_confirm=Callback::new(move |_: ()| {
                                                                if push_row_pending.get_untracked() { return; }
                                                                let id = id.clone();
                                                                let reload = reload;
                                                                set_push_row_pending.set(true);
                                                                spawn_local(async move {
                                                                    match crate::api::push_delete(&id).await {
                                                                        Ok(_) => reload(),
                                                                        Err(e) => set_err.set(e),
                                                                    }
                                                                    set_push_row_pending.set(false);
                                                                });
                                                            })
                                                        />
                                                    </td>
                                                </tr>
                                            }
                                        }
                                    />
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
                <p class="muted">"Theme: {move || theme.map(|t| t.get()).unwrap_or_default()}"</p>
                <button class="btn btn-ghost" type="button" on:click=move |_| {
                    if let Some(t) = theme {
                        let next = if t.get() == "dark" { "light" } else { "dark" };
                        t.set(next.to_string());
                        load_me2();
                    }
                }>"Toggle theme"</button>
            </div>
        </div>
    }
}

/// Tiny static helper view (keeps the profile card honest about role gating).
#[component]
fn LevelHelp() -> impl IntoView {
    view! {
        <p class="muted small">"Admin accounts see the admin console; regular accounts use webmail."</p>
    }
}

// ---------------------------------------------------------------------------
// Webmail
// ---------------------------------------------------------------------------

// MailDash-style three-column shell (plan: "Layout shell"). `lib.rs` provides
// only the top bar + `<main>` — no sidebar — so this page renders its own
// dark folder sidebar (folders / message list / reading pane) standalone.
// NOTE: `WebmailHomeLegacy` below is the pre-redesign page (unreferenced,
// kept for reference in this round); `WebmailHome` is the deliverable.
#[component]
pub fn WebmailHome() -> impl IntoView {
    let _ = require_auth();
    let (folders, set_folders) = signal(Vec::<String>::new());
    let (folder, set_folder) = signal("INBOX".to_string());
    let (messages, set_messages) = signal(Vec::<crate::api::MessageHeader>::new());
    let (selected, set_selected) = signal(Option::<u32>::None);
    let (detail, set_detail) = signal(Option::<crate::api::MessageFull>::None);
    let (err, set_err) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let query = RwSignal::new(String::new());
    let (search_rows, set_search_rows) = signal(Vec::<crate::api::SearchResult>::new());
    let (search_active, set_search_active) = signal(false);
    let (show_compose, set_show_compose) = signal(false);
    let to = RwSignal::new(String::new());
    let subject = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let from_field = RwSignal::new(String::new());
    let (sent_info, set_sent_info) = signal(String::new());
    let (drafts, set_drafts) = signal(Vec::<crate::api::MessageHeader>::new());
    let (jmap_out, set_jmap_out) = signal(String::new());
    let (quota_used, set_quota_used) = signal(0i64);
    let (quota_total, set_quota_total) = signal(0i64);

    let me_email = RwSignal::new(String::new());
    let reply_body = RwSignal::new(String::new());
    let (compose_err, set_compose_err) = signal(String::new());
    let (compose_pending, set_compose_pending) = use_pending();
    let (search_pending, set_search_pending) = use_pending();
    let (drafts_pending, set_drafts_pending) = use_pending();
    let (jmap_pending, set_jmap_pending) = use_pending();

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(r) = crate::api::me().await {
                me_email.set(r.email.clone());
                from_field.set(r.email);
            }
            match crate::api::mail_folders().await {
                Ok(r) => set_folders.set(r.folders),
                Err(e) => set_err.set(e),
            }
            if let Ok(q) = crate::api::quota().await {
                set_quota_used.set(q.used_mb);
                set_quota_total.set(q.quota_mb);
            }
        });
    });

    let load_messages = move || {
        let fld = folder.get();
        set_loading.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::mail_messages(&fld, 25u32).await {
                Ok(r) => set_messages.set(r.messages),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    };
    let load_messages2 = load_messages;
    Effect::new(move |_| load_messages());

    Effect::new(move |_| {
        let uid = selected.get();
        if uid.is_none() {
            return;
        }
        let uid = uid.unwrap();
        spawn_local(async move {
            match crate::api::mail_message(uid).await {
                Ok(r) => set_detail.set(Some(r)),
                Err(e) => set_err.set(e),
            }
        });
    });

    let search = move || {
        if search_pending.get_untracked() {
            return;
        }
        let q = query.get().trim().to_string();
        if q.is_empty() {
            set_search_active.set(false);
            set_search_rows.set(Vec::new());
            return;
        }
        set_search_pending.set(true);
        spawn_local(async move {
            match crate::api::mail_search(&q).await {
                Ok(r) => {
                    set_search_rows.set(r.results);
                    set_search_active.set(true);
                }
                Err(e) => set_err.set(e),
            }
            set_search_pending.set(false);
        });
    };

    let send = move || {
        if compose_pending.get_untracked() {
            return;
        }
        let mut from = from_field.get();
        if from.trim().is_empty() {
            from = me_email.get();
        }
        let t = to.get().trim().to_string();
        let sj = subject.get();
        let bd = body.get();
        if from.trim().is_empty() {
            set_compose_err
                .set("From address is required (signing in again may fix this).".to_string());
            return;
        }
        if t.is_empty() || sj.trim().is_empty() || bd.trim().is_empty() {
            set_compose_err.set("To, subject, and body are all required.".to_string());
            return;
        }
        set_compose_err.set(String::new());
        set_compose_pending.set(true);
        let reload = load_messages2;
        spawn_local(async move {
            match crate::api::mail_send(&crate::api::SendMailReq {
                from,
                to: t,
                subject: sj,
                body: bd,
            })
            .await
            {
                Ok(_r) => {
                    set_sent_info.set("Message sent".to_string());
                    to.set(String::new());
                    subject.set(String::new());
                    body.set(String::new());
                    set_show_compose.set(false);
                    set_search_active.set(false);
                    set_search_rows.set(Vec::new());
                    reload();
                }
                Err(e) => set_compose_err.set(e),
            }
            set_compose_pending.set(false);
        });
    };

    let load_drafts = move || {
        if drafts_pending.get_untracked() {
            return;
        }
        set_drafts_pending.set(true);
        spawn_local(async move {
            match crate::api::mail_drafts().await {
                Ok(r) => set_drafts.set(r.drafts),
                Err(e) => set_err.set(e),
            }
            set_drafts_pending.set(false);
        });
    };

    view! {
        <div class="mail-layout mail-shell">
            <aside class="sidebar card folders mail-sidebar">
                <div class="mail-brand">
                    <span class="brand-mark" aria-hidden="true">"✉"</span>
                    <strong>"SynthMail"</strong>
                </div>
                <button
                    class="btn btn-primary btn-new mail-new full"
                    type="button"
                    on:click=move |_| {
                        set_compose_err.set(String::new());
                        set_show_compose.set(true);
                    }
                >
                    "✉ New Message"
                </button>
                <h3>"Folders"</h3>
                <ul class="folder-list">
                    <For
                        each=move || folders.get()
                        key=|n| n.clone()
                        children=move |name: String| {
                            let name_active = name.clone();
                            let name_label = name.clone();
                            let name_click = name.clone();
                            view! {
                                <li>
                                    <button
                                        class="folder mail-folder"
                                        class:active=move || folder.get() == name_active
                                        disabled=move || loading.get()
                                        on:click=move |_| {
                                            if loading.get_untracked() { return; }
                                            set_folder.set(name_click.clone());
                                            set_selected.set(None);
                                            set_search_active.set(false);
                                            set_search_rows.set(Vec::new());
                                        }
                                    >
                                        {name_label.clone()}
                                        {move || {
                                            if name_label.eq_ignore_ascii_case("inbox") {
                                                let unread = messages.get().iter().filter(|m| !m.seen).count();
                                                if unread > 0 {
                                                    view! { <span class="badge unread-badge">{unread.to_string()}</span> }.into_any()
                                                } else {
                                                    view! { <span></span> }.into_any()
                                                }
                                            } else {
                                                view! { <span></span> }.into_any()
                                            }
                                        }}
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
                <button
                    class="btn btn-ghost mail-custom-folder"
                    type="button"
                    title="Coming soon"
                    disabled=true
                >
                    "+ Custom Folder"
                </button>
                <button
                    class="btn btn-ghost mail-archive"
                    type="button"
                    title="Coming soon"
                    disabled=true
                >
                    "Archive"
                </button>
                <button
                    class="btn btn-ghost mail-settings"
                    type="button"
                    title="Coming soon"
                    disabled=true
                >
                    "Email/set"
                </button>
                <div class="storage-meter">
                    <p class="muted small">
                        {move || {
                            let used = quota_used.get();
                            let total = if quota_total.get() <= 0 { 200 } else { quota_total.get() };
                            let pct_val = pct(used as f64, total as f64);
                            format!("{} MB ({:.0}%) of {} MB used", used, pct_val, total)
                        }}
                    </p>
                    <progress
                        class="progress storage-bar"
                        value=move || quota_used.get().to_string()
                        max=move || if quota_total.get() <= 0 { 200 } else { quota_total.get() }.to_string()
                    ></progress>
                </div>
                <PendingButton
                    label="Show drafts".to_string()
                    pending=drafts_pending
                    class="btn".to_string()
                    on_click=Callback::new(move |_: ()| load_drafts())
                />
                {move || (!drafts.get().is_empty()).then(move || view! {
                    <ul class="folder-list">
                        <For
                            each=move || drafts.get()
                            key=|d: &crate::api::MessageHeader| d.uid
                            children=move |d: crate::api::MessageHeader| {
                                let uid = d.uid;
                                let subj = d.subject.clone();
                                view! {
                                    <li>
                                        <span class="muted small">{subj}</span>
                                        <PendingButton
                                            label="Delete draft".to_string()
                                            pending=drafts_pending
                                            class="btn btn-ghost small".to_string()
                                            on_click=Callback::new(move |_: ()| {
                                                if drafts_pending.get_untracked() { return; }
                                                set_drafts_pending.set(true);
                                                spawn_local(async move {
                                                    match crate::api::mail_draft_delete(uid).await {
                                                        Ok(_) => load_drafts(),
                                                        Err(e) => set_err.set(e),
                                                    }
                                                    set_drafts_pending.set(false);
                                                });
                                            })
                                        />
                                    </li>
                                }
                            }
                        />
                    </ul>
                })}
                <PendingButton
                    label="JMAP probe".to_string()
                    pending=jmap_pending
                    class="btn btn-ghost".to_string()
                    on_click=Callback::new(move |_: ()| {
                        if jmap_pending.get_untracked() { return; }
                        set_jmap_pending.set(true);
                        let calls = json!([["Mailbox/query", { "filter": {} }, "q"]]);
                        spawn_local(async move {
                            match crate::api::jmap_request(calls).await {
                                Ok(r) => set_jmap_out.set(pretty(&r)),
                                Err(e) => set_err.set(e),
                            }
                            set_jmap_pending.set(false);
                        });
                    })
                />
                {move || (!jmap_out.get().is_empty()).then(|| view! {
                    <pre class="code">{move || jmap_out.get()}</pre>
                })}
                <div class="sidebar-footer">
                    <LogoutButton class="btn btn-ghost side-logout".to_string()/>
                </div>
            </aside>

            <section class="card message-pane mail-list-pane">
                {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
                {move || (!sent_info.get().is_empty()).then(|| view! {
                    <div class="toast" role="status"><p>{move || sent_info.get()}</p></div>
                })}
                <div class="search-row">
                    <input
                        class="input"
                        type="search"
                        placeholder="Search messages (FTS)…"
                        prop:value=move || query.get()
                        on:input=move |ev| {
                            query.set(event_target_value(&ev));
                            if query.get_untracked().trim().is_empty() {
                                set_search_active.set(false);
                                set_search_rows.set(Vec::new());
                            }
                        }
                    />
                    <PendingButton
                        label="Search".to_string()
                        pending=search_pending
                        class="btn".to_string()
                        on_click=Callback::new(move |_: ()| search())
                    />
                    {move || (!query.get().is_empty()).then(|| view! {
                        <button
                            class="btn btn-ghost"
                            type="button"
                            on:click=move |_| {
                                query.set(String::new());
                                set_search_active.set(false);
                                set_search_rows.set(Vec::new());
                            }
                        >
                            "Clear"
                        </button>
                    })}
                </div>
                {move || if search_active.get() {
                    view! {
                        <div>
                            <p class="muted small">"Server search results (subject · folder)."</p>
                            <ul class="msg-list">
                                <For
                                    each=move || search_rows.get()
                                    key=|r: &crate::api::SearchResult| format!("{}|{}", r.subject, r.folder)
                                    children=move |r: crate::api::SearchResult| {
                                        let subj = r.subject.clone();
                                        let fld = r.folder.clone();
                                        view! {
                                            <li class="msg">
                                                <div class="msg-head"><strong>{subj}</strong><span class="muted small">{fld}</span></div>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                            {move || (search_active.get() && search_rows.get().is_empty()).then(|| view! {
                                <EmptyState text="No search results.".to_string()/>
                            })}
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            {move || loading.get().then(|| view! { <Spinner/> })}
                            <ul class="msg-list">
                                <For
                                    each=move || {
                                        let q = query.get().trim().to_lowercase();
                                        if q.is_empty() {
                                            messages.get()
                                        } else {
                                            messages.get().into_iter().filter(|m| {
                                                m.subject.to_lowercase().contains(&q)
                                                    || m.from.to_lowercase().contains(&q)
                                                    || m.preview.to_lowercase().contains(&q)
                                            }).collect()
                                        }
                                    }
                                    key=|m: &crate::api::MessageHeader| m.uid
                                    children=move |m: crate::api::MessageHeader| {
                                        let uid = m.uid;
                                        let seen = m.seen;
                                        let sender = m.from.clone();
                                        let subj = m.subject.clone();
                                        let date = m.date.clone();
                                        let preview = m.preview.clone();
                                        view! {
                                            <li
                                                class="msg msg-row"
                                                class:selected=move || selected.get() == Some(uid)
                                                class:unread=move || !seen
                                                on:click=move |_| set_selected.set(Some(uid))
                                            >
                                                <label class="msg-check">
                                                    <input type="checkbox" disabled=true aria-label="Select message"/>
                                                </label>
                                                <div class="msg-head">
                                                    <strong class=if seen { "msg-sender" } else { "msg-sender unread-text" }>{sender}</strong>
                                                    <span class="muted small">{date}</span>
                                                </div>
                                                <div class=if seen { "msg-subject" } else { "msg-subject unread-text" }>{subj}</div>
                                                <div class="msg-snippet">{preview}</div>
                                                {move || (!seen).then(|| view! { <span class="badge warn">"unread"</span> })}
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                            {move || (!loading.get() && messages.get().is_empty()).then(|| view! {
                                <EmptyState text="No messages in this folder.".to_string()/>
                            })}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card reading-pane mail-reading">
                {move || if show_compose.get() {
                    view! {
                        <div class="panel mail-compose">
                            <h2>"New Message"</h2>
                            {move || (!sent_info.get().is_empty()).then(|| view! {
                                <div class="toast" role="status"><p>{move || sent_info.get()}</p></div>
                            })}
                            {move || (!compose_err.get().is_empty()).then(|| view! {
                                <div class="card error-card" role="alert"><p>{move || compose_err.get()}</p></div>
                            })}
                            <form class="form" on:submit=move |ev| { ev.prevent_default(); send(); }>
                                <fieldset disabled=move || compose_pending.get() class="compose-fields">
                                    <label class="field">
                                        <span>"From"</span>
                                        <input
                                            class="input"
                                            type="email"
                                            placeholder="you@example.com"
                                            prop:value=move || from_field.get()
                                            on:input=move |ev| from_field.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <label class="field">
                                        <span>"To"</span>
                                        <input
                                            class="input"
                                            type="text"
                                            placeholder="recipient@example.com"
                                            prop:value=move || to.get()
                                            on:input=move |ev| to.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <label class="field">
                                        <span>"Subject"</span>
                                        <input
                                            class="input"
                                            type="text"
                                            placeholder="Subject"
                                            prop:value=move || subject.get()
                                            on:input=move |ev| subject.set(event_target_value(&ev))
                                        />
                                    </label>
                                    <label class="field">
                                        <span>"Message"</span>
                                        <textarea
                                            class="input"
                                            rows=8
                                            placeholder="Write your message…"
                                            prop:value=move || body.get()
                                            on:input=move |ev| body.set(event_target_value(&ev))
                                        ></textarea>
                                    </label>
                                </fieldset>
                                <div class="form-actions">
                                    <PendingSubmitButton label="Send".to_string() pending=compose_pending busy_label="Sending…".to_string()/>
                                    <button
                                        class="btn btn-ghost"
                                        type="button"
                                        disabled=move || compose_pending.get()
                                        on:click=move |_| set_show_compose.set(false)
                                    >
                                        "Discard"
                                    </button>
                                </div>
                            </form>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="panel mail-reader">
                            {move || match detail.get() {
                                None => view! {
                                    <div>
                                        <h2>"No message selected"</h2>
                                        <p class="muted">"Pick a message from the list to read it here."</p>
                                    </div>
                                }.into_any(),
                                Some(d) => {
                                    let subj = d.subject.clone();
                                    let from = d.from.clone();
                                    let date = d.date.clone();
                                    let raw = d.raw.clone();
                                    let to_addr = me_email.get();
                                    let from_click = from.clone();
                                    let subj_click = subj.clone();
                                    view! {
                                        <div>
                                            <div class="msg-head reader-head">
                                                <div>
                                                    <p class="muted small">"From: " </p><span class="msg-from reader-from">{from}</span>
                                                    <p class="muted small">"To: "</p><span>{to_addr}</span>
                                                </div>
                                                <span class="muted small reader-date">{date}</span>
                                            </div>
                                            <h1 class="reader-subject">{subj}</h1>
                                            <div class="body reader-body">
                                                <p class="reader-body-text">{raw}</p>
                                            </div>
                                            <div class="reply-box">
                                                <label class="field">
                                                    <span>"Reply"</span>
                                                    <textarea
                                                        class="input"
                                                        rows=4
                                                        placeholder="Write a reply…"
                                                        prop:value=move || reply_body.get()
                                                        on:input=move |ev| reply_body.set(event_target_value(&ev))
                                                    ></textarea>
                                                </label>
                                                <div class="form-actions">
                                                    <button
                                                        class="btn btn-primary"
                                                        type="button"
                                                        on:click=move |_| {
                                                            to.set(from_click.clone());
                                                            subject.set(format!("Re: {}", subj_click));
                                                            body.set(reply_body.get());
                                                            reply_body.set(String::new());
                                                            set_compose_err.set(String::new());
                                                            set_show_compose.set(true);
                                                        }
                                                    >
                                                        "Reply"
                                                    </button>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>
                    }.into_any()
                }}
            </section>
        </div>
    }
}

#[component]
pub fn WebmailHomeLegacy() -> impl IntoView {
    let _ = require_auth();
    let (folders, set_folders) = signal(Vec::<Value>::new());
    let (folder, set_folder) = signal("INBOX".to_string());
    let (messages, set_messages) = signal(Vec::<Value>::new());
    let (selected, set_selected) = signal(Option::<u32>::None);
    let (detail, set_detail) = signal(Value::Null);
    let (err, set_err) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let query = RwSignal::new(String::new());
    let (search_rows, set_search_rows) = signal(Vec::<Value>::new());
    let (show_compose, set_show_compose) = signal(false);
    let to = RwSignal::new(String::new());
    let subject = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let (sent_info, set_sent_info) = signal(String::new());
    let (drafts, set_drafts) = signal(Vec::<Value>::new());
    let (jmap_out, set_jmap_out) = signal(String::new());

    let me_email = RwSignal::new(String::new());

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(r) = crate::api::me().await {
                me_email.set(pick(&val(&r), &["email"]));
            }
            match crate::api::mail_folders().await {
                Ok(r) => set_folders.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
        });
    });

    let load_messages = move || {
        let fld = folder.get();
        set_loading.set(true);
        set_err.set(String::new());
        spawn_local(async move {
            match crate::api::mail_messages(&fld, 25u32).await {
                Ok(r) => set_messages.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
            set_loading.set(false);
        });
    };
    let load_messages2 = load_messages;
    Effect::new(move |_| load_messages());

    Effect::new(move |_| {
        let uid = selected.get();
        if uid.is_none() {
            return;
        }
        let uid = uid.unwrap();
        spawn_local(async move {
            match crate::api::mail_message(uid).await {
                Ok(r) => set_detail.set(val(&r)),
                Err(e) => set_err.set(e),
            }
        });
    });

    let search = move || {
        let q = query.get().trim().to_string();
        if q.is_empty() {
            return;
        }
        spawn_local(async move {
            match crate::api::mail_search(&q).await {
                Ok(r) => set_search_rows.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
        });
    };

    let send = move || {
        let from = me_email.get();
        let t = to.get().trim().to_string();
        let sj = subject.get();
        let bd = body.get();
        if t.is_empty() || sj.is_empty() {
            set_err.set("Recipient and subject are required.".to_string());
            return;
        }
        let reload = load_messages2;
        spawn_local(async move {
            match crate::api::mail_send(&crate::api::SendMailReq {
                from: from.clone(),
                to: t.clone(),
                subject: sj.clone(),
                body: bd.clone(),
            })
            .await
            {
                Ok(r) => {
                    set_sent_info.set(format!("Sent: {}", pretty(&val(&r))));
                    to.set(String::new());
                    subject.set(String::new());
                    body.set(String::new());
                    set_show_compose.set(false);
                    reload();
                }
                Err(e) => set_err.set(e),
            }
        });
    };

    let load_drafts = move || {
        spawn_local(async move {
            match crate::api::mail_drafts().await {
                Ok(r) => set_drafts.set(rows_of(&val(&r))),
                Err(e) => set_err.set(e),
            }
        });
    };

    view! {
        <div class="mail-layout">
            <aside class="sidebar card folders">
                <h3>"Folders"</h3>
                <ul class="folder-list">
                    <For
                        each=move || folders.get()
                        key=|v| pretty(v)
                        children=move |v| {
                            let name = pick(&v, &["name", "folder", "id"]);
                            let name_active = name.clone();
                            let name_label = name.clone();
                            view! {
                                <li>
                                    <button
                                        class="folder"
                                        class:active=move || folder.get() == name_active
                                        on:click=move |_| {
                                            set_folder.set(name.clone());
                                            set_search_rows.set(Vec::new());
                                        }
                                    >
                                        {name_label}
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
                <button class="btn btn-primary full" type="button" on:click=move |_| set_show_compose.set(true)>"Compose"</button>
                <button
                    class="btn"
                    type="button"
                    on:click=move |_| {
                        load_drafts();
                    }
                >
                    "Show drafts"
                </button>
                {move || (!drafts.get().is_empty()).then(|| view! {
                    <ul class="folder-list">
                        <For
                            each=move || drafts.get()
                            key=|v| pretty(v)
                            children=move |v| {
                                let uid = f(&v, "uid") as u32;
                                view! {
                                    <li>
                                        <span class="muted small">{pick(&v, &["subject"])}</span>
                                        <button class="btn btn-ghost small" type="button" on:click=move |_| {
                                            spawn_local(async move {
                                                match crate::api::mail_draft_delete(uid).await {
                                                    Ok(_) => load_drafts(),
                                                    Err(e) => set_err.set(e),
                                                }
                                            });
                                        }>"Delete draft"</button>
                                    </li>
                                }
                            }
                        />
                    </ul>
                })}
                <button class="btn btn-ghost" type="button" on:click=move |_| {
                    let calls = json!([["Mailbox/query", { "filter": {} }, "q"]]);
                    spawn_local(async move {
                        match crate::api::jmap_request(calls).await {
                            Ok(r) => set_jmap_out.set(pretty(&val(&r))),
                            Err(e) => set_err.set(e),
                        }
                    });
                }>"JMAP probe"</button>
                {move || (!jmap_out.get().is_empty()).then(|| view! {
                    <pre class="code">{move || jmap_out.get()}</pre>
                })}
            </aside>

            <section class="card message-pane">
                {move || (!err.get().is_empty()).then(|| view! { <ErrorCard error=err.get()/> })}
                <div class="search-row">
                    <input
                        class="input"
                        type="search"
                        placeholder="Search messages (FTS)…"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    <button class="btn" type="button" on:click=move |_| search()>"Search"</button>
                </div>
                {move || if !search_rows.get().is_empty() {
                    view! {
                        <ul class="msg-list">
                            <For
                                each=move || search_rows.get()
                                key=|v| pretty(v)
                                children=move |v| view! {
                                    <li class="msg">
                                        <div class="msg-head"><strong>{pick(&v, &["subject"])}</strong><span class="muted small">{pick(&v, &["folder"])}</span></div>
                                    </li>
                                }
                            />
                        </ul>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            {move || loading.get().then(|| view! { <Spinner/> })}
                            <ul class="msg-list">
                                <For
                                    each=move || messages.get()
                                    key=|v| pretty(v)
                                    children=move |v| {
                                        let uid = f(&v, "uid") as u32;
                                        let seen = b(&v, "seen");
                                        view! {
                                            <li
                                                class="msg"
                                                class:selected=move || selected.get() == Some(uid)
                                                on:click=move |_| set_selected.set(Some(uid))
                                            >
                                                <div class="msg-head">
                                                    <strong>{pick(&v, &["subject"])}</strong>
                                                    <span class="muted small">{pick(&v, &["date"])}</span>
                                                </div>
                                                <div class="msg-from">{pick(&v, &["from"])}</div>
                                                <div class="msg-snippet">{pick(&v, &["preview"])}</div>
                                                {move || (!seen).then(|| view! { <span class="badge warn">"unread"</span> })}
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                            {move || (!loading.get() && messages.get().is_empty()).then(|| view! {
                                <EmptyState text="No messages in this folder.".to_string()/>
                            })}
                        </div>
                    }.into_any()
                }}
            </section>

            <section class="card reading-pane">
                {move || if show_compose.get() {
                    view! {
                        <div class="panel">
                            <h2>"Compose"</h2>
                            {move || (!sent_info.get().is_empty()).then(|| view! {
                                <div class="card"><p class="muted">{move || sent_info.get()}</p></div>
                            })}
                            <form class="form" on:submit=move |ev| { ev.prevent_default(); send(); }>
                                <p class="muted">"From: " {move || me_email.get()}</p>
                                <Field label="To".to_string() value=to placeholder="recipient@example.com".to_string()/>
                                <Field label="Subject".to_string() value=subject placeholder="Subject".to_string()/>
                                <TextAreaField label="Message".to_string() value=body placeholder="Write your message…".to_string() rows=8/>
                                <div class="form-actions">
                                    <button class="btn btn-primary" type="submit">"Send"</button>
                                    <button class="btn btn-ghost" type="button" on:click=move |_| set_show_compose.set(false)>"Discard"</button>
                                </div>
                            </form>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="panel">
                            <h2>{move || pick(&detail.get(), &["subject"]).if_empty("No message selected")}</h2>
                            <p class="muted">
                                {move || format!(
                                    "From: {} · Date: {}",
                                    pick(&detail.get(), &["from"]),
                                    pick(&detail.get(), &["date"]),
                                )}
                            </p>
                            <div class="body">
                                <pre class="code">{move || {
                                    let raw = pick(&detail.get(), &["raw"]);
                                    if raw.is_empty() { pretty(&detail.get()) } else { raw }
                                }}</pre>
                            </div>
                        </div>
                    }.into_any()
                }}
            </section>
        </div>
    }
}

// ---------------------------------------------------------------------------
// 404
// ---------------------------------------------------------------------------

#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <section class="card empty">
            <h2>"Page not found"</h2>
            <p class="muted">"Use the navigation above to get back."</p>
            <a href="/" class="btn btn-primary">"Go home"</a>
        </section>
    }
}
