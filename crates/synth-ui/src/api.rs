//! Typed HTTP API client for the SynthMail Leptos UI (CSR, wasm32).
//!
//! Mirrors the backend contract in `docs/ui-design.md`:
//! - Control plane under `/api/v1` (synth-control): auth, domains, users,
//!   aliases, quota, queues, DKIM, filters, vacation, audit, bans, config,
//!   DMARC, push, TOTP, API tokens.
//! - Webmail under `/api/v1/mail` (synth-webmail): folders, messages,
//!   search, send, drafts.
//! - JMAP at `/jmap/session` + `/jmap` (synth-webmail).
//!
//! All async functions return `Result<T, String>` where the `String` is a
//! user-friendly error message. Server error bodies are JSON of the form
//! `{"detail": "...", ...}` (see `synth_common::error::to_problem_json`);
//! when present the `detail` text is surfaced, otherwise the HTTP status and
//! a truncated body are shown.
//!
//! The session token is kept in `localStorage` under [`TOKEN_KEY`]; every
//! request automatically sends `Authorization: Bearer <token>` when a token
//! is present.
//!
//! Only the token helpers touch the DOM/LS APIs. Everything else is pure
//! (URL building, error parsing, quota normalization) or wasm network code.
//! The pure helpers are covered by `//! The pure helpers are covered by `#[cfg(test)]` unit tests, which run on
//! the host target via `cargo test -p synth-ui --offline`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Base path for control + webmail endpoints (both are served under this
/// prefix by the edge/router; webmail mail calls add `/mail`).
pub const API_BASE: &str = "/api/v1";

/// `localStorage` key for the session JWT.
pub const TOKEN_KEY: &str = "synthmail_token";

// ---------------------------------------------------------------------------
// Token storage
// ---------------------------------------------------------------------------
//
// NOTE: the `web-sys` dependency in this crate's graph (via Leptos) does
// not enable the `Storage`/`local_storage` features, so `Window::local_storage`
// and `web_sys::Storage` are unavailable without Cargo.toml changes. The
// helpers below reach the same `window.localStorage` API through
// `wasm_bindgen` structural extern types over `JsValue` (the exact
// JsValue/wasm-bindgen approach), and gracefully no-op when absent.

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen]
    pub type JsWindow;

    #[wasm_bindgen::prelude::wasm_bindgen]
    pub type JsLocalStorage;

    #[wasm_bindgen(method, structural, catch, js_name = "getItem")]
    fn get_item(this: &JsLocalStorage, key: &str) -> Result<Option<String>, wasm_bindgen::JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = "setItem")]
    fn set_item(this: &JsLocalStorage, key: &str, value: &str)
    -> Result<(), wasm_bindgen::JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = "removeItem")]
    fn remove_item(this: &JsLocalStorage, key: &str) -> Result<(), wasm_bindgen::JsValue>;

    #[wasm_bindgen(method, structural, catch, js_name = "localStorage")]
    fn local_storage(this: &JsWindow) -> Result<JsLocalStorage, wasm_bindgen::JsValue>;
}

/// Borrow `window.localStorage`, or `None` when unavailable (non-wasm host
/// target, missing window, or storage access throws, e.g. sandboxed embeds).
fn get_local_storage() -> Option<JsLocalStorage> {
    use wasm_bindgen::JsCast;
    let window = leptos::web_sys::window()?;
    let w: JsWindow = JsCast::unchecked_into(window);
    w.local_storage().ok()
}

/// Read the session token from `localStorage`, or `None` when absent,
/// empty, or unavailable.
pub fn get_token() -> Option<String> {
    // In-memory fallback first: browsers with localStorage blocked
    // (private mode, storage policies) still keep the session alive for
    // the tab lifetime via `MEM_TOKEN`, so auth gating can't misfire.
    if let Some(t) = MEM_TOKEN
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.clone())
    {
        return Some(t);
    }
    let storage = get_local_storage()?;
    let tok = storage.get_item(TOKEN_KEY).ok()?;
    // localStorage.getItem returns null when missing (None) — empty-string
    // tokens are treated as absent.
    match tok {
        Some(t) if !t.is_empty() => Some(t),
        _ => None,
    }
}

static MEM_TOKEN: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

fn mem_set(token: Option<String>) {
    let m = MEM_TOKEN.get_or_init(|| std::sync::Mutex::new(None));
    if let Ok(mut g) = m.lock() {
        *g = token;
    }
}

/// Store the session token in `localStorage`. No-op if storage is absent.
pub fn set_token(token: &str) {
    mem_set(Some(token.to_string()));
    if let Some(storage) = get_local_storage() {
        let _ = storage.set_item(TOKEN_KEY, token);
    }
}

/// Remove the session token from `localStorage`. No-op if storage is absent.
pub fn clear_token() {
    mem_set(None);
    if let Some(storage) = get_local_storage() {
        let _ = storage.remove_item(TOKEN_KEY);
    }
}

/// Whether a non-empty session token is present.
pub fn has_token() -> bool {
    get_token().is_some()
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested, host-safe)
// ---------------------------------------------------------------------------

/// Join `API_BASE` with a control/webmail `path` (must start with `/`).
pub fn api_url(path: &str) -> String {
    let p = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{API_BASE}{p}")
}

