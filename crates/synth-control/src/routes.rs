use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use synth_common::{
    AppError, AppResult, ErrorResponse, create_session_token, decode_session_token, hash_password,
    verify_password,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/v1/auth/login", post(auth_login))
        .route("/api/v1/auth/logout", post(auth_logout))
        .route("/api/v1/auth/me", get(auth_me))
        .route("/api/v1/domains", get(list_domains).post(create_domain))
        .route(
            "/api/v1/domains/{id}",
            get(get_domain).put(update_domain).delete(delete_domain),
        )
        .route("/api/v1/domains/{id}/dns", get(domain_dns))
        .route("/api/v1/users", get(list_users).post(create_user))
        .route(
            "/api/v1/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .route("/api/v1/users/{id}/password", post(reset_password))
        .route("/api/v1/aliases", get(list_aliases).post(create_alias))
        .route(
            "/api/v1/aliases/{id}",
            get(get_alias).put(update_alias).delete(delete_alias),
        )
        .route("/api/v1/quota", get(quota_overview))
        .route("/api/v1/queues", get(queue_status))
        .route("/api/v1/dkim/{domain_id}/generate", post(dkim_generate))
        .route("/api/v1/dkim/{domain_id}", get(dkim_get))
        .route(
            "/api/v1/filters/{user_id}",
            get(list_filters).put(save_filters),
        )
        .route(
            "/api/v1/vacation/{user_id}",
            get(get_vacation).put(save_vacation),
        )
        .route("/api/v1/admin/audit", get(list_audit))
        .route("/api/v1/admin/bans", get(list_bans))
        .route("/api/v1/config/export", get(config_export))
        .route("/api/v1/config/import", post(config_import))
        .route(
            "/.well-known/autoconfig/mail/config-v1.1.xml",
            get(autoconfig),
        )
        .route("/.well-known/mta-sts.txt", get(mta_sts))
        .with_state(state)
}

// ---------- Health ----------

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

async fn readyz(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ErrorResponse> {
    sqlx::query("SELECT 1")
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let redis_ok = s.redis_client.get_async_connection().await.is_ok();
    Ok(Json(
        serde_json::json!({"status":"ok","db":"ok","redis": if redis_ok {"ok"} else {"degraded"}}),
    ))
}

async fn metrics() -> Json<serde_json::Value> {
    Json(serde_json::json!({"synth_control_up": 1}))
}

// ---------- Auth ----------

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

async fn auth_login(
    State(s): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, ErrorResponse> {
    let row: Option<(Uuid, String, String, bool)> =
        sqlx::query_as("SELECT id, email::text, password_hash, active FROM users WHERE email = $1")
            .bind(&req.email.to_lowercase())
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;

    let (id, email, hash, active) = row.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !active {
        return Err(ErrorResponse(AppError::Forbidden));
    }
    if !verify_password(&hash, &req.password) {
        return Err(ErrorResponse(AppError::Unauthorized));
    }

    sqlx::query("UPDATE users SET last_login_at = $1 WHERE id=$2")
        .bind(Utc::now())
        .bind(id)
        .execute(&s.pool)
        .await
        .ok();
    crate::db::audit(
        &s.pool,
        Some(id),
        "login",
        serde_json::json!({"email": email}),
    )
    .await
    .ok();

    let token = create_session_token(&s.config.session_secret, &id.to_string(), &email, "user")
        .map_err(|_| ErrorResponse(AppError::Internal("token error".into())))?;

    Ok(Json(LoginResp {
        token,
        email,
        kind: "user".into(),
    }))
}

async fn auth_logout() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

async fn auth_me(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let claims = extract_claims(&s, &headers)?;
    let row: Option<(Uuid, String, String, i32)> =
        sqlx::query_as("SELECT id, email::text, kind, quota_mb FROM users WHERE id=$1")
            .bind(Uuid::parse_str(&claims.sub).map_err(|_| ErrorResponse(AppError::Unauthorized))?)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (id, email, kind, quota) =
        row.ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    Ok(Json(
        serde_json::json!({"id": id, "email": email, "kind": kind, "quota_mb": quota}),
    ))
}

fn extract_claims(
    s: &AppState,
    headers: &HeaderMap,
) -> Result<synth_common::auth::ClaimsData, ErrorResponse> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth.strip_prefix("Bearer ").unwrap_or(auth);
    if token.is_empty() {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    decode_session_token(&s.config.session_secret, token)
        .map_err(|_| ErrorResponse(AppError::Unauthorized))
}

async fn require_admin(s: &AppState, headers: &HeaderMap) -> Result<Uuid, ErrorResponse> {
    let claims = extract_claims(s, headers)?;
    // Check DB kind
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM users WHERE id=$1")
        .bind(Uuid::parse_str(&claims.sub).unwrap_or(Uuid::nil()))
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    match kind.as_deref() {
        Some("admin") | Some("superadmin") => Ok(Uuid::parse_str(&claims.sub).unwrap()),
        _ => Err(ErrorResponse(AppError::Forbidden)),
    }
}

// ---------- Domains ----------

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct DomainRow {
    id: Uuid,
    domain: String,
    description: Option<String>,
    catch_all: bool,
    dkim_selector: Option<String>,
    dkim_enabled: bool,
    spf_value: Option<String>,
    dmarc_policy: String,
    active: bool,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateDomainReq {
    domain: String,
    description: Option<String>,
    catch_all: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateDomainReq {
    description: Option<String>,
    catch_all: Option<bool>,
    dkim_enabled: Option<bool>,
    spf_value: Option<String>,
    dmarc_policy: Option<String>,
    active: Option<bool>,
}

async fn list_domains(State(s): State<AppState>) -> Result<Json<Vec<DomainRow>>, ErrorResponse> {
    let rows = sqlx::query_as::<_, DomainRow>("SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains ORDER BY domain")
        .fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows))
}

async fn create_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateDomainReq>,
) -> Result<(axum::http::StatusCode, Json<DomainRow>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let d = req.domain.to_lowercase().trim().to_string();
    if !d.contains('.') {
        return Err(ErrorResponse(AppError::Validation("invalid domain".into())));
    }
    let id = Uuid::new_v4();
    let row: DomainRow = sqlx::query_as("INSERT INTO domains (id, domain, description, catch_all) VALUES ($1,$2,$3,$4) RETURNING id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at")
        .bind(id).bind(&d).bind(&req.description).bind(req.catch_all.unwrap_or(false))
        .fetch_one(&s.pool).await.map_err(|_| ErrorResponse(AppError::Validation("domain exists".into())))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "domain_create",
        serde_json::json!({"domain": d}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

async fn get_domain(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DomainRow>, ErrorResponse> {
    let row = sqlx::query_as::<_, DomainRow>("SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains WHERE id=$1")
        .bind(id).fetch_optional(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("domain".into())))?;
    Ok(Json(row))
}

async fn update_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDomainReq>,
) -> Result<Json<DomainRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let mut row = get_domain(State(s.clone()), Path(id)).await?.0;
    if let Some(v) = req.description {
        row.description = Some(v);
    }
    if let Some(v) = req.catch_all {
        row.catch_all = v;
    }
    if let Some(v) = req.dkim_enabled {
        row.dkim_enabled = v;
    }
    if let Some(v) = req.spf_value {
        row.spf_value = Some(v);
    }
    if let Some(v) = req.dmarc_policy {
        row.dmarc_policy = v;
    }
    if let Some(v) = req.active {
        row.active = v;
    }

    let row2: DomainRow = sqlx::query_as("UPDATE domains SET description=$2, catch_all=$3, dkim_enabled=$4, spf_value=$5, dmarc_policy=$6, active=$7 WHERE id=$1 RETURNING id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at")
        .bind(id).bind(&row.description).bind(row.catch_all).bind(row.dkim_enabled).bind(&row.spf_value).bind(&row.dmarc_policy).bind(row.active)
        .fetch_one(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "domain_update",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(row2))
}

async fn delete_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    sqlx::query("DELETE FROM domains WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "domain_delete",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"deleted": id})))
}

async fn domain_dns(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let row = get_domain(State(s.clone()), Path(id)).await?.0;
    let pubkey: Option<String> = sqlx::query_scalar("SELECT public_key FROM dkim_keys WHERE domain_id=$1 AND revoked_at IS NULL ORDER BY created_at DESC LIMIT 1")
        .bind(id).fetch_optional(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let selector = row
        .dkim_selector
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let dkim_txt = pubkey
        .as_ref()
        .map(|k| format!("v=DKIM1; k=rsa; p={}", k))
        .unwrap_or_else(|| format!("v=DKIM1; k=rsa; p=<generate via /dkim/{}/generate>", id));
    let spf = row
        .spf_value
        .clone()
        .unwrap_or_else(|| format!("v=spf1 a:{} -all", s.config.mail_host));
    Ok(Json(serde_json::json!({
        "domain": row.domain,
        "mail_host": s.config.mail_host,
        "a_record": format!("{} A (your server IP)", s.config.mail_host),
        "mx_record": format!("{} MX 10 {}", row.domain, s.config.mail_host),
        "spf": spf,
        "dkim": format!("{}._domainkey TXT {}", selector, dkim_txt),
        "dmarc": format!("v=DMARC1; p={}; rua=mailto:dmarc@{}; fo=1", row.dmarc_policy, row.domain),
        "autodiscover": format!("autodiscover CNAME {}", s.config.mail_host),
        "mta_sts": format!("_mta-sts TXT \"v=STSv1\""),
        "tls_rpt": format!("_smtp._tls TXT \"v=TLSRPTv1;\""),
    })))
}

// ---------- Users ----------

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    kind: String,
    quota_mb: i32,
    used_mb: i32,
    active: bool,
    force_password_change: bool,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct CreateUserReq {
    email: String,
    password: String,
    kind: Option<String>,
    quota_mb: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct UpdateUserReq {
    kind: Option<String>,
    quota_mb: Option<i32>,
    active: Option<bool>,
}

async fn list_users(
    State(s): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<UserRow>>, ErrorResponse> {
    let domain = params.get("domain").cloned();
    let rows = if let Some(d) = domain {
        sqlx::query_as::<_, UserRow>("SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users WHERE email LIKE $1 ORDER BY email")
            .bind(format!("%@{}", d)).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
    } else {
        sqlx::query_as::<_, UserRow>("SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users ORDER BY email")
            .fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
    };
    Ok(Json(rows))
}

async fn create_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserReq>,
) -> Result<(axum::http::StatusCode, Json<UserRow>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let email_norm = req.email.to_lowercase();
    if !email_norm.contains('@') {
        return Err(ErrorResponse(AppError::Validation("invalid email".into())));
    }
    let hash = hash_password(&req.password)
        .map_err(|_| ErrorResponse(AppError::Validation("hash failed".into())))?;
    let id = Uuid::new_v4();
    let quota = req.quota_mb.unwrap_or(s.config.max_quota_mb).min(102_400);
    let row: UserRow = sqlx::query_as("INSERT INTO users (id, email, password_hash, kind, quota_mb, active) VALUES ($1,$2,$3,$4,$5,true) RETURNING id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at")
        .bind(id).bind(&email_norm).bind(hash).bind(req.kind.unwrap_or_else(|| "user".into())).bind(quota)
        .fetch_one(&s.pool).await.map_err(|_| ErrorResponse(AppError::Validation("user exists".into())))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "user_create",
        serde_json::json!({"email": email_norm}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

async fn get_user(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<UserRow>, ErrorResponse> {
    let row = sqlx::query_as::<_, UserRow>("SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users WHERE id=$1")
        .bind(id).fetch_optional(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    Ok(Json(row))
}

async fn update_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserReq>,
) -> Result<Json<UserRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let mut row = get_user(State(s.clone()), Path(id)).await?.0;
    if let Some(v) = req.kind {
        row.kind = v;
    }
    if let Some(v) = req.quota_mb {
        row.quota_mb = v;
    }
    if let Some(v) = req.active {
        row.active = v;
    }
    let row2: UserRow = sqlx::query_as("UPDATE users SET kind=$2, quota_mb=$3, active=$4 WHERE id=$1 RETURNING id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at")
        .bind(id).bind(&row.kind).bind(row.quota_mb).bind(row.active)
        .fetch_one(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "user_update",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(row2))
}

async fn delete_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "user_delete",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"deleted": id})))
}

#[derive(Debug, Deserialize)]
struct ResetPwReq {
    password: String,
}

async fn reset_password(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<ResetPwReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let hash = hash_password(&req.password)
        .map_err(|_| ErrorResponse(AppError::Validation("hash".into())))?;
    sqlx::query("UPDATE users SET password_hash=$2 WHERE id=$1")
        .bind(id)
        .bind(hash)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "password_reset",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------- Aliases ----------

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct AliasRow {
    id: Uuid,
    address: String,
    goto: String,
    active: bool,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateAliasReq {
    address: String,
    goto: String,
}

#[derive(Debug, Deserialize)]
struct UpdateAliasReq {
    goto: Option<String>,
    active: Option<bool>,
}

async fn list_aliases(State(s): State<AppState>) -> Result<Json<Vec<AliasRow>>, ErrorResponse> {
    let rows = sqlx::query_as::<_, AliasRow>(
        "SELECT id, address::text, goto, active, created_at FROM aliases ORDER BY address",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows))
}

async fn create_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateAliasReq>,
) -> Result<(axum::http::StatusCode, Json<AliasRow>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let id = Uuid::new_v4();
    let row: AliasRow = sqlx::query_as("INSERT INTO aliases (id, address, goto, active) VALUES ($1,$2,$3,true) RETURNING id, address::text, goto, active, created_at")
        .bind(id).bind(req.address.to_lowercase()).bind(req.goto)
        .fetch_one(&s.pool).await.map_err(|_| ErrorResponse(AppError::Validation("alias exists".into())))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "alias_create",
        serde_json::json!({"address": req.address}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

async fn get_alias(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AliasRow>, ErrorResponse> {
    let row = sqlx::query_as::<_, AliasRow>(
        "SELECT id, address::text, goto, active, created_at FROM aliases WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?
    .ok_or_else(|| ErrorResponse(AppError::NotFound("alias".into())))?;
    Ok(Json(row))
}

async fn update_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAliasReq>,
) -> Result<Json<AliasRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let mut r = get_alias(State(s.clone()), Path(id)).await?.0;
    if let Some(v) = req.goto {
        r.goto = v;
    }
    if let Some(v) = req.active {
        r.active = v;
    }
    let row: AliasRow = sqlx::query_as("UPDATE aliases SET goto=$2, active=$3 WHERE id=$1 RETURNING id, address::text, goto, active, created_at")
        .bind(id).bind(&r.goto).bind(r.active).fetch_one(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "alias_update",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(row))
}

async fn delete_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    sqlx::query("DELETE FROM aliases WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "alias_delete",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"deleted": id})))
}

// ---------- Quota / Queue / DKIM ----------

async fn quota_overview(
    State(s): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let rows: Vec<(String, i32, i32, f64)> = sqlx::query_as("SELECT email::text, quota_mb, used_mb, CASE WHEN quota_mb>0 THEN (used_mb*100.0/quota_mb) ELSE 0 END FROM users ORDER BY 4 DESC LIMIT 100")
        .fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(email, quota, used, pct)| serde_json::json!({"email": email, "quota_mb": quota, "used_mb": used, "pct": pct})).collect()))
}

