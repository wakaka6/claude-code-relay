use relay_core::{ProxyConfig, RelayError, Result, TokenInfo};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

pub struct ClaudeOAuth {
    client: Client,
}

impl ClaudeOAuth {
    const TOKEN_URL: &'static str = "https://console.anthropic.com/v1/oauth/token";
    const CLIENT_ID: &'static str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn build_client(&self, proxy_config: Option<&ProxyConfig>) -> Result<Client> {
        let mut builder = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("claude-cli/1.0.56 (external, cli)");

        if let Some(proxy) = proxy_config {
            if let Some(proxy_url) = proxy.to_url() {
                let proxy = reqwest::Proxy::all(&proxy_url)
                    .map_err(|e| RelayError::Config(format!("Invalid proxy URL: {}", e)))?;
                builder = builder.proxy(proxy);
            }
        }

        builder
            .build()
            .map_err(|e| RelayError::Config(format!("Failed to build HTTP client: {}", e)))
    }

    pub async fn refresh_token(
        &self,
        refresh_token: &str,
        proxy_config: Option<&ProxyConfig>,
    ) -> Result<TokenInfo> {
        let client = self.build_client(proxy_config)?;

        debug!("Refreshing Claude OAuth token");

        let request = TokenRequest {
            grant_type: "refresh_token".to_string(),
            client_id: Self::CLIENT_ID.to_string(),
            refresh_token: refresh_token.to_string(),
        };

        let response = client
            .post(Self::TOKEN_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!("Token refresh failed: HTTP {} - {}", status, body);
            return Err(RelayError::OAuth(format!("HTTP {}: {}", status, body)));
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            RelayError::OAuth(format!("Failed to parse token response: {}", e))
        })?;

        info!(
            expires_in = token_response.expires_in,
            "Claude OAuth token refreshed successfully"
        );

        Ok(TokenInfo::new(
            token_response.access_token,
            token_response.expires_in,
        ))
    }
}

impl Default for ClaudeOAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    grant_type: String,
    client_id: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default)]
    token_type: String,
    #[serde(default)]
    scope: Option<String>,
}
