use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::app::AppState;
use synth_common::{
    AppError, ErrorResponse, create_session_token, decode_session_token, hash_password,
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
        .route("/api/v1/auth/password/change", post(auth_change_password))
        .route("/api/v1/domains", get(list_domains).post(create_domain))
        .route(
            "/api/v1/domains/{id}",
            get(get_domain).put(update_domain).delete(delete_domain),
        )
        .route("/api/v1/domains/{id}/dns", get(domain_dns))
        .route("/api/v1/domains/{id}/verify", get(domain_verify))
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
            "/api/v1/dmarc/reports",
            get(list_dmarc_reports).post(create_dmarc_report),
        )
        .route(
            "/api/v1/push/subscribe",
            get(list_push_subs).post(create_push_sub),
        )
        .route("/api/v1/push/subscribe/{id}", delete(delete_push_sub))
        .route("/api/v1/auth/totp/enroll", post(totp_enroll))
        .route("/api/v1/auth/totp/verify", post(totp_verify))
        .route("/api/v1/auth/totp/disable", post(totp_disable))
        .route(
            "/api/v1/api-tokens",
            get(list_api_tokens).post(create_api_token),
        )
        .route("/api/v1/api-tokens/{id}", delete(revoke_api_token))
        .route(
            "/.well-known/autoconfig/mail/config-v1.1.xml",
            get(autoconfig),
        )
        .route("/.well-known/mta-sts.txt", get(mta_sts))
        .layer(middleware::from_fn(count_requests))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

async fn count_requests(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    synth_common::metrics::inc_api_request();
    let t0 = std::time::Instant::now();
    let resp = next.run(req).await;
    synth_common::metrics::observe_request_latency(t0.elapsed().as_secs_f64());
    resp
}

// ---------- Helpers ----------

fn client_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("dummy-password-for-timing-parity").unwrap())
}

/// Currently authenticated principal. JWT principals are unscoped (`None`);
/// API-token principals carry their granted scopes (`Some(..)`, H-01).
#[derive(Debug, Clone)]
struct Principal {
    user_id: Uuid,
    kind: String,
    scopes: Option<Vec<String>>,
    force_password_change: bool,
}

impl Principal {
    fn is_admin(&self) -> bool {
        matches!(self.kind.as_str(), "admin" | "superadmin")
    }
    fn has_scope(&self, scope: &str) -> bool {
        match &self.scopes {
            None => true,
            Some(scopes) => scopes
                .iter()
                .any(|s| s == scope || s == "*" || (scope == "write" && s == "admin")),
        }
    }
}

