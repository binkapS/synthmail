// SynthMail Desktop — native Slint console. See docs/plans/ui-slint-migration.md.
mod api;
mod store;

slint::include_modules!();

use crate::store::admin::AdminStore;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::rc::Rc;
use std::time::Duration;

/// TAB_NAMES: AdminShell tab order (must match Sidebar).
const TAB_NAMES: [&str; 16] =
    ["overview", "server", "domains", "users", "aliases", "quota", "queues", "dkim", "filters",
     "vacation", "dmarc", "audit", "bans", "tokens", "config", "account"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::new()?;
    let app_weak = app.as_weak();

    let admin = Rc::new(AdminStore::new());

    // Wire AdminStore models into App (in-out property <[T]> on root).
    app.set_domains(ModelRc::from(admin.domains.clone()));
    app.set_users(ModelRc::from(admin.users.clone()));
    app.set_aliases(ModelRc::from(admin.aliases.clone()));
    app.set_tokens(ModelRc::from(admin.tokens.clone()));
    app.set_audit(ModelRc::from(admin.audit.clone()));
    app.set_dkim(ModelRc::from(admin.dkim.clone()));
    app.set_filters(ModelRc::from(admin.filters.clone()));
    app.set_vacation(ModelRc::from(admin.vacation.clone()));
    app.set_dmarc(ModelRc::from(admin.dmarc.clone()));
    app.set_bans(ModelRc::from(admin.bans.clone()));
    app.set_config(ModelRc::from(admin.config.clone()));
    app.set_queues(ModelRc::from(admin.queues.clone()));

    // Login/shell handlers.
    {
        let app_weak = app_weak.clone();
        app.on_request_login(move |email, password, totp| {
            store::auth::request_login(&app_weak, email, password, totp);
        });
    }
    {
        let aw = app_weak.clone();
        app.on_go_to(move |screen| store::shell::go_to(&aw, screen));
    }
    {
        let aw = app_weak.clone();
        app.on_logout(move || store::shell::go_to(&aw, Screen::Login));
    }

    // select_tab: set active + refresh that panel's data.
    {
        let admin = admin.clone();
        let aw = app_weak.clone();
        app.on_select_tab(move |index| {
            let Some(app) = aw.upgrade() else { return };
            let name = TAB_NAMES.get(index as usize)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "overview".into());
            app.set_active(name.clone().into());
            store::admin::refresh_panel(&admin, &name);
        });
    }

    // Admin callbacks (worker threads + invoke_from_event_loop). Slint
    // handlers return (); errors are set via App admin_error by panels.
    { let a = admin.clone(); app.on_refresh_panel(move |panel| store::admin::refresh_panel(&a, &panel)); }
    { let a = admin.clone(); app.on_create_domain(move |n, d| { store::admin::create_domain(&a, n, d).ok(); }); }
    { let a = admin.clone(); app.on_update_domain(move |i, n, d| { store::admin::update_domain(&a, i, n, d).ok(); }); }
    { let a = admin.clone(); app.on_delete_domain(move |i| { store::admin::delete_domain(&a, i).ok(); }); }
    { let a = admin.clone(); app.on_verify_domain(move |i| { store::admin::verify_domain(&a, i).ok(); }); }
    { let a = admin.clone(); app.on_show_dns(move |i| { store::admin::show_dns(&a, i).ok(); }); }
    { let a = admin.clone(); app.on_generate_dkim(move |i| { store::admin::generate_dkim(&a, i).ok(); }); }
    { let a = admin.clone(); app.on_create_user(move |e, p, k, q| { store::admin::create_user(&a, e, p, k, q).ok(); }); }
    { let a = admin.clone(); app.on_toggle_user(move |i, act| { store::admin::toggle_user(&a, i, act).ok(); }); }
    { let a = admin.clone(); app.on_reset_user_password(move |i, p| { store::admin::reset_user_password(&a, i, p).ok(); }); }
    { let a = admin.clone(); app.on_create_alias(move |n, t| { store::admin::create_alias(&a, n, t).ok(); }); }
    { let a = admin.clone(); app.on_update_alias(move |i, n, t| { store::admin::update_alias(&a, i, n, t).ok(); }); }
    { let a = admin.clone(); app.on_delete_alias(move |i| { store::admin::delete_alias(&a, i).ok(); }); }
    { let a = admin.clone();
      app.on_create_token(move |n, d| { store::admin::create_token(&a, n, d).ok(); }); }
    { let a = admin.clone(); app.on_revoke_token(move |i| { store::admin::revoke_token(&a, i).ok(); }); }
    { let a = admin.clone(); app.on_refresh_audit(move |_limit| store::admin::refresh_panel(&a, "audit")); }
    { let a = admin.clone(); app.on_save_filters(move |j| { store::admin::save_filters(&a, j.to_string()).ok(); }); }
    { let a = admin.clone(); app.on_save_vacation(move |j| { store::admin::save_vacation(&a, j.to_string()).ok(); }); }
    { let a = admin.clone(); let aw = app_weak.clone();
      app.on_export_config(move || {
          if let Ok(s) = store::admin::export_config(&a) {
              if let Some(app) = aw.upgrade() { app.set_export_result(s.into()); }
          }
      }); }
    { let a = admin.clone(); app.on_import_config(move |j| { store::admin::import_config(&a, j.to_string()).ok(); }); }
    { let a = admin.clone(); app.on_refresh_queues(move || store::admin::refresh_panel(&a, "queues")); }
    { let a = admin.clone(); let aw = app_weak.clone();
      app.on_refresh_stats(move || {
          let rows = store::admin::refresh_stats(&a, &store::auth::client());
          if let Some(app) = aw.upgrade() {
              app.set_stats(ModelRc::new(VecModel::from(rows)));
          }
      }); }
    { let a = admin.clone(); app.on_revoke_ban(move |ip| { store::admin::revoke_ban(&a, ip).ok(); }); }

    // ---- Mail handlers ----
    let mail = Rc::new(store::mail::MailStore::new());
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_compose(move || { if let Some(app) = aw.upgrade() { app.set_compose_mode(true); } }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_select_folder(move |f| { if let Some(app) = aw.upgrade() { app.set_selected_folder(f); m.load_messages(&app.get_selected_folder().to_string()); } }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_select_message(move |uid| { if let Some(app) = aw.upgrade() { app.set_selected_uid(uid); m.load_full(uid); } }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_send(move |to, subject, body| {
          if let Some(app) = aw.upgrade() {
              app.set_compose_mode(false);
              app.set_compose_to(SharedString::default());
              app.set_compose_subject(SharedString::default());
              app.set_compose_body(SharedString::default());
              app.set_mail_info(format!("Sent to {}.", to).into());
          }
          m.send(to, subject, body);
      }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_reply_send(move |to, text| { if let Some(app) = aw.upgrade() { app.set_reply_text(SharedString::default()); } m.reply(to, text); }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_on_draft_delete(move |uid| {
          if let Some(app) = aw.upgrade() {
              app.set_mail_info("Draft deleted.".into());
              m.delete_draft(uid);
              m.load_messages(&app.get_selected_folder().to_string());
          }
      }); }
    { let m = mail.clone(); let aw = app_weak.clone();
      app.on_refresh_mail(move || { if let Some(app) = aw.upgrade() { m.load_folders(); m.load_messages(&app.get_selected_folder().to_string()); } }); }
    // Poll messages every 30s while the Mail screen is open.
    { let m = mail.clone(); let aw = app_weak.clone();
      let timer = Timer::default();
      timer.start(TimerMode::Repeated, Duration::from_secs(30), move || {
          if let Some(app) = aw.upgrade() {
              if app.get_screen() == Screen::Mail {
                  m.load_messages(&app.get_selected_folder().to_string());
              }
          }
      }); }

    store::shell::bootstrap(&app);

    store::shell::bootstrap(&app);
    app.set_screen(Screen::Login);

    app.run()?;
    Ok(())
}
