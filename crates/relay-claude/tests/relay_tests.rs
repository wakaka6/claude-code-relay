use bytes::Bytes;
use relay_claude::{extract_usage_from_chunk, ClaudeRelay};

#[test]
fn test_beta_header_contains_all_features() {
    let beta_header = ClaudeRelay::beta_header();

    assert!(
        beta_header.contains("claude-code-20250219"),
        "Missing claude-code"
    );
    assert!(beta_header.contains("oauth-2025-04-20"), "Missing oauth");
    assert!(
        beta_header.contains("interleaved-thinking-2025-05-14"),
        "Missing interleaved-thinking"
    );
    assert!(
        beta_header.contains("fine-grained-tool-streaming-2025-05-14"),
        "Missing fine-grained-tool-streaming"
    );
}

#[test]
fn test_haiku_model_uses_minimal_beta() {
    let beta = ClaudeRelay::beta_header_for_model("claude-3-5-haiku-20241022");

    assert!(beta.contains("oauth-2025-04-20"), "Haiku should have oauth");
    assert!(
        beta.contains("interleaved-thinking-2025-05-14"),
        "Haiku should have interleaved-thinking"
    );
    assert!(
        !beta.contains("claude-code-20250219"),
        "Haiku should NOT have claude-code"
    );
    assert!(
        !beta.contains("fine-grained-tool-streaming"),
        "Haiku should NOT have fine-grained-tool-streaming"
    );
}

#[test]
fn test_non_haiku_uses_full_beta() {
    let beta = ClaudeRelay::beta_header_for_model("claude-sonnet-4-20250514");

    assert!(beta.contains("claude-code-20250219"));
    assert!(beta.contains("oauth-2025-04-20"));
    assert!(beta.contains("interleaved-thinking-2025-05-14"));
    assert!(beta.contains("fine-grained-tool-streaming-2025-05-14"));
}

#[test]
fn test_extract_usage_with_cache_tokens() {
    let chunk = Bytes::from(
        r#"data: {"type":"message_delta","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":20,"cache_read_input_tokens":30}}

"#,
    );

    let usage = extract_usage_from_chunk(&chunk).expect("Should extract usage");

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_creation_input_tokens, Some(20));
    assert_eq!(usage.cache_read_input_tokens, Some(30));
}

#[test]
fn test_extract_usage_without_cache_tokens() {
    let chunk = Bytes::from(
        r#"data: {"type":"message_delta","usage":{"input_tokens":100,"output_tokens":50}}

"#,
    );

    let usage = extract_usage_from_chunk(&chunk).expect("Should extract usage");

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_creation_input_tokens, None);
    assert_eq!(usage.cache_read_input_tokens, None);
}