async fn load_principal(
    s: &AppState,
    user_id: Uuid,
    scopes: Option<Vec<String>>,
) -> Result<Option<Principal>, ErrorResponse> {
    let row: Option<(String, bool, bool)> =
        sqlx::query_as("SELECT kind, active, force_password_change FROM users WHERE id=$1")
            .bind(user_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(row
        .filter(|(_, active, _)| *active)
        .map(|(kind, _, fpc)| Principal {
            user_id,
            kind,
            scopes,
            force_password_change: fpc,
        }))
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

/// Authenticate via JWT or API token. Rejects inactive users and JWTs whose
/// token version no longer matches the DB (H-04 revocation).
async fn authenticate(s: &AppState, headers: &HeaderMap) -> Result<Principal, ErrorResponse> {
    if let Some((user_id, scopes)) = authenticate_api_key(s, headers).await {
        return load_principal(s, user_id, Some(scopes))
            .await?
            .ok_or_else(|| ErrorResponse(AppError::Unauthorized));
    }
    // If a synth_ token was presented but invalid/expired, do not fall through.
    let raw = headers
        .get("authorization")
        .or_else(|| headers.get("x-api-key"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).trim())
        .unwrap_or("");
    if raw.starts_with("synth_") {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    let claims = extract_claims(s, headers)?;
    let user_id =
        Uuid::parse_str(&claims.sub).map_err(|_| ErrorResponse(AppError::Unauthorized))?;
    let db_tv: Option<i64> = sqlx::query_scalar("SELECT token_version FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let db_tv = db_tv.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if claims.tv as i64 != db_tv {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    load_principal(s, user_id, None)
        .await?
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))
}

/// Admin gate. `write=true` requires a write-capable API token; `write=false`
/// accepts read-capable tokens. JWT principals always pass the scope check.
async fn require_admin_principal(
    s: &AppState,
    headers: &HeaderMap,
    write: bool,
) -> Result<Principal, ErrorResponse> {
    let p = authenticate(s, headers).await?;
    if !p.is_admin() {
        return Err(ErrorResponse(AppError::Forbidden));
    }
    if p.force_password_change {
        return Err(ErrorResponse(AppError::Validation(
            "password change required before using admin APIs".into(),
        )));
    }
    let needed = if write { "write" } else { "read" };
    if !p.has_scope(needed) {
        return Err(ErrorResponse(AppError::Forbidden));
    }
    Ok(p)
}

async fn require_admin(s: &AppState, headers: &HeaderMap) -> Result<Uuid, ErrorResponse> {
    Ok(require_admin_principal(s, headers, true).await?.user_id)
}

async fn require_admin_or_self(
    s: &AppState,
    headers: &HeaderMap,
    user_id: Uuid,
    write: bool,
) -> Result<Uuid, ErrorResponse> {
    let p = authenticate(s, headers).await?;
    let needed = if write { "write" } else { "read" };
    if (p.is_admin() && p.has_scope(needed)) || p.user_id == user_id {
        // Self access via API token still needs the scope.
        if p.user_id == user_id && !p.is_admin() && !p.has_scope(needed) {
            return Err(ErrorResponse(AppError::Forbidden));
        }
        Ok(p.user_id)
    } else {
        Err(ErrorResponse(AppError::Forbidden))
    }
}

fn is_valid_domain(d: &str) -> bool {
    let d = d.trim_end_matches('.');
    if d.is_empty() || d.len() > 253 || !d.contains('.') {
        return false;
    }
    d.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

fn is_valid_email(e: &str) -> bool {
    let mut parts = e.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty() && local.len() <= 64 && is_valid_domain(domain)
        }
        _ => false,
    }
}

fn clamp_quota(q: i32) -> i32 {
    q.clamp(0, 102_400)
}

fn check_dmarc_policy(p: &str) -> Result<(), ErrorResponse> {
    match p {
        "none" | "quarantine" | "reject" => Ok(()),
        _ => Err(ErrorResponse(AppError::Validation(
            "dmarc_policy must be none|quarantine|reject".into(),
        ))),
    }
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
    let redis_ok = s
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .is_ok();
    Ok(Json(
        serde_json::json!({"status":"ok","db":"ok","redis": if redis_ok {"ok"} else {"degraded"}}),
    ))
}

async fn metrics() -> String {
    synth_common::metrics::render_prometheus("synth_control")
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

async fn auth_login(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginReq>,
) -> Result<Json<LoginResp>, ErrorResponse> {
    let ip = client_ip(&headers);
    // H-02: per-IP+account in-app rate limiting.
    let rate_key = format!(
        "login:{}:{}",
        ip.as_deref().unwrap_or("unknown"),
        req.email.to_lowercase()
    );
    if !s.limiter.check(&rate_key).await {
        return Err(ErrorResponse(AppError::TooManyRequests));
    }
    // DB lockout: many recent failures for this account.
    let recent_failures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM login_attempts WHERE email=$1 AND NOT success AND created_at > now() - INTERVAL '15 minutes'",
    )
    .bind(req.email.to_lowercase())
    .fetch_optional(&s.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(0);
    if recent_failures >= 20 {
        return Err(ErrorResponse(AppError::TooManyRequests));
    }

    let row: Option<(Uuid, String, String, bool, String, i64)> = sqlx::query_as(
        "SELECT id, email::text, password_hash, active, kind, token_version FROM users WHERE email = $1",
    )
    .bind(req.email.to_lowercase())
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;

    // M-08: unknown/inactive/wrong-password all return identical 401 after an
    // Argon2 verification (dummy hash when the account doesn't exist).
    let (id, email, hash, active, kind, token_version) = match row {
        Some(r) => r,
        None => {
            let _ = verify_password(dummy_hash(), &req.password);
            record_login_attempt(&s, &req.email.to_lowercase(), ip.as_deref(), false).await;
            synth_common::metrics::inc_auth_failure();
            return Err(ErrorResponse(AppError::Unauthorized));
        }
    };
    if !active {
        let _ = verify_password(&hash, &req.password);
        record_login_attempt(&s, &email, ip.as_deref(), false).await;
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    if !verify_password(&hash, &req.password) {
        record_login_attempt(&s, &email, ip.as_deref(), false).await;
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    synth_common::metrics::inc_auth_attempt();
    require_totp_or_ok(&s, id, &email, req.totp_code.as_deref()).await?;
    record_login_attempt(&s, &email, ip.as_deref(), true).await;

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

    let token = create_session_token(
        &s.config.session_secret,
        &id.to_string(),
        &email,
        &kind,
        token_version as u64,
    )
    .map_err(|_| ErrorResponse(AppError::Internal("token error".into())))?;

    Ok(Json(LoginResp { token, email, kind }))
}

async fn record_login_attempt(s: &AppState, email: &str, ip: Option<&str>, success: bool) {
    let _ = sqlx::query("INSERT INTO login_attempts (email, ip, success) VALUES ($1,$2,$3)")
        .bind(email)
        .bind(ip)
        .bind(success)
        .execute(&s.pool)
        .await;
}

async fn auth_logout(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // H-04: logout now really revokes all sessions for this user.
    let p = authenticate(&s, &headers).await?;
    crate::db::bump_token_version(&s.pool, p.user_id)
        .await
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    crate::db::audit(&s.pool, Some(p.user_id), "logout", serde_json::json!({}))
        .await
        .ok();
    Ok(Json(serde_json::json!({"status":"ok"})))
}

async fn auth_me(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    let row: Option<(Uuid, String, String, i32, bool)> = sqlx::query_as(
        "SELECT id, email::text, kind, quota_mb, force_password_change FROM users WHERE id=$1",
    )
    .bind(p.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (id, email, kind, quota, force_pw) =
        row.ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    Ok(Json(serde_json::json!({
        "id": id, "email": email, "kind": kind, "quota_mb": quota,
        "force_password_change": force_pw,
    })))
}

#[derive(Debug, Deserialize)]
struct ChangePasswordReq {
    current_password: String,
    new_password: String,
}

async fn auth_change_password(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(p.user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !verify_password(&hash, &req.current_password) {
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    synth_common::check_password_allowed(
        &req.new_password,
        s.config.hibp_enabled,
        s.config.min_password_len,
    )
    .await
    .map_err(AppError::Validation)?;
    let new_hash = hash_password(&req.new_password)
        .map_err(|_| ErrorResponse(AppError::Validation("hash failed".into())))?;
    sqlx::query(
        "UPDATE users SET password_hash=$2, force_password_change=false, token_version=token_version+1 WHERE id=$1",
    )
    .bind(p.user_id)
    .bind(new_hash)
    .execute(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(p.user_id),
        "password_change",
        serde_json::json!({}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"ok": true})))
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

async fn fetch_domain(s: &AppState, id: Uuid) -> Result<DomainRow, ErrorResponse> {
    let q = "SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains WHERE id=$1";
    sqlx::query_as::<_, DomainRow>(q)
        .bind(id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("domain".into())))
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

async fn list_domains(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DomainRow>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let q = "SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains ORDER BY domain";
    let rows = sqlx::query_as::<_, DomainRow>(q)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows))
}

async fn create_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateDomainReq>,
) -> Result<(StatusCode, Json<DomainRow>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let d = req.domain.to_lowercase().trim().to_string();
    if !is_valid_domain(&d) {
        return Err(ErrorResponse(AppError::Validation("invalid domain".into())));
    }
    let id = Uuid::new_v4();
    let q = "INSERT INTO domains (id, domain, description, catch_all) VALUES ($1,$2,$3,$4) RETURNING id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at";
    let row: DomainRow = sqlx::query_as(q)
        .bind(id)
        .bind(&d)
        .bind(&req.description)
        .bind(req.catch_all.unwrap_or(false))
        .fetch_one(&s.pool)
        .await
        .map_err(|_| ErrorResponse(AppError::Validation("domain exists".into())))?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<DomainRow>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    Ok(Json(fetch_domain(&s, id).await?))
}

async fn update_domain(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDomainReq>,
) -> Result<Json<DomainRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    if let Some(p) = &req.dmarc_policy {
        check_dmarc_policy(p)?;
    }
    // Transactional read-modify-write (M-03).
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let q = "SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains WHERE id=$1 FOR UPDATE";
    let mut row: DomainRow = sqlx::query_as::<_, DomainRow>(q)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("domain".into())))?;
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
    let q2 = "UPDATE domains SET description=$2, catch_all=$3, dkim_enabled=$4, spf_value=$5, dmarc_policy=$6, active=$7 WHERE id=$1 RETURNING id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at";
    let row2: DomainRow = sqlx::query_as(q2)
        .bind(id)
        .bind(&row.description)
        .bind(row.catch_all)
        .bind(row.dkim_enabled)
        .bind(&row.spf_value)
        .bind(&row.dmarc_policy)
        .bind(row.active)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    tx.commit()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
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
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let row = fetch_domain(&s, id).await?;
    let pubkey: Option<String> = sqlx::query_scalar(
        "SELECT public_key FROM dkim_keys WHERE domain_id=$1 AND revoked_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
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
        "mta_sts": "_mta-sts TXT \"v=STSv1\"".to_string(),
        "tls_rpt": "_smtp._tls TXT \"v=TLSRPTv1;\"".to_string(),
    })))
}

