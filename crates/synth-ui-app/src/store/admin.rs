// AdminStore — data for the 16 admin panels: models + fetch + mutations.
// Fetch: worker threads + mpsc channel + slint::Timer polling on the main
// thread (models are Rc-based; invoke_from_event_loop needs Send).
use crate::api::{ApiClient, ApiError};
use crate::slint_generatedApp::*; // Slint row structs (models.slint)
use serde::Deserialize;
use slint::{Model, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------- serde DTOs (backend responses) ----------

#[derive(Deserialize, Default)]
struct DomainDto { id: String, name: String, description: Option<String>,
                    catch_all: Option<bool>, verified: Option<bool>, created_at: Option<String> }
impl From<&DomainDto> for DomainRow {
    fn from(d: &DomainDto) -> Self {
        DomainRow { id: d.id.clone().into(), name: d.name.clone().into(),
                    description: d.description.clone().unwrap_or_default().into(),
                    catch_all: d.catch_all.unwrap_or(false), verified: d.verified.unwrap_or(false),
                    created_at: d.created_at.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct UserDto { id: String, email: String, kind: Option<String>, status: Option<String>,
                 quota_used: Option<i32>, quota_total: Option<i32> }
impl From<&UserDto> for UserRow {
    fn from(d: &UserDto) -> Self {
        UserRow { id: d.id.clone().into(), email: d.email.clone().into(),
                  role: d.kind.clone().unwrap_or_default().into(), status: d.status.clone().unwrap_or_default().into(),
                  quota_used: d.quota_used.unwrap_or(0), quota_total: d.quota_total.unwrap_or(0) }
    }
}

#[derive(Deserialize, Default)]
struct AliasDto { id: String, name: String, target: Option<String>, status: Option<String> }
impl From<&AliasDto> for AliasRow {
    fn from(d: &AliasDto) -> Self {
        AliasRow { id: d.id.clone().into(), name: d.name.clone().into(),
                   target: d.target.clone().unwrap_or_default().into(), status: d.status.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct TokenDto { id: String, name: String, created_at: Option<String>, expiry: Option<String>, status: Option<String> }
impl From<&TokenDto> for TokenRow {
    fn from(d: &TokenDto) -> Self {
        TokenRow { id: d.id.clone().into(), name: d.name.clone().into(),
                   expiry: d.expiry.clone().unwrap_or_default().into(), created_at: d.created_at.clone().unwrap_or_default().into(),
                   status: d.status.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct AuditDto { time: Option<String>, actor: Option<String>, action: Option<String>,
                  target: Option<String>, id: Option<String>, detail: Option<String> }
impl From<&AuditDto> for AuditRow {
    fn from(d: &AuditDto) -> Self {
        AuditRow { time: d.time.clone().or(d.id.clone()).unwrap_or_default().into(),
                   actor: d.actor.clone().unwrap_or_default().into(), action: d.action.clone().unwrap_or_default().into(),
                   target: d.target.clone().unwrap_or_default().into(), detail: d.detail.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct DkimRowDto { domain: Option<String>, domain_name: Option<String>, selector: Option<String>,
                    key_type: Option<String>, status: Option<String> }
impl From<&DkimRowDto> for DkimRow {
    fn from(d: &DkimRowDto) -> Self {
        DkimRow { domain: d.domain.clone().or(d.domain_name.clone()).unwrap_or_default().into(),
                  selector: d.selector.clone().unwrap_or_default().into(),
                  key_type: d.key_type.clone().unwrap_or_default().into(),
                  status: d.status.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct FilterRowDto { id: String, name: String, action: Option<String>, priority: Option<i32> }
impl From<&FilterRowDto> for FilterRow {
    fn from(d: &FilterRowDto) -> Self {
        FilterRow { id: d.id.clone().into(), name: d.name.clone().into(),
                    action: d.action.clone().unwrap_or_default().into(), priority: d.priority.unwrap_or(0) }
    }
}

#[derive(Deserialize, Default)]
struct VacationRowDto { domain: String, enabled: Option<bool>, start: Option<String>,
                        end: Option<String>, subject: Option<String> }
impl From<&VacationRowDto> for VacationRow {
    fn from(d: &VacationRowDto) -> Self {
        VacationRow { domain: d.domain.clone().into(), enabled: d.enabled.unwrap_or(false),
                      start: d.start.clone().unwrap_or_default().into(), end: d.end.clone().unwrap_or_default().into(),
                      subject: d.subject.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct DmarcRowDto { domain: String, aggregate_count: Option<i32>, last_received: Option<String> }
impl From<&DmarcRowDto> for DmarcRow {
    fn from(d: &DmarcRowDto) -> Self {
        DmarcRow { domain: d.domain.clone().into(), aggregate_count: d.aggregate_count.unwrap_or(0),
                   last_received: d.last_received.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct BanRowDto { ip: String, until: Option<String>, reason: Option<String> }
impl From<&BanRowDto> for BanRow {
    fn from(d: &BanRowDto) -> Self {
        BanRow { ip: d.ip.clone().into(), until: d.until.clone().unwrap_or_default().into(),
                 reason: d.reason.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct ConfigRowDto { key: String, value: Option<String> }
impl From<&ConfigRowDto> for ConfigRow {
    fn from(d: &ConfigRowDto) -> Self {
        ConfigRow { key: d.key.clone().into(), value: d.value.clone().unwrap_or_default().into() }
    }
}

#[derive(Deserialize, Default)]
struct QueueRowDto { name: String, depth: Option<i32> }
impl From<&QueueRowDto> for QueueRow {
    fn from(d: &QueueRowDto) -> Self {
        QueueRow { name: d.name.clone().into(), depth: d.depth.unwrap_or(0) }
    }
}

// ---------- AdminStore ----------

pub struct AdminStore {
    pub domains: Rc<VecModel<DomainRow>>,
    pub users: Rc<VecModel<UserRow>>,
    pub aliases: Rc<VecModel<AliasRow>>,
    pub tokens: Rc<VecModel<TokenRow>>,
    pub audit: Rc<VecModel<AuditRow>>,
    pub dkim: Rc<VecModel<DkimRow>>,
    pub filters: Rc<VecModel<FilterRow>>,
    pub vacation: Rc<VecModel<VacationRow>>,
    pub dmarc: Rc<VecModel<DmarcRow>>,
    pub bans: Rc<VecModel<BanRow>>,
    pub config: Rc<VecModel<ConfigRow>>,
    pub queues: Rc<VecModel<QueueRow>>,
    pub timers: RefCell<Vec<Timer>>,
}

impl AdminStore {
    pub fn new() -> Self {
        Self {
            domains: Rc::new(VecModel::from(vec![])),
            users: Rc::new(VecModel::from(vec![])),
            aliases: Rc::new(VecModel::from(vec![])),
            tokens: Rc::new(VecModel::from(vec![])),
            audit: Rc::new(VecModel::from(vec![])),
            dkim: Rc::new(VecModel::from(vec![])),
            filters: Rc::new(VecModel::from(vec![])),
            vacation: Rc::new(VecModel::from(vec![])),
            dmarc: Rc::new(VecModel::from(vec![])),
            bans: Rc::new(VecModel::from(vec![])),
            config: Rc::new(VecModel::from(vec![])),
            queues: Rc::new(VecModel::from(vec![])),
            timers: RefCell::new(vec![]),
        }
    }

    /// Spawns a worker thread; a Repeated timer on the main thread polls the
    /// channel until the result arrives, then applies it to the model.
    fn drain<M: Clone + 'static>(
        &self,
        model: &Rc<VecModel<M>>,
        rx: mpsc::Receiver<Result<Vec<M>, ApiError>>,
    ) {
        let model = model.clone();
        let rx = Arc::new(Mutex::new(rx));
        let timer = Timer::default();
        let mut stopped = false;
        timer.start(TimerMode::Repeated, Duration::from_millis(20), move || {
            if stopped { return; }
            if let Ok(result) = rx.lock().unwrap().try_recv() {
                stopped = true;
                if let Ok(rows) = result {
                    model.clear();
                    model.extend_from_slice(&rows);
                }
            }
        });
        self.timers.borrow_mut().push(timer);
    }

    // ---- refresh (fetch-on-select / refresh buttons) ----

    pub fn refresh_domains(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<DomainDto>>("/api/v1/domains").map(|v| v.iter().map(DomainRow::from).collect()));
        });
        self.drain(&self.domains, rx);
    }
    pub fn refresh_users(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<UserDto>>("/api/v1/users").map(|v| v.iter().map(UserRow::from).collect()));
        });
        self.drain(&self.users, rx);
    }
    pub fn refresh_aliases(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<AliasDto>>("/api/v1/aliases").map(|v| v.iter().map(AliasRow::from).collect()));
        });
        self.drain(&self.aliases, rx);
    }
    pub fn refresh_tokens(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<TokenDto>>("/api/v1/api-tokens").map(|v| v.iter().map(TokenRow::from).collect()));
        });
        self.drain(&self.tokens, rx);
    }
    pub fn refresh_audit(&self, c: &ApiClient, limit: i64) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<AuditDto>>(&format!("/api/v1/admin/audit?limit={limit}")).map(|v| v.iter().map(AuditRow::from).collect()));
        });
        self.drain(&self.audit, rx);
    }
    pub fn refresh_dkim(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<DkimRowDto>>("/api/v1/dkim").map(|v| v.iter().map(DkimRow::from).collect()));
        });
        self.drain(&self.dkim, rx);
    }
    pub fn refresh_filters(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<FilterRowDto>>("/api/v1/filters").map(|v| v.iter().map(FilterRow::from).collect()));
        });
        self.drain(&self.filters, rx);
    }
    pub fn refresh_vacation(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<VacationRowDto>>("/api/v1/vacation").map(|v| v.iter().map(VacationRow::from).collect()));
        });
        self.drain(&self.vacation, rx);
    }
    pub fn refresh_dmarc(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<DmarcRowDto>>("/api/v1/dmarc/reports").map(|v| v.iter().map(DmarcRow::from).collect()));
        });
        self.drain(&self.dmarc, rx);
    }
    pub fn refresh_bans(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<BanRowDto>>("/api/v1/admin/bans").map(|v| v.iter().map(BanRow::from).collect()));
        });
        self.drain(&self.bans, rx);
    }
    pub fn refresh_config(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<Vec<ConfigRowDto>>("/api/v1/config/export").map(|v| v.iter().map(ConfigRow::from).collect()));
        });
        self.drain(&self.config, rx);
    }
    pub fn refresh_queues(&self, c: &ApiClient) {
        let (tx, rx) = mpsc::channel();
        let c = c.clone();
        std::thread::spawn(move || {
            let _ = tx.send(c.get::<serde_json::Value>("/api/v1/queues").map(|v| {
                let q = v.get("queue").cloned().unwrap_or(v.clone());
                q.as_object().map(|o| o.iter().filter_map(|(k, val)|
                    Some(QueueRow { name: k.clone().into(), depth: val.as_i64().unwrap_or(0) as i32 })
                ).collect()).unwrap_or_default()
            }));
        });
        self.drain(&self.queues, rx);
    }

    fn refresh_stats_impl(c: &ApiClient) -> Vec<StatRow> {
        let domains: Vec<DomainDto> = c.get("/api/v1/domains").unwrap_or_default();
        let users: Vec<UserDto> = c.get("/api/v1/users").unwrap_or_default();
        let quota: serde_json::Value = c.get("/api/v1/quota").unwrap_or_default();
        let queues: serde_json::Value = c.get("/api/v1/queues").unwrap_or_default();
        let deferred = queues.get("queue").and_then(|q| q.get("deferred")).and_then(|v| v.as_i64()).unwrap_or(0);
        vec![
            StatRow { label: "Domains".into(), value: domains.len().to_string().into(), sub: "Total hosted domains".into() },
            StatRow { label: "Users".into(), value: users.len().to_string().into(), sub: "Mail accounts".into() },
            StatRow { label: "Deferred".into(), value: deferred.to_string().into(), sub: "Queue backlog".into() },
            StatRow { label: "Quota".into(),
                      value: format!("{} MB", quota.get("used_mb").and_then(|v| v.as_i64()).unwrap_or(0)).into(),
                      sub: format!("of {} MB", quota.get("quota_mb").and_then(|v| v.as_i64()).unwrap_or(0)).into() },
        ]
    }

    // ---- mutations (main thread, synchronous) ----

    pub fn create_domain(&self, c: &ApiClient, name: &str, desc: Option<&str>) -> Result<(), ApiError> {
        c.post::<serde_json::Value>("/api/v1/domains", &serde_json::json!({"domain": name, "description": desc, "catch_all": null})).map(|_| ())
    }
    pub fn update_domain(&self, c: &ApiClient, id: &str, name: &str, desc: Option<&str>) -> Result<(), ApiError> {
        c.put::<serde_json::Value>(&format!("/api/v1/domains/{id}"), &serde_json::json!({"name": name, "description": desc})).map(|_| ())
    }
    pub fn delete_domain(&self, c: &ApiClient, id: &str) -> Result<(), ApiError> {
        let m = self.domains.clone();
        c.delete::<serde_json::Value>(&format!("/api/v1/domains/{id}")).map(|_| {
            if let Some(i) = (0..m.row_count()).find(|&i| m.row_data(i).map(|x| x.id.to_string()) == Some(id.to_string())) {
                m.remove(i);
            }
        })
    }
    pub fn create_user(&self, c: &ApiClient, email: &str, password: &str, kind: &str, quota_mb: i64) -> Result<(), ApiError> {
        c.post::<serde_json::Value>("/api/v1/users", &serde_json::json!({"email": email, "password": password, "kind": kind, "quota_mb": quota_mb})).map(|_| ())
    }
    pub fn toggle_user(&self, c: &ApiClient, id: &str, active: bool) -> Result<(), ApiError> {
        let m = self.users.clone();
        c.put::<serde_json::Value>(&format!("/api/v1/users/{id}"), &serde_json::json!({"active": active})).map(|_| {
            if let Some(i) = (0..m.row_count()).find(|&i| m.row_data(i).map(|x| x.id.to_string()) == Some(id.to_string())) {
                let mut row = m.row_data(i).unwrap();
                row.status = if active { "active".into() } else { "disabled".into() };
                m.set_row_data(i, row);
            }
        })
    }
    pub fn reset_user_password(&self, c: &ApiClient, id: &str, password: &str) -> Result<(), ApiError> {
        c.post::<serde_json::Value>(&format!("/api/v1/users/{id}/password"), &serde_json::json!({"password": password})).map(|_| ())
    }
    pub fn create_alias(&self, c: &ApiClient, name: &str, target: &str) -> Result<(), ApiError> {
        c.post::<serde_json::Value>("/api/v1/aliases", &serde_json::json!({"name": name, "target": target})).map(|_| ())
    }
    pub fn update_alias(&self, c: &ApiClient, id: &str, name: &str, target: &str) -> Result<(), ApiError> {
        c.put::<serde_json::Value>(&format!("/api/v1/aliases/{id}"), &serde_json::json!({"name": name, "target": target})).map(|_| ())
    }
    pub fn delete_alias(&self, c: &ApiClient, id: &str) -> Result<(), ApiError> {
        let m = self.aliases.clone();
        c.delete::<serde_json::Value>(&format!("/api/v1/aliases/{id}")).map(|_| {
            if let Some(i) = (0..m.row_count()).find(|&i| m.row_data(i).map(|x| x.id.to_string()) == Some(id.to_string())) {
                m.remove(i);
            }
        })
    }
    /// Creates a token, refreshes the list, and returns the raw token text for
    /// one-time display (panels show it in a modal).
    pub fn create_token(&self, c: &ApiClient, name: &str, expires_in_days: i64) -> Result<String, ApiError> {
        let token: String = c.post::<serde_json::Value>("/api/v1/api-tokens",
            &serde_json::json!({"name": name, "expires_in_days": expires_in_days}))?
            .get("token").and_then(|t| t.as_str()).unwrap_or_default().to_string();
        self.refresh_tokens(c);
        Ok(token)
    }
    pub fn revoke_token(&self, c: &ApiClient, id: &str) -> Result<(), ApiError> {
        c.delete::<serde_json::Value>(&format!("/api/v1/api-tokens/{id}")).map(|_| ())
    }
    pub fn save_filters(&self, c: &ApiClient, json: &str) -> Result<(), ApiError> {
        c.put::<serde_json::Value>("/api/v1/filters", &json.parse::<serde_json::Value>().map_err(|e| ApiError { status: None, message: e.to_string() })?).map(|_| ())
    }
    pub fn save_vacation(&self, c: &ApiClient, json: &str) -> Result<(), ApiError> {
        c.put::<serde_json::Value>("/api/v1/vacation", &json.parse::<serde_json::Value>().map_err(|e| ApiError { status: None, message: e.to_string() })?).map(|_| ())
    }
    pub fn export_config(&self, c: &ApiClient) -> Result<String, ApiError> {
        let v: serde_json::Value = c.get("/api/v1/config/export")?;
        Ok(serde_json::to_string_pretty(&v).unwrap_or_default())
    }
    pub fn import_config(&self, c: &ApiClient, json: &str) -> Result<(), ApiError> {
        c.post::<serde_json::Value>("/api/v1/config/import", &json.parse::<serde_json::Value>().map_err(|e| ApiError { status: None, message: e.to_string() })?).map(|_| ())
    }
    pub fn generate_dkim(&self, c: &ApiClient, domain_id: &str) -> Result<(), ApiError> {
        c.post::<serde_json::Value>(&format!("/api/v1/dkim/{domain_id}/generate"), &serde_json::json!({"key_type": null, "selector": null})).map(|_| ())
    }
}

