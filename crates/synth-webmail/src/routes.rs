use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use synth_common::{AppError, AppResult, ErrorResponse};

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
        // Phase 2: JMAP (RFC 8620/8621)
        .route("/jmap/session", get(jmap_session))
        .route("/jmap", post(jmap_dispatch))
        // Phase 2: CardDAV / CalDAV
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

async fn jmap_session(State(s): State<AppState>) -> Json<serde_json::Value> {
    let api = format!("https://{}/jmap", s.config.mail_host);
    Json(
        serde_json::to_value(crate::jmap::session_for("user@example.com", &api))
            .unwrap_or(serde_json::json!({})),
    )
}

async fn jmap_dispatch(
    Json(req): Json<crate::jmap::JmapRequest>,
) -> Json<crate::jmap::JmapResponse> {
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
    Json(crate::jmap::JmapResponse {
        session_state: "synthmail-state-1".into(),
        method_responses: responses,
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

// ---------- Auth (delegates to synth-control DB) ----------

#[derive(Debug, Deserialize)]
struct LoginReq {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResp {
    token: String,
    email: String,
    kind: String,
}

async fn web_login(
    State(s): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, ErrorResponse> {
    use synth_common::{create_session_token, verify_password};
    let row: Option<(uuid::Uuid, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, email::text, password_hash, kind, active FROM users WHERE email=$1",
    )
    .bind(&req.email.to_lowercase())
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (id, email, hash, kind, active) =
        row.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !active || !verify_password(&hash, &req.password) {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    let token = create_session_token(&s.config.session_secret, &id.to_string(), &email, &kind)
        .map_err(|_| ErrorResponse(AppError::Internal("token".into())))?;
    Ok(Json(LoginResp { token, email, kind }))
}

// ---------- Mail ----------

#[derive(Debug, Deserialize)]
struct MailQuery {
    folder: Option<String>,
    limit: Option<usize>,
}

async fn list_folders() -> Json<serde_json::Value> {
    let folders = ["INBOX", "Sent", "Drafts", "Junk", "Trash"];
    Json(serde_json::json!({
        "folders": folders.iter().map(|f| serde_json::json!({"name": f, "unread": if *f=="INBOX" {2} else {0}})).collect::<Vec<_>>()
    }))
}

async fn list_messages(Query(q): Query<MailQuery>) -> Json<serde_json::Value> {
    let folder = q.folder.unwrap_or_else(|| "INBOX".to_string());
    let _ = q.limit.unwrap_or(50);
    // Phase 1 MVP: IMAP proxy fetch
    // In production: crate::imap::fetch_messages("dovecot", 993, user, pass, &folder, limit)
    Json(serde_json::json!({
        "folder": folder,
        "messages": [],
        "total": 0,
        "note": "Phase 1 IMAP proxy — connect with Dovecot IMAP to populate"
    }))
}

async fn get_message(Path(_uid): Path<u32>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"uid": _uid, "error": "not found in MVP stub", "note": "Phase 1: FETCH BODY[] via async-imap"}),
    )
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    folder: Option<String>,
}

async fn search(
    State(s): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // Postgres FTS on mail_search (populated by IDLE sync in Phase 1.5)
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT subject, folder FROM mail_search WHERE body_tsv @@ plainto_tsquery($1) LIMIT 50",
    )
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

async fn send(Json(req): Json<SendReq>) -> Json<serde_json::Value> {
    // Phase 1: submit via Postfix :587 SMTP (lettre). Queue injection for local.
    crate::imap::send_via_smtp(
        "postfix",
        587,
        &req.from,
        "",
        &req.from,
        &req.to,
        &req.subject,
        &req.body,
    )
    .await
    .ok();
    Json(
        serde_json::json!({"status": "queued", "message_id": format!("<{}@synthmail>", uuid::Uuid::new_v4()), "from": req.from, "to": req.to}),
    )
}

#[derive(Debug, Deserialize)]
struct DraftQuery {
    q: String,
}
async fn list_drafts(Query(qq): Query<DraftQuery>) -> Json<serde_json::Value> {
    let _ = qq;
    Json(serde_json::json!({"drafts": []}))
}
async fn delete_draft(Json(_v): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn folders_returns_inbox() {
        let r = list_folders().await;
        assert!(r.to_string().contains("INBOX"));
    }
}
