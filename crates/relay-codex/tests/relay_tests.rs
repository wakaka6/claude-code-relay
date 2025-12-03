use relay_codex::{CodexRelay, ResponsesRequest};

#[test]
fn test_codex_relay_creation() {
    let relay = CodexRelay::new();
    assert!(relay.default_api_url().contains("openai.com"));
}

#[test]
fn test_responses_request_serialization() {
    let request = ResponsesRequest {
        model: "gpt-4".to_string(),
        stream: true,
        extra: serde_json::Map::new(),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("\"model\":\"gpt-4\""));
    assert!(json.contains("\"stream\":true"));
}

#[test]
fn test_get_api_url_with_custom_url() {
    let relay = CodexRelay::new();
    let url = relay.build_url(Some("https://custom.api.com/v1"), "/responses");
    assert_eq!(url, "https://custom.api.com/v1/responses");
}

#[test]
fn test_get_api_url_with_default() {
    let relay = CodexRelay::new();
    let url = relay.build_url(None, "/responses");
    assert_eq!(url, "https://api.openai.com/v1/responses");
}

#[test]
fn test_get_api_url_with_trailing_slash() {
    let relay = CodexRelay::new();
    let url = relay.build_url(Some("https://custom.api.com/v1/"), "/responses");
    assert_eq!(url, "https://custom.api.com/v1/responses");
}
