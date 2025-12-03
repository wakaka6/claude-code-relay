use sqlx::{migrate::MigrationSource, sqlite::SqlitePoolOptions, Pool, Sqlite};
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

#[derive(Debug, sqlx::FromRow)]
pub struct UsageAggregate {
    pub account_id: String,
    pub total_input: i64,
    pub total_output: i64,
    pub total_requests: i64,
}

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
