use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use relay_core::{AccountProvider, BoxStream, Credentials, ProxyConfig, Relay, RelayError, Result};
use reqwest::Client;
use tracing::{debug, info};

use crate::types::{ClientHeaders, MessagesRequest, MessagesResponse, StreamUsage};

pub struct ClaudeRelay {
    default_client: Client,
}

impl ClaudeRelay {
    const DEFAULT_API_URL: &'static str = "https://api.anthropic.com/v1/messages";
    const API_VERSION: &'static str = "2023-06-01";
    const BETA_HEADER_FULL: &'static str = "claude-code-20250219,oauth-2025-04-20,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14";
    const BETA_HEADER_HAIKU: &'static str = "oauth-2025-04-20,interleaved-thinking-2025-05-14";

    pub fn new() -> Self {
        Self {
            default_client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    pub fn default_api_url() -> &'static str {
        Self::DEFAULT_API_URL
    }

    pub fn beta_header() -> &'static str {
        Self::BETA_HEADER_FULL
    }

    pub fn beta_header_for_model(model: &str) -> &'static str {
        if model.contains("haiku") {
            Self::BETA_HEADER_HAIKU
        } else {
            Self::BETA_HEADER_FULL
        }
    }

    fn get_api_url(account: &dyn AccountProvider) -> String {
        account
            .api_url()
            .map(|url| {
                let base = url.trim_end_matches('/');
                if base.ends_with("/v1/messages") {
                    base.to_string()
                } else if base.ends_with("/v1") {
                    format!("{}/messages", base)
                } else {
                    format!("{}/v1/messages", base)
                }
            })
            .unwrap_or_else(|| Self::DEFAULT_API_URL.to_string())
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

    fn build_auth_header(credentials: &Credentials) -> (&'static str, String) {
        match credentials {
            Credentials::Bearer(token) => ("Authorization", format!("Bearer {}", token)),
            Credentials::ApiKey(key) => ("x-api-key", key.clone()),
        }
    }

    async fn handle_error_response(&self, response: reqwest::Response) -> RelayError {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        RelayError::from_response_body(status, &body)
    }

    fn apply_client_headers(
        builder: reqwest::RequestBuilder,
        client_headers: &ClientHeaders,
    ) -> reqwest::RequestBuilder {
        let mut builder = builder;
        for (key, value) in client_headers.iter() {
            builder = builder.header(key.as_str(), value.as_str());
        }
        builder
    }

    pub async fn relay_with_headers(
        &self,
        account: &dyn AccountProvider,
        request: MessagesRequest,
        client_headers: &ClientHeaders,
    ) -> Result<MessagesResponse> {
        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let (auth_header_name, auth_header_value) = Self::build_auth_header(&credentials);
        let api_url = Self::get_api_url(account);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying non-streaming request to Claude API with client headers"
        );

        let mut builder = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json");

        builder = Self::apply_client_headers(builder, client_headers);
        let response = builder.json(&request).send().await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let resp: MessagesResponse = response.json().await?;

        info!(
            account_id = account.id(),
            input_tokens = resp.usage.input_tokens,
            output_tokens = resp.usage.output_tokens,
            "Claude request completed"
        );

        Ok(resp)
    }

    pub async fn relay_stream_with_headers(
        &self,
        account: &dyn AccountProvider,
        mut request: MessagesRequest,
        client_headers: &ClientHeaders,
    ) -> Result<BoxStream<Result<Bytes>>> {
        request.stream = true;

        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let (auth_header_name, auth_header_value) = Self::build_auth_header(&credentials);
        let api_url = Self::get_api_url(account);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying streaming request to Claude API with client headers"
        );

        let mut builder = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json");

        builder = Self::apply_client_headers(builder, client_headers);
        let response = builder.json(&request).send().await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let account_id = account.id().to_string();

        let stream = try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut total_usage = StreamUsage::default();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result?;

                if let Some(usage) = extract_usage_from_chunk(&chunk) {
                    total_usage.input_tokens = total_usage.input_tokens.max(usage.input_tokens);
                    total_usage.output_tokens = total_usage.output_tokens.max(usage.output_tokens);
                    if usage.cache_creation_input_tokens.is_some() {
                        total_usage.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                    }
                    if usage.cache_read_input_tokens.is_some() {
                        total_usage.cache_read_input_tokens = usage.cache_read_input_tokens;
                    }
                }

                yield chunk;
            }

            if total_usage.input_tokens > 0 || total_usage.output_tokens > 0 {
                info!(
                    account_id = account_id,
                    input_tokens = total_usage.input_tokens,
                    output_tokens = total_usage.output_tokens,
                    cache_creation = total_usage.cache_creation_input_tokens,
                    cache_read = total_usage.cache_read_input_tokens,
                    "Claude streaming request completed"
                );
            }
        };

        Ok(Box::pin(stream))
    }
}

