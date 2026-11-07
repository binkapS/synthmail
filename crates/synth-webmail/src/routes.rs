use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use synth_common::{AppError, ErrorResponse};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/v1/mail/folders", get(list_folders))
        .route("/api/v1/mail/messages", get(list_messages))
        .route("/api/v1/mail/messages/{uid}", get(get_message))
        .route("/api/v1/mail/search", get(search))
        .route("/api/v1/mail/send", post(send))
        .route("/api/v1/mail/drafts", get(list_drafts).post(delete_draft))
        .route("/api/v1/auth/login", post(web_login))
        .route("/jmap/session", get(jmap_session))
        .route("/jmap", post(jmap_dispatch))
        .route(
            "/api/v1/carddav/contacts",
            get(crate::carddav::list_contacts).post(crate::carddav::create_contact),
        )
        .route(
            "/api/v1/carddav/contacts/{id}",
            axum::routing::delete(crate::carddav::delete_contact),
        )
        .route(
            "/api/v1/caldav/events",
            get(crate::carddav::list_events).post(crate::carddav::create_event),
        )
        .route("/api/v1/caldav/query", get(crate::carddav::query_events))
        .route("/dav", get(crate::carddav::dav_discovery))
        .route("/.well-known/carddav", get(crate::carddav::dav_discovery))
        .route("/.well-known/caldav", get(crate::carddav::dav_discovery))
        .with_state(state)
}

/// Authenticated webmail user (C-04).
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: uuid::Uuid,
    pub email: String,
    pub kind: String,
}