/// Percent-encode one query-string component. Unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`) pass through; everything else becomes `%XX`.
pub fn encode_query_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the `GET /mail/messages` URL with optional `folder` / `limit`.
pub fn messages_url(folder: Option<&str>, limit: Option<u32>) -> String {
    let mut url = api_url("/mail/messages");
    let mut qs = String::new();
    if let Some(f) = folder {
        qs.push_str(&format!("folder={}", encode_query_component(f)));
    }
    if let Some(l) = limit {
        if !qs.is_empty() {
            qs.push('&');
        }
        qs.push_str(&format!("limit={l}"));
    }
    if !qs.is_empty() {
        url.push('?');
        url.push_str(&qs);
    }
    url
}

/// Build the `GET /mail/search` URL for query `q`.
pub fn mail_search_url(q: &str) -> String {
    format!(
        "{}?q={}",
        api_url("/mail/search"),
        encode_query_component(q)
    )
}

/// Build the `GET /admin/audit` URL with optional `limit`.
pub fn audit_url(limit: Option<u32>) -> String {
    let mut url = api_url("/admin/audit");
    if let Some(l) = limit {
        url.push_str(&format!("?limit={l}"));
    }
    url
}

/// Extract a human-readable detail from a JSON error body, if present.
/// Looks at `detail` first (the synth backend's field), then `message`,
/// `error`, then `title`.
fn err_detail_from_body(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body.trim()).ok()?;
    for key in ["detail", "message", "error", "title"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// User-friendly error message for a failed HTTP response: the server's
/// `detail` when available, otherwise the status + a truncated body.
pub fn http_error_message(status: u16, body: &str) -> String {
    if let Some(detail) = err_detail_from_body(body) {
        return format!("{detail} (HTTP {status})");
    }
    let text = body.trim();
    if text.is_empty() {
        format!("HTTP {status}")
    } else {
        let short: String = text.chars().take(300).collect();
        if text.chars().count() > 300 {
            format!("HTTP {status}: {short}…")
        } else {
            format!("HTTP {status}: {short}")
        }
    }
}

/// Body of the dispatch request sent to `POST /jmap`. When `method_calls`
/// is already an object containing `method_calls` it is passed through;
/// otherwise it is wrapped as the standard JMAP envelope with the core +
/// mail capabilities advertised and `sessionState` consumed server-side.
pub fn jmap_request_body(method_calls: &Value) -> Value {
    if method_calls.get("method_calls").is_some() {
        return method_calls.clone();
    }
    serde_json::json!({
        "using": ["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail"],
        "method_calls": method_calls,
    })
}

// ---------------------------------------------------------------------------
// Response/request models
// ---------------------------------------------------------------------------

/// `POST /auth/login` response: `{token, email, kind}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginResp {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub kind: String,
}

/// `GET /auth/me` response: `{id, email, kind, quota_mb, force_password_change}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeResp {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub quota_mb: i64,
    #[serde(default)]
    pub force_password_change: bool,
}

/// User row (synth-control `users` table). Date fields are RFC3339 strings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub quota_mb: i64,
    #[serde(default)]
    pub used_mb: i64,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub force_password_change: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_login_at: Option<String>,
}

/// Domain row (synth-control `domains` table).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Domain {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub catch_all: bool,
    #[serde(default)]
    pub dkim_selector: Option<String>,
    #[serde(default)]
    pub dkim_enabled: bool,
    #[serde(default)]
    pub spf_value: Option<String>,
    #[serde(default)]
    pub dmarc_policy: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub created_at: String,
}

/// Alias row (synth-control `aliases` table).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Alias {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub goto: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub created_at: String,
}

/// One row of the quota overview.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuotaUser {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub quota_mb: i64,
    #[serde(default)]
    pub used_mb: i64,
    #[serde(default)]
    pub pct: f64,
}

/// Quota overview, normalized to `{quota_mb, used_mb, users}`.
///
/// The backend currently returns a bare array of per-user rows; when the
/// response is a bare array the totals are summed from the rows. When the
/// server later returns the wrapped object shape, extra/missing totals are
/// filled from the rows.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QuotaResp {
    #[serde(default)]
    pub quota_mb: i64,
    #[serde(default)]
    pub used_mb: i64,
    #[serde(default)]
    pub users: Vec<QuotaUser>,
}

/// Normalize either quota wire shape into [`QuotaResp`]. Pure / unit-tested.
pub fn parse_quota(body: &str) -> Result<QuotaResp, String> {
    let v: Value =
        serde_json::from_str(body.trim()).map_err(|e| format!("Invalid response JSON: {e}"))?;
    match v {
        Value::Array(_) => {
            let users: Vec<QuotaUser> =
                serde_json::from_value(v).map_err(|e| format!("Invalid response JSON: {e}"))?;
            let quota_mb: i64 = users.iter().map(|u| u.quota_mb).sum();
            let used_mb: i64 = users.iter().map(|u| u.used_mb).sum();
            Ok(QuotaResp {
                quota_mb,
                used_mb,
                users,
            })
        }
        Value::Object(_) => {
            let mut resp: QuotaResp =
                serde_json::from_value(v).map_err(|e| format!("Invalid response JSON: {e}"))?;
            if !resp.users.is_empty() {
                if resp.quota_mb == 0 {
                    resp.quota_mb = resp.users.iter().map(|u| u.quota_mb).sum();
                }
                if resp.used_mb == 0 {
                    resp.used_mb = resp.users.iter().map(|u| u.used_mb).sum();
                }
            }
            Ok(resp)
        }
        _ => Err("Unexpected quota response shape".to_string()),
    }
}

