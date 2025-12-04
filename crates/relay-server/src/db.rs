use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;
use tracing::info;

pub type DbPool = Pool<Sqlite>;

pub async fn init_database(path: &str) -> Result<DbPool, sqlx::Error> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let database_url = format!("sqlite:{}?mode=rwc", path);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS usage_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            request_count INTEGER NOT NULL DEFAULT 1,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_usage_account_date
        ON usage_stats(account_id, created_at)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sticky_sessions (
            session_hash TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            expires_at DATETIME NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sticky_expires
        ON sticky_sessions(expires_at)
        "#,
    )
    .execute(&pool)
    .await?;

    info!(database = %path, "Database initialized");

    Ok(pool)
}

#[allow(dead_code)] // Reserved for usage statistics feature
pub async fn record_usage(
    pool: &DbPool,
    account_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO usage_stats
        (account_id, model, input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(account_id)
    .bind(model)
    .bind(input_tokens as i64)
    .bind(output_tokens as i64)
    .bind(cache_creation_tokens as i64)
    .bind(cache_read_tokens as i64)
    .execute(pool)
    .await?;

    Ok(())
}

#[allow(dead_code)] // Reserved for usage statistics feature
#[derive(Debug, sqlx::FromRow)]
pub struct UsageAggregate {
    pub account_id: String,
    pub total_input: i64,
    pub total_output: i64,
    pub total_requests: i64,
}

#[allow(dead_code)] // Reserved for usage statistics feature
pub async fn get_usage_by_account(
    pool: &DbPool,
    account_id: &str,
    days: i32,
) -> Result<UsageAggregate, sqlx::Error> {
    let result = sqlx::query_as::<_, UsageAggregate>(
        r#"
        SELECT
            account_id,
            COALESCE(SUM(input_tokens), 0) as total_input,
            COALESCE(SUM(output_tokens), 0) as total_output,
            COALESCE(SUM(request_count), 0) as total_requests
        FROM usage_stats
        WHERE account_id = ?
        AND created_at >= datetime('now', ? || ' days')
        GROUP BY account_id
        "#,
    )
    .bind(account_id)
    .bind(-days)
    .fetch_optional(pool)
    .await?;

    Ok(result.unwrap_or(UsageAggregate {
        account_id: account_id.to_string(),
        total_input: 0,
        total_output: 0,
        total_requests: 0,
    }))
}

// ============================================================================
// Sticky Session CRUD
// ============================================================================

#[derive(Debug, sqlx::FromRow)]
struct StickySessionRow {
    account_id: String,
    remaining_seconds: i64,
}

/// Get sticky session by hash, returns (account_id, remaining_seconds) if valid
pub async fn get_sticky_session(
    pool: &DbPool,
    session_hash: &str,
) -> Result<Option<(String, i64)>, sqlx::Error> {
    let result = sqlx::query_as::<_, StickySessionRow>(
        r#"
        SELECT
            account_id,
            CAST((julianday(expires_at) - julianday('now')) * 86400 AS INTEGER) as remaining_seconds
        FROM sticky_sessions
        WHERE session_hash = ?
        AND expires_at > datetime('now')
        "#,
    )
    .bind(session_hash)
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|r| (r.account_id, r.remaining_seconds)))
}

/// Create or update sticky session with TTL in seconds
pub async fn upsert_sticky_session(
    pool: &DbPool,
    session_hash: &str,
    account_id: &str,
    ttl_secs: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO sticky_sessions (session_hash, account_id, expires_at)
        VALUES (?, ?, datetime('now', '+' || ? || ' seconds'))
        ON CONFLICT(session_hash) DO UPDATE SET
            account_id = excluded.account_id,
            expires_at = excluded.expires_at
        "#,
    )
    .bind(session_hash)
    .bind(account_id)
    .bind(ttl_secs)
    .execute(pool)
    .await?;

    Ok(())
}