/// Validate the session JWT (shared secret with synth-control) and resolve the
/// user, verifying `active` and the DB token_version (H-04). Every webmail
/// route except /healthz, /readyz, /metrics and /api/v1/auth/login must call
/// this and scope its queries by `user_id`. Users flagged
/// `force_password_change` are rejected here (audit R-04) so the webmail
/// API is unusable until the password has been rotated.
pub async fn authenticate_user(
    s: &AppState,
    headers: &HeaderMap,
) -> Result<AuthUser, ErrorResponse> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth.strip_prefix("Bearer ").unwrap_or(auth);
    if token.is_empty() {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    let claims = synth_common::auth::decode_session_token(&s.config.session_secret, token)
        .map_err(|_| ErrorResponse(AppError::Unauthorized))?;
    let user_id =
        uuid::Uuid::parse_str(&claims.sub).map_err(|_| ErrorResponse(AppError::Unauthorized))?;
    let row: Option<(String, bool, i64, bool)> = sqlx::query_as(
        "SELECT kind, active, token_version, force_password_change FROM users WHERE id=$1",
    )
    .bind(user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (kind, active, db_tv, force_password_change) =
        row.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !active || claims.tv as i64 != db_tv {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    if force_password_change {
        return Err(ErrorResponse(AppError::Validation(
            "password change required before using the webmail API".into(),
        )));
    }
    Ok(AuthUser {
        user_id,
        email: claims.email,
        kind,
    })
}

async fn jmap_session(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let api = format!("https://{}/jmap", s.config.mail_host);
    Ok(Json(
        serde_json::to_value(crate::jmap::session_for(&user.email, &api))
            .unwrap_or(serde_json::json!({})),
    ))
}

async fn jmap_dispatch(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<crate::jmap::JmapRequest>,
) -> Result<Json<crate::jmap::JmapResponse>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let root = maildir_root();
    let mut responses = Vec::new();
    for c in &req.method_calls {
        let mut v = match c.name.as_str() {
            // Email/query + Email/get read the user's own Maildir (JMAP).
            "Email/query" | "Email/get" => email_jmap(c, &root, &user.email).await,
            _ => crate::jmap::handle_method(c),
        };
        if let Some(o) = v.as_object_mut() {
            o.insert("tag".to_string(), serde_json::json!(c.tag));
        }
        responses.push(v);
    }
    Ok(Json(crate::jmap::JmapResponse {
        session_state: "synthmail-state-1".into(),
        method_responses: responses,
    }))
}

async fn email_jmap(call: &crate::jmap::MethodCall, root: &str, email: &str) -> serde_json::Value {
    let account = call
        .args
        .get("accountId")
        .cloned()
        .unwrap_or(serde_json::json!("urn:ietf:params:jmap:account"));
    let mailbox_id = call
        .args
        .get("filter")
        .and_then(|f| f.get("inMailbox"))
        .and_then(|m| m.as_str())
        .unwrap_or("inbox");
    let folder = match mailbox_id {
        "sent" => "Sent",
        "drafts" => "Drafts",
        "junk" => "Junk",
        "trash" => "Trash",
        _ => "INBOX",
    };
    if call.name == "Email/query" {
        let msgs = crate::imap::fetch_messages(root, email, folder, 500)
            .await
            .unwrap_or_default();
        let ids: Vec<String> = msgs.iter().map(|m| m.uid.to_string()).collect();
        return serde_json::json!({"accountId": account, "ids": ids, "total": msgs.len()});
    }
    // Email/get: ids may be given; default to all in the queried mailbox.
    let msgs = crate::imap::fetch_messages(root, email, folder, 500)
        .await
        .unwrap_or_default();
    let want: Vec<String> = match call.args.get("ids").and_then(|i| i.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        None => msgs.iter().map(|m| m.uid.to_string()).collect(),
    };
    let mut list = Vec::new();
    let mut not_found = Vec::new();
    for id in &want {
        match id.parse::<u32>().ok() {
            Some(uid) => match crate::imap::fetch_raw(root, email, folder, uid).await {
                Ok(raw) => {
                    let meta = crate::imap::fetch_message(root, email, folder, uid)
                        .await
                        .ok();
                    list.push(serde_json::json!({
                        "id": id,
                        "subject": meta.as_ref().map(|m| m.subject.clone()).unwrap_or_default(),
                        "from": meta.as_ref().map(|m| m.from.clone()).unwrap_or_default(),
                        "receivedAt": meta.as_ref().map(|m| m.date.clone()).unwrap_or_default(),
                        "body": raw,
                    }));
                }
                Err(_) => not_found.push(serde_json::json!(id)),
            },
            None => not_found.push(serde_json::json!(id)),
        }
    }
    serde_json::json!({
        "accountId": account,
        "list": list,
        "notFound": not_found,
        "notModified": false,
    })
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

async fn readyz(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ErrorResponse> {
    sqlx::query("SELECT 1")
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(serde_json::json!({"status":"ok"})))
}

async fn metrics() -> String {
    synth_common::metrics::render_prometheus("synth_webmail")
}

// ---------- Auth ----------

#[derive(Debug, Deserialize)]
struct LoginReq {
    email: String,
    password: String,
    #[serde(default)]
    totp_code: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoginResp {
    token: String,
    email: String,
    kind: String,
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// C-03: webmail login enforces the SAME TOTP (and rate limiting) as the
/// control plane. A token is only issued after every required factor passes.
async fn web_login(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, ErrorResponse> {
    use synth_common::{create_session_token, verify_password};
    let ip = client_ip(&headers);
    if !s
        .limiter
        .check(&format!(
            "login:{}:{}",
            ip.as_deref().unwrap_or("unknown"),
            req.email.to_lowercase()
        ))
        .await
    {
        return Err(ErrorResponse(AppError::TooManyRequests));
    }
    let row: Option<(uuid::Uuid, String, String, String, bool, i64)> = sqlx::query_as(
        "SELECT id, email::text, password_hash, kind, active, token_version FROM users WHERE email=$1",
    )
    .bind(req.email.to_lowercase())
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (id, email, hash, kind, active, token_version) =
        row.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !active || !verify_password(&hash, &req.password) {
        let _ = sqlx::query("INSERT INTO login_attempts (email, ip, success) VALUES ($1,$2,false)")
            .bind(&email)
            .bind(ip.as_deref())
            .execute(&s.pool)
            .await;
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    // Enforce TOTP exactly like synth-control (with replay protection).
    // Audit M-15: secret is encrypted at rest — decrypt before verifying.
    let totp_row: Option<(bool, String, Option<i64>)> =
        sqlx::query_as("SELECT enabled, secret, last_used_step FROM user_totp WHERE user_id=$1")
            .bind(id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    if let Some((true, secret, last_step)) = totp_row {
        let code = req
            .totp_code
            .as_deref()
            .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
        let plain = synth_common::secrets::decrypt(&s.config.encryption_key, &secret)
            .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
        let step = synth_common::totp::verify_with_step(
            &plain,
            code,
            chrono::Utc::now().timestamp() as u64,
        )
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))? as i64;
        if let Some(last) = last_step
            && step <= last
        {
            return Err(ErrorResponse(AppError::Unauthorized));
        }
        sqlx::query("UPDATE user_totp SET last_used_step=$2 WHERE user_id=$1")
            .bind(id)
            .bind(step)
            .execute(&s.pool)
            .await
            .ok();
    }
    let _ = sqlx::query("INSERT INTO login_attempts (email, ip, success) VALUES ($1,$2,true)")
        .bind(&email)
        .bind(ip.as_deref())
        .execute(&s.pool)
        .await;
    synth_common::metrics::inc_auth_attempt();
    let token = create_session_token(
        &s.config.session_secret,
        &id.to_string(),
        &email,
        &kind,
        token_version as u64,
    )
    .map_err(|_| ErrorResponse(AppError::Internal("token".into())))?;
    Ok(Json(LoginResp { token, email, kind }))
}

// ---------- Mail ----------

#[derive(Debug, Deserialize)]
struct MailQuery {
    folder: Option<String>,
    limit: Option<usize>,
}

/// H-10: mail access via the shared Maildir (user-scoped, no fake success).
fn maildir_root() -> String {
    std::env::var("MAILDIR_DIR").unwrap_or_else(|_| "/var/vmail".to_string())
}

async fn list_folders(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let folders = crate::imap::list_folders(&maildir_root(), &user.email)
        .await
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    Ok(Json(serde_json::json!({"folders": folders})))
}

async fn list_messages(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<MailQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let msgs = crate::imap::fetch_messages(
        &maildir_root(),
        &user.email,
        q.folder.as_deref().unwrap_or("INBOX"),
        q.limit.unwrap_or(50).min(200),
    )
    .await
    .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    Ok(Json(
        serde_json::json!({"messages": msgs.into_iter().map(|m| serde_json::json!({
        "uid": m.uid, "subject": m.subject, "from": m.from, "date": m.date,
        "preview": m.preview, "seen": m.seen, "message_id": m.message_id
    })).collect::<Vec<_>>()}),
    ))
}

async fn get_message(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(uid): Path<u32>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let m = crate::imap::fetch_message(&maildir_root(), &user.email, "INBOX", uid)
        .await
        .map_err(|e| ErrorResponse(AppError::NotFound(e.to_string())))?;
    let raw = crate::imap::fetch_raw(&maildir_root(), &user.email, "INBOX", uid)
        .await
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "uid": m.uid, "subject": m.subject, "from": m.from, "date": m.date,
        "seen": m.seen, "message_id": m.message_id, "raw": raw
    })))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    folder: Option<String>,
}

async fn search(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // C-04/M-11: always scoped to the authenticated user.
    let user = authenticate_user(&s, &headers).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT subject, folder FROM mail_search WHERE user_id=$1 AND body_tsv @@ plainto_tsquery($2) LIMIT 50",
    )
    .bind(user.user_id)
    .bind(&q.q)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(serde_json::json!({
        "query": q.q,
        "results": rows.into_iter().map(|(subject, folder)| serde_json::json!({"subject": subject, "folder": folder})).collect::<Vec<_>>()
    })))
}

