use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

const LOGIN_URL: &str = "/api/v1/auth/login";

type ThemeSignal = RwSignal<String>;

#[component]
fn App() -> impl IntoView {
    let theme: ThemeSignal = RwSignal::new("dark".to_string());

    Effect::new(move |_| {
        let t = theme.get();
        if let Some(doc) = leptos::web_sys::window().and_then(|w| w.document()) {
            let _ = doc
                .document_element()
                .map(|el| el.set_attribute("data-theme", &t));
            if let Some(body) = doc.body() {
                let _ = body.set_attribute(
                    "class",
                    if t == "dark" {
                        "theme-dark"
                    } else {
                        "theme-light"
                    },
                );
            }
        }
    });

    let auth_token: RwSignal<Option<String>> = RwSignal::new(None);
    provide_context(theme);
    provide_context(auth_token);

    view! {
        <Router>
            <div class="app-shell" data-theme=move || theme.get()>
                <TopBar theme=theme />
                <main id="main" class="main">
                    <Routes fallback=NotFound>
                        <Route path=path!("/") view=Home />
                        <Route path=path!("/login") view=LoginPage />
                        <Route path=path!("/admin") view=AdminHome />
                        <Route path=path!("/mail") view=WebmailHome />
                    </Routes>
                </main>
                <footer class="footer">
                    <span>"SynthMail — self-hosted mail & admin"</span>
                    <span class="muted">"Phase 2 · Leptos CSR"</span>
                </footer>
            </div>
        </Router>
    }
}

#[component]
fn TopBar(theme: ThemeSignal) -> impl IntoView {
    let toggle_theme = move |_| {
        let next = if theme.get() == "dark" {
            "light"
        } else {
            "dark"
        };
        theme.set(next.to_string());
    };
    view! {
        <header class="topbar">
            <div class="brand">
                <span class="brand-mark" aria-hidden="true">"SM"</span>
                <strong>"SynthMail"</strong>
            </div>
            <nav class="topnav" aria-label="Primary">
                <a href="/" class="nav-link">"Home"</a>
                <a href="/login" class="nav-link">"Login"</a>
                <a href="/admin" class="nav-link">"Admin"</a>
                <a href="/mail" class="nav-link">"Webmail"</a>
                <a class="nav-link" href="/dav/" target="_blank" rel="noopener">"CardDAV/CalDAV"</a>
            </nav>
            <div class="topbar-actions">
                <button class="icon-btn" type="button" on:click=toggle_theme aria-label="Toggle color theme">
                    {move || if theme.get() == "dark" { "🌙" } else { "☀️" } }
                </button>
            </div>
        </header>
    }
}

