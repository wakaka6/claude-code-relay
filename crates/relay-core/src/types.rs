use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Claude,
    Gemini,
    OpenAI,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Claude => write!(f, "claude"),
            Platform::Gemini => write!(f, "gemini"),
            Platform::OpenAI => write!(f, "openai"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProxyConfig {
    Socks5 {
        host: String,
        port: u16,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    Http {
        host: String,
        port: u16,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    None,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        ProxyConfig::None
    }
}

impl ProxyConfig {
    pub fn is_none(&self) -> bool {
        matches!(self, ProxyConfig::None)
    }

    pub fn to_url(&self) -> Option<String> {
        match self {
            ProxyConfig::Socks5 {
                host,
                port,
                username,
                password,
            } => {
                if let (Some(user), Some(pass)) = (username, password) {
                    Some(format!("socks5://{}:{}@{}:{}", user, pass, host, port))
                } else {
                    Some(format!("socks5://{}:{}", host, port))
                }
            }
            ProxyConfig::Http {
                host,
                port,
                username,
                password,
            } => {
                if let (Some(user), Some(pass)) = (username, password) {
                    Some(format!("http://{}:{}@{}:{}", user, pass, host, port))
                } else {
                    Some(format!("http://{}:{}", host, port))
                }
            }
            ProxyConfig::None => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub access_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl TokenInfo {
    pub fn new(access_token: String, expires_in_secs: u64) -> Self {
        Self {
            access_token,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(expires_in_secs as i64),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.expires_at > chrono::Utc::now() + chrono::Duration::seconds(10)
    }

    pub fn is_expired(&self) -> bool {
        !self.is_valid()
    }

    pub fn expires_in(&self) -> Duration {
        let now = chrono::Utc::now();
        if self.expires_at > now {
            (self.expires_at - now).to_std().unwrap_or(Duration::ZERO)
        } else {
            Duration::ZERO
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageData {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

impl UsageData {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}