async fn domain_verify(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // Admin-only: triggers outbound DNS (abuse vector, C-01).
    require_admin_principal(&s, &headers, false).await?;
    let domain = fetch_domain(&s, id).await?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|_| ErrorResponse(AppError::Internal("client".into())))?;
    let mut checks = serde_json::json!({ "domain": domain.domain, "checks": {} });

    let mx = synth_common::dns_check::doh_query(&client, &domain.domain, "MX").await;
    let mx_recs = mx
        .as_ref()
        .map(synth_common::dns_check::extract_mx)
        .unwrap_or_default();
    checks["checks"]["mx"] =
        serde_json::json!({"ok": !mx_recs.is_empty(), "records": mx_recs.clone()});

    let spf_txt = synth_common::dns_check::doh_query(&client, &domain.domain, "TXT").await;
    let spf_recs = spf_txt
        .as_ref()
        .map(synth_common::dns_check::extract_txt)
        .unwrap_or_default();
    checks["checks"]["spf"] = serde_json::json!({"ok": synth_common::dns_check::check_spf(&spf_recs), "records": spf_recs});

    let selector = domain
        .dkim_selector
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let dkim_name = format!("{}._domainkey.{}", selector, domain.domain);
    let dkim_txt = synth_common::dns_check::doh_query(&client, &dkim_name, "TXT").await;
    let dkim_recs = dkim_txt
        .as_ref()
        .map(synth_common::dns_check::extract_txt)
        .unwrap_or_default();
    checks["checks"]["dkim"] = serde_json::json!({"ok": synth_common::dns_check::check_dkim(&dkim_recs), "selector": selector, "records": dkim_recs});

    let dmarc_name = format!("_dmarc.{}", domain.domain);
    let dmarc_txt = synth_common::dns_check::doh_query(&client, &dmarc_name, "TXT").await;
    let dmarc_recs = dmarc_txt
        .as_ref()
        .map(synth_common::dns_check::extract_txt)
        .unwrap_or_default();
    checks["checks"]["dmarc"] = serde_json::json!({"ok": synth_common::dns_check::check_dmarc(&dmarc_recs), "records": dmarc_recs});

    let sts_name = format!("_mta-sts.{}", domain.domain);
    let sts_txt = synth_common::dns_check::doh_query(&client, &sts_name, "TXT").await;
    let sts_recs = sts_txt
        .as_ref()
        .map(synth_common::dns_check::extract_txt)
        .unwrap_or_default();
    checks["checks"]["mta_sts"] = serde_json::json!({"ok": synth_common::dns_check::check_sts(&sts_recs), "records": sts_recs});

    let rpt_name = format!("_smtp._tls.{}", domain.domain);
    let rpt_txt = synth_common::dns_check::doh_query(&client, &rpt_name, "TXT").await;
    let rpt_recs = rpt_txt
        .as_ref()
        .map(synth_common::dns_check::extract_txt)
        .unwrap_or_default();
    checks["checks"]["tls_rpt"] = serde_json::json!({"ok": synth_common::dns_check::check_tls_rpt(&rpt_recs), "records": rpt_recs});

    let c = &checks["checks"];
    let all_ok = c["mx"]["ok"].as_bool().unwrap_or(false)
        && c["spf"]["ok"].as_bool().unwrap_or(false)
        && c["dkim"]["ok"].as_bool().unwrap_or(false)
        && c["dmarc"]["ok"].as_bool().unwrap_or(false);
    checks["deliverable"] = serde_json::json!(all_ok);
    checks["score_hint"] = if all_ok {
        serde_json::json!("ready for mail-tester.com")
    } else {
        serde_json::json!("fix failing checks, then verify at mail-tester.com")
    };
    Ok(Json(checks))
}

// ---------- Users ----------

impl UserRow {
    fn is_admin(&self) -> bool {
        matches!(self.kind.as_str(), "admin" | "superadmin")
    }
}

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

async fn fetch_user(s: &AppState, id: Uuid) -> Result<UserRow, ErrorResponse> {
    let q = "SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users WHERE id=$1";
    sqlx::query_as::<_, UserRow>(q)
        .bind(id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))
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
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<UserRow>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let domain = params.get("domain").cloned();
    let rows = if let Some(d) = domain {
        if !is_valid_domain(&d) {
            return Err(ErrorResponse(AppError::Validation("invalid domain".into())));
        }
        let q = "SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users WHERE email LIKE $1 ORDER BY email";
        sqlx::query_as::<_, UserRow>(q)
            .bind(format!("%@{}", d))
            .fetch_all(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?
    } else {
        let q = "SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users ORDER BY email";
        sqlx::query_as::<_, UserRow>(q)
            .fetch_all(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?
    };
    Ok(Json(rows))
}

async fn create_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateUserReq>,
) -> Result<(StatusCode, Json<UserRow>), ErrorResponse> {
    let actor_p = require_admin_principal(&s, &headers, true).await?;
    let email_norm = req.email.to_lowercase();
    if !is_valid_email(&email_norm) {
        return Err(ErrorResponse(AppError::Validation("invalid email".into())));
    }
    // M-14: the email's domain must be a managed domain.
    let email_domain = email_norm.split('@').nth(1).unwrap_or("");
    let domain_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM domains WHERE domain=$1)")
            .bind(email_domain)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?
            .unwrap_or(false);
    if !domain_exists {
        return Err(ErrorResponse(AppError::Validation(
            "email domain is not managed by this server".into(),
        )));
    }
    let kind = req.kind.clone().unwrap_or_else(|| "user".into());
    if !matches!(kind.as_str(), "user" | "relay" | "admin" | "superadmin") {
        return Err(ErrorResponse(AppError::Validation("invalid kind".into())));
    }
    // M-14: only superadmins may create admin accounts.
    if matches!(kind.as_str(), "admin" | "superadmin") && actor_p.kind != "superadmin" {
        return Err(ErrorResponse(AppError::Forbidden));
    }
    synth_common::check_password_allowed(
        &req.password,
        s.config.hibp_enabled,
        s.config.min_password_len,
    )
    .await
    .map_err(AppError::Validation)?;
    let hash = hash_password(&req.password)
        .map_err(|_| ErrorResponse(AppError::Validation("hash failed".into())))?;
    let id = Uuid::new_v4();
    let quota = clamp_quota(req.quota_mb.unwrap_or(s.config.max_quota_mb));
    let q = "INSERT INTO users (id, email, password_hash, kind, quota_mb, active) VALUES ($1,$2,$3,$4,$5,true) RETURNING id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at";
    let row: UserRow = sqlx::query_as(q)
        .bind(id)
        .bind(&email_norm)
        .bind(hash)
        .bind(&kind)
        .bind(quota)
        .fetch_one(&s.pool)
        .await
        .map_err(|_| ErrorResponse(AppError::Validation("user exists".into())))?;
    crate::db::audit(
        &s.pool,
        Some(actor_p.user_id),
        "user_create",
        serde_json::json!({"email": email_norm}),
    )
    .await
    .ok();
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<UserRow>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    Ok(Json(fetch_user(&s, id).await?))
}