#[component]
fn Home() -> impl IntoView {
    view! {
        <section class="hero card">
            <div class="hero-text">
                <p class="eyebrow">"Self-hosted mail platform"</p>
                <h1>"Run your own mail, admin and webmail."</h1>
                <p class="muted">
                    "SynthMail combines SMTP, IMAP/JMAP, admin controls, DKIM/DNS tooling, "
                    "monitoring and a modern web experience in one stack."
                </p>
                <div class="hero-actions">
                    <a href="/login" class="btn primary">"Sign in"</a>
                    <a href="/admin" class="btn ghost">"Open admin"</a>
                </div>
            </div>
            <div class="hero-art" aria-hidden="true">
                <div class="blob b1"></div>
                <div class="blob b2"></div>
                <div class="hero-card">
                    <div class="stat"><span class="stat-value">"99.9%"</span><span class="stat-label">"Uptime target"</span></div>
                    <div class="stat"><span class="stat-value">"TLS"</span><span class="stat-label">"End-to-end"</span></div>
                    <div class="stat"><span class="stat-value">"JMAP"</span><span class="stat-label">"RFC 8620/21"</span></div>
                </div>
            </div>
        </section>

        <section class="grid mt">
            <article class="card feature">
                <h3>"Domains & DNS"</h3>
                <p class="muted">"Manage domains, aliases, DKIM keys and DNS wizard exports."</p>
            </article>
            <article class="card feature">
                <h3>"Users & 2FA"</h3>
                <p class="muted">"Provision mailboxes, scoped API tokens and TOTP enrollment."</p>
            </article>
            <article class="card feature">
                <h3>"Queues & Monitoring"</h3>
                <p class="muted">"Postfix qshape, Prometheus metrics and audit trails."</p>
            </article>
            <article class="card feature">
                <h3>"Webmail"</h3>
                <p class="muted">"Folder sidebar, message list, search (FTS) and Sieve/vacation."</p>
            </article>
        </section>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <section class="card empty">
            <h2>"Page not found"</h2>
            <p class="muted">"Use the navigation above to get back."</p>
            <a href="/" class="btn primary">"Go home"</a>
        </section>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let totp = RwSignal::new(String::new());
    let (loading, set_loading) = signal(false);
    let status = RwSignal::new("Not signed in".to_string());

    let auth_token =
        use_context::<RwSignal<Option<String>>>().unwrap_or_else(|| RwSignal::new(None));

    let on_login = move |ev: leptos::ev::MouseEvent| {
        ev.prevent_default();
        let email = email.get();
        let password = password.get();
        let totp_code = totp.get();

        if email.trim().is_empty() || password.is_empty() {
            status.set("Enter email and password.".to_string());
            return;
        }
        set_loading.set(true);
        status.set("Signing in…".to_string());

        let status = status;
        let set_loading = set_loading;
        let auth_token = auth_token;
        leptos::task::spawn_local(async move {
            let body = serde_json::json!({
                "email": email.trim(),
                "password": password,
                "totp_code": if totp_code.trim().is_empty() { None } else { Some(totp_code.trim()) }
            });
            let result = (|| async {
                let req = gloo_net::http::Request::post(LOGIN_URL)
                    .header("Content-Type", "application/json")
                    .header("Accept", "application/json")
                    .body(body.to_string())
                    .map_err(|e| format!("request build failed: {e}"))?;
                let resp = req
                    .send()
                    .await
                    .map_err(|e| format!("network error: {e}"))?;
                let code = resp.status();
                if resp.ok() {
                    let text = resp.text().await.unwrap_or_default();
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(tok) = v.get("token").and_then(|x| x.as_str()) {
                            auth_token.set(Some(tok.to_string()));
                        } else if let Some(tok) = v.get("access_token").and_then(|x| x.as_str()) {
                            auth_token.set(Some(tok.to_string()));
                        }
                    }
                    Ok(format!("Signed in successfully (HTTP {code})."))
                } else {
                    let detail = resp.text().await.unwrap_or_default();
                    let short = if detail.trim().is_empty() {
                        "no error body".to_string()
                    } else if detail.chars().count() > 220 {
                        format!("{}…", detail.chars().take(220).collect::<String>())
                    } else {
                        detail
                    };
                    Err(format!("Login failed (HTTP {code}): {short}"))
                }
            })()
            .await;

            set_loading.set(false);
            match result {
                Ok(msg) => status.set(msg),
                Err(msg) => status.set(msg),
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
                <form class="form" on:submit=move |ev| ev.prevent_default()>
                    <label class="field">
                        <span>"Email"</span>
                        <input
                            type="email"
                            autocomplete="username"
                            placeholder="you@example.com"
                            prop:value=move || email.get()
                            on:input=move |ev| email.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="field">
                        <span>"Password"</span>
                        <input
                            type="password"
                            autocomplete="current-password"
                            placeholder="••••••••"
                            prop:value=move || password.get()
                            on:input=move |ev| password.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="field">
                        <span>"TOTP code (optional)"</span>
                        <input
                            type="text"
                            inputmode="numeric"
                            autocomplete="one-time-code"
                            placeholder="123456"
                            prop:value=move || totp.get()
                            on:input=move |ev| totp.set(event_target_value(&ev))
                        />
                    </label>
                    <div class="form-actions">
                        <button class="btn primary" type="button" disabled=move || loading.get() on:click=on_login>
                            {move || if loading.get() { "Signing in…" } else { "Sign in" } }
                        </button>
                        <span class="muted small">"POST " {LOGIN_URL} </span>
                    </div>
                    <div class="status" role="status" aria-live="polite">
                        {move || status.get()}
                    </div>
                </form>
            </div>
        </section>
    }
}

#[component]
fn AdminHome() -> impl IntoView {
    let (active, set_active) = signal("domains".to_string());
    view! {
        <div class="admin-layout">
            <aside class="sidebar card">
                <h3>"Admin"</h3>
                <nav aria-label="Admin sections">
                    <button class="side-link" class:active=move || active.get()=="domains" on:click=move |_| set_active.set("domains".to_string())>"Domains"</button>
                    <button class="side-link" class:active=move || active.get()=="users" on:click=move |_| set_active.set("users".to_string())>"Users"</button>
                    <button class="side-link" class:active=move || active.get()=="queues" on:click=move |_| set_active.set("queues".to_string())>"Queues"</button>
                    <button class="side-link" class:active=move || active.get()=="dkim" on:click=move |_| set_active.set("dkim".to_string())>"DKIM / DNS"</button>
                    <button class="side-link" class:active=move || active.get()=="tokens" on:click=move |_| set_active.set("tokens".to_string())>"API Tokens"</button>
                    <button class="side-link" class:active=move || active.get()=="twofa" on:click=move |_| set_active.set("twofa".to_string())>"2FA"</button>
                    <button class="side-link" class:active=move || active.get()=="audit" on:click=move |_| set_active.set("audit".to_string())>"Audit"</button>
                    <button class="side-link" class:active=move || active.get()=="monitor" on:click=move |_| set_active.set("monitor".to_string())>"Monitoring"</button>
                </nav>
            </aside>
            <section class="content">
                {move || match active.get().as_str() {
                    "users" => view! { <UsersPanel/> }.into_any(),
                    "queues" => view! { <QueuesPanel/> }.into_any(),
                    "dkim" => view! { <DkimPanel/> }.into_any(),
                    "tokens" => view! { <TokensPanel/> }.into_any(),
                    "twofa" => view! { <TwoFaPanel/> }.into_any(),
                    "audit" => view! { <AuditPanel/> }.into_any(),
                    "monitor" => view! { <MonitorPanel/> }.into_any(),
                    _ => view! { <DomainsPanel/> }.into_any(),
                }}
            </section>
        </div>
    }
}

#[component]
fn DomainsPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <div class="panel-head">
                <h2>"Domains"</h2>
                <button class="btn primary" type="button">"Add domain"</button>
            </div>
            <div class="table-wrap">
                <table class="table">
                    <thead><tr><th>"Domain"</th><th>"Status"</th><th>"DKIM"</th><th>"Actions"</th></tr></thead>
                    <tbody>
                        <tr><td>"example.com"</td><td><span class="badge ok">"active"</span></td><td>"selector1"</td><td><button class="btn small" type="button">"DNS"</button></td></tr>
                        <tr><td>"mail.example.org"</td><td><span class="badge warn">"pending"</span></td><td>"—"</td><td><button class="btn small" type="button">"Verify"</button></td></tr>
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
fn UsersPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <div class="panel-head"><h2>"Users"</h2><button class="btn primary" type="button">"New user"</button></div>
            <div class="table-wrap">
                <table class="table"><thead><tr><th>"Email"</th><th>"Quota"</th><th>"2FA"</th><th>"Status"</th></tr></thead>
                <tbody>
                    <tr><td>"admin@example.com"</td><td>"2 / 10 GB"</td><td>"enabled"</td><td><span class="badge ok">"active"</span></td></tr>
                    <tr><td>"user@example.com"</td><td>"0.3 / 5 GB"</td><td>"disabled"</td><td><span class="badge ok">"active"</span></td></tr>
                </tbody></table>
            </div>
        </div>
    }
}

#[component]
fn QueuesPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"Queues"</h2>
            <p class="muted">"Postfix qshape polled every 30s. Admin endpoints: /api/v1/queues."</p>
            <div class="grid">
                <div class="card kpi"><span class="kpi-value">"12"</span><span class="kpi-label">"Deferred"</span></div>
                <div class="card kpi"><span class="kpi-value">"3"</span><span class="kpi-label">"Active"</span></div>
                <div class="card kpi"><span class="kpi-value">"0"</span><span class="kpi-label">"Bounced (24h)"</span></div>
            </div>
        </div>
    }
}