#[derive(Debug, Deserialize)]
struct SendReq {
    from: String,
    to: String,
    subject: String,
    body: String,
    compose_from_webmail: Option<bool>,
}

async fn send(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SendReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    // C-04: users may only send as themselves.
    if !req.from.eq_ignore_ascii_case(&user.email) {
        return Err(ErrorResponse(AppError::Forbidden));
    }
    // Outbound submission via Postfix on the mailnet (network-trusted;
    // external SASL submission is the hardened path — see security runbook).
    let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "postfix".to_string());
    let from: lettre::message::Mailbox = req
        .from
        .parse()
        .map_err(|_| ErrorResponse(AppError::Validation("invalid from".into())))?;
    let to: lettre::message::Mailbox = req
        .to
        .parse()
        .map_err(|_| ErrorResponse(AppError::Validation("invalid to".into())))?;
    let email = lettre::Message::builder()
        .from(from)
        .to(to)
        .subject(req.subject.clone())
        .body(req.body.clone())
        .map_err(|e| ErrorResponse(AppError::Validation(e.to_string())))?;
    let mailer = lettre::SmtpTransport::builder_dangerous(&smtp_host).build();
    // Sync lettre transport (no async-std dep); run off the runtime thread.
    let envelope = email.envelope().clone();
    let raw = email.formatted();
    tokio::task::spawn_blocking(move || {
        use lettre::Transport;
        mailer.send_raw(&envelope, &raw)
    })
    .await
    .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?
    .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    synth_common::metrics::inc_mail_sent();
    Ok(Json(serde_json::json!({"status": "sent"})))
}

#[derive(Debug, Deserialize)]
struct DraftQuery {
    #[serde(default)]
    q: Option<String>,
}

async fn list_drafts(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(_qq): Query<DraftQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let msgs = crate::imap::fetch_messages(&maildir_root(), &user.email, "Drafts", 50)
        .await
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    Ok(Json(
        serde_json::json!({"drafts": msgs.into_iter().map(|m| serde_json::json!({
        "uid": m.uid, "subject": m.subject, "from": m.from, "date": m.date, "preview": m.preview
    })).collect::<Vec<_>>()}),
    ))
}
async fn delete_draft(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(v): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let user = authenticate_user(&s, &headers).await?;
    let uid = v["uid"].as_u64().unwrap_or(0) as u32;
    let moved = crate::imap::move_to_trash(&maildir_root(), &user.email, "Drafts", uid)
        .await
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    Ok(Json(serde_json::json!({"deleted": moved})))
}
