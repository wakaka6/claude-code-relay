use relay_openai_to_anthropic::types::{ChatCompletionRequest, ChatMessage, MessageContent};
use relay_openai_to_anthropic::OpenAIToClaudeConverter;

#[test]
fn test_model_passthrough_no_mapping() {
    let request = ChatCompletionRequest {
        model: "gpt-4o".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let claude_request = OpenAIToClaudeConverter::convert_request(request).unwrap();

    assert_eq!(
        claude_request.model, "gpt-4o",
        "Model should be passed through without mapping"
    );
}

#[test]
fn test_claude_model_passthrough() {
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let claude_request = OpenAIToClaudeConverter::convert_request(request).unwrap();

    assert_eq!(claude_request.model, "claude-3-5-sonnet-20241022");
}

#[test]
fn test_arbitrary_model_passthrough() {
    let request = ChatCompletionRequest {
        model: "my-custom-model".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello".to_string()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let claude_request = OpenAIToClaudeConverter::convert_request(request).unwrap();

    assert_eq!(
        claude_request.model, "my-custom-model",
        "Any model name should be passed through as-is"
    );
}

#[test]
fn test_xcode_system_message_preserved() {
    let request = ChatCompletionRequest {
        model: "gpt-4o".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(
                    "You are currently in Xcode working on a project".to_string(),
                ),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let claude_request = OpenAIToClaudeConverter::convert_request(request).unwrap();

    let system_text = claude_request.system.unwrap();
    assert!(
        system_text.as_str().unwrap().contains("Xcode"),
        "Xcode system message should be preserved"
    );
}

#[test]
fn test_non_xcode_gets_claude_code_prompt() {
    let request = ChatCompletionRequest {
        model: "gpt-4o".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text("You are a helpful assistant".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        tools: None,
        tool_choice: None,
        extra: serde_json::Map::new(),
    };

    let claude_request = OpenAIToClaudeConverter::convert_request(request).unwrap();

    let system_text = claude_request.system.unwrap();
    assert!(
        system_text.as_str().unwrap().contains("Claude Code"),
        "Non-Xcode should get Claude Code system prompt"
    );
}
