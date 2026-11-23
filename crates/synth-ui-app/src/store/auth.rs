// Auth flow: login via reqwest (worker thread) → invoke_from_event_loop →
// Slint state updates. Bootstrap: restore token → me() → choose screen.
use crate::api::{ApiClient, DEFAULT_CONTROL_URL, DEFAULT_WEBMAIL_URL};
use crate::store::token::TokenStore;
use crate::{App, Screen};
use slint::{SharedString, Weak};

fn client() -> ApiClient {
    ApiClient::new(
        &std::env::var("SYNTH_CONTROL_URL").unwrap_or_else(|_| DEFAULT_CONTROL_URL.into()),
        &std::env::var("SYNTH_WEBMAIL_URL").unwrap_or_else(|_| DEFAULT_WEBMAIL_URL.into()),
        TokenStore::load(),
    )
}

/// Rust-side login handler: spawned thread performs the request, then applies
/// results on the Slint event loop.
pub fn request_login(app: &Weak<App>, email: SharedString, password: SharedString, totp: SharedString) {
    if let Some(app) = app.upgrade() {
        app.set_login_loading(true);
        app.set_login_error(SharedString::default());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let result = client().login(
            &email.to_string(),
            &password.to_string(),
            Some(totp.to_string()).filter(|t| !t.is_empty()).as_deref(),
        );
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = app.upgrade() else { return };
            app.set_login_loading(false);
            match result {
                Ok(resp) => {
                    if let Some(t) = &resp.token {
                        TokenStore::save(t);
                        // Parity: admin → Dashboard, user → Mail (old /login).
                        app.set_screen(if resp.is_admin() { Screen::Dashboard } else { Screen::Mail });
                    } else {
                        app.set_login_error("login response contained no token".into());
                    }
                }
                Err(e) => app.set_login_error(e.to_string().into()),
            }
        });
    });
}

// (startup restore of sessions lives in store::shell::bootstrap)