/// refresh_panel: fetch data for one panel ("domains", "users", "aliases",
/// "tokens", "audit", "dkim", "filters", "vacation", "dmarc", "bans",
/// "config", "queues", "stats", "server", "quota").
pub fn refresh_panel(store: &AdminStore, panel: &str) {
    let c = crate::store::auth::client();
    match panel {
        "domains" => store.refresh_domains(&c),
        "users" => store.refresh_users(&c),
        "aliases" => store.refresh_aliases(&c),
        "tokens" => store.refresh_tokens(&c),
        "audit" => store.refresh_audit(&c, 50),
        "dkim" => store.refresh_dkim(&c),
        "filters" => store.refresh_filters(&c),
        "vacation" => store.refresh_vacation(&c),
        "dmarc" => store.refresh_dmarc(&c),
        "bans" => store.refresh_bans(&c),
        "config" => store.refresh_config(&c),
        "queues" => store.refresh_queues(&c),
        "stats" | "overview" | "server" | "quota" => { refresh_stats(store, &c); }
        _ => {}
    }
}

/// refresh_stats: synchronous fetch of the dashboard stats (overview/server/
/// quota). Called on the main thread via App callbacks.
pub fn refresh_stats(_store: &AdminStore, c: &ApiClient) -> Vec<StatRow> {
    AdminStore::refresh_stats_impl(c)
}

