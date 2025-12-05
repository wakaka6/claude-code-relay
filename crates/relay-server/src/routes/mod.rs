pub mod claude;
pub mod codex;
pub mod gemini;
pub mod openai;

pub use claude::ClaudeRouteState;
pub use codex::CodexRouteState;
pub use gemini::GeminiRouteState;
pub use openai::OpenAIRouteState;

use crate::db::{self, DbPool};
use crate::middleware::ClientApiKeyHash;

pub async fn record_usage_if_valid(
    pool: &DbPool,
    api_key_hash: &ClientApiKeyHash,
    account_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation: u32,
    cache_read: u32,
) {
    if input_tokens == 0 && output_tokens == 0 {
        return;
    }
    if let Err(e) = db::record_usage(
        pool,
        &api_key_hash.0,
        account_id,
        model,
        input_tokens,
        output_tokens,
        cache_creation,
        cache_read,
    )
    .await
    {
        tracing::error!(error = %e, "Failed to record usage");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_database;

    async fn setup_test_db() -> DbPool {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let path_str = path.to_str().unwrap().to_string();
        std::mem::forget(dir);
        init_database(&path_str).await.unwrap()
    }

    #[tokio::test]
    async fn test_record_usage_skips_zero_tokens() {
        let pool = setup_test_db().await;
        let api_key_hash = ClientApiKeyHash::from_api_key("test-key");

        record_usage_if_valid(&pool, &api_key_hash, "acc1", "model", 0, 0, 0, 0).await;

        let usage = db::get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.total_requests, 0);
    }

    #[tokio::test]
    async fn test_record_usage_with_input_only() {
        let pool = setup_test_db().await;
        let api_key_hash = ClientApiKeyHash::from_api_key("test-key");

        record_usage_if_valid(&pool, &api_key_hash, "acc1", "model", 100, 0, 0, 0).await;

        let usage = db::get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.total_requests, 1);
        assert_eq!(usage.total_input, 100);
        assert_eq!(usage.total_output, 0);
    }

    #[tokio::test]
    async fn test_record_usage_with_output_only() {
        let pool = setup_test_db().await;
        let api_key_hash = ClientApiKeyHash::from_api_key("test-key");

        record_usage_if_valid(&pool, &api_key_hash, "acc1", "model", 0, 50, 0, 0).await;

        let usage = db::get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.total_requests, 1);
        assert_eq!(usage.total_input, 0);
        assert_eq!(usage.total_output, 50);
    }

    #[tokio::test]
    async fn test_record_usage_with_cache_tokens() {
        let pool = setup_test_db().await;
        let api_key_hash = ClientApiKeyHash::from_api_key("test-key");

        record_usage_if_valid(&pool, &api_key_hash, "acc1", "model", 100, 50, 20, 30).await;

        let usage = db::get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.total_requests, 1);
        assert_eq!(usage.total_input, 100);
        assert_eq!(usage.total_output, 50);
    }

    #[tokio::test]
    async fn test_record_usage_anonymous_key() {
        let pool = setup_test_db().await;
        let api_key_hash = ClientApiKeyHash::anonymous();

        record_usage_if_valid(&pool, &api_key_hash, "acc1", "model", 100, 50, 0, 0).await;

        let usage = db::get_usage_by_account(&pool, "acc1", 1).await.unwrap();
        assert_eq!(usage.total_requests, 1);
    }
}