async fn update_user(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateUserReq>,
) -> Result<Json<UserRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    if let Some(k) = &req.kind
        && !matches!(k.as_str(), "user" | "relay" | "admin" | "superadmin")
    {
        return Err(ErrorResponse(AppError::Validation("invalid kind".into())));
    }
    // M-14: never demote/deactivate the last admin.
    let existing = fetch_user(&s, id).await?;
    let demotes_admin = existing.is_admin()
        && (req
            .kind
            .as_deref()
            .is_some_and(|k| !matches!(k, "admin" | "superadmin"))
            || req.active == Some(false));
    if demotes_admin {
        let admin_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE kind IN ('admin','superadmin') AND active=true",
        )
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .unwrap_or(0);
        if admin_count <= 1 {
            return Err(ErrorResponse(AppError::Validation(
                "cannot demote or deactivate the last admin".into(),
            )));
        }
    }
    // Transactional read-modify-write (M-03).
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let q = "SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users WHERE id=$1 FOR UPDATE";
    let mut row: UserRow = sqlx::query_as::<_, UserRow>(q)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    if let Some(v) = req.kind {
        row.kind = v;
    }
    if let Some(v) = req.quota_mb {
        row.quota_mb = clamp_quota(v);
    }
    if let Some(v) = req.active {
        row.active = v;
    }
    let q2 = "UPDATE users SET kind=$2, quota_mb=$3, active=$4 WHERE id=$1 RETURNING id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at";
    let row2: UserRow = sqlx::query_as(q2)
        .bind(id)
        .bind(&row.kind)
        .bind(row.quota_mb)
        .bind(row.active)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    // Deactivation revokes existing sessions (H-04).
    if !row2.active {
        sqlx::query("UPDATE users SET token_version=token_version+1 WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    }
    tx.commit()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
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
    if id == actor {
        return Err(ErrorResponse(AppError::Validation(
            "cannot delete your own account".into(),
        )));
    }
    let existing = fetch_user(&s, id).await?;
    if existing.is_admin() {
        let admin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE kind IN ('admin','superadmin')")
                .fetch_optional(&s.pool)
                .await
                .map_err(|e| ErrorResponse(AppError::Database(e)))?
                .unwrap_or(0);
        if admin_count <= 1 {
            return Err(ErrorResponse(AppError::Validation(
                "cannot delete the last admin".into(),
            )));
        }
    }
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
    synth_common::check_password_allowed(
        &req.password,
        s.config.hibp_enabled,
        s.config.min_password_len,
    )
    .await
    .map_err(AppError::Validation)?;
    let hash = hash_password(&req.password)
        .map_err(|_| ErrorResponse(AppError::Validation("hash".into())))?;
    sqlx::query(
        "UPDATE users SET password_hash=$2, force_password_change=true, token_version=token_version+1 WHERE id=$1",
    )
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

async fn fetch_alias(s: &AppState, id: Uuid) -> Result<AliasRow, ErrorResponse> {
    let q = "SELECT id, address::text, goto, active, created_at FROM aliases WHERE id=$1";
    sqlx::query_as::<_, AliasRow>(q)
        .bind(id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("alias".into())))
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

fn check_alias_targets(address: &str, goto: &str) -> Result<(), ErrorResponse> {
    if !is_valid_email(address) {
        return Err(ErrorResponse(AppError::Validation(
            "invalid alias address".into(),
        )));
    }
    let dests: Vec<&str> = goto.split(',').map(str::trim).collect();
    if dests.is_empty() || dests.iter().any(|d| !is_valid_email(d)) {
        return Err(ErrorResponse(AppError::Validation(
            "goto must be a comma-separated list of valid email addresses".into(),
        )));
    }
    // Self-loop detection (M-14): alias may not forward to itself.
    let addr = address.to_lowercase();
    if dests.iter().any(|d| d.to_lowercase() == addr) {
        return Err(ErrorResponse(AppError::Validation(
            "alias cannot forward to itself".into(),
        )));
    }
    Ok(())
}

async fn list_aliases(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AliasRow>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let q = "SELECT id, address::text, goto, active, created_at FROM aliases ORDER BY address";
    let rows = sqlx::query_as::<_, AliasRow>(q)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows))
}

async fn create_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateAliasReq>,
) -> Result<(StatusCode, Json<AliasRow>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    check_alias_targets(&req.address, &req.goto)?;
    let id = Uuid::new_v4();
    let q = "INSERT INTO aliases (id, address, goto, active) VALUES ($1,$2,$3,true) RETURNING id, address::text, goto, active, created_at";
    let row: AliasRow = sqlx::query_as(q)
        .bind(id)
        .bind(req.address.to_lowercase())
        .bind(req.goto)
        .fetch_one(&s.pool)
        .await
        .map_err(|_| ErrorResponse(AppError::Validation("alias exists".into())))?;
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
    Ok((StatusCode::CREATED, Json(row)))
}

async fn get_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<AliasRow>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    Ok(Json(fetch_alias(&s, id).await?))
}

async fn update_alias(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAliasReq>,
) -> Result<Json<AliasRow>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let mut r = fetch_alias(&s, id).await?;
    if let Some(v) = req.goto {
        check_alias_targets(&r.address, &v)?;
        r.goto = v;
    }
    if let Some(v) = req.active {
        r.active = v;
    }
    let q = "UPDATE aliases SET goto=$2, active=$3 WHERE id=$1 RETURNING id, address::text, goto, active, created_at";
    let row: AliasRow = sqlx::query_as(q)
        .bind(id)
        .bind(&r.goto)
        .bind(r.active)
        .fetch_one(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
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
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let rows: Vec<(String, i32, i32, f64)> = sqlx::query_as("SELECT email::text, quota_mb, used_mb, CASE WHEN quota_mb>0 THEN (used_mb*100.0/quota_mb) ELSE 0 END FROM users ORDER BY 4 DESC LIMIT 100")
        .fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(email, quota, used, pct)| serde_json::json!({"email": email, "quota_mb": quota, "used_mb": used, "pct": pct})).collect()))
}

