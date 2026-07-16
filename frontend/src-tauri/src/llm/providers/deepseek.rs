//! DeepSeek provider (OpenAI-compatible) through the operator-approved managed
//! Memento gateway. Upstream protocol:
//! base `https://api.deepseek.com/v1`, `POST /chat/completions`, Bearer auth,
//! models `deepseek-v4-pro` / `deepseek-v4-flash` (the legacy `deepseek-chat` /
//! `deepseek-reasoner` aliases retire 2026-07-24). Non-streaming (summary/extract/RAG).

use serde_json::json;

pub const DEFAULT_BASE_URL: &str = crate::gateway_identity::PRIMARY_DEEPSEEK_BASE_URL;
pub const DEFAULT_MODEL: &str = "deepseek-v4-pro";

/// A configured DeepSeek client. Cheap to construct per call.
pub struct DeepSeekClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Run a system+user completion, returning the assistant text.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "stream": false,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("deepseek request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "deepseek error {status}: {}",
                text.chars().take(300).collect::<String>()
            ));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("deepseek response parse failed: {e}"))?;
        extract_openai_content(&v)
            .ok_or_else(|| "deepseek response missing choices[0].message.content".to_string())
    }
}

/// Pull `choices[0].message.content` out of an OpenAI-shaped response. Shared with the
/// GigaChat client (its chat response is OpenAI-shaped too).
pub fn extract_openai_content(v: &serde_json::Value) -> Option<String> {
    v.get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_shaped_response() {
        let v = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "привет"}}]
        });
        assert_eq!(extract_openai_content(&v).as_deref(), Some("привет"));
        assert!(extract_openai_content(&serde_json::json!({"choices": []})).is_none());
    }

    #[test]
    fn default_gateway_uses_the_central_managed_endpoint() {
        assert_eq!(
            DEFAULT_BASE_URL,
            crate::gateway_identity::PRIMARY_DEEPSEEK_BASE_URL
        );
    }
}
