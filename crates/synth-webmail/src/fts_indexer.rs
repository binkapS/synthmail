//! Full-text index worker: periodically indexes mail_search rows.
//! IMAP IDLE sync in full version; here we recompute tsvector from subject+body.

use sqlx::postgres::PgPool;

pub async fn reindex_batch(pool: &PgPool, limit: i64) -> anyhow::Result<usize> {
    let rows = sqlx::query("UPDATE mail_search SET body_tsv = to_tsvector('simple', COALESCE(subject,'') || ' ' || COALESCE(body,'')) WHERE id IN (SELECT id FROM mail_search WHERE body IS NOT NULL ORDER BY received_at DESC LIMIT $1)")
        .bind(limit).execute(pool).await?;
    Ok(rows.rows_affected() as usize)
}

/// Tenant-scoped full-text search (audit N-04): every result is restricted
/// to `user_id`. The public HTTP route additionally authenticates.
#[allow(dead_code)] // reserved for a future FTS-backed /api/v1/mail/search
pub async fn search(
    pool: &PgPool,
    user_id: uuid::Uuid,
    q: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let rows: Vec<(String, Option<String>, Option<String>, f32)> = sqlx::query_as(
        "SELECT subject, COALESCE(snippet, ''), folder, ts_rank(body_tsv, plainto_tsquery('simple', $2)) AS rank FROM mail_search WHERE user_id=$1 AND body_tsv @@ plainto_tsquery('simple', $2) AND (folder IS NULL OR folder <> '') ORDER BY rank DESC LIMIT 50",
    )
    .bind(user_id)
    .bind(q)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(subject, snippet, folder, rank)| {
            serde_json::json!({"subject": subject, "snippet": snippet, "folder": folder, "rank": rank})
        })
        .collect())
}

#[allow(dead_code)] // helper for FTS snippet display; not yet wired to a route
pub fn build_snippet(body: &str) -> String {
    body.chars()
        .take(280)
        .collect::<String>()
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snippet_truncates() {
        let s = build_snippet("a\nb");
        assert_eq!(s, "a b");
        assert!(build_snippet(&"x".repeat(1000)).len() <= 280);
    }
}
