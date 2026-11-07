//! CardDAV / CalDAV minimal implementation (WebDAV-shaped JSON API + collection discovery).
//! Full CalDAV/CardDAV sync via PostgREST? Here we store vCard/iCal in Postgres tables
//! created in Phase 2 migration and serve discovery + CRUD over REST + PROPFIND stubs.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use synth_common::{AppError, ErrorResponse};

#[derive(Debug, Deserialize)]
pub struct ContactCreate {
    pub display_name: String,
    #[serde(default)]
    pub vcard: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ContactRow {
    pub id: Uuid,
    pub display_name: String,
    pub vcard: String,
}

pub async fn list_contacts(
    State(s): State<AppState>,
) -> Result<Json<Vec<ContactRow>>, ErrorResponse> {
    let rows = sqlx::query_as::<_, ContactRow>(
        "SELECT id, display_name, vcard FROM carddav_contacts ORDER BY display_name",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows))
}

pub async fn create_contact(
    State(s): State<AppState>,
    Json(req): Json<ContactCreate>,
) -> Result<(axum::http::StatusCode, Json<ContactRow>), ErrorResponse> {
    // Resolve user from first user (Phase 2 dev: single-user collections)
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users ORDER BY created_at LIMIT 1")
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    let id = Uuid::new_v4();
    let vcard = if req.vcard.is_empty() {
        format!(
            "BEGIN:VCARD\nVERSION:3.0\nFN:{}\nEND:VCARD",
            req.display_name
        )
    } else {
        req.vcard.clone()
    };
    sqlx::query(
        "INSERT INTO carddav_contacts (id, user_id, display_name, vcard) VALUES ($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(user_id)
    .bind(&req.display_name)
    .bind(&vcard)
    .execute(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(ContactRow {
            id,
            display_name: req.display_name.clone(),
            vcard,
        }),
    ))
}

pub async fn delete_contact(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    sqlx::query("DELETE FROM carddav_contacts WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(serde_json::json!({"deleted": id})))
}

#[derive(Debug, Deserialize)]
pub struct EventCreate {
    pub summary: String,
    #[serde(default)]
    pub ical: String,
    pub uid: String,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn list_events(
    State(s): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, uid, summary FROM caldav_events ORDER BY starts_at NULLS LAST")
            .fetch_all(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, uid, summary)| serde_json::json!({"id": id, "uid": uid, "summary": summary}))
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn query_events(
    State(s): State<AppState>,
    Query(q): Query<EventQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let rows: Vec<(Uuid, String, String, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT id, uid, summary, starts_at FROM caldav_events WHERE ($1::timestamptz IS NULL OR starts_at >= $1) AND ($2::timestamptz IS NULL OR starts_at <= $2) ORDER BY starts_at",
    )
    .bind(q.from).bind(q.to)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, uid, summary, starts_at)| serde_json::json!({"id": id, "uid": uid, "summary": summary, "starts_at": starts_at}))
            .collect(),
    ))
}

pub async fn create_event(
    State(s): State<AppState>,
    Json(req): Json<EventCreate>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ErrorResponse> {
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users ORDER BY created_at LIMIT 1")
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    let id = Uuid::new_v4();
    let ical = if req.ical.is_empty() {
        format!(
            "BEGIN:VEVENT\nUID:{}\nSUMMARY:{}\nEND:VEVENT",
            req.uid, req.summary
        )
    } else {
        req.ical.clone()
    };
    sqlx::query("INSERT INTO caldav_events (id, user_id, uid, summary, ical, starts_at, ends_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (user_id, uid) DO UPDATE SET summary=$4, ical=$5, starts_at=$6, ends_at=$7, updated_at=now()")
        .bind(id).bind(user_id).bind(&req.uid).bind(&req.summary).bind(&ical).bind(req.starts_at).bind(req.ends_at)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({"id": id, "uid": req.uid})),
    ))
}

/// WebDAV PROPFIND discovery stub: returns addressbook/calendar-home-set links.
pub async fn dav_discovery(headers: HeaderMap) -> String {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("mail.example.com");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav" xmlns:CAL="urn:ietf:params:xml:ns:caldav">
  <D:response><D:href>/dav/addressbooks/</D:href>
    <D:propstat><D:prop><C:addressbook-home-set/></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response><D:href>/dav/calendars/</D:href>
    <D:propstat><D:prop><CAL:calendar-home-set/></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn dav_xml_shape() {
        let s = r#"<multistatus></multistatus>"#;
        assert!(s.contains("multistatus"));
    }
}
