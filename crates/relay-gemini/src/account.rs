use async_trait::async_trait;
use parking_lot::RwLock;
use relay_core::{AccountProvider, Credentials, Platform, ProxyConfig, Result, TokenInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::oauth::GeminiOAuth;

pub struct GeminiAccount {
    id: String,
    name: String,
    priority: u32,
    enabled: AtomicBool,
    refresh_token: String,
    api_url: Option<String>,
    proxy: Option<ProxyConfig>,
    token_cache: RwLock<Option<TokenInfo>>,
    oauth: GeminiOAuth,
    unavailable_until: RwLock<Option<Instant>>,
}

impl GeminiAccount {
    pub fn new(
        id: String,
        name: String,
        priority: u32,
        enabled: bool,
        refresh_token: String,
        api_url: Option<String>,
        proxy: Option<ProxyConfig>,
    ) -> Self {
        Self {
            id,
            name,
            priority,
            enabled: AtomicBool::new(enabled),
            refresh_token,
            api_url,
            proxy,
            token_cache: RwLock::new(None),
            oauth: GeminiOAuth::new(),
            unavailable_until: RwLock::new(None),
        }
    }
}

#[async_trait]
impl AccountProvider for GeminiAccount {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn platform(&self) -> Platform {
        Platform::Gemini
    }

    fn priority(&self) -> u32 {
        self.priority
    }

    fn is_available(&self) -> bool {
        if !self.enabled.load(Ordering::Relaxed) {
            return false;
        }

        if let Some(until) = *self.unavailable_until.read() {
            if Instant::now() < until {
                return false;
            }
        }

        true
    }

    async fn get_credentials(&self) -> Result<Credentials> {
        {
            let cache = self.token_cache.read();
            if let Some(ref token) = *cache {
                if token.is_valid() {
                    return Ok(Credentials::Bearer(token.access_token.clone()));
                }
            }
        }

        let new_token = self
            .oauth
            .refresh_token(&self.refresh_token, self.proxy.as_ref())
            .await?;

        {
            let mut cache = self.token_cache.write();
            *cache = Some(new_token.clone());
        }

        Ok(Credentials::Bearer(new_token.access_token))
    }

    fn proxy_config(&self) -> Option<&ProxyConfig> {
        self.proxy.as_ref()
    }

    fn api_url(&self) -> Option<&str> {
        self.api_url.as_deref()
    }

    fn mark_unavailable(&self, duration: Duration, _reason: &str) {
        let mut until = self.unavailable_until.write();
        *until = Some(Instant::now() + duration);
    }

    fn mark_available(&self) {
        let mut until = self.unavailable_until.write();
        *until = None;
    }
}