pub fn create_domain(store: &AdminStore, name: SharedString, desc: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let name = name.to_string();
    let desc = desc.to_string();
    let r = store.create_domain(&c, &name, if desc.is_empty() { None } else { Some(desc.as_str()) });
    if r.is_ok() { refresh_panel(store, "domains"); }
    r
}
pub fn update_domain(store: &AdminStore, id: SharedString, name: SharedString, desc: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let id = id.to_string();
    let name = name.to_string();
    let desc = desc.to_string();
    let r = store.update_domain(&c, &id, &name, if desc.is_empty() { None } else { Some(desc.as_str()) });
    if r.is_ok() { refresh_panel(store, "domains"); }
    r
}
pub fn delete_domain(store: &AdminStore, id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let r = store.delete_domain(&c, &id.to_string());
    if r.is_ok() { refresh_panel(store, "domains"); }
    r
}
pub fn verify_domain(_store: &AdminStore, id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    c.get::<serde_json::Value>(&format!("/api/v1/domains/{id}/verify")).map(|_| ())
}
pub fn show_dns(_store: &AdminStore, id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    c.get::<serde_json::Value>(&format!("/api/v1/domains/{id}/dns")).map(|_| ())
}
pub fn generate_dkim(store: &AdminStore, domain_id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.generate_dkim(&c, &domain_id.to_string())
}
pub fn create_user(store: &AdminStore, email: SharedString, password: SharedString, kind: SharedString, quota_mb: i32) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let r = store.create_user(&c, &email.to_string(), &password.to_string(), &kind.to_string(), quota_mb as i64);
    if r.is_ok() { refresh_panel(store, "users"); }
    r
}
pub fn toggle_user(store: &AdminStore, id: SharedString, active: bool) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.toggle_user(&c, &id.to_string(), active)
}
pub fn reset_user_password(store: &AdminStore, id: SharedString, password: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.reset_user_password(&c, &id.to_string(), &password.to_string())
}
pub fn create_alias(store: &AdminStore, name: SharedString, target: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let r = store.create_alias(&c, &name.to_string(), &target.to_string());
    if r.is_ok() { refresh_panel(store, "aliases"); }
    r
}
pub fn update_alias(store: &AdminStore, id: SharedString, name: SharedString, target: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.update_alias(&c, &id.to_string(), &name.to_string(), &target.to_string())
}
pub fn delete_alias(store: &AdminStore, id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    let r = store.delete_alias(&c, &id.to_string());
    if r.is_ok() { refresh_panel(store, "aliases"); }
    r
}
pub fn create_token(store: &AdminStore, name: SharedString, expires_in_days: i32) -> Result<String, ApiError> {
    let c = crate::store::auth::client();
    store.create_token(&c, &name.to_string(), expires_in_days as i64)
}
pub fn revoke_token(store: &AdminStore, id: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.revoke_token(&c, &id.to_string())
}
pub fn save_filters(store: &AdminStore, json: String) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.save_filters(&c, &json)
}
pub fn save_vacation(store: &AdminStore, json: String) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.save_vacation(&c, &json)
}
pub fn export_config(store: &AdminStore) -> Result<String, ApiError> {
    let c = crate::store::auth::client();
    store.export_config(&c)
}
pub fn import_config(store: &AdminStore, json: String) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    store.import_config(&c, &json)
}
pub fn revoke_ban(_store: &AdminStore, ip: SharedString) -> Result<(), ApiError> {
    let c = crate::store::auth::client();
    c.delete::<serde_json::Value>(&format!("/api/v1/admin/bans/{ip}")).map(|_| ())
}