/// Queues response: `{queue: {deferred, active, hold}, activity_1h,
/// postfix_queue_size}` (plus an informational `note`, ignored).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueResp {
    #[serde(default)]
    pub queue: QueueCounts,
    #[serde(default)]
    pub activity_1h: i64,
    #[serde(default)]
    pub postfix_queue_size: i64,
}

/// Postfix queue counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueCounts {
    #[serde(default)]
    pub deferred: i64,
    #[serde(default)]
    pub active: i64,
    #[serde(default)]
    pub hold: i64,
}

/// `POST /dkim/{domain_id}/generate` response. The backend returns
/// `{domain, selector, key_type, public_key, dns_record, full_dns,
/// keytable_entry}`; `synced_keys`/`opendkim_note` are kept as optional
/// fields so richer or older backends still parse.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DkimGenResp {
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub selector: String,
    #[serde(default)]
    pub key_type: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub dns_record: String,
    #[serde(default)]
    pub full_dns: String,
    #[serde(default)]
    pub keytable_entry: String,
    #[serde(default)]
    pub synced_keys: Vec<Value>,
    #[serde(default)]
    pub opendkim_note: String,
}

/// One key returned by `GET /dkim/{domain_id}`: `{id, selector,
/// public_key, dns_value, revoked}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DkimKey {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub selector: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub dns_value: String,
    #[serde(default)]
    pub revoked: bool,
}

/// DMARC aggregate report row (`GET /dmarc/reports`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DmarcRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub report_id: String,
    #[serde(default)]
    pub report_type: String,
    #[serde(default)]
    pub created_at: String,
}

/// API token row (`GET /api-tokens`). The backend returns
/// `{id, name, prefix, scopes, expires_at, created_at}` (revoked tokens are
/// excluded server-side); `revoked` defaults to `false` for compatibility.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub created_at: String,
}

/// `POST /api-tokens` response — `token` is the plaintext secret, shown once.
/// Shape: `{id, token, prefix, scopes, expires_at}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreatedToken {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// Push subscription row (`GET /push/subscribe`): `{id, endpoint}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushSub {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub endpoint: String,
}

/// `POST /auth/totp/enroll` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TotpEnrollResp {
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub otpauth_url: String,
}

/// Message list header (webmail). Reused for drafts, where `seen` /
/// `message_id` may simply be absent (defaults apply).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageHeader {
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub seen: bool,
    #[serde(default)]
    pub message_id: Option<String>,
}

/// `GET /mail/folders` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoldersResp {
    #[serde(default)]
    pub folders: Vec<String>,
}

/// `GET /mail/messages` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagesResp {
    #[serde(default)]
    pub messages: Vec<MessageHeader>,
}

/// `GET /mail/messages/{uid}` response (full message incl. raw text).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageFull {
    #[serde(default)]
    pub uid: u32,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub seen: bool,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub raw: String,
}

/// `GET /mail/search` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResp {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

/// One `GET /mail/search` result row.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub folder: String,
}

/// `GET /mail/drafts` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DraftsResp {
    #[serde(default)]
    pub drafts: Vec<MessageHeader>,
}

/// `POST /mail/send` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendResp {
    #[serde(default)]
    pub status: String,
}

// --- Request bodies --------------------------------------------------------

/// `POST /domains` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateDomainReq {
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catch_all: Option<bool>,
}

/// `PUT /domains/{id}` body (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateDomainReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catch_all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dkim_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spf_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dmarc_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// `POST /users` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateUserReq {
    pub email: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_mb: Option<i64>,
}

/// `PUT /users/{id}` body (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateUserReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_mb: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// `POST /aliases` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateAliasReq {
    pub address: String,
    pub goto: String,
}

/// `PUT /aliases/{id}` body (all fields optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateAliasReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

/// `POST /dkim/{domain_id}/generate` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DkimGenReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

/// `POST /push/subscribe` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushSubReq {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// `POST /api-tokens` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateTokenReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<i64>,
}

/// `POST /mail/send` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendMailReq {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Request core
// ---------------------------------------------------------------------------

/// Send `method` to `url` (full path, e.g. from [`api_url`]) and return the
/// raw response text on 2xx, or a user-friendly error string otherwise.
/// Always sends `Accept: application/json` and, when a token is stored,
/// `Authorization: Bearer <token>`.
async fn request_text(method: &str, url: &str, body: Option<&Value>) -> Result<String, String> {
    let base = match method {
        "GET" => gloo_net::http::Request::get(url),
        "POST" => gloo_net::http::Request::post(url),
        "PUT" => gloo_net::http::Request::put(url),
        "DELETE" => gloo_net::http::Request::delete(url),
        other => return Err(format!("unsupported method: {other}")),
    };
    let mut builder = base.header("Accept", "application/json");
    if let Some(tok) = get_token() {
        builder = builder.header("Authorization", &format!("Bearer {tok}"));
    }
    let req = if let Some(v) = body {
        builder
            .header("Content-Type", "application/json")
            .body(v.to_string())
    } else {
        builder.build()
    }
    .map_err(|e| format!("Request build failed: {e}"))?;
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if (200..300).contains(&status) {
        Ok(text)
    } else {
        Err(http_error_message(status, &text))
    }
}

