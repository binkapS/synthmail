use sqlx::PgPool;
use synth_common::hash_password;
use uuid::Uuid;

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    // Migrations are embedded via sqlx::migrate! at compile time if ./migrations exists
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| anyhow::anyhow!("migrate: {e}"))?;
    Ok(())
}

pub async fn seed_admin(pool: &PgPool) -> anyhow::Result<()> {
    let email =
        std::env::var("INITIAL_ADMIN_EMAIL").unwrap_or_else(|_| "admin@example.com".to_string());
    let password =
        std::env::var("INITIAL_ADMIN_PASSWORD").unwrap_or_else(|_| "ChangeMe123!Admin".to_string());
    seed_admin_inner(pool, &email, &password).await
}

pub async fn seed_admin_if_empty(pool: &PgPool) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE kind='admin'")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(false);
    }
    // Only auto-seed if env vars present OR in dev default
    let email =
        std::env::var("INITIAL_ADMIN_EMAIL").unwrap_or_else(|_| "admin@example.com".to_string());
    let password =
        std::env::var("INITIAL_ADMIN_PASSWORD").unwrap_or_else(|_| "ChangeMe123!Admin".to_string());
    seed_admin_inner(pool, &email, &password).await?;
    Ok(true)
}

async fn seed_admin_inner(pool: &PgPool, email: &str, password: &str) -> anyhow::Result<()> {
    let hash = hash_password(password).map_err(|e| anyhow::anyhow!("hash: {e}"))?;
    let id = Uuid::new_v4();
    // Ensure domain exists
    let domain = email.split('@').nth(1).unwrap_or("example.com");
    sqlx::query("INSERT INTO domains (id, domain, description) VALUES ($1,$2,$3) ON CONFLICT (domain) DO NOTHING")
        .bind(Uuid::new_v4()).bind(domain).bind("default").execute(pool).await?;
    sqlx::query("INSERT INTO users (id, email, password_hash, kind, quota_mb, active) VALUES ($1,$2,$3,'admin',5120,true) ON CONFLICT (email) DO UPDATE SET password_hash=EXCLUDED.password_hash, active=true")
        .bind(id).bind(email).bind(hash).execute(pool).await?;
    tracing::info!(email, "seeded admin user");
    // audit
    sqlx::query("INSERT INTO audit_log (actor, action, target) VALUES ($1,$2,$3)")
        .bind(id)
        .bind("seed_admin")
        .bind(sqlx::types::Json(serde_json::json!({"email": email})))
        .execute(pool)
        .await?;
    Ok(())
}

/// Audit helper: log an action
pub async fn audit(
    pool: &PgPool,
    actor: Option<Uuid>,
    action: &str,
    target: serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO audit_log (actor, action, target) VALUES ($1,$2,$3)")
        .bind(actor)
        .bind(action)
        .bind(sqlx::types::Json(target))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn redis_invalidate(pool: &PgPool, redis_client: &redis::Client) -> anyhow::Result<()> {
    if let Ok(mut c) = redis_client.get_async_connection().await {
        // Publish cache invalidation event
        let _: Option<String> =
            redis::AsyncCommands::publish(&mut c, "synth:config:invalidate", "1")
                .await
                .ok();
    }
    let _ = pool;
    Ok(())
}