#[allow(dead_code)] // Reserved for session management API
/// Delete sticky session by hash
pub async fn delete_sticky_session(pool: &DbPool, session_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sticky_sessions WHERE session_hash = ?")
        .bind(session_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Cleanup expired sessions, returns number of deleted rows
pub async fn cleanup_expired_sessions(pool: &DbPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM sticky_sessions WHERE expires_at < datetime('now')")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> DbPool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        // Keep the tempdir alive by leaking it (tests are short-lived anyway)
        let path_str = path.to_str().unwrap().to_string();
        std::mem::forget(dir);
        init_database(&path_str).await.unwrap()
    }

    // ========================================================================
    // get_sticky_session tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_sticky_session_not_found() {
        let pool = setup_test_db().await;
        let result = get_sticky_session(&pool, "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_sticky_session_expired() {
        let pool = setup_test_db().await;
        // Insert an expired session
        sqlx::query("INSERT INTO sticky_sessions VALUES (?, ?, datetime('now', '-1 hour'))")
            .bind("expired_hash")
            .bind("account_1")
            .execute(&pool)
            .await
            .unwrap();

        let result = get_sticky_session(&pool, "expired_hash").await.unwrap();
        assert!(result.is_none()); // Expired session should return None
    }

    #[tokio::test]
    async fn test_get_sticky_session_valid() {
        let pool = setup_test_db().await;
        // Insert a valid session (expires in 1 hour)
        sqlx::query("INSERT INTO sticky_sessions VALUES (?, ?, datetime('now', '+1 hour'))")
            .bind("valid_hash")
            .bind("account_1")
            .execute(&pool)
            .await
            .unwrap();

        let result = get_sticky_session(&pool, "valid_hash").await.unwrap();
        assert!(result.is_some());
        let (account_id, remaining_secs) = result.unwrap();
        assert_eq!(account_id, "account_1");
        assert!(
            remaining_secs > 3500 && remaining_secs <= 3600,
            "remaining_secs should be ~3600, got {}",
            remaining_secs
        );
    }

    // ========================================================================
    // upsert_sticky_session tests
    // ========================================================================

    #[tokio::test]
    async fn test_upsert_sticky_session_insert() {
        let pool = setup_test_db().await;

        upsert_sticky_session(&pool, "new_hash", "account_1", 3600)
            .await
            .unwrap();

        let result = get_sticky_session(&pool, "new_hash").await.unwrap();
        assert!(result.is_some());
        let (account_id, remaining) = result.unwrap();
        assert_eq!(account_id, "account_1");
        assert!(remaining > 3590, "remaining should be ~3600, got {}", remaining);
    }

    #[tokio::test]
    async fn test_upsert_sticky_session_update() {
        let pool = setup_test_db().await;

        // First insert
        upsert_sticky_session(&pool, "hash", "account_1", 1800)
            .await
            .unwrap();
        // Update with different account and longer TTL
        upsert_sticky_session(&pool, "hash", "account_2", 3600)
            .await
            .unwrap();

        let result = get_sticky_session(&pool, "hash").await.unwrap().unwrap();
        assert_eq!(result.0, "account_2");
        assert!(result.1 > 3590);
    }

    // ========================================================================
    // delete_sticky_session tests
    // ========================================================================

    #[tokio::test]
    async fn test_delete_sticky_session() {
        let pool = setup_test_db().await;

        upsert_sticky_session(&pool, "hash", "account_1", 3600)
            .await
            .unwrap();
        assert!(get_sticky_session(&pool, "hash").await.unwrap().is_some());

        delete_sticky_session(&pool, "hash").await.unwrap();
        assert!(get_sticky_session(&pool, "hash").await.unwrap().is_none());
    }

    // ========================================================================
    // cleanup_expired_sessions tests
    // ========================================================================

    #[tokio::test]
    async fn test_cleanup_expired_sessions() {
        let pool = setup_test_db().await;

        // Insert expired and valid sessions
        sqlx::query("INSERT INTO sticky_sessions VALUES (?, ?, datetime('now', '-1 hour'))")
            .bind("expired")
            .bind("acc1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sticky_sessions VALUES (?, ?, datetime('now', '+1 hour'))")
            .bind("valid")
            .bind("acc2")
            .execute(&pool)
            .await
            .unwrap();

        let deleted = cleanup_expired_sessions(&pool).await.unwrap();
        assert_eq!(deleted, 1);

        // Verify only valid session remains
        assert!(get_sticky_session(&pool, "expired").await.unwrap().is_none());
        assert!(get_sticky_session(&pool, "valid").await.unwrap().is_some());
    }

    // ========================================================================
    // record_usage tests (existing functionality)
    // ========================================================================

    #[tokio::test]
    async fn test_record_usage() {
        let pool = setup_test_db().await;

        record_usage(&pool, "acc1", "claude-3-opus", 100, 50, 10, 5)
            .await
            .unwrap();

        let usage = get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.account_id, "acc1");
        assert_eq!(usage.total_input, 100);
        assert_eq!(usage.total_output, 50);
        assert_eq!(usage.total_requests, 1);
    }
}
