use relay_codex::CodexAccount;
use relay_core::{AccountProvider, Platform};

#[test]
fn test_codex_account_creation() {
    let account = CodexAccount::new(
        "codex-1".to_string(),
        "Test Codex Account".to_string(),
        100,
        true,
        "sk-test-api-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        None,
    );

    assert_eq!(account.id(), "codex-1");
    assert_eq!(account.name(), "Test Codex Account");
    assert_eq!(account.platform(), Platform::Codex);
    assert_eq!(account.priority(), 100);
    assert!(account.is_available());
}

#[test]
fn test_codex_account_api_url() {
    let account = CodexAccount::new(
        "codex-1".to_string(),
        "Test".to_string(),
        100,
        true,
        "sk-test".to_string(),
        Some("https://custom.api.com/v1".to_string()),
        None,
    );

    assert_eq!(account.api_url(), Some("https://custom.api.com/v1"));
}

#[tokio::test]
async fn test_codex_account_credentials() {
    let account = CodexAccount::new(
        "codex-1".to_string(),
        "Test".to_string(),
        100,
        true,
        "sk-test-key-123".to_string(),
        None,
        None,
    );

    let creds = account.get_credentials().await.unwrap();
    match creds {
        relay_core::Credentials::ApiKey(key) => assert_eq!(key, "sk-test-key-123"),
        _ => panic!("Expected ApiKey credentials"),
    }
}
