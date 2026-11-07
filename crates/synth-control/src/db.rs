use sqlx::PgPool;
use synth_common::hash_password;
use uuid::Uuid;

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| anyhow::anyhow!("migrate: {e}"))?;
    Ok(())
}

fn random_password() -> String {
    use rand::RngCore;
    let mut b = [0u8; 18];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn initial_admin_password() -> Option<String> {
    std::env::var("INITIAL_ADMIN_PASSWORD")
        .ok()
        .filter(|p| !p.trim().is_empty() && !p.eq_ignore_ascii_case("changeme123!admin"))
}

pub async fn seed_admin(pool: &PgPool) -> anyhow::Result<()> {
    let email =
        std::env::var("INITIAL_ADMIN_EMAIL").unwrap_or_else(|_| "admin@example.com".to_string());
    let generated;
    let password = match initial_admin_password() {
        Some(p) => p,
        None => {
            generated = random_password();
            tracing::error!(
                email = %email,
                password = %generated,
                "INITIAL_ADMIN_PASSWORD not set; generated one-time admin password (store it, then rotate). No default password is used."
            );
            generated
        }
    };
    seed_admin_inner(pool, &email, &password).await
}

pub async fn seed_admin_if_empty(pool: &PgPool) -> anyhow::Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE kind IN ('admin','superadmin')")
            .fetch_one(pool)
            .await?;
    if count > 0 {
        return Ok(false);
    }
    seed_admin(pool).await?;
    Ok(true)
}

async fn seed_admin_inner(pool: &PgPool, email: &str, password: &str) -> anyhow::Result<()> {
    let hash = hash_password(password).map_err(|e| anyhow::anyhow!("hash: {e}"))?;
    let id = Uuid::new_v4();
    let domain = email.split('@').nth(1).unwrap_or("example.com");
    sqlx::query("INSERT INTO domains (id, domain, description) VALUES ($1,$2,$3) ON CONFLICT (domain) DO NOTHING")
        .bind(Uuid::new_v4()).bind(domain).bind("default").execute(pool).await?;
    sqlx::query("INSERT INTO users (id, email, password_hash, kind, quota_mb, active) VALUES ($1,$2,$3,'admin',5120,true) ON CONFLICT (email) DO NOTHING")
        .bind(id).bind(email.to_lowercase()).bind(hash).execute(pool).await?;
    tracing::info!(
        email,
        "seeded admin user (password unchanged if already present)"
    );
    sqlx::query("INSERT INTO audit_log (actor, action, target) VALUES ($1,$2,$3)")
        .bind(id)
        .bind("seed_admin")
        .bind(sqlx::types::Json(serde_json::json!({"email": email})))
        .execute(pool)
        .await?;
    Ok(())
}

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
    if let Ok(mut c) = redis_client.get_multiplexed_async_connection().await {
        let _: Option<String> =
            redis::AsyncCommands::publish(&mut c, "synth:config:invalidate", "1")
                .await
                .ok();
    }
    let _ = pool;
    Ok(())
}

pub async fn bump_token_version(pool: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE users SET token_version = token_version + 1 WHERE id=$1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
