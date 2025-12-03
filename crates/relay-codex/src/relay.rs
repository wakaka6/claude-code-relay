use async_stream::try_stream;
use bytes::Bytes;
use futures::StreamExt;
use relay_core::{
    read_error_response_body, AccountProvider, BoxStream, ProxyConfig, RelayError, Result,
};
use reqwest::Client;
use tracing::{debug, info};

use crate::types::{ResponsesRequest, ResponsesResponse};

const DEFAULT_API_URL: &str = "https://api.openai.com/v1";

pub struct CodexRelay {
    default_client: Client,
}

impl CodexRelay {
    pub fn new() -> Self {
        Self {
            default_client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub fn default_api_url(&self) -> &'static str {
        DEFAULT_API_URL
    }

    pub fn build_url(&self, custom_url: Option<&str>, path: &str) -> String {
        let base = custom_url.unwrap_or(DEFAULT_API_URL);
        let base = base.trim_end_matches('/');
        format!("{}{}", base, path)
    }

    fn build_client(&self, proxy_config: Option<&ProxyConfig>) -> Result<Client> {
        if proxy_config.is_none() || proxy_config.map(|p| p.is_none()).unwrap_or(true) {
            return Ok(self.default_client.clone());
        }

        let proxy = proxy_config.unwrap();
        let mut builder = Client::builder().timeout(std::time::Duration::from_secs(600));

        if let Some(proxy_url) = proxy.to_url() {
            let proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|e| RelayError::Config(format!("Invalid proxy URL: {}", e)))?;
            builder = builder.proxy(proxy);
        }

        builder
            .build()
            .map_err(|e| RelayError::Config(format!("Failed to build HTTP client: {}", e)))
    }

    pub async fn relay(
        &self,
        account: &dyn AccountProvider,
        request: ResponsesRequest,
        path: &str,
    ) -> Result<ResponsesResponse> {
        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let api_url = self.build_url(account.api_url(), path);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying non-streaming Codex request"
        );

        let api_key = credentials.as_api_key().ok_or_else(|| {
            RelayError::Unauthorized("Expected API key credentials".to_string())
        })?;

        let response = client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let (status, body) = read_error_response_body(response).await;
            return Err(RelayError::from_response_body(status, &body));
        }

        let resp: ResponsesResponse = response.json().await?;

        info!(
            account_id = account.id(),
            response_id = resp.id,
            "Codex request completed"
        );

        Ok(resp)
    }

    pub async fn relay_stream(
        &self,
        account: &dyn AccountProvider,
        mut request: ResponsesRequest,
        path: &str,
    ) -> Result<BoxStream<Result<Bytes>>> {
        request.stream = true;

        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let api_url = self.build_url(account.api_url(), path);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying streaming Codex request"
        );

        let api_key = credentials.as_api_key().ok_or_else(|| {
            RelayError::Unauthorized("Expected API key credentials".to_string())
        })?;

        let response = client
            .post(&api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let (status, body) = read_error_response_body(response).await;
            return Err(RelayError::from_response_body(status, &body));
        }

        let account_id = account.id().to_string();

        let stream = try_stream! {
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result?;
                yield chunk;
            }

            info!(
                account_id = account_id,
                "Codex streaming request completed"
            );
        };

        Ok(Box::pin(stream))
    }
}

impl Default for CodexRelay {
    fn default() -> Self {
        Self::new()
    }
}
