use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use relay_core::{
    read_error_response_body, AccountProvider, BoxStream, Credentials, ProxyConfig, Relay,
    RelayError, Result,
};
use reqwest::Client;
use tracing::{debug, info, trace, warn};

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

    /// Log detailed request information for debugging
    fn log_request_details(request: &MessagesRequest, account_id: &str, api_url: &str, stream: bool) {
        let message_count = request.messages.len();
        let has_system = request.system.is_some();
        let has_tools = request.tools.as_ref().map(|t| t.len()).unwrap_or(0);
        let has_tool_choice = request.tool_choice.is_some();

        debug!(
            account_id = %account_id,
            model = %request.model,
            api_url = %api_url,
            stream = stream,
            message_count = message_count,
            max_tokens = request.max_tokens,
            has_system = has_system,
            tools_count = has_tools,
            has_tool_choice = has_tool_choice,
            temperature = ?request.temperature,
            top_p = ?request.top_p,
            top_k = ?request.top_k,
            "Preparing Claude API request"
        );

        // Log extra fields if any
        if !request.extra.is_empty() {
            debug!(
                extra_fields = ?request.extra.keys().collect::<Vec<_>>(),
                "Request contains extra fields"
            );
        }

        // Trace level: log each message role and content type
        for (i, msg) in request.messages.iter().enumerate() {
            let content_info = if let Some(arr) = msg.content.as_array() {
                let types: Vec<&str> = arr
                    .iter()
                    .filter_map(|c| c.get("type").and_then(|t| t.as_str()))
                    .collect();
                format!("array[{}]: {:?}", arr.len(), types)
            } else if let Some(s) = msg.content.as_str() {
                format!("string(len={})", s.len())
            } else {
                format!("{:?}", msg.content)
            };

            trace!(
                message_index = i,
                role = %msg.role,
                content = %content_info,
                "Message details"
            );
        }
    }

    /// Log client headers for debugging
    fn log_client_headers(client_headers: &ClientHeaders, account_id: &str) {
        if !client_headers.is_empty() {
            let header_keys: Vec<&String> = client_headers.headers.keys().collect();
            debug!(
                account_id = %account_id,
                header_count = header_keys.len(),
                headers = ?header_keys,
                "Client headers"
            );

            // Trace level: log header values (be careful with sensitive data)
            for (key, value) in client_headers.iter() {
                // Skip potentially sensitive headers
                if key.to_lowercase().contains("auth") || key.to_lowercase().contains("key") {
                    trace!(header = %key, value = "[REDACTED]", "Header value");
                } else {
                    trace!(header = %key, value = %value, "Header value");
                }
            }
        }
    }

    /// Log error with full request details
    fn log_request_error(
        request: &MessagesRequest,
        error: &RelayError,
        account_id: &str,
        api_url: &str,
    ) {
        warn!(
            account_id = %account_id,
            api_url = %api_url,
            model = %request.model,
            message_count = request.messages.len(),
            error = %error,
            "Request failed"
        );

        // Debug level: dump full request body
        if let Ok(request_json) = serde_json::to_string_pretty(&request) {
            debug!(
                account_id = %account_id,
                "Failed request body:\n{}", request_json
            );
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
        let (status, body) = read_error_response_body(response).await;
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
        let auth_type = match &credentials {
            Credentials::Bearer(_) => "Bearer",
            Credentials::ApiKey(_) => "ApiKey",
        };

        // Log detailed request information
        Self::log_request_details(&request, account.id(), &api_url, false);
        Self::log_client_headers(client_headers, account.id());

        debug!(
            account_id = %account.id(),
            auth_type = auth_type,
            anthropic_version = Self::API_VERSION,
            anthropic_beta = Self::beta_header_for_model(&request.model),
            "Sending non-streaming request"
        );

        let mut builder = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json");

        builder = Self::apply_client_headers(builder, client_headers);
        let response = builder.json(&request).send().await?;

        let status = response.status();
        debug!(
            account_id = %account.id(),
            status = %status,
            "Received response"
        );

        if !status.is_success() {
            let error = self.handle_error_response(response).await;
            Self::log_request_error(&request, &error, account.id(), &api_url);
            return Err(error);
        }

        let resp: MessagesResponse = response.json().await?;

        info!(
            account_id = %account.id(),
            input_tokens = resp.usage.input_tokens,
            output_tokens = resp.usage.output_tokens,
            cache_creation = resp.usage.cache_creation_input_tokens,
            cache_read = resp.usage.cache_read_input_tokens,
            stop_reason = ?resp.stop_reason,
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
        let auth_type = match &credentials {
            Credentials::Bearer(_) => "Bearer",
            Credentials::ApiKey(_) => "ApiKey",
        };

        // Log detailed request information
        Self::log_request_details(&request, account.id(), &api_url, true);
        Self::log_client_headers(client_headers, account.id());

        debug!(
            account_id = %account.id(),
            auth_type = auth_type,
            anthropic_version = Self::API_VERSION,
            anthropic_beta = Self::beta_header_for_model(&request.model),
            "Sending streaming request"
        );

        let mut builder = client
            .post(&api_url)
            .header(auth_header_name, auth_header_value)
            .header("anthropic-version", Self::API_VERSION)
            .header("anthropic-beta", Self::beta_header_for_model(&request.model))
            .header("Content-Type", "application/json");

        builder = Self::apply_client_headers(builder, client_headers);
        let response = builder.json(&request).send().await?;

        let status = response.status();
        debug!(
            account_id = %account.id(),
            status = %status,
            "Received streaming response"
        );

        if !status.is_success() {
            let error = self.handle_error_response(response).await;
            Self::log_request_error(&request, &error, account.id(), &api_url);
            return Err(error);
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
        let auth_type = match &credentials {
            Credentials::Bearer(_) => "Bearer",
            Credentials::ApiKey(_) => "ApiKey",
        };

        // Log detailed request information
        Self::log_request_details(&request, account.id(), &api_url, false);

        debug!(
            account_id = %account.id(),
            auth_type = auth_type,
            anthropic_version = Self::API_VERSION,
            anthropic_beta = Self::beta_header_for_model(&request.model),
            "Sending non-streaming request (no client headers)"
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

        let status = response.status();
        debug!(
            account_id = %account.id(),
            status = %status,
            "Received response"
        );

        if !status.is_success() {
            let error = self.handle_error_response(response).await;
            Self::log_request_error(&request, &error, account.id(), &api_url);
            return Err(error);
        }

        let resp: MessagesResponse = response.json().await?;

        info!(
            account_id = %account.id(),
            input_tokens = resp.usage.input_tokens,
            output_tokens = resp.usage.output_tokens,
            cache_creation = resp.usage.cache_creation_input_tokens,
            cache_read = resp.usage.cache_read_input_tokens,
            stop_reason = ?resp.stop_reason,
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
        let auth_type = match &credentials {
            Credentials::Bearer(_) => "Bearer",
            Credentials::ApiKey(_) => "ApiKey",
        };

        // Log detailed request information
        Self::log_request_details(&request, account.id(), &api_url, true);

        debug!(
            account_id = %account.id(),
            auth_type = auth_type,
            anthropic_version = Self::API_VERSION,
            anthropic_beta = Self::beta_header_for_model(&request.model),
            "Sending streaming request (no client headers)"
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

        let status = response.status();
        debug!(
            account_id = %account.id(),
            status = %status,
            "Received streaming response"
        );

        if !status.is_success() {
            let error = self.handle_error_response(response).await;
            Self::log_request_error(&request, &error, account.id(), &api_url);
            return Err(error);
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