async fn queue_status(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ErrorResponse> {
    //.Try postfix qshape via file or redis cached; fallback to stub
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE created_at > now() - INTERVAL '1 hour'",
    )
    .fetch_optional(&s.pool)
    .await
    .unwrap_or(Some(0))
    .map(|v| v)
    .unwrap_or(0);
    let _ = s;
    Ok(Json(
        serde_json::json!({"queue":{"deferred":0,"active":0,"hold":0},"activity_1h": count, "note":"postfix qshape polled every 30s in Phase 2"}),
    ))
}

#[derive(Debug, Deserialize)]
struct DkimGenReq {
    key_type: Option<String>,
    selector: Option<String>,
}

async fn dkim_generate(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(domain_id): Path<Uuid>,
    Json(req): Json<DkimGenReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let domain_row = get_domain(State(s.clone()), Path(domain_id)).await?.0;
    let selector = req.selector.unwrap_or_else(|| "default".to_string());
    let ktype = req.key_type.unwrap_or_else(|| "rsa2048".to_string());
    let pair = synth_dkim::generate(&selector, &ktype)
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO dkim_keys (id, domain_id, selector, public_key, private_key_enc, key_type) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(domain_id).bind(&selector).bind(&pair.public_key_b64).bind(&pair.private_key_pem).bind(&pair.key_type)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("UPDATE domains SET dkim_selector=$2, dkim_enabled=true WHERE id=$1")
        .bind(domain_id)
        .bind(&selector)
        .execute(&s.pool)
        .await
        .ok();

    crate::db::audit(
        &s.pool,
        Some(actor),
        "dkim_generate",
        serde_json::json!({"domain": domain_row.domain, "selector": selector}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();

    Ok(Json(serde_json::json!({
        "domain": domain_row.domain,
        "selector": selector,
        "key_type": pair.key_type,
        "public_key": pair.public_key_b64,
        "dns_record": format!("{}._domainkey TXT {}", domain_row.domain.split('.').last().unwrap_or(""), pair.dns_txt),
        "full_dns": format!("{}._domainkey.{} TXT \"{}\"", selector, domain_row.domain, pair.dns_txt),
        "keytable_entry": synth_dkim::opendkim_keytable_entry(&domain_row.domain, &pair),
    })))
}

async fn dkim_get(
    State(s): State<AppState>,
    Path(domain_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let rows: Vec<(Uuid, String, String, String, Option<DateTime<Utc>>)> = sqlx::query_as("SELECT id, selector, public_key, dns_value, revoked_at FROM dkim_keys WHERE domain_id=$1 ORDER BY created_at DESC")
        .bind(domain_id).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(id, sel, pubk, dns, revoked)| serde_json::json!({"id": id, "selector": sel, "public_key": pubk, "dns_value": dns, "revoked": revoked.is_some()})).collect()))
}

