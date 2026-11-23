// SynthMail Desktop — native Slint console. See docs/plans/ui-slint-migration.md.
mod api;
mod store;

slint::include_modules!();

use slint::{ComponentHandle, Weak};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new()?;
    let app_weak = app.as_weak();

    {
        let app_weak = app_weak.clone();
        app.on_request_login(move |email, password, totp| {
            store::auth::request_login(&app_weak, email, password, totp);
        });
    }

    store::auth::bootstrap(&app);
    app.set_screen(Screen::Login);

    app.run()?;
    Ok(())
}
