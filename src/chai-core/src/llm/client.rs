use crate::llm::config::{LlmConfig, Provider};
use reqwest::Client as HttpClient;
use std::time::Duration;

const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
const CLOUD_TIMEOUT: Duration = Duration::from_secs(30);
const SYSTEM_PROMPT: &str = "You are a NixOS configuration assistant. \
    Generate declarative Nix expressions based on user requests. \
    Always respond with valid JSON containing 'packages' and 'services' arrays.";

pub struct Client {
    config: LlmConfig,
    http: HttpClient,
}

impl Client {
    pub fn new(config: LlmConfig) -> Self {
        let timeout = match config.provider {
            Provider::Local => LOCAL_TIMEOUT,
            Provider::OpenAI | Provider::Gemini => CLOUD_TIMEOUT,
        };
        let http = HttpClient::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to build HTTP client");
        Self { config, http }
    }

    pub async fn generate(&self, prompt: &str) -> Result<String, LlmError> {
        match self.config.provider {
            Provider::Local => self.generate_local(prompt).await,
            Provider::OpenAI => self.generate_openai(prompt).await,
            Provider::Gemini => self.generate_gemini(prompt).await,
        }
    }

    async fn generate_local(&self, prompt: &str) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.config.model,
            "prompt": prompt,
            "system": SYSTEM_PROMPT,
            "stream": false,
        });

        let resp = self
            .http
            .post("http://localhost:11434/api/generate")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::ProviderUnavailable("Ollama timeout after 60s".into())
                } else {
                    LlmError::ProviderUnavailable(format!("Ollama error: {e}"))
                }
            })?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse Ollama response: {e}")))?;

        data["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("Missing 'response' field in Ollama output".into()))
    }

    async fn generate_openai(&self, prompt: &str) -> Result<String, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| LlmError::ConfigError("OpenAI provider requires api_key".into()))?;

        let body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": prompt}
            ]
        });

        let resp = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::ProviderUnavailable("OpenAI timeout after 30s".into())
                } else {
                    LlmError::ProviderUnavailable(format!("OpenAI error: {e}"))
                }
            })?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse OpenAI response: {e}")))?;

        data["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("Missing content in OpenAI response".into()))
    }

    async fn generate_gemini(&self, prompt: &str) -> Result<String, LlmError> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| LlmError::ConfigError("Gemini provider requires api_key".into()))?;

        let body = serde_json::json!({
            "contents": [{
                "parts": [{"text": format!("{}\n\n{}", SYSTEM_PROMPT, prompt)}]
            }]
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.config.model, api_key
        );

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::ProviderUnavailable("Gemini timeout after 30s".into())
                } else {
                    LlmError::ProviderUnavailable(format!("Gemini error: {e}"))
                }
            })?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse Gemini response: {e}")))?;

        data["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LlmError::ParseError("Missing text in Gemini response".into()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("Failed to parse LLM response: {0}")]
    ParseError(String),
}