async fn queue_status(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    // M-02: filesystem access off the async runtime.
    let qshape_path = std::env::var("POSTFIX_QSHAPE_PATH")
        .unwrap_or_else(|_| "/var/spool/postfix/deferred".to_string());
    let deferred = tokio::task::spawn_blocking(move || {
        std::fs::read_dir(&qshape_path)
            .map(|r| r.count() as i64)
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE created_at > now() - INTERVAL '1 hour'",
    )
    .fetch_optional(&s.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(0);
    synth_common::metrics::set_queue_depth(if deferred > 0 { deferred as u64 } else { 0 });
    Ok(Json(
        serde_json::json!({"queue":{"deferred":deferred,"active":0,"hold":0},"activity_1h": count,
            "postfix_queue_size": deferred,
            "note":"qshape polled every 30s; set POSTFIX_QSHAPE_PATH to spool dir for live count"}),
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
    let domain_row = fetch_domain(&s, domain_id).await?;
    let selector = req.selector.unwrap_or_else(|| "default".to_string());
    if selector.len() > 63
        || !selector
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ErrorResponse(AppError::Validation(
            "invalid selector".into(),
        )));
    }
    let ktype = req.key_type.unwrap_or_else(|| "rsa2048".to_string());
    // M-02: RSA keygen is blocking; run off the runtime thread.
    let selector_for_gen = selector.clone();
    let pair = tokio::task::spawn_blocking(move || synth_dkim::generate(&selector_for_gen, &ktype))
        .await
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;

    let id = Uuid::new_v4();
    // H-07/M-15: encrypt the private key at rest (AES-256-GCM, ENCRYPTION_KEY)
    // and sync the plaintext into the shared opendkim volume so signing works
    // immediately. The DB copy is ciphertext; the volume copy is 0600.
    let private_key_enc =
        synth_common::secrets::encrypt(&s.config.encryption_key, &pair.private_key_pem)
            .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    sqlx::query("INSERT INTO dkim_keys (id, domain_id, selector, public_key, private_key_enc, key_type) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(domain_id).bind(&selector).bind(&pair.public_key_b64).bind(&private_key_enc).bind(&pair.key_type)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("UPDATE domains SET dkim_selector=$2, dkim_enabled=true WHERE id=$1")
        .bind(domain_id)
        .bind(&selector)
        .execute(&s.pool)
        .await
        .ok();
    // Sync to the shared OpenDKIM volume (/var/dkim-sync == /etc/opendkim/keys
    // in the opendkim container). Best-effort: signing failure must not break
    // the API, but log loudly so an operator notices unsigned mail.
    let sync_dir = "/var/dkim-sync";
    if !pair.private_key_pem.is_empty() {
        let key_path = format!("{sync_dir}/{selector}.pem");
        if std::fs::write(&key_path, pair.private_key_pem.as_bytes()).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            let keytable = format!(
                "{}.{} {}:{}:/etc/opendkim/keys/{}.pem\n",
                domain_row.domain, selector, domain_row.domain, selector, selector
            );
            let signingtable = format!(
                "@{} {}._domainkey.{}\n",
                domain_row.domain, selector, domain_row.domain
            );
            let _ = std::fs::write(format!("{sync_dir}/KeyTable"), keytable);
            let _ = std::fs::write(format!("{sync_dir}/SigningTable"), signingtable);
        } else {
            tracing::warn!(domain = %domain_row.domain, selector = %selector, "dkim key write to shared volume failed — mail will be unsigned");
        }
    }

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
        // M-05: use the full domain, not the TLD.
        "dns_record": format!("{}._domainkey TXT {}", selector, pair.dns_txt),
        "full_dns": format!("{}._domainkey.{} TXT \"{}\"", selector, domain_row.domain, pair.dns_txt),
        "keytable_entry": synth_dkim::opendkim_keytable_entry(&domain_row.domain, &pair),
    })))
}

#[allow(clippy::type_complexity)]
async fn dkim_get(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(domain_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let rows: Vec<(Uuid, String, String, String, Option<DateTime<Utc>>)> = sqlx::query_as("SELECT id, selector, public_key, dns_value, revoked_at FROM dkim_keys WHERE domain_id=$1 ORDER BY created_at DESC")
        .bind(domain_id).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(id, sel, pubk, dns, revoked)| serde_json::json!({"id": id, "selector": sel, "public_key": pubk, "dns_value": dns, "revoked": revoked.is_some()})).collect()))
}

// ---------- Filters / Vacation ----------

async fn list_filters(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_or_self(&s, &headers, user_id, false).await?;
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
    require_admin_or_self(&s, &headers, user_id, true).await?;
    for sc in &req.scripts {
        synth_filter::validate_sieve(&sc.content)
            .map_err(|e| ErrorResponse(AppError::Validation(e)))?;
    }
    // M-03: delete+insert atomically so a failed insert cannot wipe filters.
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("DELETE FROM sieve_scripts WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    for sc in &req.scripts {
        sqlx::query("INSERT INTO sieve_scripts (id, user_id, name, content, active) VALUES ($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(user_id).bind(&sc.name).bind(&sc.content).bind(sc.active)
            .execute(&mut *tx).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    }
    tx.commit()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(serde_json::json!({"ok": true})))
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
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    require_admin_or_self(&s, &headers, user_id, false).await?;
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
    require_admin_or_self(&s, &headers, user_id, true).await?;
    let email: String = sqlx::query_scalar("SELECT email::text FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    let sieve = synth_filter::generate_vacation_sieve(&synth_filter::VacationConfig {
        user_id: email,
        enabled: req.enabled,
        subject: req.subject.clone(),
        message: req.message.clone(),
        start: req.start_date.map(|d| d.to_rfc3339()),
        end: req.end_date.map(|d| d.to_rfc3339()),
    });
    // M-03: replace (not duplicate) the generated vacation script atomically.
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("INSERT INTO vacation (user_id, subject, message, enabled, start_date, end_date) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (user_id) DO UPDATE SET subject=$2, message=$3, enabled=$4, start_date=$5, end_date=$6")
        .bind(user_id).bind(&req.subject).bind(&req.message).bind(req.enabled).bind(req.start_date).bind(req.end_date)
        .execute(&mut *tx).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("DELETE FROM sieve_scripts WHERE user_id=$1 AND name='vacation'")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    sqlx::query("INSERT INTO sieve_scripts (id, user_id, name, content, active) VALUES ($1,$2,'vacation',$3,$4)")
        .bind(Uuid::new_v4()).bind(user_id).bind(&sieve).bind(req.enabled).execute(&mut *tx).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    tx.commit()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::redis_invalidate(&s.pool, &s.redis_client)
        .await
        .ok();
    Ok(Json(
        serde_json::json!({"ok": true, "sieve_preview": sieve}),
    ))
}

// ---------- Admin ----------

#[allow(clippy::type_complexity)]
async fn list_audit(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .clamp(1, 500);
    let rows: Vec<(i64, Option<Uuid>, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as("SELECT id, actor, action, target, created_at FROM audit_log ORDER BY created_at DESC LIMIT $1")
        .bind(limit).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(rows.into_iter().map(|(id, actor, action, target, created_at)| serde_json::json!({"id": id, "actor": actor, "action": action, "target": target, "created_at": created_at})).collect()))
}

