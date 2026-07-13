use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Local,
    OpenAI,
    Gemini,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfig {
    pub provider: Provider,
    pub api_key: Option<String>,
    pub model: String,
}

impl LlmConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let path = Path::new("/etc/chai/config.json");
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(format!("Failed to read {}: {}", path.display(), e)))?;
        serde_json::from_str(&content)
            .map_err(|e| ConfigError::Parse(format!("Invalid config JSON: {e}")))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
}
