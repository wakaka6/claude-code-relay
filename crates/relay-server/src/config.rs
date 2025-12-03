use relay_core::ProxyConfig;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    #[serde(default)]
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub database_path: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_db_path() -> String {
    "data/relay.db".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database_path: default_db_path(),
            log_level: default_log_level(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AccountConfig {
    ClaudeOauth {
        id: String,
        name: String,
        #[serde(default = "default_priority")]
        priority: u32,
        #[serde(default = "default_enabled")]
        enabled: bool,
        refresh_token: String,
        #[serde(default)]
        api_url: Option<String>,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
    },
    ClaudeApi {
        id: String,
        name: String,
        #[serde(default = "default_priority")]
        priority: u32,
        #[serde(default = "default_enabled")]
        enabled: bool,
        api_key: String,
        #[serde(default)]
        api_url: Option<String>,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
    },
    Gemini {
        id: String,
        name: String,
        #[serde(default = "default_priority")]
        priority: u32,
        #[serde(default = "default_enabled")]
        enabled: bool,
        refresh_token: String,
        #[serde(default)]
        api_url: Option<String>,
        #[serde(default)]
        proxy: Option<ProxyConfig>,
    },
}

fn default_priority() -> u32 {
    100
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_sticky_ttl")]
    pub sticky_ttl_seconds: u64,
    #[serde(default = "default_renewal_threshold")]
    pub renewal_threshold_seconds: u64,
}

fn default_sticky_ttl() -> u64 {
    3600
}

fn default_renewal_threshold() -> u64 {
    300
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            sticky_ttl_seconds: default_sticky_ttl(),
            renewal_threshold_seconds: default_renewal_threshold(),
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io {
            path: path.as_ref().display().to_string(),
            source: e,
        })?;

        let config: Config =
            toml::from_str(&content).map_err(|e| ConfigError::Parse { source: e })?;

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.accounts.is_empty() {
            return Err(ConfigError::Validation(
                "At least one account must be configured".to_string(),
            ));
        }

        let mut ids = std::collections::HashSet::new();
        for account in &self.accounts {
            let id = match account {
                AccountConfig::ClaudeOauth { id, .. } => id,
                AccountConfig::ClaudeApi { id, .. } => id,
                AccountConfig::Gemini { id, .. } => id,
            };
            if !ids.insert(id.clone()) {
                return Err(ConfigError::Validation(format!(
                    "Duplicate account ID: {}",
                    id
                )));
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to parse config: {source}")]
    Parse {
        #[source]
        source: toml::de::Error,
    },
    #[error("Config validation error: {0}")]
    Validation(String),
}
