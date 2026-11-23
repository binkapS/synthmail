// Shell/session logic: navigation, logout, auth guards.
use crate::api::{ApiClient, DEFAULT_CONTROL_URL, DEFAULT_WEBMAIL_URL};
use crate::store::token::TokenStore;
use crate::{App, Screen};
use slint::{ComponentHandle, Weak};

fn client() -> ApiClient {
    ApiClient::new(
        &std::env::var("SYNTH_CONTROL_URL").unwrap_or_else(|_| DEFAULT_CONTROL_URL.into()),
        &std::env::var("SYNTH_WEBMAIL_URL").unwrap_or_else(|_| DEFAULT_WEBMAIL_URL.into()),
        TokenStore::load(),
    )
}

/// go_to: navigation + logout handling.
pub fn go_to(app: &Weak<App>, screen: Screen) {
    if let Some(app) = app.upgrade() {
        if screen == Screen::Login {
            TokenStore::clear();
        }
        app.set_screen(screen);
    }
}

/// Guard: redirect to the right screen when authenticated.
pub fn guard_authenticated(app: &App) {
    if TokenStore::load().is_none() { return; }
    let app_weak = app.as_weak();
    {
        std::thread::spawn(move || {
            let result = client().me();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(app) = app_weak.upgrade() else { return };
                if let Ok(me) = result {
                    let _ = (&me.email, &me.force_password_change);
                    let screen = if matches!(me.kind.as_deref(), Some("admin" | "superadmin")) {
                        Screen::Dashboard
                    } else {
                        Screen::Mail
                    };
                    app.set_screen(screen);
                }
            });
        });
    }
}

/// Bootstrap: restore session on startup (like AuthGuard+load).
pub fn bootstrap(app: &App) {
    guard_authenticated(app);
}
