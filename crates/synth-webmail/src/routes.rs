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
/// this and scope its queries by `user_id`.
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
    let row: Option<(String, bool, i64)> =
        sqlx::query_as("SELECT kind, active, token_version FROM users WHERE id=$1")
            .bind(user_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (kind, active, db_tv) = row.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !active || claims.tv as i64 != db_tv {
        return Err(ErrorResponse(AppError::Unauthorized));
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
    authenticate_user(&s, &headers).await?;
    let responses = req
        .method_calls
        .iter()
        .map(|c| {
            let mut v = crate::jmap::handle_method(c);
            if let Some(o) = v.as_object_mut() {
                o.insert("tag".to_string(), serde_json::json!(c.tag));
            }
            v
        })
        .collect();
    Ok(Json(crate::jmap::JmapResponse {
        session_state: "synthmail-state-1".into(),
        method_responses: responses,
    }))
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
    if !s.limiter.check(&format!(
        "login:{}:{}",
        ip.as_deref().unwrap_or("unknown"),
        req.email.to_lowercase()
    )) {
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
        let step = synth_common::totp::verify_with_step(
            &secret,
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

/// H-10: IMAP is still a stub — be honest instead of faking success.
fn not_implemented() -> ErrorResponse {
    ErrorResponse(AppError::NotImplemented(
        "webmail IMAP integration is not yet implemented".into(),
    ))
}

async fn list_folders(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    authenticate_user(&s, &headers).await?;
    Err(not_implemented())
}

async fn list_messages(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<MailQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    authenticate_user(&s, &headers).await?;
    let _ = q;
    Err(not_implemented())
}

async fn get_message(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(_uid): Path<u32>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    authenticate_user(&s, &headers).await?;
    Err(not_implemented())
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
    // H-10: honest 501, not a fake "queued".
    Err(not_implemented())
}

#[derive(Debug, Deserialize)]
struct DraftQuery {
    q: String,
}
async fn list_drafts(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(_qq): Query<DraftQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    authenticate_user(&s, &headers).await?;
    Err(not_implemented())
}
async fn delete_draft(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(_v): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    authenticate_user(&s, &headers).await?;
    Err(not_implemented())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn not_implemented_is_501() {
        let r = not_implemented();
        assert_eq!(r.0.status().as_u16(), 501);
    }
}