#[allow(clippy::type_complexity)]
async fn list_bans(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let rows: Vec<(i64, String, Option<String>, bool, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, email::text, ip, success, created_at FROM login_attempts ORDER BY created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let bans: Vec<serde_json::Value> = rows
        .into_iter()
        .filter(|r| !r.3)
        .take(50)
        .map(|(id, email, ip, _success, at)| {
            serde_json::json!({"attempt_id": id, "email": email, "ip": ip, "at": at, "banned": false})
        })
        .collect();
    Ok(Json(bans))
}

// ---------- Config export/import ----------

async fn config_export(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let q = "SELECT id, domain, description, catch_all, dkim_selector, dkim_enabled, spf_value, dmarc_policy, active, created_at FROM domains ORDER BY domain";
    let domains = sqlx::query_as::<_, DomainRow>(q)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let uq = "SELECT id, email::text, kind, quota_mb, used_mb, active, force_password_change, created_at, last_login_at FROM users ORDER BY email";
    let users = sqlx::query_as::<_, UserRow>(uq)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let aq = "SELECT id, address::text, goto, active, created_at FROM aliases ORDER BY address";
    let aliases = sqlx::query_as::<_, AliasRow>(aq)
        .fetch_all(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
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
    #[serde(default)]
    domains: Vec<serde_json::Value>,
    #[serde(default)]
    users: Vec<serde_json::Value>,
}

async fn config_import(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ImportReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    // M-06: transactional, reports created vs skipped honestly. User import is
    // intentionally NOT implemented (no plaintext credential import path); the
    // field is accepted for contract compatibility and reported as skipped.
    let skipped_users = req.users.len();
    let mut created = 0usize;
    let mut skipped = 0usize;
    let mut tx = s
        .pool
        .begin()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    for d in &req.domains {
        if let Some(dom) = d.get("domain").and_then(|v| v.as_str()) {
            let dom = dom.to_lowercase();
            if !is_valid_domain(&dom) {
                skipped += 1;
                continue;
            }
            let res = sqlx::query("INSERT INTO domains (id, domain, description) VALUES ($1,$2,$3) ON CONFLICT (domain) DO NOTHING")
                .bind(Uuid::new_v4()).bind(&dom).bind(d.get("description").and_then(|v| v.as_str()))
                .execute(&mut *tx).await;
            match res {
                Ok(r) if r.rows_affected() > 0 => created += 1,
                _ => skipped += 1,
            }
        } else {
            skipped += 1;
        }
    }
    tx.commit()
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "config_import",
        serde_json::json!({"imported_domains": created, "skipped": skipped, "skipped_users": skipped_users}),
    )
    .await
    .ok();
    Ok(Json(
        serde_json::json!({"imported": created, "skipped": skipped, "skipped_users": skipped_users}),
    ))
}

// ---------- DMARC aggregate report viewer ----------

#[derive(Debug, Deserialize)]
struct DmarcCreate {
    domain: String,
    report_id: String,
    xml: String,
    #[serde(default = "default_report_type")]
    report_type: String,
}
fn default_report_type() -> String {
    "aggregate".to_string()
}

async fn create_dmarc_report(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DmarcCreate>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let _actor = require_admin(&s, &headers).await?;
    // M-13: size limit + full XML parse with entity protection (quick-xml,
    // DTD rejected pre-parse; root must be <feedback>).
    if req.xml.len() > 1_000_000 {
        return Err(ErrorResponse(AppError::Validation(
            "report too large".into(),
        )));
    }
    let (_root, summary) = synth_common::dmarc::parse_dmarc_report(&req.xml).map_err(|e| {
        ErrorResponse(AppError::Validation(format!(
            "invalid DMARC report XML: {e}"
        )))
    })?;
    // Cross-check: the XML report_id must match the API field when present,
    // so callers cannot file a report under another report's id.
    if summary
        .report_id
        .as_deref()
        .is_some_and(|xml_id| xml_id != req.report_id)
    {
        return Err(ErrorResponse(AppError::Validation(
            "report_id does not match XML content".into(),
        )));
    }
    if !matches!(
        req.report_type.as_str(),
        "aggregate" | "failure" | "default"
    ) {
        return Err(ErrorResponse(AppError::Validation(
            "invalid report_type".into(),
        )));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO dmarc_reports (id, domain, report_id, xml, report_type) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (domain, report_id) DO NOTHING")
        .bind(id).bind(req.domain.to_lowercase()).bind(&req.report_id).bind(&req.xml).bind(&req.report_type)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(serde_json::json!({
        "id": id,
        "domain": req.domain,
        "report_id": req.report_id,
        "org_name": summary.org_name,
        "date_begin": summary.date_begin,
        "date_end": summary.date_end,
        "policy_domain": summary.policy_domain,
    })))
}

async fn list_dmarc_reports(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    require_admin_principal(&s, &headers, false).await?;
    let domain = params.get("domain").cloned();
    let rows: Vec<(Uuid, String, String, String, DateTime<Utc>)> = if let Some(d) = domain {
        sqlx::query_as("SELECT id, domain::text, report_id, report_type, created_at FROM dmarc_reports WHERE domain=$1 ORDER BY created_at DESC LIMIT 100")
            .bind(d).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
    } else {
        sqlx::query_as("SELECT id, domain::text, report_id, report_type, created_at FROM dmarc_reports ORDER BY created_at DESC LIMIT 100")
            .fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?
    };
    Ok(Json(rows.into_iter().map(|(id, domain, report_id, report_type, at)| {
        serde_json::json!({"id": id, "domain": domain, "report_id": report_id, "report_type": report_type, "created_at": at})
    }).collect()))
}

// ---------- Push subscriptions (C-04/H-12: authenticated, self-scoped) ----------

#[derive(Debug, Deserialize)]
struct PushSubReq {
    endpoint: String,
    p256dh: String,
    auth: String,
}

async fn create_push_sub(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PushSubReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    if !req.endpoint.starts_with("https://") || req.endpoint.len() > 2048 {
        return Err(ErrorResponse(AppError::Validation(
            "invalid push endpoint".into(),
        )));
    }
    if req.p256dh.is_empty() || req.auth.is_empty() {
        return Err(ErrorResponse(AppError::Validation(
            "p256dh/auth required".into(),
        )));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (user_id, endpoint) DO UPDATE SET p256dh=EXCLUDED.p256dh, auth=EXCLUDED.auth")
        .bind(id).bind(p.user_id).bind(&req.endpoint).bind(&req.p256dh).bind(&req.auth)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(
        serde_json::json!({"id": id, "endpoint": req.endpoint}),
    ))
}

async fn list_push_subs(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, endpoint FROM push_subscriptions WHERE user_id=$1 ORDER BY created_at DESC",
    )
    .bind(p.user_id)
    .fetch_all(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, endpoint)| serde_json::json!({"id": id, "endpoint": endpoint}))
            .collect(),
    ))
}

async fn delete_push_sub(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    sqlx::query("DELETE FROM push_subscriptions WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(p.user_id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(serde_json::json!({"deleted": id})))
}

// ---------- Well-known ----------

async fn autoconfig(State(s): State<AppState>) -> Response {
    // M-04: correct namespace, real domain, XML content type.
    let host = s.config.mail_host.clone();
    let domain = s.config.mail_domain.clone();
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig xmlns="http://autoconfig.thunderbird.net/v1.1">
  <emailProvider id="synthmail">
    <domain>{domain}</domain>
    <displayName>SynthMail</displayName>
    <incomingServer type="imap"><hostname>{host}</hostname><port>993</port><socketType>SSL</socketType><authentication>password</authentication></incomingServer>
    <outgoingServer type="smtp"><hostname>{host}</hostname><port>587</port><socketType>STARTTLS</socketType><authentication>password</authentication></outgoingServer>
  </emailProvider>
</clientConfig>"#
    );
    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/xml; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

async fn mta_sts(State(s): State<AppState>) -> String {
    format!(
        "version: STSv1\nmode: enforce\nmx: {}\nmax_age: 86400\n",
        s.config.mail_host
    )
}

// ---------- 2FA (TOTP) — C-02 rewrite ----------

#[derive(Debug, Deserialize)]
struct TotpEnrollReq {
    password: String,
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Serialize)]
struct TotpEnrollResp {
    secret: String,
    otpauth_url: String,
}

