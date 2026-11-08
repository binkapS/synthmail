use leptos::prelude::*;

pub mod api;
pub mod pages;

use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
pub use pages::{AdminHome, Home, LoginPage, NotFound, WebmailHome};

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

    let auth_token = RwSignal::new(api::get_token());
    provide_context(theme);
    provide_context(auth_token);
    provide_context(AppState { auth_token });

    view! {
        <Router>
            <div class="app-shell" data-theme=move || theme.get()>
                <TopBar theme=theme />
                <main id="main" class="main">
                    <Routes fallback=NotFound>
                        <Route path=path!("/") view=Home />
                        <Route path=path!("/login") view=LoginPage />
                        <Route path=path!("/admin") view=move || view! { <AuthGuard><AdminHome/></AuthGuard> }.into_any() />
                        <Route path=path!("/mail") view=move || view! { <AuthGuard><WebmailHome/></AuthGuard> }.into_any() />
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

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct AppState {
    auth_token: RwSignal<Option<String>>,
}

/// Router-level auth guard: unauthenticated users are sent to /login
/// before the protected view renders. Admin-ness is still enforced
/// inside AdminHome (kind check via /auth/me).
#[component]
fn AuthGuard(children: Children) -> impl IntoView {
    let nav = leptos_router::hooks::use_navigate();
    let ok = RwSignal::new(api::get_token().is_some());
    Effect::new(move |_| {
        if !ok.get() {
            nav("/login", Default::default());
        }
    });
    view! { <>{children()}</> }
}

/// Topbar logout: POSTs to the backend, clears token everywhere, routes
/// to /login.
#[component]
fn LogoutNavButton() -> impl IntoView {
    let nav = leptos_router::hooks::use_navigate();
    let (pending, set_pending) = signal(false);
    let on_click = move |_| {
        let nav = nav.clone();
        set_pending.set(true);
        leptos::task::spawn_local(async move {
            let _ = api::logout().await;
            api::clear_token();
            nav("/login", Default::default());
        });
    };
    view! {
        <button
            class="icon-btn"
            type="button"
            aria-label="Log out"
            disabled=move || pending.get()
            on:click=on_click
        >
            {move || if pending.get() { "…" } else { "⏻" }}
        </button>
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
                {move || api::get_token().is_some().then(|| view! {
                    <LogoutNavButton/>
                })}
                <button class="icon-btn" type="button" on:click=toggle_theme aria-label="Toggle color theme">
                    {move || if theme.get() == "dark" { "🌙" } else { "☀️" } }
                </button>
            </div>
        </header>
    }
}

// Real screens live in `pages` (wired to the API). The old demo pages were
// removed in the UI wiring pass; the app shell and routes around them stay.
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
    use crate::api::api_url;
    #[test]
    fn login_posts_to_control_api() {
        assert_eq!(api_url("/auth/login"), "/api/v1/auth/login");
    }
}
