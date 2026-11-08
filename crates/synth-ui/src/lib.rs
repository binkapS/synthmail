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
