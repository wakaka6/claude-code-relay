use crate::types::Platform;

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("OAuth error: {0}")]
    OAuth(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("No available account for platform {0:?}")]
    NoAccount(Platform),

    #[error("Rate limited, retry after {0}s")]
    RateLimited(u64),

    #[error("Upstream API error: {status} - {message}")]
    Upstream { status: u16, message: String },

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Organization disabled: {0}")]
    OrganizationDisabled(String),

    #[error("API overloaded, retry after {retry_after_minutes} minutes")]
    Overloaded { retry_after_minutes: u32 },

    #[error("Opus weekly limit reached")]
    OpusWeeklyLimit,

    #[error("Insufficient balance. Please check your daily limit and total quota.")]
    InsufficientQuota,

    #[error("Database error: {0}")]
    Database(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl RelayError {
    pub fn from_response_body(status: u16, body: &str) -> Self {
        match status {
            401 => RelayError::Unauthorized(body.to_string()),
            402 => RelayError::InsufficientQuota,
            403 if body.contains("organization has been disabled") => {
                RelayError::OrganizationDisabled(body.to_string())
            }
            403 => RelayError::Unauthorized(body.to_string()),
            429 if body.contains("weekly usage limit") && body.to_lowercase().contains("opus") => {
                RelayError::OpusWeeklyLimit
            }
            429 => RelayError::RateLimited(60),
            529 => RelayError::Overloaded {
                retry_after_minutes: 5,
            },
            _ => RelayError::Upstream {
                status,
                message: body.to_string(),
            },
        }
    }

    pub fn to_json_error(&self) -> serde_json::Value {
        match self {
            RelayError::InsufficientQuota => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "402",
                    "type": "insufficient_quota",
                    "message": "Insufficient balance. Please check your daily limit and total quota."
                }
            }),
            RelayError::RateLimited(retry_after) => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "429",
                    "type": "rate_limited",
                    "message": format!("Rate limited. Retry after {} seconds.", retry_after)
                }
            }),
            RelayError::Unauthorized(msg) => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "401",
                    "type": "unauthorized",
                    "message": msg
                }
            }),
            RelayError::OrganizationDisabled(msg) => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "403",
                    "type": "organization_disabled",
                    "message": msg
                }
            }),
            RelayError::OpusWeeklyLimit => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "429",
                    "type": "opus_weekly_limit",
                    "message": "Opus weekly usage limit reached."
                }
            }),
            RelayError::Overloaded { retry_after_minutes } => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "529",
                    "type": "overloaded",
                    "message": format!("API overloaded. Retry after {} minutes.", retry_after_minutes)
                }
            }),
            RelayError::NoAccount(platform) => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "503",
                    "type": "no_available_account",
                    "message": format!("No available account for platform {:?}", platform)
                }
            }),
            _ => serde_json::json!({
                "type": "error",
                "error": {
                    "code": "500",
                    "type": "internal_error",
                    "message": self.to_string()
                }
            }),
        }
    }
}

pub type Result<T> = std::result::Result<T, RelayError>;

impl From<serde_json::Error> for RelayError {
    fn from(e: serde_json::Error) -> Self {
        RelayError::Internal(e.to_string())
    }
}
