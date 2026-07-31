//! OpenRouter provider for every network-backed LLM task in Memento.
//!
//! OpenRouter exposes an OpenAI-compatible chat-completions API. Keeping the
//! client here gives chat, extraction, reports, and summaries one provider and
//! one model identity instead of silently falling back to DeepSeek/GigaChat.

use reqwest::header;
use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_MODEL: &str = "~anthropic/claude-sonnet-latest";
pub const DEFAULT_MAX_TOKENS: u32 = 8_192;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

pub struct OpenRouterClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    max_tokens: u32,
}

impl OpenRouterClient {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        self.post_chat(system, user, 0.2, false).await
    }

    pub async fn complete_json(
        &self,
        system: &str,
        user: &str,
        temperature: f32,
    ) -> Result<String, String> {
        self.post_chat(system, user, temperature, true).await
    }

    async fn post_chat(
        &self,
        system: &str,
        user: &str,
        temperature: f32,
        json_mode: bool,
    ) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ],
            "max_tokens": self.max_tokens,
            "temperature": temperature,
            "stream": false
        });
        if json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .header(header::CONTENT_TYPE, "application/json")
            .header("HTTP-Referer", "https://memento.local")
            .header("X-Title", "Memento")
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("OpenRouter request failed: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!(
                "OpenRouter error {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }

        let value: Value = response
            .json()
            .await
            .map_err(|error| format!("OpenRouter response parse failed: {error}"))?;
        let choice = value
            .get("choices")
            .and_then(|choices| choices.get(0))
            .ok_or_else(|| "OpenRouter response is missing choices[0]".to_string())?;
        let finish_reason = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop");
        if finish_reason != "stop" {
            return Err(format!(
                "OpenRouter response is incomplete: finish_reason={finish_reason}"
            ));
        }
        choice
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| "OpenRouter response has no final content".to_string())
    }
}