#[component]
fn DkimPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"DKIM / DNS Wizard"</h2>
            <p class="muted">"Copy-paste zone file from " <code>"/api/v1/domains/{id}/dns"</code> "."</p>
            <pre class="code">"v=DKIM1; k=rsa; p=MIIBIjANBgkh..."</pre>
        </div>
    }
}

#[component]
fn TokensPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"API Tokens"</h2>
            <p class="muted">"Create scoped tokens via /api/v1/api-tokens."</p>
            <button class="btn primary" type="button">"Issue token"</button>
        </div>
    }
}

#[component]
fn TwoFaPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"2FA / TOTP"</h2>
            <p class="muted">"Enroll TOTP via /api/v1/auth/totp/enroll. Show QR code + confirm code."</p>
        </div>
    }
}

#[component]
fn AuditPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"Audit log"</h2>
            <div class="table-wrap">
                <table class="table"><thead><tr><th>"When"</th><th>"Actor"</th><th>"Action"</th></tr></thead>
                <tbody>
                    <tr><td>"2026-11-08 02:10"</td><td>"admin"</td><td>"login.success"</td></tr>
                    <tr><td>"2026-11-08 01:58"</td><td>"admin"</td><td>"domain.create"</td></tr>
                </tbody></table>
            </div>
        </div>
    }
}

