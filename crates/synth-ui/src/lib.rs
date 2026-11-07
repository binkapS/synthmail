use leptos::prelude::*;

const LOGIN_URL: &str = "/api/v1/auth/login";

#[component]
fn App() -> impl IntoView {
    view! {
        <main>
            <h1>"SynthMail"</h1>
            <p>"Self-hosted mail — admin + webmail (Phase 2)"</p>
            <nav>
                <a href="/admin">"Admin UI"</a>
                <a href="/mail">"Webmail"</a>
                <a href="/dav/">"CardDAV / CalDAV"</a>
            </nav>
            <LoginPage/>
            <AdminHome/>
            <WebmailHome/>
        </main>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (status, set_status) = signal("Not signed in".to_string());
    let on_login = move |_| {
        let email = email.get();
        let password = password.get();
        leptos::task::spawn_local(async move {
            let body = serde_json::json!({"email": email, "password": password});
            match gloo_net::http::Request::post(LOGIN_URL)
                .header("Content-Type", "application/json")
                .body(body.to_string())
            {
                Ok(req) => match req.send().await {
                    Ok(resp) if resp.ok() => {
                        set_status.set("Signed in — token issued by the API".to_string());
                    }
                    Ok(resp) => {
                        set_status.set(format!("Login failed (HTTP {})", resp.status()));
                    }
                    Err(_) => set_status.set("Login request failed".to_string()),
                },
                Err(_) => set_status.set("Login request failed".to_string()),
            }
        });
    };
    view! {
        <section>
            <h2>"Login (2FA ready)"</h2>
            <label>
                "Email"
                <input
                    type="text"
                    prop:value=email
                    on:input=move |ev| set_email.set(event_target_value(&ev))
                />
            </label>
            <label>
                "Password"
                <input
                    type="password"
                    prop:value=password
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                />
            </label>
            <button on:click=on_login>"Sign in"</button>
            <p>{status}</p>
        </section>
    }
}

#[component]
fn AdminHome() -> impl IntoView {
    view! {
        <main>
            <h2>"Admin — Domains / Users / Aliases / Queues / Audit / API Tokens / 2FA"</h2>
            <section><h3>"Domains"</h3><ul><li>"example.com — active"</li></ul></section>
            <section><h3>"Users"</h3><ul><li>"admin@example.com"</li></ul></section>
            <section><h3>"Queues"</h3><p>"postfix qshape polled every 30s"</p></section>
            <section><h3>"DKIM / DNS Wizard"</h3><p>"Copy-paste zone file from /api/v1/domains/{id}/dns"</p></section>
            <section><h3>"API Tokens"</h3><p>"Create scoped tokens via /api/v1/api-tokens"</p></section>
            <section><h3>"2FA"</h3><p>"Enroll TOTP via /api/v1/auth/totp/enroll"</p></section>
            <section><h3>"Monitoring"</h3><p>"Prometheus /metrics + Grafana (profile: monitoring)"</p></section>
        </main>
    }
}

#[component]
fn WebmailHome() -> impl IntoView {
    view! {
        <main>
            <h2>"Webmail — Inbox / Compose / Search (JMAP + IMAP)"</h2>
            <aside><ul><li>"INBOX"</li><li>"Sent"</li><li>"Drafts"</li><li>"Junk"</li><li>"Trash"</li></ul></aside>
            <section><p>"Messages via /api/v1/mail/* and /jmap (RFC 8620/8621)"</p></section>
            <section><h3>"Search (FTS)"</h3><p>"Full-text via Postgres tsvector — /api/v1/mail/search?q="</p></section>
            <section><h3>"Vacation & Sieve"</h3><p>"Endpoints /api/v1/vacation/{id} and /api/v1/filters/{id}"</p></section>
        </main>
    }
}

// Bootstrap for WASM
pub fn mount_app() {
    leptos::mount::mount_to_body(App);
}

// Trunk entrypoint (used when building with Trunk)
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
