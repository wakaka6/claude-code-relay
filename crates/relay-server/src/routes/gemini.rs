use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use relay_core::{Platform, Relay, RelayError};
use relay_gemini::{GeminiRelay, GeminiRequest, GenerateContentRequest};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

use super::claude::AppError;
use crate::scheduler::UnifiedScheduler;

pub struct GeminiRouteState {
    pub scheduler: Arc<UnifiedScheduler>,
    pub relay: Arc<GeminiRelay>,
}

fn parse_model_and_method(path: &str) -> Result<(String, String), RelayError> {
    if let Some(colon_pos) = path.rfind(':') {
        let model = path[..colon_pos].to_string();
        let method = path[colon_pos + 1..].to_string();
        Ok((model, method))
    } else {
        Err(RelayError::InvalidRequest(format!(
            "Invalid path format: {}. Expected format: model:method",
            path
        )))
    }
}

pub async fn generate_content(
    State(state): State<Arc<GeminiRouteState>>,
    Path(model_method): Path<String>,
    Json(body): Json<GenerateContentRequest>,
) -> Result<Response, AppError> {
    let (model, method) = parse_model_and_method(&model_method)?;

    info!(model = %model, method = %method, "Received Gemini request");

    let is_stream = method == "streamGenerateContent";

    let body_value = serde_json::to_value(&body).unwrap_or_default();
    let account = state
        .scheduler
        .select_account(Platform::Gemini, &body_value)?;

    let request = GeminiRequest {
        model,
        body,
        stream: is_stream,
    };

    if is_stream {
        let stream = state.relay.relay_stream(account.as_ref(), request).await?;

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

        tokio::spawn(async move {
            let mut stream = stream;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if tx.send(Ok(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Stream error");
                        break;
                    }
                }
            }
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
        let response = state.relay.relay(account.as_ref(), request).await?;
        Ok(Json(response).into_response())
    }
}

pub async fn models() -> impl IntoResponse {
    Json(serde_json::json!({
        "models": [
            {"name": "models/gemini-2.0-flash-exp", "displayName": "Gemini 2.0 Flash"},
            {"name": "models/gemini-1.5-pro", "displayName": "Gemini 1.5 Pro"},
            {"name": "models/gemini-1.5-flash", "displayName": "Gemini 1.5 Flash"}
        ]
    }))
}
