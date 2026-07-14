//! DeepSeek provider (OpenAI-compatible). Protocol per GigaTool's provider config:
//! base `https://api.deepseek.com/v1`, `POST /chat/completions`, Bearer auth,
//! models `deepseek-v4-pro` / `deepseek-v4-flash` (the legacy `deepseek-chat` /
//! `deepseek-reasoner` aliases retire 2026-07-24). Non-streaming (summary/extract/RAG).

use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://gw.gigatool.app/deepseek/v1";
pub const DEFAULT_MODEL: &str = "deepseek-v4-pro";
pub const DEFAULT_MAX_TOKENS: u32 = 8_192;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

/// Summary/extraction calls need predictable latency and a complete final answer rather
/// than a long chain-of-thought. DeepSeek V4 enables thinking by default, so disable it
/// explicitly and use conservative sampling for stable structured output.
pub fn build_request_body(model: &str, system: &str, user: &str, max_tokens: Option<u32>) -> Value {
    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "thinking": {"type": "disabled"},
        "max_tokens": max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "temperature": 0.2,
        "top_p": 0.9,
        "stream": false,
    })
}

/// Validate the OpenAI-shaped response contract used by DeepSeek. A 200 response is not
/// necessarily a successful summary: `length`, content filtering, or an interrupted
/// inference can all leave a plausible-looking but incomplete document.
pub fn parse_response(v: &Value) -> Result<String, String> {
    let choice = v
        .get("choices")
        .and_then(|choices| choices.get(0))
        .ok_or_else(|| "deepseek response missing choices[0]".to_string())?;

    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| "deepseek response missing finish_reason".to_string())?;

    if finish_reason != "stop" {
        let explanation = match finish_reason {
            "length" => "output was truncated at the token limit",
            "content_filter" => "output was blocked by the content filter",
            "insufficient_system_resource" => "generation was interrupted by the provider",
            "tool_calls" => "provider returned a tool call instead of a summary",
            _ => "generation did not reach a natural stop",
        };
        return Err(format!(
            "deepseek summary incomplete: finish_reason={finish_reason} ({explanation})"
        ));
    }

    let content = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "deepseek response has no final answer content".to_string())?;

    Ok(content.to_string())
}

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
        let body = build_request_body(&self.model, system, user, None);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
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

        let v: Value = resp
            .json()
            .await
            .map_err(|e| format!("deepseek response parse failed: {e}"))?;
        parse_response(&v)
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
            "choices": [{
                "finish_reason": "stop",
                "message": {"role": "assistant", "content": "привет"}
            }]
        });
        assert_eq!(extract_openai_content(&v).as_deref(), Some("привет"));
        assert_eq!(parse_response(&v).unwrap(), "привет");
        assert!(extract_openai_content(&serde_json::json!({"choices": []})).is_none());
    }

    #[test]
    fn request_disables_thinking_and_bounds_output() {
        let body = build_request_body("deepseek-v4-pro", "system", "user", None);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["top_p"], 0.9);
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn rejects_truncated_or_empty_responses() {
        let truncated = serde_json::json!({
            "choices": [{
                "finish_reason": "length",
                "message": {"content": "# Partial"}
            }]
        });
        assert!(parse_response(&truncated)
            .unwrap_err()
            .contains("truncated"));

        let empty = serde_json::json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {"content": "  "}
            }]
        });
        assert!(parse_response(&empty)
            .unwrap_err()
            .contains("no final answer"));
    }
}
