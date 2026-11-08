use leptos::prelude::*;

pub mod api;
pub mod pages;

use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
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
    let nav_open = RwSignal::new(false);
    provide_context(theme);
    provide_context(auth_token);
    provide_context(AppState { auth_token });

    view! {
        <Router>
            <div class="app-shell" data-theme=move || theme.get()>
                <a href="#main" class="skip-link btn">"Skip to content"</a>
                <TopBar theme=theme nav_open=nav_open />
                {move || nav_open.get().then(move || {
                    view! {
                        <div
                            class="scrim"
                            on:click=move |_| nav_open.set(false)
                            aria-hidden="true"
                        ></div>
                        <div class="nav-sheet" role="dialog" aria-label="Navigation">
                            <a href="/" class="nav-sheet-link" on:click=move |_| nav_open.set(false)>
                                <span>"⌂"</span><span>"Home"</span>
                            </a>
                            <a href="/login" class="nav-sheet-link" on:click=move |_| nav_open.set(false)>
                                <span>"→"</span><span>"Sign in"</span>
                            </a>
                            <a href="/admin" class="nav-sheet-link" on:click=move |_| nav_open.set(false)>
                                <span>"⚙"</span><span>"Admin"</span>
                            </a>
                            <a href="/mail" class="nav-sheet-link" on:click=move |_| nav_open.set(false)>
                                <span>"✉"</span><span>"Webmail"</span>
                            </a>
                            <a href="/dav/" class="nav-sheet-link" target="_blank" rel="noopener">
                                <span>"↗"</span><span>"CardDAV / CalDAV"</span>
                            </a>
                        </div>
                    }
                })}
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
                    <span class="muted">"Leptos · responsive"</span>
                </footer>
            </div>
            <BottomNav />
        </Router>
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct AppState {
    auth_token: RwSignal<Option<String>>,
}

/// Mobile bottom tab bar (≤820px via CSS) + active highlighting from router.
#[component]
fn BottomNav() -> impl IntoView {
    let loc = use_location();
    let path = move || loc.pathname.get();
    let is = move |p: &str| path() == p;
    view! {
        <nav class="bottom-nav" aria-label="Primary (mobile)">
            <a href="/" class:active=move || is("/") class="bottom-link">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="m3 10 9-7 9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
                    <path d="M9 22V12h6v10" />
                </svg>
                <span>"Home"</span>
            </a>
            <a href="/login" class:active=move || is("/login") class="bottom-link">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" />
                    <path d="m10 17 5-5-5-5" />
                    <path d="M15 12H3" />
                </svg>
                <span>"Sign in"</span>
            </a>
            <a href="/admin" class:active=move || is("/admin") class="bottom-link">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
                    <circle cx="12" cy="12" r="3" />
                </svg>
                <span>"Admin"</span>
            </a>
            <a href="/mail" class:active=move || is("/mail") class="bottom-link">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <rect width="20" height="16" x="2" y="4" rx="2" />
                    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
                </svg>
                <span>"Mail"</span>
            </a>
            <a href="/dav/" class="bottom-link" target="_blank" rel="noopener">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                    <path d="M15 3h6v6" />
                    <path d="M10 14 21 3" />
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                </svg>
                <span>"DAV"</span>
            </a>
        </nav>
    }
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
fn TopBar(theme: ThemeSignal, nav_open: RwSignal<bool>) -> impl IntoView {
    let loc = use_location();
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
            <button
                class="icon-btn nav-toggle"
                type="button"
                aria-label="Open navigation"
                aria-expanded=move || nav_open.get().to_string()
                on:click=move |_| nav_open.update(|v| *v = !*v)
            >
                {move || if nav_open.get() { "✕" } else { "☰" }}
            </button>
            <a href="/" class="brand">
                <span class="brand-mark" aria-hidden="true">"SM"</span>
                <span>"SynthMail"</span>
            </a>
            <nav class="topnav" aria-label="Primary">
                <a href="/" class="nav-link" class:active=move || loc.pathname.get() == "/">"Home"</a>
                <a href="/login" class="nav-link" class:active=move || loc.pathname.get() == "/login">"Sign in"</a>
                <a href="/admin" class="nav-link" class:active=move || loc.pathname.get() == "/admin">"Admin"</a>
                <a href="/mail" class="nav-link" class:active=move || loc.pathname.get() == "/mail">"Webmail"</a>
                <a class="nav-link" href="/dav/" target="_blank" rel="noopener">"CardDAV/CalDAV"</a>
            </nav>
            <div class="topbar-actions">
                {move || api::get_token().is_some().then(|| view! {
                    <LogoutNavButton/>
                })}
                <button class="icon-btn" type="button" on:click=toggle_theme aria-label="Toggle color theme">
                    {move || if theme.get() == "dark" { "☀" } else { "☾" }}
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
