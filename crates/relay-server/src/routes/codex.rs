use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use relay_codex::{CodexRelay, ResponsesRequest};
use relay_core::{Platform, RelayError};
use std::collections::HashSet;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info, warn};

use super::claude::AppError;
use crate::db::DbPool;
use crate::scheduler::UnifiedScheduler;

pub struct CodexRouteState {
    pub scheduler: Arc<UnifiedScheduler>,
    pub relay: Arc<CodexRelay>,
    #[allow(dead_code)] // Reserved for future usage tracking when Codex API exposes token counts
    pub db_pool: DbPool,
}

const MAX_RETRIES: usize = 3;

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
        RelayError::Unauthorized(_) => {
            scheduler.mark_account_unavailable(account_id, "unauthorized");
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

pub async fn responses(
    State(state): State<Arc<CodexRouteState>>,
    _headers: HeaderMap,
    Json(request): Json<ResponsesRequest>,
) -> Result<Response, AppError> {
    let is_stream = request.stream;
    let model = request.model.clone();

    info!(model = %model, stream = is_stream, "Received OpenAI Responses request");

    let body_value = serde_json::to_value(&request).unwrap_or_default();

    let mut excluded_accounts: HashSet<String> = HashSet::new();
    let mut last_error: Option<RelayError> = None;

    for attempt in 0..MAX_RETRIES {
        let account = match state
            .scheduler
            .select_account_excluding(Platform::Codex, &body_value, &excluded_accounts)
            .await
        {
            Ok(acc) => acc,
            Err(e) => {
                if let Some(prev_error) = last_error {
                    return Err(AppError::from(prev_error));
                }
                return Err(AppError::from(e));
            }
        };

        let account_id = account.id().to_string();

        if attempt > 0 {
            info!(
                account_id = %account_id,
                attempt = attempt + 1,
                "Retrying Codex request with different account"
            );
        }

        let result = if is_stream {
            state
                .relay
                .relay_stream(account.as_ref(), request.clone(), "/responses")
                .await
        } else {
            match state
                .relay
                .relay(account.as_ref(), request.clone(), "/responses")
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
                                error!(error = %e, "Codex stream error");
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
                        "Codex request failed, will try another account"
                    );
                    excluded_accounts.insert(account_id);
                    last_error = Some(e);
                    continue;
                }

                return Err(AppError::from(e));
            }
        }
    }

    Err(AppError::from(last_error.unwrap_or(RelayError::NoAccount(Platform::Codex))))
}
