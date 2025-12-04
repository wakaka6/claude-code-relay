use relay_claude::{Message, MessagesRequest, MessagesResponse};
use relay_core::RelayError;

use crate::types::*;

pub struct OpenAIToClaudeConverter;

const CLAUDE_CODE_SYSTEM_PROMPT: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

impl OpenAIToClaudeConverter {
    pub fn convert_request(req: ChatCompletionRequest) -> Result<MessagesRequest, RelayError> {
        let mut system: Option<serde_json::Value> = None;
        let mut messages: Vec<Message> = Vec::new();

        for msg in req.messages {
            match msg.role.as_str() {
                "system" => {
                    let text = match msg.content {
                        MessageContent::Text(t) => t,
                        MessageContent::Parts(parts) => {
                            parts
                                .into_iter()
                                .filter_map(|p| match p {
                                    ContentPart::Text { text } => Some(text),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    };

                    if text.contains("You are currently in Xcode") {
                        system = Some(serde_json::json!(text));
                    } else {
                        system = Some(serde_json::json!(CLAUDE_CODE_SYSTEM_PROMPT));
                    }
                }
                "user" | "assistant" => {
                    let content = Self::convert_content(msg.content, msg.tool_calls)?;
                    messages.push(Message {
                        role: msg.role,
                        content,
                    });
                }
                "tool" => {
                    let tool_result = serde_json::json!([{
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.unwrap_or_default(),
                        "content": match msg.content {
                            MessageContent::Text(t) => t,
                            MessageContent::Parts(_) => "".to_string(),
                        }
                    }]);
                    messages.push(Message {
                        role: "user".to_string(),
                        content: tool_result,
                    });
                }
                _ => {}
            }
        }

        let tools = req.tools.map(|tools| {
            tools
                .into_iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.function.name,
                        "description": t.function.description,
                        "input_schema": t.function.parameters.unwrap_or(serde_json::json!({"type": "object", "properties": {}}))
                    })
                })
                .collect()
        });

        Ok(MessagesRequest {
            model: req.model.clone(),
            messages,
            max_tokens: req.max_tokens.unwrap_or(4096),
            stream: req.stream,
            system,
            temperature: req.temperature,
            top_p: req.top_p,
            top_k: None,
            metadata: None,
            tools,
            tool_choice: req.tool_choice,
            extra: serde_json::Map::new(),
        })
    }

    fn convert_content(
        content: MessageContent,
        tool_calls: Option<Vec<ToolCall>>,
    ) -> Result<serde_json::Value, RelayError> {
        let mut blocks: Vec<serde_json::Value> = Vec::new();

        match content {
            MessageContent::Text(text) => {
                if !text.is_empty() {
                    blocks.push(serde_json::json!({
                        "type": "text",
                        "text": text
                    }));
                }
            }
            MessageContent::Parts(parts) => {
                for part in parts {
                    match part {
                        ContentPart::Text { text } => {
                            blocks.push(serde_json::json!({
                                "type": "text",
                                "text": text
                            }));
                        }
                        ContentPart::ImageUrl { image_url } => {
                            if image_url.url.starts_with("data:") {
                                if let Some((media_type, data)) =
                                    Self::parse_data_url(&image_url.url)
                                {
                                    blocks.push(serde_json::json!({
                                        "type": "image",
                                        "source": {
                                            "type": "base64",
                                            "media_type": media_type,
                                            "data": data
                                        }
                                    }));
                                }
                            } else {
                                blocks.push(serde_json::json!({
                                    "type": "image",
                                    "source": {
                                        "type": "url",
                                        "url": image_url.url
                                    }
                                }));
                            }
                        }
                    }
                }
            }
        }

        if let Some(calls) = tool_calls {
            for call in calls {
                let args: serde_json::Value =
                    serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::json!({}));
                blocks.push(serde_json::json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.function.name,
                    "input": args
                }));
            }
        }

        if blocks.len() == 1 {
            if let Some(text) = blocks[0].get("text") {
                return Ok(text.clone());
            }
        }

        Ok(serde_json::Value::Array(blocks))
    }

    fn parse_data_url(url: &str) -> Option<(String, String)> {
        let url = url.strip_prefix("data:")?;
        let (metadata, data) = url.split_once(',')?;
        let media_type = metadata.split(';').next()?;
        Some((media_type.to_string(), data.to_string()))
    }

    pub fn convert_response(resp: MessagesResponse) -> ChatCompletionResponse {
        let mut content: Option<String> = None;
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        // Handle content as serde_json::Value for full passthrough compatibility
        if let Some(blocks) = resp.content.as_array() {
            for block in blocks {
                if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                content = Some(text.to_string());
                            }
                        }
                        "tool_use" => {
                            let id = block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let name = block
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                            tool_calls.push(ToolCall {
                                id,
                                call_type: "function".to_string(),
                                function: FunctionCall {
                                    name,
                                    arguments: serde_json::to_string(&input).unwrap_or_default(),
                                },
                            });
                        }
                        _ => {} // Ignore other content types (thinking, etc.)
                    }
                }
            }
        }

        let finish_reason = resp.stop_reason.as_deref().map(|r| match r {
            "end_turn" => "stop",
            "max_tokens" => "length",
            "tool_use" => "tool_calls",
            "stop_sequence" => "stop",
            _ => "stop",
        });

        ChatCompletionResponse {
            id: resp.id,
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model: resp.model,
            choices: vec![Choice {
                index: 0,
                message: ResponseMessage {
                    role: "assistant".to_string(),
                    content,
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                },
                finish_reason: finish_reason.map(|s| s.to_string()),
            }],
            usage: Some(Usage {
                prompt_tokens: resp.usage.input_tokens,
                completion_tokens: resp.usage.output_tokens,
                total_tokens: resp.usage.total_tokens(),
            }),
        }
    }
}