// ---------- Filters / Vacation ----------

async fn list_filters(
    State(s): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let rows: Vec<(Uuid, String, String, bool)> =
        sqlx::query_as("SELECT id, name, content, active FROM sieve_scripts WHERE user_id=$1")
            .bind(user_id)
            .fetch_all(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(id, name, content, active)| serde_json::json!({"id": id, "name": name, "content": content, "active": active})).collect()))
}

#[derive(Debug, Deserialize)]
struct SaveFiltersReq {
    scripts: Vec<FilterScript>,
}
#[derive(Debug, Deserialize)]
struct FilterScript {
    name: String,
    content: String,
    active: bool,
}

async fn save_filters(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(req): Json<SaveFiltersReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let _ = require_admin_or_self(&s, &headers, user_id).await?;
    for sc in &req.scripts {
        synth_filter::validate_sieve(&sc.content)
            .map_err(|e| ErrorResponse(AppError::Validation(e)))?;
    }
    sqlx::query("DELETE FROM sieve_scripts WHERE user_id=$1")
        .bind(user_id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    for sc in &req.scripts {
        sqlx::query("INSERT INTO sieve_scripts (id, user_id, name, content, active) VALUES ($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(user_id).bind(&sc.name).bind(&sc.content).bind(sc.active)
            .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    }
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn require_admin_or_self(
    s: &AppState,
    headers: &HeaderMap,
    user_id: Uuid,
) -> Result<Uuid, ErrorResponse> {
    // Allow if admin OR self
    let claims = extract_claims(s, headers)?;
    let sub = Uuid::parse_str(&claims.sub).unwrap_or(Uuid::nil());
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM users WHERE id=$1")
        .bind(sub)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    if matches!(kind.as_deref(), Some("admin") | Some("superadmin")) || sub == user_id {
        Ok(sub)
    } else {
        Err(ErrorResponse(AppError::Forbidden))
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct VacationRow {
    user_id: Uuid,
    subject: String,
    message: String,
    enabled: bool,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct SaveVacationReq {
    subject: String,
    message: String,
    enabled: bool,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
}

async fn get_vacation(
    State(s): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let row: Option<VacationRow> = sqlx::query_as("SELECT user_id, subject, message, enabled, start_date, end_date FROM vacation WHERE user_id=$1").bind(user_id).fetch_optional(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    match row {
        Some(r) => Ok(Json(serde_json::json!(r))),
        None => Ok(Json(
            serde_json::json!({"user_id": user_id, "subject": "", "message": "", "enabled": false}),
        )),
    }
}

async fn save_vacation(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(req): Json<SaveVacationReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let _ = require_admin_or_self(&s, &headers, user_id).await?;
    sqlx::query("INSERT INTO vacation (user_id, subject, message, enabled, start_date, end_date) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (user_id) DO UPDATE SET subject=$2, message=$3, enabled=$4, start_date=$5, end_date=$6")
        .bind(user_id).bind(&req.subject).bind(&req.message).bind(req.enabled).bind(req.start_date).bind(req.end_date)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    // Generate sieve file content (stored, dovecot picks up via background sync)
    let email: String = sqlx::query_scalar("SELECT email::text FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .unwrap_or_else(|| "user@example.com".to_string());
    let sieve = synth_filter::generate_vacation_sieve(&synth_filter::VacationConfig {
        user_id: email,
        enabled: req.enabled,
        subject: req.subject,
        message: req.message,
        start: None,
        end: None,
    });
    sqlx::query("INSERT INTO sieve_scripts (id, user_id, name, content, active) VALUES ($1,$2,'vacation',$3,$4)")
        .bind(Uuid::new_v4()).bind(user_id).bind(&sieve).bind(req.enabled).execute(&s.pool).await.ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(
        serde_json::json!({"ok": true, "sieve_preview": sieve}),
    ))
}

// ---------- Admin ----------

async fn list_audit(
    State(s): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let rows: Vec<(i64, Option<Uuid>, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as("SELECT id, actor, action, target, created_at FROM audit_log ORDER BY created_at DESC LIMIT $1")
        .bind(limit).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(id, actor, action, target, created_at)| serde_json::json!({"id": id, "actor": actor, "action": action, "target": target, "created_at": created_at})).collect()))
}

async fn list_bans(
    State(_s): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    // fail2ban reads from redis/logs; in Phase 1 return empty, Phase 2 polls fail2ban-client
    Ok(Json(vec![]))
}

// ---------- Config export/import ----------

async fn config_export(
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let domains = list_domains(State(s.clone())).await?.0;
    let users = list_users(State(s.clone()), Query(std::collections::HashMap::new()))
        .await?
        .0;
    let aliases = list_aliases(State(s.clone())).await?.0;
    Ok(Json(serde_json::json!({
        "version": "1",
        "exported_at": Utc::now(),
        "domains": domains,
        "users": users.iter().map(|u| serde_json::json!({"email": u.email, "kind": u.kind, "quota_mb": u.quota_mb})).collect::<Vec<_>>(),
        "aliases": aliases,
    })))
}

#[derive(Debug, Deserialize)]
struct ImportReq {
    domains: Vec<serde_json::Value>,
    users: Vec<serde_json::Value>,
}

async fn config_import(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ImportReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let mut imported = 0;
    for d in &req.domains {
        if let Some(dom) = d.get("domain").and_then(|v| v.as_str()) {
            let id = Uuid::new_v4();
            let res = sqlx::query("INSERT INTO domains (id, domain, description) VALUES ($1,$2,$3) ON CONFLICT (domain) DO NOTHING")
                .bind(id).bind(dom).bind(d.get("description").and_then(|v| v.as_str()))
                .execute(&s.pool).await;
            if res.is_ok() {
                imported += 1;
            }
        }
    }
    crate::db::audit(
        &s.pool,
        Some(actor),
        "config_import",
        serde_json::json!({"imported_domains": imported}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"imported": imported})))
}

// ---------- Well-known ----------

async fn autoconfig(State(s): State<AppState>) -> String {
    let host = s.config.mail_host.clone();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig xmlns="http://test.kណ្ម� art.com/autoconfig/1.2">
  <emailProvider id="synthmail">
    <domain>example.com</domain>
    <displayName>SynthMail</displayName>
    <incomingServer type="imap"><hostname>{host}</hostname><port>993</port><socketType>SSL</socketType><authentication>password</authentication></incomingServer>
    <outgoingServer type="smtp"><hostname>{host}</hostname><port>587</port><socketType>STARTTLS</socketType><authentication>password</authentication></outgoingServer>
  </emailProvider>
</clientConfig>"#
    )
}

async fn mta_sts(State(s): State<AppState>) -> String {
    format!(
        "version: STSv1\nmode: enforce\nmx: {}\nmax_age: 86400\n",
        s.config.mail_host
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn import_shape() {
        let v = serde_json::json!({"domains": []});
        assert_eq!(v["domains"].as_array().unwrap().len(), 0);
    }
}
