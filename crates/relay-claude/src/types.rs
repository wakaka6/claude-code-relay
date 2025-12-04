use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub max_tokens: u32,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for MessagesRequest {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![],
            max_tokens: 4096,
            stream: false,
            system: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            tools: None,
            tool_choice: None,
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    /// Content blocks - using serde_json::Value for full passthrough
    /// to avoid losing unknown content types during re-serialization
    pub content: serde_json::Value,
    pub model: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u32>,
}

impl Usage {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientHeaders {
    pub headers: std::collections::HashMap<String, String>,
}

impl ClientHeaders {
    pub fn new() -> Self {
        Self {
            headers: std::collections::HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut headers = std::collections::HashMap::new();
        headers.insert("x-stainless-retry-count".to_string(), "0".to_string());
        headers.insert("x-stainless-timeout".to_string(), "60".to_string());
        headers.insert("x-stainless-lang".to_string(), "js".to_string());
        headers.insert("x-stainless-package-version".to_string(), "0.55.1".to_string());
        headers.insert("x-stainless-os".to_string(), "Linux".to_string());
        headers.insert("x-stainless-arch".to_string(), "x64".to_string());
        headers.insert("x-stainless-runtime".to_string(), "node".to_string());
        headers.insert("x-stainless-runtime-version".to_string(), "v20.19.2".to_string());
        headers.insert("anthropic-dangerous-direct-browser-access".to_string(), "true".to_string());
        headers.insert("x-app".to_string(), "cli".to_string());
        headers.insert("user-agent".to_string(), "claude-cli/1.0.57 (external, cli)".to_string());
        headers.insert("accept-language".to_string(), "*".to_string());
        headers.insert("sec-fetch-mode".to_string(), "cors".to_string());
        Self { headers }
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.headers.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.headers.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }
}