impl Default for ClaudeRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Relay for ClaudeRelay {
    type Request = MessagesRequest;
    type Response = MessagesResponse;

    async fn relay(
        &self,
        account: &dyn AccountProvider,
        request: Self::Request,
    ) -> Result<Self::Response> {
        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let (auth_header_name, auth_header_value) = Self::build_auth_header(&credentials);
        let api_url = Self::get_api_url(account);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying non-streaming request to Claude API"
        );

        let response = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let resp: MessagesResponse = response.json().await?;

        info!(
            account_id = account.id(),
            input_tokens = resp.usage.input_tokens,
            output_tokens = resp.usage.output_tokens,
            "Claude request completed"
        );

        Ok(resp)
    }

    async fn relay_stream(
        &self,
        account: &dyn AccountProvider,
        mut request: Self::Request,
    ) -> Result<BoxStream<Result<Bytes>>> {
        request.stream = true;

        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;
        let (auth_header_name, auth_header_value) = Self::build_auth_header(&credentials);
        let api_url = Self::get_api_url(account);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %api_url,
            "Relaying streaming request to Claude API"
        );

        let response = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let account_id = account.id().to_string();

        let stream = try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut total_usage = StreamUsage::default();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result?;

                if let Some(usage) = extract_usage_from_chunk(&chunk) {
                    total_usage.input_tokens = total_usage.input_tokens.max(usage.input_tokens);
                    total_usage.output_tokens = total_usage.output_tokens.max(usage.output_tokens);
                    if usage.cache_creation_input_tokens.is_some() {
                        total_usage.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                    }
                    if usage.cache_read_input_tokens.is_some() {
                        total_usage.cache_read_input_tokens = usage.cache_read_input_tokens;
                    }
                }

                yield chunk;
            }

            if total_usage.input_tokens > 0 || total_usage.output_tokens > 0 {
                info!(
                    account_id = account_id,
                    input_tokens = total_usage.input_tokens,
                    output_tokens = total_usage.output_tokens,
                    cache_creation = total_usage.cache_creation_input_tokens,
                    cache_read = total_usage.cache_read_input_tokens,
                    "Claude streaming request completed"
                );
            }
        };

        Ok(Box::pin(stream))
    }
}

pub fn extract_usage_from_chunk(chunk: &Bytes) -> Option<StreamUsage> {
    let text = std::str::from_utf8(chunk).ok()?;

    for line in text.lines() {
        if !line.starts_with("data: ") {
            continue;
        }

        let json_str = line.strip_prefix("data: ")?;
        if json_str == "[DONE]" {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(json_str).ok()?;

        if let Some(usage) = value.get("usage") {
            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);

            if input > 0 || output > 0 {
                return Some(StreamUsage {
                    input_tokens: input,
                    output_tokens: output,
                    cache_creation_input_tokens: cache_creation,
                    cache_read_input_tokens: cache_read,
                });
            }
        }

        if let Some(message) = value.get("message") {
            if let Some(usage) = message.get("usage") {
                let input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let cache_creation = usage
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);

                if input > 0 || output > 0 {
                    return Some(StreamUsage {
                        input_tokens: input,
                        output_tokens: output,
                        cache_creation_input_tokens: cache_creation,
                        cache_read_input_tokens: cache_read,
                    });
                }
            }
        }
    }

    None
}
