use async_trait::async_trait;
use parking_lot::RwLock;
use relay_core::{AccountProvider, Credentials, Platform, ProxyConfig, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct ClaudeApiAccount {
    id: String,
    name: String,
    priority: u32,
    enabled: AtomicBool,
    api_key: String,
    api_url: Option<String>,
    proxy: Option<ProxyConfig>,
    unavailable_until: RwLock<Option<Instant>>,
}

impl ClaudeApiAccount {
    pub fn new(
        id: String,
        name: String,
        priority: u32,
        enabled: bool,
        api_key: String,
        api_url: Option<String>,
        proxy: Option<ProxyConfig>,
    ) -> Self {
        Self {
            id,
            name,
            priority,
            enabled: AtomicBool::new(enabled),
            api_key,
            api_url,
            proxy,
            unavailable_until: RwLock::new(None),
        }
    }
}

#[async_trait]
impl AccountProvider for ClaudeApiAccount {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn platform(&self) -> Platform {
        Platform::Claude
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
        Ok(Credentials::ApiKey(self.api_key.clone()))
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
