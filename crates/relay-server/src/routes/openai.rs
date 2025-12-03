use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use relay_claude::ClaudeRelay;
use relay_core::{Platform, Relay};
use relay_openai_to_anthropic::{ChatCompletionRequest, OpenAIToClaudeConverter};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

use super::claude::AppError;
use crate::scheduler::UnifiedScheduler;

pub struct OpenAIRouteState {
    pub scheduler: Arc<UnifiedScheduler>,
    pub relay: Arc<ClaudeRelay>,
}

pub async fn chat_completions(
    State(state): State<Arc<OpenAIRouteState>>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, AppError> {
    let is_stream = request.stream;
    let model = request.model.clone();

    info!(model = %model, stream = is_stream, "Received OpenAI chat/completions request");

    let claude_request = OpenAIToClaudeConverter::convert_request(request)?;
    let body_value = serde_json::to_value(&claude_request).unwrap_or_default();

    let account = state
        .scheduler
        .select_account(Platform::Claude, &body_value)?;

    if is_stream {
        let stream = state
            .relay
            .relay_stream(account.as_ref(), claude_request)
            .await?;

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

        tokio::spawn(async move {
            let mut stream = stream;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            buffer.push_str(text);

                            while let Some(pos) = buffer.find("\n\n") {
                                let line = buffer[..pos].to_string();
                                buffer = buffer[pos + 2..].to_string();

                                if let Some(openai_chunk) = convert_sse_chunk(&line) {
                                    let sse_data =
                                        format!("data: {}\n\n", serde_json::to_string(&openai_chunk).unwrap());
                                    if tx.send(Ok(Bytes::from(sse_data))).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Stream error");
                        break;
                    }
                }
            }

            let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n"))).await;
        });

        let body = Body::from_stream(ReceiverStream::new(rx));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(body)
            .unwrap())
    } else {
        let response = state.relay.relay(account.as_ref(), claude_request).await?;
        let openai_response = OpenAIToClaudeConverter::convert_response(response);
        Ok(Json(openai_response).into_response())
    }
}

fn convert_sse_chunk(line: &str) -> Option<serde_json::Value> {
    if !line.starts_with("data: ") {
        return None;
    }

    let json_str = line.strip_prefix("data: ")?;
    if json_str == "[DONE]" {
        return None;
    }

    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let event_type = value.get("type")?.as_str()?;

    match event_type {
        "content_block_delta" => {
            let delta = value.get("delta")?;
            let text = delta.get("text")?.as_str()?;

            Some(serde_json::json!({
                "id": "chatcmpl-relay",
                "object": "chat.completion.chunk",
                "created": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                "model": "claude",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "content": text
                    },
                    "finish_reason": null
                }]
            }))
        }
        "message_start" => Some(serde_json::json!({
            "id": "chatcmpl-relay",
            "object": "chat.completion.chunk",
            "created": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "model": "claude",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant"
                },
                "finish_reason": null
            }]
        })),
        "message_stop" => Some(serde_json::json!({
            "id": "chatcmpl-relay",
            "object": "chat.completion.chunk",
            "created": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "model": "claude",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": "stop"
            }]
        })),
        _ => None,
    }
}

pub async fn models() -> impl IntoResponse {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {"id": "gpt-4o", "object": "model", "created": 1704067200, "owned_by": "openai"},
            {"id": "gpt-4o-mini", "object": "model", "created": 1704067200, "owned_by": "openai"},
            {"id": "gpt-4-turbo", "object": "model", "created": 1704067200, "owned_by": "openai"},
            {"id": "gpt-3.5-turbo", "object": "model", "created": 1704067200, "owned_by": "openai"}
        ]
    }))
}
