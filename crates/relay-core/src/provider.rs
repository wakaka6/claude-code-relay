use crate::{Platform, ProxyConfig, Result};
use async_trait::async_trait;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Credentials {
    Bearer(String),
    ApiKey(String),
}

impl Credentials {
    pub fn as_bearer(&self) -> Option<&str> {
        match self {
            Credentials::Bearer(token) => Some(token),
            _ => None,
        }
    }

    pub fn as_api_key(&self) -> Option<&str> {
        match self {
            Credentials::ApiKey(key) => Some(key),
            _ => None,
        }
    }
}

#[async_trait]
pub trait AccountProvider: Send + Sync + 'static {
    fn id(&self) -> &str;

    fn name(&self) -> &str;

    fn platform(&self) -> Platform;

    fn priority(&self) -> u32;

    fn is_available(&self) -> bool;

    async fn get_credentials(&self) -> Result<Credentials>;

    fn proxy_config(&self) -> Option<&ProxyConfig>;

    fn api_url(&self) -> Option<&str> {
        None
    }

    fn mark_unavailable(&self, duration: Duration, reason: &str);

    fn mark_available(&self);
}
