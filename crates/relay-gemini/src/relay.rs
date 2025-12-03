use async_stream::try_stream;
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use relay_core::{
    read_error_response_body, AccountProvider, BoxStream, Credentials, ProxyConfig, Relay,
    RelayError, Result,
};
use reqwest::Client;
use tracing::{debug, info};

use crate::types::{GenerateContentRequest, GenerateContentResponse, UsageMetadata};

pub struct GeminiRelay {
    default_client: Client,
}

impl GeminiRelay {
    const DEFAULT_API_BASE: &'static str = "https://cloudcode.googleapis.com/v1";

    pub fn default_api_base() -> &'static str {
        Self::DEFAULT_API_BASE
    }

    pub fn new() -> Self {
        Self {
            default_client: Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("Failed to create HTTP client"),
        }
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

    fn get_api_base(account: &dyn AccountProvider) -> String {
        account
            .api_url()
            .map(|url| {
                let base = url.trim_end_matches('/');
                if base.ends_with("/v1") {
                    base.to_string()
                } else {
                    format!("{}/v1", base)
                }
            })
            .unwrap_or_else(|| Self::DEFAULT_API_BASE.to_string())
    }

    fn build_url(api_base: &str, model: &str, stream: bool) -> String {
        let method = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        format!("{}/models/{}:{}", api_base, model, method)
    }

    async fn handle_error_response(&self, response: reqwest::Response) -> RelayError {
        let (status, body) = read_error_response_body(response).await;
        RelayError::from_response_body(status, &body)
    }
}

impl Default for GeminiRelay {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GeminiRequest {
    pub model: String,
    pub body: GenerateContentRequest,
    pub stream: bool,
}

#[async_trait]
impl Relay for GeminiRelay {
    type Request = GeminiRequest;
    type Response = GenerateContentResponse;

    async fn relay(
        &self,
        account: &dyn AccountProvider,
        request: Self::Request,
    ) -> Result<Self::Response> {
        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;

        let token = match credentials {
            Credentials::Bearer(t) => t,
            Credentials::ApiKey(k) => k,
        };

        let api_base = Self::get_api_base(account);
        let url = Self::build_url(&api_base, &request.model, false);

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %url,
            "Relaying non-streaming request to Gemini API"
        );

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&request.body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let resp: GenerateContentResponse = response.json().await?;

        if let Some(ref usage) = resp.usage_metadata {
            info!(
                account_id = account.id(),
                prompt_tokens = usage.prompt_token_count,
                candidates_tokens = usage.candidates_token_count,
                "Gemini request completed"
            );
        }

        Ok(resp)
    }

    async fn relay_stream(
        &self,
        account: &dyn AccountProvider,
        request: Self::Request,
    ) -> Result<BoxStream<Result<Bytes>>> {
        let credentials = account.get_credentials().await?;
        let client = self.build_client(account.proxy_config())?;

        let token = match credentials {
            Credentials::Bearer(t) => t,
            Credentials::ApiKey(k) => k,
        };

        let api_base = Self::get_api_base(account);
        let url = format!("{}?alt=sse", Self::build_url(&api_base, &request.model, true));

        debug!(
            account_id = account.id(),
            model = request.model,
            api_url = %url,
            "Relaying streaming request to Gemini API"
        );

        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&request.body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let account_id = account.id().to_string();

        let stream = try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut total_usage = UsageMetadata::default();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = chunk_result?;

                if let Some(usage) = extract_usage_from_chunk(&chunk) {
                    total_usage.prompt_token_count = total_usage.prompt_token_count.max(usage.prompt_token_count);
                    total_usage.candidates_token_count = total_usage.candidates_token_count.max(usage.candidates_token_count);
                }

                yield chunk;
            }

            if total_usage.prompt_token_count > 0 || total_usage.candidates_token_count > 0 {
                info!(
                    account_id = account_id,
                    prompt_tokens = total_usage.prompt_token_count,
                    candidates_tokens = total_usage.candidates_token_count,
                    "Gemini streaming request completed"
                );
            }
        };

        Ok(Box::pin(stream))
    }
}

fn extract_usage_from_chunk(chunk: &Bytes) -> Option<UsageMetadata> {
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

        if let Some(usage) = value.get("usageMetadata") {
            let prompt = usage
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let candidates = usage
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            if prompt > 0 || candidates > 0 {
                return Some(UsageMetadata {
                    prompt_token_count: prompt,
                    candidates_token_count: candidates,
                    total_token_count: prompt + candidates,
                });
            }
        }
    }

    None
}