/// Like [`request_text`], but deserializes a 2xx JSON body into `T`.
async fn request_json<T: DeserializeOwned>(
    method: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<T, String> {
    let text = request_text(method, url, body).await?;
    let text = text.trim();
    if text.is_empty() {
        return Err("Empty response body".to_string());
    }
    serde_json::from_str(text).map_err(|e| format!("Invalid response JSON: {e}"))
}

/// Serialize a request-body struct to JSON [`Value`] (infallible for the
/// plain data structs used here).
fn to_value<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(serde_json::json!({}))
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// `POST /api/v1/auth/login` with `{email, password}` plus optional
/// `totp_code` when the account has 2FA enabled. On success stores the
/// returned session token via [`set_token`].
pub async fn login(
    email: &str,
    password: &str,
    totp_code: Option<&str>,
) -> Result<LoginResp, String> {
    let mut body = serde_json::json!({ "email": email, "password": password });
    if let Some(code) = totp_code
        && !code.trim().is_empty()
    {
        body["totp_code"] = serde_json::json!(code.trim());
    }
    let resp: LoginResp = request_json("POST", &api_url("/auth/login"), Some(&body)).await?;
    if !resp.token.is_empty() {
        set_token(&resp.token);
    }
    Ok(resp)
}

/// `POST /api/v1/auth/logout` — revokes all sessions server-side and clears
/// the local token.
pub async fn logout() -> Result<Value, String> {
    let resp: Value = request_json("POST", &api_url("/auth/logout"), None).await?;
    clear_token();
    Ok(resp)
}

/// `GET /api/v1/auth/me` — current identity.
pub async fn me() -> Result<MeResp, String> {
    request_json("GET", &api_url("/auth/me"), None).await
}

/// `POST /api/v1/auth/password/change` — change own password.
pub async fn change_password(current_password: &str, new_password: &str) -> Result<Value, String> {
    let body = serde_json::json!({
        "current_password": current_password,
        "new_password": new_password,
    });
    request_json("POST", &api_url("/auth/password/change"), Some(&body)).await
}

// ---------------------------------------------------------------------------
// Domains
// ---------------------------------------------------------------------------

/// `GET /api/v1/domains` — all domains (bare array).
pub async fn list_domains() -> Result<Vec<Domain>, String> {
    request_json("GET", &api_url("/domains"), None).await
}

/// `GET /api/v1/domains/{id}`.
pub async fn get_domain(id: &str) -> Result<Domain, String> {
    request_json("GET", &api_url(&format!("/domains/{id}")), None).await
}

/// `POST /api/v1/domains`.
pub async fn create_domain(req: &CreateDomainReq) -> Result<Domain, String> {
    request_json("POST", &api_url("/domains"), Some(&to_value(req))).await
}

/// `PUT /api/v1/domains/{id}`.
pub async fn update_domain(id: &str, req: &UpdateDomainReq) -> Result<Domain, String> {
    request_json(
        "PUT",
        &api_url(&format!("/domains/{id}")),
        Some(&to_value(req)),
    )
    .await
}

/// `DELETE /api/v1/domains/{id}` — returns `{"deleted": id}`.
pub async fn delete_domain(id: &str) -> Result<Value, String> {
    request_json("DELETE", &api_url(&format!("/domains/{id}")), None).await
}

/// `GET /api/v1/domains/{id}/dns` — dynamic DNS wizard record set.
pub async fn domain_dns(id: &str) -> Result<Value, String> {
    request_json("GET", &api_url(&format!("/domains/{id}/dns")), None).await
}

/// `GET /api/v1/domains/{id}/verify` — live DNS checks (dynamic shape).
pub async fn domain_verify(id: &str) -> Result<Value, String> {
    request_json("GET", &api_url(&format!("/domains/{id}/verify")), None).await
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

/// `GET /api/v1/users` — all users (bare array), optionally filtered with
/// `?domain=` when `domain` is given.
pub async fn list_users(domain: Option<&str>) -> Result<Vec<User>, String> {
    let mut url = api_url("/users");
    if let Some(d) = domain
        && !d.is_empty()
    {
        url.push_str(&format!("?domain={}", encode_query_component(d)));
    }
    request_json("GET", &url, None).await
}

/// `GET /api/v1/users/{id}`.
pub async fn get_user(id: &str) -> Result<User, String> {
    request_json("GET", &api_url(&format!("/users/{id}")), None).await
}

/// `POST /api/v1/users`.
pub async fn create_user(req: &CreateUserReq) -> Result<User, String> {
    request_json("POST", &api_url("/users"), Some(&to_value(req))).await
}

/// `PUT /api/v1/users/{id}`.
pub async fn update_user(id: &str, req: &UpdateUserReq) -> Result<User, String> {
    request_json(
        "PUT",
        &api_url(&format!("/users/{id}")),
        Some(&to_value(req)),
    )
    .await
}

/// `DELETE /api/v1/users/{id}` — returns `{"deleted": id}`.
pub async fn delete_user(id: &str) -> Result<Value, String> {
    request_json("DELETE", &api_url(&format!("/users/{id}")), None).await
}

/// `POST /api/v1/users/{id}/password` with `{password}` — admin password reset.
pub async fn reset_password(id: &str, password: &str) -> Result<Value, String> {
    let body = serde_json::json!({ "password": password });
    request_json(
        "POST",
        &api_url(&format!("/users/{id}/password")),
        Some(&body),
    )
    .await
}

// ---------------------------------------------------------------------------
// Aliases
// ---------------------------------------------------------------------------

/// `GET /api/v1/aliases` — all aliases (bare array).
pub async fn list_aliases() -> Result<Vec<Alias>, String> {
    request_json("GET", &api_url("/aliases"), None).await
}

/// `GET /api/v1/aliases/{id}`.
pub async fn get_alias(id: &str) -> Result<Alias, String> {
    request_json("GET", &api_url(&format!("/aliases/{id}")), None).await
}

/// `POST /api/v1/aliases`.
pub async fn create_alias(req: &CreateAliasReq) -> Result<Alias, String> {
    request_json("POST", &api_url("/aliases"), Some(&to_value(req))).await
}

/// `PUT /api/v1/aliases/{id}`.
pub async fn update_alias(id: &str, req: &UpdateAliasReq) -> Result<Alias, String> {
    request_json(
        "PUT",
        &api_url(&format!("/aliases/{id}")),
        Some(&to_value(req)),
    )
    .await
}

/// `DELETE /api/v1/aliases/{id}` — returns `{"deleted": id}`.
pub async fn delete_alias(id: &str) -> Result<Value, String> {
    request_json("DELETE", &api_url(&format!("/aliases/{id}")), None).await
}

// ---------------------------------------------------------------------------
// Quota / queues / DKIM
// ---------------------------------------------------------------------------

/// `GET /api/v1/quota` — per-user quota overview normalized to [`QuotaResp`]
/// (handles both the current bare-array and wrapped-object shapes).
pub async fn quota() -> Result<QuotaResp, String> {
    let text = request_text("GET", &api_url("/quota"), None).await?;
    parse_quota(&text)
}

/// Convenience wrapper returning just the per-user quota rows.
pub async fn quota_users() -> Result<Vec<QuotaUser>, String> {
    Ok(quota().await?.users)
}

/// `GET /api/v1/queues` — postfix queue status.
pub async fn queues() -> Result<QueueResp, String> {
    request_json("GET", &api_url("/queues"), None).await
}

/// `POST /api/v1/dkim/{domain_id}/generate` — generate (or regenerate) a
/// DKIM key pair. The request body is `{key_type?, selector?}`.
pub async fn dkim_generate(domain_id: &str, req: &DkimGenReq) -> Result<DkimGenResp, String> {
    request_json(
        "POST",
        &api_url(&format!("/dkim/{domain_id}/generate")),
        Some(&to_value(req)),
    )
    .await
}

/// `GET /api/v1/dkim/{domain_id}` — existing keys for a domain (bare array).
pub async fn dkim_list(domain_id: &str) -> Result<Vec<DkimKey>, String> {
    request_json("GET", &api_url(&format!("/dkim/{domain_id}")), None).await
}

// ---------------------------------------------------------------------------
// Filters / vacation (dynamic shapes → Value passthrough)
// ---------------------------------------------------------------------------

/// `GET /api/v1/filters/{user_id}` — sieve scripts (bare array of
/// `{id, name, content, active}`) returned as [`Value`].
pub async fn get_filters(user_id: &str) -> Result<Value, String> {
    request_json("GET", &api_url(&format!("/filters/{user_id}")), None).await
}

/// `PUT /api/v1/filters/{user_id}` — body `{"scripts": [{name, content,
/// active}]}`. Pass the payload as [`Value`] so callers can edit raw sieve.
pub async fn save_filters(user_id: &str, body: &Value) -> Result<Value, String> {
    request_json("PUT", &api_url(&format!("/filters/{user_id}")), Some(body)).await
}

/// `GET /api/v1/vacation/{user_id}` — vacation settings object as [`Value`].
pub async fn get_vacation(user_id: &str) -> Result<Value, String> {
    request_json("GET", &api_url(&format!("/vacation/{user_id}")), None).await
}

/// `PUT /api/v1/vacation/{user_id}` — body `{subject, message, enabled,
/// start_date?, end_date?}` as [`Value`].
pub async fn save_vacation(user_id: &str, body: &Value) -> Result<Value, String> {
    request_json("PUT", &api_url(&format!("/vacation/{user_id}")), Some(body)).await
}

// ---------------------------------------------------------------------------
// Admin: audit / bans / config
// ---------------------------------------------------------------------------

/// `GET /api/v1/admin/audit` — audit rows (bare array, newest first).
/// `limit` defaults server-side to 50 when omitted.
pub async fn list_audit(limit: Option<u32>) -> Result<Vec<Value>, String> {
    request_json("GET", &audit_url(limit), None).await
}

/// `GET /api/v1/admin/bans` — recent ban/failed-login rows (bare array).
pub async fn list_bans() -> Result<Vec<Value>, String> {
    request_json("GET", &api_url("/admin/bans"), None).await
}

/// `GET /api/v1/config/export` — full config as a [`Value`].
pub async fn export_config_value() -> Result<Value, String> {
    request_json("GET", &api_url("/config/export"), None).await
}

/// `GET /api/v1/config/export` rendered as pretty JSON text (for display
/// or download).
pub async fn export_config() -> Result<String, String> {
    let v = export_config_value().await?;
    serde_json::to_string_pretty(&v).map_err(|e| format!("Failed to encode config export: {e}"))
}

/// `POST /api/v1/config/import` — body is the full config JSON (users are
/// intentionally skipped server-side). Returns an import summary.
pub async fn import_config(body: &Value) -> Result<Value, String> {
    request_json("POST", &api_url("/config/import"), Some(body)).await
}

// ---------------------------------------------------------------------------
// DMARC
// ---------------------------------------------------------------------------

/// `GET /api/v1/dmarc/reports` — DMARC rows (bare array), optionally
/// filtered with `?domain=`.
pub async fn list_dmarc(domain: Option<&str>) -> Result<Vec<DmarcRow>, String> {
    let mut url = api_url("/dmarc/reports");
    if let Some(d) = domain
        && !d.is_empty()
    {
        url.push_str(&format!("?domain={}", encode_query_component(d)));
    }
    request_json("GET", &url, None).await
}

/// `POST /api/v1/dmarc/reports` — ingest a DMARC report by XML. Body:
/// `{domain, report_id, xml, report_type}` passed as [`Value`].
pub async fn create_dmarc_report(body: &Value) -> Result<Value, String> {
    request_json("POST", &api_url("/dmarc/reports"), Some(body)).await
}

// ---------------------------------------------------------------------------
// Push subscriptions
// ---------------------------------------------------------------------------

/// `GET /api/v1/push/subscribe` — current user's subscriptions (bare array).
pub async fn list_push() -> Result<Vec<PushSub>, String> {
    request_json("GET", &api_url("/push/subscribe"), None).await
}

/// `POST /api/v1/push/subscribe` — subscribe this client. Body:
/// `{endpoint, p256dh, auth}`.
pub async fn create_push(req: &PushSubReq) -> Result<Value, String> {
    request_json("POST", &api_url("/push/subscribe"), Some(&to_value(req))).await
}

/// `DELETE /api/v1/push/subscribe/{id}` — returns `{"deleted": id}`.
pub async fn delete_push(id: &str) -> Result<Value, String> {
    request_json("DELETE", &api_url(&format!("/push/subscribe/{id}")), None).await
}

// ---------------------------------------------------------------------------
// TOTP 2FA
// ---------------------------------------------------------------------------

/// `POST /api/v1/auth/totp/enroll` — (re-)enroll. Body: `{password, code?}`
/// where `code` is the optional current TOTP code for already-enabled
/// accounts. Returns the pending secret + otpauth URL.
pub async fn totp_enroll(password: &str, code: Option<&str>) -> Result<TotpEnrollResp, String> {
    let mut body = serde_json::json!({ "password": password });
    if let Some(c) = code
        && !c.trim().is_empty()
    {
        body["code"] = serde_json::json!(c.trim());
    }
    request_json("POST", &api_url("/auth/totp/enroll"), Some(&body)).await
}

/// `POST /api/v1/auth/totp/verify` — confirm enrollment with a TOTP code.
/// Body: `{"totp_code": code}`. Returns `{"enabled": true, ...}`.
pub async fn totp_verify(totp_code: &str) -> Result<Value, String> {
    let body = serde_json::json!({ "totp_code": totp_code });
    request_json("POST", &api_url("/auth/totp/verify"), Some(&body)).await
}

/// `POST /api/v1/auth/totp/disable` — disable 2FA (requires password
/// re-auth). Body: `{"password": password}`.
pub async fn totp_disable(password: &str) -> Result<Value, String> {
    let body = serde_json::json!({ "password": password });
    request_json("POST", &api_url("/auth/totp/disable"), Some(&body)).await
}

// ---------------------------------------------------------------------------
// API tokens
// ---------------------------------------------------------------------------

/// `GET /api/v1/api-tokens` — current user's tokens (bare array).
pub async fn list_api_tokens() -> Result<Vec<TokenRow>, String> {
    request_json("GET", &api_url("/api-tokens"), None).await
}

/// `POST /api/v1/api-tokens` — issue a token. The returned `token` is the
/// plaintext secret and is only shown once.
pub async fn create_api_token(req: &CreateTokenReq) -> Result<CreatedToken, String> {
    request_json("POST", &api_url("/api-tokens"), Some(&to_value(req))).await
}

/// `DELETE /api/v1/api-tokens/{id}` — revoke a token.
pub async fn revoke_api_token(id: &str) -> Result<Value, String> {
    request_json("DELETE", &api_url(&format!("/api-tokens/{id}")), None).await
}

// ---------------------------------------------------------------------------
// Webmail
// ---------------------------------------------------------------------------

/// `GET /api/v1/mail/folders` — mailbox folders.
pub async fn mail_folders() -> Result<FoldersResp, String> {
    request_json("GET", &api_url("/mail/folders"), None).await
}

/// `GET /api/v1/mail/messages?folder=&limit=` — message headers for a folder
/// (defaults to `INBOX`, 50 rows server-side).
pub async fn mail_list_messages(
    folder: Option<&str>,
    limit: Option<u32>,
) -> Result<MessagesResp, String> {
    request_json("GET", &messages_url(folder, limit), None).await
}

/// `GET /api/v1/mail/messages/{uid}` — one full message.
pub async fn mail_get_message(uid: u32) -> Result<MessageFull, String> {
    request_json("GET", &api_url(&format!("/mail/messages/{uid}")), None).await
}

/// `GET /api/v1/mail/search?q=` — full-text search.
pub async fn mail_search(q: &str) -> Result<SearchResp, String> {
    request_json("GET", &mail_search_url(q), None).await
}

/// `POST /api/v1/mail/send` with `{from, to, subject, body}`.
pub async fn mail_send(req: &SendMailReq) -> Result<SendResp, String> {
    request_json("POST", &api_url("/mail/send"), Some(&to_value(req))).await
}

/// `GET /api/v1/mail/drafts` — drafts (headers in the `drafts` array).
pub async fn mail_list_drafts() -> Result<DraftsResp, String> {
    request_json("GET", &api_url("/mail/drafts"), None).await
}

/// `POST /api/v1/mail/drafts` with `{uid}` — delete a draft (moves it to
/// Trash). Returns `{"deleted": ...}`.
pub async fn mail_delete_draft(uid: u32) -> Result<Value, String> {
    let body = serde_json::json!({ "uid": uid });
    request_json("POST", &api_url("/mail/drafts"), Some(&body)).await
}

// ---------------------------------------------------------------------------
// JMAP
// ---------------------------------------------------------------------------

/// `GET /jmap/session` — JMAP session discovery document (dynamic shape,
/// returned as [`Value`]). Note: this path is intentionally *not* under
/// `/api/v1`.
pub async fn jmap_session() -> Result<Value, String> {
    request_json("GET", "/jmap/session", None).await
}

/// `POST /jmap` — dispatch JMAP method calls. `method_calls` may be the raw
/// array of method-call objects (recommended) or a pre-built envelope object
/// containing `method_calls`; it is wrapped with the standard capabilities
/// when needed. Returns the full JMAP response as [`Value`].
pub async fn jmap_request(method_calls: Value) -> Result<Value, String> {
    let body = jmap_request_body(&method_calls);
    request_json("POST", "/jmap", Some(&body)).await
}

// ---------------------------------------------------------------------------
// Tests (pure helpers only — host-safe)
// ---------------------------------------------------------------------------

// ---------- Compatibility shims for `pages` (rename + arg-shape adapters) ----------

pub async fn tokens_list() -> Result<Vec<TokenRow>, String> {
    list_api_tokens().await
}

pub async fn tokens_create(
    name: &str,
    scopes: Vec<String>,
    expires_days: i64,
) -> Result<CreatedToken, String> {
    create_api_token(&CreateTokenReq {
        name: Some(name.to_string()),
        scopes: Some(scopes),
        expires_in_days: if expires_days > 0 {
            Some(expires_days)
        } else {
            None
        },
    })
    .await
}

pub async fn tokens_revoke(id: &str) -> Result<Value, String> {
    revoke_api_token(id).await
}

pub async fn push_list() -> Result<Vec<PushSub>, String> {
    list_push().await
}

pub async fn push_create(endpoint: &str, p256dh: &str, auth: &str) -> Result<Value, String> {
    create_push(&PushSubReq {
        endpoint: endpoint.to_string(),
        p256dh: p256dh.to_string(),
        auth: auth.to_string(),
    })
    .await
}

pub async fn push_delete(id: &str) -> Result<Value, String> {
    delete_push(id).await
}

pub async fn audit(limit: Option<u32>) -> Result<Vec<Value>, String> {
    list_audit(limit).await
}

pub async fn bans() -> Result<Vec<Value>, String> {
    list_bans().await
}

pub async fn filters_get(user_id: &str) -> Result<Value, String> {
    get_filters(user_id).await
}

pub async fn filters_put(user_id: &str, sieve: &str) -> Result<Value, String> {
    save_filters(user_id, &serde_json::json!({ "sieve": sieve })).await
}

pub async fn vacation_get(user_id: &str) -> Result<Value, String> {
    get_vacation(user_id).await
}

pub async fn vacation_put(user_id: &str, body: serde_json::Value) -> Result<Value, String> {
    save_vacation(user_id, &body).await
}

pub async fn config_export() -> Result<String, String> {
    export_config().await
}

pub async fn config_import(body: serde_json::Value) -> Result<Value, String> {
    import_config(&body).await
}

pub async fn dmarc_list() -> Result<Vec<DmarcRow>, String> {
    list_dmarc(None).await
}

pub async fn mail_messages(folder: &str, limit: u32) -> Result<MessagesResp, String> {
    mail_list_messages(Some(folder), Some(limit)).await
}

pub async fn mail_message(uid: u32) -> Result<MessageFull, String> {
    mail_get_message(uid).await
}

pub async fn mail_drafts() -> Result<DraftsResp, String> {
    mail_list_drafts().await
}

pub async fn mail_draft_delete(uid: u32) -> Result<Value, String> {
    mail_delete_draft(uid).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_url_joins_base_and_path() {
        assert_eq!(api_url("/domains"), "/api/v1/domains");
        assert_eq!(api_url("domains"), "/api/v1/domains");
        assert_eq!(api_url("/mail/folders"), "/api/v1/mail/folders");
    }

    #[test]
    fn encode_leaves_unreserved_and_escapes_rest() {
        assert_eq!(encode_query_component("abc-XY_09.~"), "abc-XY_09.~");
        assert_eq!(encode_query_component("a b"), "a%20b");
        assert_eq!(encode_query_component("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_query_component("caf\u{e9}"), "caf%C3%A9");
    }

    #[test]
    fn messages_url_builds_query() {
        assert_eq!(messages_url(None, None), "/api/v1/mail/messages");
        assert_eq!(
            messages_url(Some("INBOX"), None),
            "/api/v1/mail/messages?folder=INBOX"
        );
        assert_eq!(
            messages_url(None, Some(20)),
            "/api/v1/mail/messages?limit=20"
        );
        assert_eq!(
            messages_url(Some("Sent Items"), Some(5)),
            "/api/v1/mail/messages?folder=Sent%20Items&limit=5"
        );
    }

    #[test]
    fn search_url_escapes_query() {
        assert_eq!(
            mail_search_url("hello & bye"),
            "/api/v1/mail/search?q=hello%20%26%20bye"
        );
    }

    #[test]
    fn audit_url_optional_limit() {
        assert_eq!(audit_url(None), "/api/v1/admin/audit");
        assert_eq!(audit_url(Some(10)), "/api/v1/admin/audit?limit=10");
    }

    #[test]
    fn error_message_prefers_detail() {
        let body = r#"{"type":"about:blank","title":"Unprocessable Entity","status":422,"detail":"validation error: bad domain"}"#;
        assert_eq!(
            http_error_message(422, body),
            "validation error: bad domain (HTTP 422)"
        );
    }

    #[test]
    fn error_message_falls_back_to_status_and_body() {
        assert_eq!(http_error_message(500, ""), "HTTP 500");
        assert_eq!(http_error_message(404, "not here"), "HTTP 404: not here");
    }

    #[test]
    fn error_message_truncates_long_plain_body() {
        let long = "x".repeat(400);
        let msg = http_error_message(500, &long);
        assert!(msg.starts_with("HTTP 500: "));
        assert!(msg.ends_with('…'));
        // 300 kept chars + prefix + ellipsis
        assert_eq!(msg.len(), "HTTP 500: ".len() + 300 + "…".len());
    }

    #[test]
    fn quota_parses_bare_row_array_and_sums_totals() {
        let body = r#"[{"email":"a@example.com","quota_mb":1024,"used_mb":100,"pct":9.7},{"email":"b@example.com","quota_mb":2048,"used_mb":512,"pct":25.0}]"#;
        let q = parse_quota(body).expect("parse");
        assert_eq!(q.users.len(), 2);
        assert_eq!(q.quota_mb, 3072);
        assert_eq!(q.used_mb, 612);
        assert_eq!(q.users[0].email, "a@example.com");
    }

    #[test]
    fn quota_parses_wrapped_object_shape() {
        let body = r#"{"quota_mb":3072,"used_mb":612,"users":[{"email":"a@example.com","quota_mb":1024,"used_mb":100,"pct":9.7}]}"#;
        let q = parse_quota(body).expect("parse");
        assert_eq!(q.quota_mb, 3072);
        assert_eq!(q.used_mb, 612);
        assert_eq!(q.users.len(), 1);
    }

    #[test]
    fn quota_rejects_garbage() {
        assert!(parse_quota("not json").is_err());
        assert!(parse_quota("42").is_err());
    }

    #[test]
    fn jmap_body_wraps_array_with_capabilities() {
        let calls = serde_json::json!([["Mailbox/query", {"accountId": "a"}, "t1"]]);
        let body = jmap_request_body(&calls);
        assert_eq!(
            body["using"][0],
            serde_json::json!("urn:ietf:params:jmap:core")
        );
        assert_eq!(body["method_calls"], calls);
    }

    #[test]
    fn jmap_body_passes_full_envelope_through() {
        let env = serde_json::json!({"using": ["x"], "method_calls": []});
        assert_eq!(jmap_request_body(&env), env);
    }

    #[test]
    fn login_resp_deserializes_backend_shape() {
        let r: LoginResp =
            serde_json::from_str(r#"{"token":"t123","email":"a@example.com","kind":"admin"}"#)
                .expect("parse");
        assert_eq!(r.token, "t123");
        assert_eq!(r.email, "a@example.com");
        assert_eq!(r.kind, "admin");
    }

    #[test]
    fn domain_user_row_defaults_tolerate_missing_fields() {
        let d: Domain =
            serde_json::from_value(serde_json::json!({"id": "1", "domain": "example.com"}))
                .expect("parse");
        assert_eq!(d.domain, "example.com");
        assert!(!d.active);
        assert!(!d.catch_all);
        assert_eq!(d.dkim_selector, None);
        let u: User = serde_json::from_value(serde_json::json!({"id": "2", "email": "a@b.c"}))
            .expect("parse");
        assert_eq!(u.email, "a@b.c");
        assert_eq!(u.quota_mb, 0);
    }

    #[test]
    fn token_row_parses_both_backend_and_spec_shapes() {
        // backend: {id, name, prefix, scopes, expires_at, created_at}
        let t: TokenRow = serde_json::from_value(serde_json::json!({
            "id": "1", "name": null, "prefix": "synth_abcdef", "scopes": ["admin"],
            "expires_at": null, "created_at": "2026-01-01T00:00:00Z"
        }))
        .expect("parse");
        assert_eq!(t.prefix.as_deref(), Some("synth_abcdef"));
        assert!(!t.revoked);
        // spec-ish: includes revoked
        let t2: TokenRow = serde_json::from_value(
            serde_json::json!({"id": "2", "name": "ci", "scopes": [], "revoked": true}),
        )
        .expect("parse");
        assert!(t2.revoked);
    }

    #[test]
    fn message_header_parses_mail_shape() {
        let m: MessageHeader = serde_json::from_value(serde_json::json!({
            "uid": 7, "subject": "hi", "from": "a@b", "date": "2026-01-01",
            "preview": "pre", "seen": true, "message_id": "<m1>"
        }))
        .expect("parse");
        assert_eq!(m.uid, 7);
        assert!(m.seen);
        assert_eq!(m.message_id.as_deref(), Some("<m1>"));
    }
}
