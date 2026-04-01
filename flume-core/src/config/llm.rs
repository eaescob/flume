use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    Anthropic,
    #[serde(alias = "openai")]
    OpenAi,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: LlmProvider,
    /// Vault secret name for the API key (e.g., "flume_llm_key").
    /// Use `/secure set flume_llm_key <your-api-key>` to store it.
    #[serde(default = "default_api_key_secret")]
    pub api_key_secret: String,
    /// Model name (e.g., "claude-sonnet-4-20250514", "gpt-4o").
    #[serde(default = "default_model")]
    pub model: String,
    /// Sampling temperature (0.0-1.0).
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Max tokens in the response.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::default(),
            api_key_secret: default_api_key_secret(),
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
        }
    }
}

fn default_api_key_secret() -> String {
    "flume_llm_key".to_string()
}
fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_temperature() -> f64 {
    0.3
}
fn default_max_tokens() -> u32 {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, LlmProvider::Anthropic);
        assert_eq!(config.api_key_secret, "flume_llm_key");
        assert_eq!(config.temperature, 0.3);
    }

    #[test]
    fn deserialize_openai() {
        let toml_str = r#"
provider = "openai"
model = "gpt-4o"
api_key_secret = "openai_key"
"#;
        let config: LlmConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider, LlmProvider::OpenAi);
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_key_secret, "openai_key");
    }

    #[test]
    fn deserialize_anthropic() {
        let toml_str = r#"
provider = "anthropic"
model = "claude-sonnet-4-20250514"
"#;
        let config: LlmConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.provider, LlmProvider::Anthropic);
    }
}