#[component]
fn MonitorPanel() -> impl IntoView {
    view! {
        <div class="panel">
            <h2>"Monitoring"</h2>
            <p class="muted">"Prometheus /metrics + Grafana (profile: monitoring)."</p>
            <div class="grid">
                <div class="card kpi"><span class="kpi-value">"OK"</span><span class="kpi-label">"SMTP"</span></div>
                <div class="card kpi"><span class="kpi-value">"OK"</span><span class="kpi-label">"IMAP/JMAP"</span></div>
                <div class="card kpi"><span class="kpi-value">"OK"</span><span class="kpi-label">"Postgres"</span></div>
            </div>
        </div>
    }
}

#[component]
fn WebmailHome() -> impl IntoView {
    let folder = RwSignal::new("INBOX".to_string());
    let selected = RwSignal::new(1u32);
    let query = RwSignal::new(String::new());
    let compose = RwSignal::new(false);

    view! {
        <div class="mail-layout">
            <aside class="sidebar card folders">
                <h3>"Folders"</h3>
                <ul class="folder-list">
                    <li><button class="folder" class:active=move || folder.get()=="INBOX" on:click=move |_| folder.set("INBOX".to_string())>"Inbox"</button><span class="count">"24"</span></li>
                    <li><button class="folder" class:active=move || folder.get()=="Sent" on:click=move |_| folder.set("Sent".to_string())>"Sent"</button><span class="count">"8"</span></li>
                    <li><button class="folder" class:active=move || folder.get()=="Drafts" on:click=move |_| folder.set("Drafts".to_string())>"Drafts"</button><span class="count">"2"</span></li>
                    <li><button class="folder" class:active=move || folder.get()=="Junk" on:click=move |_| folder.set("Junk".to_string())>"Junk"</button><span class="count">"1"</span></li>
                    <li><button class="folder" class:active=move || folder.get()=="Trash" on:click=move |_| folder.set("Trash".to_string())>"Trash"</button><span class="count">"0"</span></li>
                </ul>
                <button class="btn primary full" type="button" on:click=move |_| compose.set(true)>"Compose"</button>
            </aside>

            <section class="card message-pane">
                <div class="search-row">
                    <input type="search" placeholder="Search messages (FTS)…" prop:value=move || query.get() on:input=move |ev| query.set(event_target_value(&ev)) />
                    <button class="btn" type="button">"Search"</button>
                </div>
                <ul class="msg-list">
                    <li class="msg" class:selected=move || selected.get()==1 on:click=move |_| selected.set(1)>
                        <div class="msg-head"><strong>"Welcome to SynthMail"</strong><span class="muted small">"09:12"</span></div>
                        <div class="msg-from">"postmaster@example.com"</div>
                        <div class="msg-snippet">"Your mailbox has been provisioned. Messages via JMAP/IMAP…"</div>
                    </li>
                    <li class="msg" class:selected=move || selected.get()==2 on:click=move |_| selected.set(2)>
                        <div class="msg-head"><strong>"Quarterly report"</strong><span class="muted small">"yesterday"</span></div>
                        <div class="msg-from">"reports@example.com"</div>
                        <div class="msg-snippet">"Attached is the quarterly summary (FTS-indexed)…"</div>
                    </li>
                    <li class="msg" class:selected=move || selected.get()==3 on:click=move |_| selected.set(3)>
                        <div class="msg-head"><strong>"Security notice"</strong><span class="muted small">"2d ago"</span></div>
                        <div class="msg-from">"security@example.com"</div>
                        <div class="msg-snippet">"Please review recent login activity…"</div>
                    </li>
                </ul>
            </section>

            <section class="card reading-pane">
                {move || if compose.get() {
                    view! {
                        <div class="panel">
                            <h2>"Compose"</h2>
                            <form class="form">
                                <label class="field"><span>"To"</span><input type="email" placeholder="recipient@example.com" /></label>
                                <label class="field"><span>"Subject"</span><input type="text" placeholder="Subject" /></label>
                                <label class="field"><span>"Message"</span><textarea rows="8" placeholder="Write your message…"></textarea></label>
                                <div class="form-actions">
                                    <button class="btn primary" type="button">"Send"</button>
                                    <button class="btn ghost" type="button" on:click=move |_| compose.set(false)>"Discard"</button>
                                </div>
                            </form>
                        </div>
                    }.into_any()
                } else if selected.get()==1 {
                    view! {
                        <div class="panel">
                            <h2>"Welcome to SynthMail"</h2>
                            <p class="muted">"From: postmaster@example.com · Today 09:12"</p>
                            <div class="body">
                                <p>"Your mailbox is ready. You can read messages via JMAP (RFC 8620/8621) or IMAP."</p>
                                <p>"Set up Sieve/vacation filters under settings."</p>
                            </div>
                        </div>
                    }.into_any()
                } else if selected.get()==2 {
                    view! {
                        <div class="panel">
                            <h2>"Quarterly report"</h2>
                            <p class="muted">"From: reports@example.com · yesterday"</p>
                            <div class="body"><p>"Please find the attached quarterly summary."</p></div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="panel">
                            <h2>"Security notice"</h2>
                            <p class="muted">"From: security@example.com · 2 days ago"</p>
                            <div class="body"><p>"Please review recent login activity."</p></div>
                        </div>
                    }.into_any()
                }}
            </section>
        </div>
    }
}

pub fn mount_app() {
    leptos::mount::mount_to_body(App);
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(log::Level::Debug).ok();
    mount_app();
}

#[cfg(test)]
mod tests {
    use super::LOGIN_URL;
    #[test]
    fn login_posts_to_control_api() {
        assert_eq!(LOGIN_URL, "/api/v1/auth/login");
    }
}
