use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};
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

#[derive(Clone, Debug)]
pub struct ClientApiKeyHash(pub String);

impl ClientApiKeyHash {
    pub fn from_api_key(api_key: &str) -> Self {
        Self(hex::encode(Sha256::digest(api_key.as_bytes())))
    }

    pub fn anonymous() -> Self {
        Self("anonymous".to_string())
    }
}

pub async fn auth_middleware(
    State(validator): State<Arc<ApiKeyValidator>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if validator.is_empty() {
        request.extensions_mut().insert(ClientApiKeyHash::anonymous());
        return Ok(next.run(request).await);
    }

    let api_key = {
        let auth_header = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());

        match auth_header {
            Some(h) if h.starts_with("Bearer ") => {
                h.strip_prefix("Bearer ").unwrap().to_string()
            }
            _ => {
                if let Some(key) = request.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
                    key.to_string()
                } else {
                    warn!("Missing API key in request");
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
        }
    };

    if !validator.validate(&api_key) {
        warn!(api_key = %mask_key(&api_key), "Invalid API key");
        return Err(StatusCode::UNAUTHORIZED);
    }

    request
        .extensions_mut()
        .insert(ClientApiKeyHash::from_api_key(&api_key));

    Ok(next.run(request).await)
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..4], &key[key.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_api_key_hash_consistency() {
        let key = "sk-ant-test-key-123456789";
        let hash1 = ClientApiKeyHash::from_api_key(key);
        let hash2 = ClientApiKeyHash::from_api_key(key);
        assert_eq!(hash1.0, hash2.0);
    }

    #[test]
    fn test_client_api_key_hash_uniqueness() {
        let hash1 = ClientApiKeyHash::from_api_key("key1");
        let hash2 = ClientApiKeyHash::from_api_key("key2");
        assert_ne!(hash1.0, hash2.0);
    }

    #[test]
    fn test_client_api_key_hash_format() {
        let hash = ClientApiKeyHash::from_api_key("test-api-key");
        assert_eq!(hash.0.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(hash.0.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_anonymous_hash() {
        let hash = ClientApiKeyHash::anonymous();
        assert_eq!(hash.0, "anonymous");
    }

    #[test]
    fn test_mask_key_short() {
        assert_eq!(mask_key("12345678"), "***");
        assert_eq!(mask_key("1234"), "***");
    }

    #[test]
    fn test_mask_key_long() {
        assert_eq!(mask_key("123456789"), "1234...6789");
        assert_eq!(mask_key("sk-ant-api-key-xxxxx"), "sk-a...xxxx");
    }
}