async fn totp_enroll(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TotpEnrollReq>,
) -> Result<Json<TotpEnrollResp>, ErrorResponse> {
    // C-02: target is the authenticated user only — never a client-supplied email.
    let p = authenticate(&s, &headers).await?;
    if !s
        .tight_limiter
        .check(&format!("totp-enroll:{}", p.user_id))
        .await
    {
        return Err(ErrorResponse(AppError::TooManyRequests));
    }
    let row: Option<(String, bool, String, Option<i64>)> = sqlx::query_as(
        "SELECT u.email::text, t.enabled, t.secret, t.last_used_step FROM users u LEFT JOIN user_totp t ON t.user_id=u.id WHERE u.id=$1",
    )
    .bind(p.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (email, already_enabled, current_secret, last_step) =
        row.ok_or_else(|| ErrorResponse(AppError::NotFound("user".into())))?;
    // Password re-auth is required to (re)enroll.
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(p.user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !verify_password(&hash, &req.password) {
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    // If 2FA is already active, changing it also requires a current code.
    // (current_secret may be ciphertext — decrypt before verifying.)
    if already_enabled {
        let code = req
            .code
            .as_deref()
            .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
        let plain = synth_common::secrets::decrypt(&s.config.encryption_key, &current_secret)
            .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
        check_totp_code(&plain, last_step, code)?;
    }
    // Audit N-08: store the new secret as *pending* (encrypted) instead of
    // clobbering the active secret. Activation happens only in totp_verify,
    // so abandoning enrollment cannot disable 2FA or log the user out.
    let secret = synth_common::totp::generate_secret();
    let pending_enc = synth_common::secrets::encrypt(&s.config.encryption_key, &secret)
        .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
    let pending_expiry = Utc::now() + chrono::Duration::minutes(10);
    sqlx::query("INSERT INTO user_totp (user_id, secret, enabled, last_used_step, secret_pending, pending_expires_at) VALUES ($1,$2,false,NULL,$3,$4) ON CONFLICT (user_id) DO UPDATE SET secret_pending=EXCLUDED.secret_pending, pending_expires_at=EXCLUDED.pending_expires_at")
        .bind(p.user_id).bind(pending_enc.clone()).bind(&pending_enc).bind(pending_expiry).execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(p.user_id),
        "totp_enroll",
        serde_json::json!({}),
    )
    .await
    .ok();
    Ok(Json(TotpEnrollResp {
        secret: secret.clone(),
        otpauth_url: synth_common::totp::otpauth_url(&secret, &email, "SynthMail"),
    }))
}

#[derive(Debug, Deserialize)]
struct TotpVerifyReq {
    totp_code: String,
    #[serde(default)]
    password: Option<String>,
}

async fn totp_verify(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TotpVerifyReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    // C-02: authentication required; target is the caller only.
    let p = authenticate(&s, &headers).await?;
    if !s
        .tight_limiter
        .check(&format!("totp-verify:{}", p.user_id))
        .await
    {
        return Err(ErrorResponse(AppError::TooManyRequests));
    }
    if let Some(pw) = &req.password {
        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
            .bind(p.user_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?
            .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
        if !verify_password(&hash, pw) {
            synth_common::metrics::inc_auth_failure();
            return Err(ErrorResponse(AppError::Unauthorized));
        }
    }
    let row: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT u.email::text, t.secret, t.secret_pending, t.last_used_step FROM users u JOIN user_totp t ON t.user_id=u.id WHERE u.id=$1",
    )
    .bind(p.user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    let (email, active_secret, pending_secret, last_step) =
        row.ok_or_else(|| ErrorResponse(AppError::NotFound("2FA not enrolled".into())))?;
    // Audit N-08: verify against the *pending* secret first; fall back to the
    // active secret so the endpoint also confirms already-enabled accounts.
    let decrypt = |encoded: &str| {
        synth_common::secrets::decrypt(&s.config.encryption_key, encoded)
            .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))
    };
    let (candidate_secret, step, activating) = {
        let mut verified: Option<(i64, bool)> = None;
        if !pending_secret.is_empty() {
            let plain = decrypt(&pending_secret)?;
            if let Ok(step) = check_totp_code(&plain, last_step, &req.totp_code) {
                verified = Some((step, true));
            }
        }
        if verified.is_none() && !active_secret.is_empty() {
            let plain = decrypt(&active_secret)?;
            if let Ok(step) = check_totp_code(&plain, last_step, &req.totp_code) {
                verified = Some((step, false));
            }
        }
        match verified {
            Some((step, activating)) => (pending_secret.clone(), step, activating),
            None => return Err(ErrorResponse(AppError::Unauthorized)),
        }
    };
    let _ = candidate_secret;
    if activating {
        // Move pending -> active, clear pending (encrypted at rest in both).
        sqlx::query("UPDATE user_totp SET secret=secret_pending, secret_pending=NULL, pending_expires_at=NULL, enabled=true, verified_at=now(), last_used_step=$2 WHERE user_id=$1")
            .bind(p.user_id).bind(step)
            .execute(&s.pool)
            .await
            .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    } else {
        sqlx::query(
            "UPDATE user_totp SET enabled=true, verified_at=now(), last_used_step=$2 WHERE user_id=$1",
        )
        .bind(p.user_id)
        .bind(step)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    }
    crate::db::audit(
        &s.pool,
        Some(p.user_id),
        "totp_verify",
        serde_json::json!({}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"enabled": true, "email": email})))
}

/// Verify a TOTP code with replay protection (L-01): returns the matched
/// step, rejecting codes at or before `last_used_step`.
fn check_totp_code(
    secret: &str,
    last_used_step: Option<i64>,
    code: &str,
) -> Result<i64, ErrorResponse> {
    let step = synth_common::totp::verify_with_step(secret, code, Utc::now().timestamp() as u64)
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))? as i64;
    if let Some(last) = last_used_step
        && step <= last
    {
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    Ok(step)
}

#[derive(Debug, Deserialize)]
struct TotpDisableReq {
    password: String,
}

async fn totp_disable(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<TotpDisableReq>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let p = authenticate(&s, &headers).await?;
    // Password re-auth required to disable 2FA.
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(p.user_id)
        .fetch_optional(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?
        .ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
    if !verify_password(&hash, &req.password) {
        synth_common::metrics::inc_auth_failure();
        return Err(ErrorResponse(AppError::Unauthorized));
    }
    sqlx::query("UPDATE user_totp SET enabled=false, secret_pending=NULL, pending_expires_at=NULL WHERE user_id=$1")
        .bind(p.user_id)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::bump_token_version(&s.pool, p.user_id).await.ok();
    crate::db::audit(
        &s.pool,
        Some(p.user_id),
        "totp_disable",
        serde_json::json!({}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"enabled": false})))
}

