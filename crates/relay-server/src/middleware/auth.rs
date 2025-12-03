use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

#[derive(Clone)]
pub struct ApiKeyValidator {
    valid_keys: HashSet<String>,
}

impl ApiKeyValidator {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            valid_keys: keys.into_iter().collect(),
        }
    }

    pub fn validate(&self, key: &str) -> bool {
        self.valid_keys.contains(key)
    }

    pub fn is_empty(&self) -> bool {
        self.valid_keys.is_empty()
    }
}

pub async fn auth_middleware(
    State(validator): State<Arc<ApiKeyValidator>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if validator.is_empty() {
        return Ok(next.run(request).await);
    }

    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let api_key = match auth_header {
        Some(h) if h.starts_with("Bearer ") => h.strip_prefix("Bearer ").unwrap(),
        _ => {
            if let Some(key) = request.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
                key
            } else {
                warn!("Missing API key in request");
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    };

    if !validator.validate(api_key) {
        warn!(api_key = %mask_key(api_key), "Invalid API key");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}
