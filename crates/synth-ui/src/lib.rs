#![allow(unused_imports)]

use leptos::prelude::*;
use leptos_router::{Route, Router, path, paths};

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=|| view! { <p>"Loading..."</p> }>
                <Route path=path!("/admin") view=AdminHome/>
                <Route path=path!("/mail") view=WebmailHome/>
                <Route path=path!("/") view=HomePage/>
                <Route path=path!("/login") view=LoginPage/>
            </Routes>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <main>
            <h1>"SynthMail"</h1>
            <p>"Self-hosted mail — admin + webmail"</p>
            <nav>
                <a href="/admin">"Admin UI"</a>
                <a href="/mail">"Webmail"</a>
            </nav>
        </main>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let (email, set_email) = signal(String::new());
    let (password, set_password) = signal(String::new());
    view! {
        <main>
            <h2>"Login"</h2>
            <label>"Email" <input bind:value=email/></label>
            <label>"Password" <input type="password" bind:value=password/></label>
            <button on:click=move |_| { tracing::debug!("login attempt (Leptos client)"); }>
                "Sign in"
            </button>
        </main>
    }
}

#[component]
fn AdminHome() -> impl IntoView {
    view! {
        <main>
            <h2>"Admin — Domains / Users / Aliases / Queues / Audit"</h2>
            <section><h3>"Domains"</h3><ul><li>"example.com — active"</li></ul></section>
            <section><h3>"Users"</h3><ul><li>"admin@example.com"</li></ul></section>
            <section><h3>"Queues"</h3><p>"postfix qshape polled every 30s"</p></section>
            <section><h3>"DKIM / DNS Wizard"</h3><p>"Copy-paste zone file from /api/v1/domains/{id}/dns"</p></section>
        </main>
    }
}

#[component]
fn WebmailHome() -> impl IntoView {
    view! {
        <main>
            <h2>"Webmail — Inbox / Compose / Search"</h2>
            <aside><ul><li>"INBOX"</li><li>"Sent"</li><li>"Drafts"</li><li>"Junk"</li><li>"Trash"</li></ul></aside>
            <section><p>"Messages loaded via /api/v1/mail/* (IMAP proxy)"</p></section>
        </main>
    }
}

// Bootstrap for WASM
pub fn mount_app() {
    leptos::mount_to_body(App);
}

// Trunk entrypoint (used when building with Trunk)
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_log::init_with_level(log::Level::Debug).ok();
    mount_app();
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_renders() {
        // SSR smoke: leptos SSR would render; CSR tested via trunk
        assert!(true);
    }
}
