use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub struct Metadata {
    pub user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub control_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ContentPart {
    Text {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
        #[serde(default)]
        cache_control: Option<CacheControl>,
    },
    Other(serde_json::Value),
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    #[serde(default)]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Parts(Vec<ContentPart>),
}

pub fn generate_session_hash(body: &serde_json::Value) -> Option<String> {
    if let Some(metadata) = body.get("metadata") {
        if let Some(user_id) = metadata.get("user_id").and_then(|v| v.as_str()) {
            if let Some(captures) = Regex::new(r"session_([a-f0-9-]{36})")
                .ok()?
                .captures(user_id)
            {
                return Some(captures[1].to_string());
            }
        }
    }

    let cacheable = extract_cacheable_content(body);
    if !cacheable.is_empty() {
        return Some(hash_content(&cacheable));
    }

    if let Some(system) = body.get("system") {
        let text = extract_system_text(system);
        if !text.is_empty() {
            return Some(hash_content(&text));
        }
    }

    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        if let Some(first) = messages.first() {
            let text = extract_message_text(first);
            if !text.is_empty() {
                return Some(hash_content(&text));
            }
        }
    }

    None
}

fn extract_cacheable_content(body: &serde_json::Value) -> String {
    let mut content = String::new();

    if let Some(system) = body.get("system") {
        if let Some(parts) = system.as_array() {
            for part in parts {
                if let Some(cache_control) = part.get("cache_control") {
                    if cache_control.get("type").and_then(|t| t.as_str()) == Some("ephemeral") {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            content.push_str(text);
                        }
                    }
                }
            }
        }
    }

    if let Some(messages) = body.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            let has_cache = check_message_cache_control(msg);
            if has_cache {
                content.push_str(&extract_message_text(msg));
                break;
            }
        }
    }

    content
}

fn check_message_cache_control(msg: &serde_json::Value) -> bool {
    if let Some(cache_control) = msg.get("cache_control") {
        if cache_control.get("type").and_then(|t| t.as_str()) == Some("ephemeral") {
            return true;
        }
    }

    if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
        for part in content {
            if let Some(cache_control) = part.get("cache_control") {
                if cache_control.get("type").and_then(|t| t.as_str()) == Some("ephemeral") {
                    return true;
                }
            }
        }
    }

    false
}

fn extract_system_text(system: &serde_json::Value) -> String {
    if let Some(text) = system.as_str() {
        return text.to_string();
    }

    if let Some(parts) = system.as_array() {
        return parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("");
    }

    String::new()
}

fn extract_message_text(msg: &serde_json::Value) -> String {
    if let Some(content) = msg.get("content") {
        if let Some(text) = content.as_str() {
            return text.to_string();
        }

        if let Some(parts) = content.as_array() {
            return parts
                .iter()
                .filter_map(|p| {
                    if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                        p.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
        }
    }

    String::new()
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_hash_content() {
        let hash = hash_content("test content");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_session_hash_from_metadata() {
        let body = json!({
            "metadata": {
                "user_id": "user_session_12345678-1234-1234-1234-123456789012_abc"
            }
        });
        let hash = generate_session_hash(&body);
        assert_eq!(hash, Some("12345678-1234-1234-1234-123456789012".to_string()));
    }

    #[test]
    fn test_session_hash_from_system() {
        let body = json!({
            "system": "You are a helpful assistant."
        });
        let hash = generate_session_hash(&body);
        assert!(hash.is_some());
        assert_eq!(hash.unwrap().len(), 32);
    }
}
