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
    OpenaiResponses {
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
    #[serde(default = "default_unavailable_cooldown")]
    pub unavailable_cooldown_seconds: u64,
}

fn default_sticky_ttl() -> u64 {
    3600
}

fn default_renewal_threshold() -> u64 {
    300
}

fn default_unavailable_cooldown() -> u64 {
    3600
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            sticky_ttl_seconds: default_sticky_ttl(),
            renewal_threshold_seconds: default_renewal_threshold(),
            unavailable_cooldown_seconds: default_unavailable_cooldown(),
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
                AccountConfig::OpenaiResponses { id, .. } => id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_responses_account_config_parsing() {
        let config_content = r#"
[server]
host = "127.0.0.1"
port = 3000
database_path = "data/relay.db"

[[accounts]]
type = "openai-responses"
id = "codex-1"
name = "Codex Account"
priority = 100
enabled = true
api_key = "sk-test-key"
api_url = "https://api.openai.com/v1"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.accounts.len(), 1);

        match &config.accounts[0] {
            AccountConfig::OpenaiResponses {
                id,
                name,
                priority,
                enabled,
                api_key,
                api_url,
                ..
            } => {
                assert_eq!(id, "codex-1");
                assert_eq!(name, "Codex Account");
                assert_eq!(*priority, 100);
                assert!(*enabled);
                assert_eq!(api_key, "sk-test-key");
                assert_eq!(api_url.as_deref(), Some("https://api.openai.com/v1"));
            }
            _ => panic!("Expected OpenaiResponses account"),
        }
    }

    #[test]
    fn test_session_config_default_values() {
        let config_content = r#"
[server]
host = "127.0.0.1"
port = 3000

[[accounts]]
type = "claude-api"
id = "test-1"
name = "Test Account"
api_key = "sk-test"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.session.sticky_ttl_seconds, 3600);
        assert_eq!(config.session.renewal_threshold_seconds, 300);
        assert_eq!(config.session.unavailable_cooldown_seconds, 3600);
    }

    #[test]
    fn test_session_config_custom_values() {
        let config_content = r#"
[server]
host = "127.0.0.1"
port = 3000

[session]
sticky_ttl_seconds = 7200
renewal_threshold_seconds = 600
unavailable_cooldown_seconds = 1800

[[accounts]]
type = "claude-api"
id = "test-1"
name = "Test Account"
api_key = "sk-test"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        assert_eq!(config.session.sticky_ttl_seconds, 7200);
        assert_eq!(config.session.renewal_threshold_seconds, 600);
        assert_eq!(config.session.unavailable_cooldown_seconds, 1800);
    }

    #[test]
    fn test_session_config_partial_override() {
        let config_content = r#"
[server]
host = "127.0.0.1"
port = 3000

[session]
unavailable_cooldown_seconds = 300

[[accounts]]
type = "claude-api"
id = "test-1"
name = "Test Account"
api_key = "sk-test"
"#;

        let config: Config = toml::from_str(config_content).unwrap();
        // Default values for unspecified fields
        assert_eq!(config.session.sticky_ttl_seconds, 3600);
        assert_eq!(config.session.renewal_threshold_seconds, 300);
        // Custom value
        assert_eq!(config.session.unavailable_cooldown_seconds, 300);
    }
}
