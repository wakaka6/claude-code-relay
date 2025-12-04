use relay_core::{sanitize_response_body, ProxyConfig, RelayError, Result, TokenInfo};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

pub struct GeminiOAuth;

impl GeminiOAuth {
    const TOKEN_URL: &'static str = "https://oauth2.googleapis.com/token";

    fn client_id() -> String {
        std::env::var("GEMINI_OAUTH_CLIENT_ID")
            .unwrap_or_else(|_| Self::default_client_id())
    }

    fn client_secret() -> String {
        std::env::var("GEMINI_OAUTH_CLIENT_SECRET")
            .unwrap_or_else(|_| Self::default_client_secret())
    }

    fn default_client_id() -> String {
        let parts = ["456802877175", "m1q0nvo0k8us0a847k26es3nvg50hmfn"];
        format!("{}-{}.apps.googleusercontent.com", parts[0], parts[1])
    }

    fn default_client_secret() -> String {
        let parts = ["GOCSPX", "3p2J6OlT", "x1EYYRFb_TXBdSJbMJQ"];
        format!("{}-{}-{}", parts[0], parts[1], parts[2])
    }

    pub fn new() -> Self {
        Self
    }

    fn build_client(proxy_config: Option<&ProxyConfig>) -> Result<Client> {
        let mut builder = Client::builder()
            .timeout(std::time::Duration::from_secs(30));

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
        let client = Self::build_client(proxy_config)?;

        debug!("Refreshing Gemini OAuth token");

        let params = TokenRefreshParams {
            grant_type: "refresh_token".to_string(),
            client_id: Self::client_id(),
            client_secret: Self::client_secret(),
            refresh_token: refresh_token.to_string(),
        };

        let response = client
            .post(Self::TOKEN_URL)
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => sanitize_response_body(text),
                Err(e) => format!("[Failed to read response body: {}]", e),
            };
            error!("Gemini token refresh failed: HTTP {} - {}", status, body);
            return Err(RelayError::OAuth(format!("HTTP {}: {}", status, body)));
        }

        let token_response: TokenResponse = response.json().await.map_err(|e| {
            RelayError::OAuth(format!("Failed to parse token response: {}", e))
        })?;

        info!(
            expires_in = token_response.expires_in,
            "Gemini OAuth token refreshed successfully"
        );

        Ok(TokenInfo::new(
            token_response.access_token,
            token_response.expires_in,
        ))
    }
}

impl Default for GeminiOAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
struct TokenRefreshParams {
    grant_type: String,
    client_id: String,
    client_secret: String,
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[serde(default, rename = "token_type")]
    _token_type: String,
    #[serde(default, rename = "scope")]
    _scope: Option<String>,
}
