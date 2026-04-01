pub mod prompts;

use crate::config::llm::{LlmConfig, LlmProvider};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error: {status} — {message}")]
    Api { status: u16, message: String },
    #[error("No API key found. Use /secure set {0} <your-api-key>")]
    NoApiKey(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

/// A request to the LLM.
pub struct LlmRequest {
    pub system: String,
    pub user: String,
}

/// A response from the LLM.
pub struct LlmResponse {
    pub content: String,
}

/// LLM client that can call Anthropic or OpenAI APIs.
pub struct LlmClient {
    config: LlmConfig,
    api_key: String,
    #[cfg(feature = "llm")]
    http: reqwest::Client,
}

impl LlmClient {
    /// Create a new LLM client. `api_key` should be retrieved from the vault.
    pub fn new(config: LlmConfig, api_key: String) -> Self {
        Self {
            config,
            api_key,
            #[cfg(feature = "llm")]
            http: reqwest::Client::new(),
        }
    }

    /// Generate a response from the LLM.
    #[cfg(feature = "llm")]
    pub async fn generate(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        match self.config.provider {
            LlmProvider::Anthropic => self.generate_anthropic(request).await,
            LlmProvider::OpenAi => self.generate_openai(request).await,
        }
    }

    #[cfg(feature = "llm")]
    async fn generate_anthropic(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
            "system": request.system,
            "messages": [
                {"role": "user", "content": request.user}
            ]
        });

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let resp_text = resp.text().await.map_err(|e| LlmError::Http(e.to_string()))?;

        if status != 200 {
            return Err(LlmError::Api {
                status,
                message: resp_text,
            });
        }

        let json: serde_json::Value =
            serde_json::from_str(&resp_text).map_err(|e| LlmError::Parse(e.to_string()))?;

        let content = json["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .unwrap_or("")
            .to_string();

        Ok(LlmResponse { content })
    }

    #[cfg(feature = "llm")]
    async fn generate_openai(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "temperature": self.config.temperature,
            "messages": [
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.user}
            ]
        });

        let resp = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let resp_text = resp.text().await.map_err(|e| LlmError::Http(e.to_string()))?;

        if status != 200 {
            return Err(LlmError::Api {
                status,
                message: resp_text,
            });
        }

        let json: serde_json::Value =
            serde_json::from_str(&resp_text).map_err(|e| LlmError::Parse(e.to_string()))?;

        let content = json["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|choice| choice["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        Ok(LlmResponse { content })
    }
}

/// Extract code from LLM response that may be wrapped in markdown code fences.
pub fn extract_code(content: &str) -> String {
    // Try to find a code block
    if let Some(start) = content.find("```") {
        let after_fence = &content[start + 3..];
        // Skip language identifier on the first line
        let code_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let code = &after_fence[code_start..];
        if let Some(end) = code.find("```") {
            return code[..end].trim().to_string();
        }
    }
    // No code block found — return as-is
    content.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_code_with_fences() {
        let content = "Here's the script:\n```lua\nflume.event.on(\"message\", function(e)\n  print(e.text)\nend)\n```\nEnjoy!";
        let code = extract_code(content);
        assert!(code.starts_with("flume.event.on"));
        assert!(code.ends_with("end)"));
        assert!(!code.contains("```"));
    }

    #[test]
    fn extract_code_no_fences() {
        let content = "flume.event.on(\"message\", function(e) end)";
        let code = extract_code(content);
        assert_eq!(code, content);
    }

    #[test]
    fn extract_code_with_language_tag() {
        let content = "```python\nimport flume\nflume.event.on('msg', lambda e: None)\n```";
        let code = extract_code(content);
        assert!(code.starts_with("import flume"));
    }
}
