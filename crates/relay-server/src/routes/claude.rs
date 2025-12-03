use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use relay_claude::{ClientHeaders, ClaudeRelay, MessagesRequest};
use relay_core::{Platform, RelayError};
use std::collections::HashSet;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info, warn};

use crate::scheduler::UnifiedScheduler;

pub struct ClaudeRouteState {
    pub scheduler: Arc<UnifiedScheduler>,
    pub relay: Arc<ClaudeRelay>,
}

const CLAUDE_CODE_HEADER_KEYS: &[&str] = &[
    "x-stainless-retry-count",
    "x-stainless-timeout",
    "x-stainless-lang",
    "x-stainless-package-version",
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
    "anthropic-dangerous-direct-browser-access",
    "x-app",
    "user-agent",
    "accept-language",
    "sec-fetch-mode",
    "accept-encoding",
];

const MAX_RETRIES: usize = 3;

fn extract_client_headers(headers: &HeaderMap) -> ClientHeaders {
    let mut client_headers = ClientHeaders::new();

    for key in CLAUDE_CODE_HEADER_KEYS {
        if let Some(value) = headers.get(*key) {
            if let Ok(v) = value.to_str() {
                client_headers.insert(key.to_string(), v.to_string());
            }
        }
    }

    if client_headers.is_empty() {
        return ClientHeaders::with_defaults();
    }

    client_headers
}

fn handle_relay_error(
    error: &RelayError,
    account_id: &str,
    scheduler: &UnifiedScheduler,
) -> bool {
    match error {
        RelayError::RateLimited(retry_after) => {
            scheduler.mark_account_rate_limited(account_id, *retry_after);
            true
        }
        RelayError::Overloaded { retry_after_minutes } => {
            scheduler.mark_account_overloaded(account_id, *retry_after_minutes as u64);
            true
        }
        RelayError::OpusWeeklyLimit => {
            scheduler.mark_account_unavailable(account_id, "opus_weekly_limit");
            true
        }
        RelayError::Unauthorized(_) => {
            scheduler.mark_account_unavailable(account_id, "unauthorized");
            true
        }
        RelayError::OrganizationDisabled(_) => {
            scheduler.mark_account_unavailable(account_id, "organization_disabled");
            true
        }
        RelayError::InsufficientQuota => {
            scheduler.mark_account_unavailable(account_id, "insufficient_quota");
            true
        }
        RelayError::ContentFiltered(_) => {
            false
        }
        _ => false,
    }
}

pub async fn messages(
    State(state): State<Arc<ClaudeRouteState>>,
    headers: HeaderMap,
    Json(request): Json<MessagesRequest>,
) -> Result<Response, AppError> {
    let is_stream = request.stream;
    let model = request.model.clone();

    info!(model = %model, stream = is_stream, "Received Claude messages request");

    let body_value = serde_json::to_value(&request).unwrap_or_default();
    let client_headers = extract_client_headers(&headers);

    let mut excluded_accounts: HashSet<String> = HashSet::new();
    let mut last_error: Option<RelayError> = None;

    for attempt in 0..MAX_RETRIES {
        let account = match state
            .scheduler
            .select_account_excluding(Platform::Claude, &body_value, &excluded_accounts)
        {
            Ok(acc) => acc,
            Err(e) => {
                if let Some(prev_error) = last_error {
                    return Err(AppError(prev_error));
                }
                return Err(AppError(e));
            }
        };

        let account_id = account.id().to_string();

        if attempt > 0 {
            info!(
                account_id = %account_id,
                attempt = attempt + 1,
                "Retrying with different account"
            );
        }

        let result = if is_stream {
            state
                .relay
                .relay_stream_with_headers(account.as_ref(), request.clone(), &client_headers)
                .await
        } else {
            match state
                .relay
                .relay_with_headers(account.as_ref(), request.clone(), &client_headers)
                .await
            {
                Ok(response) => return Ok(Json(response).into_response()),
                Err(e) => Err(e),
            }
        };

        match result {
            Ok(stream) => {
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

                return Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .header("X-Accel-Buffering", "no")
                    .body(body)
                    .unwrap());
            }
            Err(e) => {
                let should_retry = handle_relay_error(&e, &account_id, &state.scheduler);

                if should_retry {
                    warn!(
                        account_id = %account_id,
                        error = %e,
                        attempt = attempt + 1,
                        "Request failed, will try another account"
                    );
                    excluded_accounts.insert(account_id);
                    last_error = Some(e);
                    continue;
                }

                return Err(AppError(e));
            }
        }
    }

    Err(AppError(last_error.unwrap_or(RelayError::NoAccount(Platform::Claude))))
}

pub async fn models() -> impl IntoResponse {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {"id": "claude-sonnet-4-20250514", "object": "model", "created": 1704067200, "owned_by": "anthropic"},
            {"id": "claude-3-5-sonnet-20241022", "object": "model", "created": 1704067200, "owned_by": "anthropic"},
            {"id": "claude-3-5-haiku-20241022", "object": "model", "created": 1704067200, "owned_by": "anthropic"},
            {"id": "claude-3-opus-20240229", "object": "model", "created": 1704067200, "owned_by": "anthropic"},
            {"id": "claude-opus-4-20250514", "object": "model", "created": 1704067200, "owned_by": "anthropic"}
        ]
    }))
}

pub struct AppError(RelayError);

impl From<RelayError> for AppError {
    fn from(err: RelayError) -> Self {
        AppError(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            RelayError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            RelayError::ContentFiltered(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            RelayError::OrganizationDisabled(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            RelayError::RateLimited(retry_after) => (
                StatusCode::TOO_MANY_REQUESTS,
                format!("Rate limited, retry after {} seconds", retry_after),
            ),
            RelayError::NoAccount(platform) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("No available account for {:?}", platform),
            ),
            RelayError::Upstream { status, message } => (
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
                message.clone(),
            ),
            e => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        error!(error = %self.0, "Request error");

        let body = serde_json::json!({
            "error": {
                "type": "api_error",
                "message": message
            }
        });

        (status, Json(body)).into_response()
    }
}