/// Check 2FA on login: if user has enabled TOTP, require a fresh code.
async fn require_totp_or_ok(
    s: &AppState,
    user_id: Uuid,
    _email: &str,
    code: Option<&str>,
) -> Result<bool, ErrorResponse> {
    let row: Option<(bool, String, Option<i64>)> = sqlx::query_as(
        "SELECT t.enabled, t.secret, t.last_used_step FROM user_totp t WHERE t.user_id=$1",
    )
    .bind(user_id)
    .fetch_optional(&s.pool)
    .await
    .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    match row {
        Some((true, secret, last_step)) => {
            let code = code.ok_or_else(|| ErrorResponse(AppError::Unauthorized))?;
            // Audit M-15: secret is encrypted at rest; decrypt before use.
            let plain = synth_common::secrets::decrypt(&s.config.encryption_key, &secret)
                .map_err(|e| ErrorResponse(AppError::Internal(e.to_string())))?;
            let step = check_totp_code(&plain, last_step, code)?;
            sqlx::query("UPDATE user_totp SET last_used_step=$2 WHERE user_id=$1")
                .bind(user_id)
                .bind(step)
                .execute(&s.pool)
                .await
                .map_err(|e| ErrorResponse(AppError::Database(e)))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

// ---------- API tokens (H-01: expiry + scopes enforced) ----------

#[derive(Debug, Deserialize)]
struct CreateTokenReq {
    name: Option<String>,
    scopes: Option<Vec<String>>,
    expires_in_days: Option<i64>,
}

fn generate_api_token_raw() -> String {
    use rand::RngCore;
    let mut b = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut b);
    hex_encode(&b)
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

async fn create_api_token(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateTokenReq>,
) -> Result<(StatusCode, Json<serde_json::Value>), ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let raw = format!("synth_{}", generate_api_token_raw());
    let hash = hash_password(&raw).map_err(|_| ErrorResponse(AppError::Internal("hash".into())))?;
    let prefix = raw[..18].to_string();
    // Default to full-write admin scope for backward compatibility; empty
    // scopes are now denied by has_scope.
    let scopes = req.scopes.unwrap_or_else(|| vec!["admin".to_string()]);
    if scopes.iter().any(|sc| sc.trim().is_empty()) {
        return Err(ErrorResponse(AppError::Validation("invalid scope".into())));
    }
    let id = Uuid::new_v4();
    let expires = match req.expires_in_days {
        Some(d) if !(1..=3650).contains(&d) => {
            return Err(ErrorResponse(AppError::Validation(
                "expires_in_days must be 1..=3650".into(),
            )));
        }
        Some(d) => Some(Utc::now() + chrono::Duration::days(d)),
        None => None,
    };
    sqlx::query("INSERT INTO api_tokens (id, user_id, token_hash, scopes, expires_at, name, token_prefix) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(actor).bind(hash).bind(&scopes).bind(expires).bind(&req.name).bind(&prefix)
        .execute(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "api_token_create",
        serde_json::json!({"id": id, "name": req.name}),
    )
    .await
    .ok();
    Ok((
        StatusCode::CREATED,
        Json(
            serde_json::json!({"id": id, "token": raw, "prefix": prefix, "scopes": scopes, "expires_at": expires}),
        ),
    ))
}

#[allow(clippy::type_complexity)]
async fn list_api_tokens(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    let rows: Vec<(Uuid, Option<String>, Option<String>, Vec<String>, Option<DateTime<Utc>>, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, name, token_prefix, COALESCE(scopes,'{}'::text[]), expires_at, created_at FROM api_tokens WHERE user_id=$1 AND revoked_at IS NULL ORDER BY created_at DESC")
            .bind(actor).fetch_all(&s.pool).await.map_err(|e| ErrorResponse(AppError::Database(e)))?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, prefix, scopes, expires, created)| {
                serde_json::json!({"id": id, "name": name, "prefix": prefix, "scopes": scopes, "expires_at": expires, "created_at": created})
            })
            .collect(),
    ))
}

async fn revoke_api_token(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let actor = require_admin(&s, &headers).await?;
    sqlx::query("UPDATE api_tokens SET revoked_at=now() WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(actor)
        .execute(&s.pool)
        .await
        .map_err(|e| ErrorResponse(AppError::Database(e)))?;
    crate::db::audit(
        &s.pool,
        Some(actor),
        "api_token_revoke",
        serde_json::json!({"id": id}),
    )
    .await
    .ok();
    Ok(Json(serde_json::json!({"revoked": id})))
}

/// Validate API token from `X-Api-Key` or `Authorization: Bearer <synth_...>`.
/// Enforces expiry (H-01) and returns (user_id, scopes).
pub async fn authenticate_api_key(
    s: &AppState,
    headers: &HeaderMap,
) -> Option<(Uuid, Vec<String>)> {
    let key = headers
        .get("x-api-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).trim().to_string())?;
    if !key.starts_with("synth_") {
        return None;
    }
    let prefix = &key[..18.min(key.len())];
    let rows: Vec<(Uuid, Option<Uuid>, String, Vec<String>)> = sqlx::query_as(
        "SELECT id, user_id, token_hash, COALESCE(scopes,'{}'::text[]) FROM api_tokens WHERE token_prefix=$1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(prefix)
    .fetch_all(&s.pool)
    .await
    .ok()?;
    for (id, user_id, hash, scopes) in rows {
        if verify_password(&hash, &key) {
            let _ = sqlx::query("UPDATE api_tokens SET last_used_at=now() WHERE id=$1")
                .bind(id)
                .execute(&s.pool)
                .await;
            if let Some(uid) = user_id {
                return Some((uid, scopes));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_shape() {
        let v = serde_json::json!({"domains": []});
        assert_eq!(v["domains"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn token_prefix_shape() {
        let raw = "synth_abcdef0123456789abcdef01";
        assert!(raw.starts_with("synth_"));
        assert_eq!(raw[..18].len(), 18);
    }
    #[test]
    fn domain_validation() {
        assert!(is_valid_domain("example.com"));
        assert!(is_valid_domain("mail.sub.example.org"));
        assert!(!is_valid_domain("not a domain"));
        assert!(!is_valid_domain(""));
        assert!(!is_valid_domain("-bad.com"));
        assert!(!is_valid_domain("nodot"));
    }
    #[test]
    fn email_validation() {
        assert!(is_valid_email("alice@example.com"));
        assert!(!is_valid_email("alice@"));
        assert!(!is_valid_email("alice@@example.com"));
        assert!(!is_valid_email("@example.com"));
    }
    #[test]
    fn alias_loop_detected() {
        assert!(check_alias_targets("a@example.com", "a@example.com").is_err());
        assert!(check_alias_targets("a@example.com", "b@example.com").is_ok());
    }
    #[test]
    fn dmarc_whitelist() {
        assert!(check_dmarc_policy("reject").is_ok());
        assert!(check_dmarc_policy("whatever").is_err());
    }
}
