use reqwest::{header, Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

const REQUEST_TIMEOUT_DURATION: Duration = Duration::from_secs(300);

// Generic structure for OpenAI-compatible API chat messages
#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// Generic structure for OpenAI-compatible API chat requests
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

// Generic structure for OpenAI-compatible API chat responses
#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub message: MessageContent,
}

#[derive(Deserialize, Debug)]
pub struct MessageContent {
    pub content: String,
}

// Claude-specific request structure
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<ChatMessage>,
}

// Claude-specific response structure
#[derive(Deserialize, Debug)]
pub struct ClaudeChatResponse {
    pub content: Vec<ClaudeChatContent>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeChatContent {
    pub text: String,
}

/// LLM Provider enumeration for multi-provider support
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    OpenAI,
    Claude,
    Groq,
    Ollama,
    OpenRouter,
    BuiltInAI,
    CustomOpenAI,
}

impl LLMProvider {
    /// Parse provider from string (case-insensitive)
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "claude" => Ok(Self::Claude),
            "groq" => Ok(Self::Groq),
            "ollama" => Ok(Self::Ollama),
            "openrouter" => Ok(Self::OpenRouter),
            "builtin-ai" | "local-llama" | "localllama" => Ok(Self::BuiltInAI),
            "custom-openai" => Ok(Self::CustomOpenAI),
            _ => Err(format!("Unsupported LLM provider: {}", s)),
        }
    }
}
// ============================================================
// PR-31: LLM retry infrastructure
// ============================================================

/// Retry policy for transient HTTP failures (timeouts, connect errors, 5xx, 429).
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 8000,
        }
    }
}

/// Whether a HTTP status code is safe to retry on.
pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

/// Mask secret-like substrings (Bearer tokens) before logging or returning errors.
pub(crate) fn sanitize_error(msg: &str) -> String {
    msg.replace("Bearer ", "Bearer ***")
}

/// Exponential backoff with deterministic ±20% jitter based on attempt parity.
/// No new crate dependency; uses std-only ops.
fn backoff_ms(policy: &RetryPolicy, attempt: u32) -> u64 {
    let exp = policy
        .initial_backoff_ms
        .saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(policy.max_backoff_ms);
    let jitter = capped / 5;
    if attempt % 2 == 0 {
        capped.saturating_sub(jitter)
    } else {
        capped.saturating_add(jitter)
    }
}
/// Send an LLM HTTP request with retry on transient failures.
///
/// - `request_builder`: the prepared `RequestBuilder` (must be cloneable, i.e. JSON body).
/// - `retry_policy`: retry settings; pass `RetryPolicy::default()` for the project default.
/// - `cancellation_token`: optional cancellation.
///
/// Returns the successful response (status 2xx), or an error string with secrets masked.
/// Retries on: timeouts, connect errors, request build errors, 5xx, 429.
/// Does NOT retry on: 4xx other than 429, parse errors, cancellation.
pub(crate) async fn send_request_with_retry(
    request_builder: RequestBuilder,
    retry_policy: &RetryPolicy,
    cancellation_token: Option<&CancellationToken>,
) -> Result<Response, String> {
    let mut last_err: Option<String> = None;

    for attempt in 0..=retry_policy.max_retries {
        if let Some(token) = cancellation_token {
            if token.is_cancelled() {
                return Err("Summary generation was cancelled".to_string());
            }
        }

        let req = request_builder
            .try_clone()
            .ok_or_else(|| "Failed to clone request builder for retry".to_string())?;

        let send_result = if let Some(token) = cancellation_token {
            tokio::select! {
                r = req.send() => Some(r),
                _ = token.cancelled() => return Err("Summary generation was cancelled".to_string()),
            }
        } else {
            Some(req.send().await)
        };

        match send_result {
            Some(Ok(response)) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(response);
                }
                if !is_retryable_status(status) {
                    let body = response.text().await.unwrap_or_default();
                    return Err(sanitize_error(&format!(
                        "LLM API request failed ({}): {}",
                        status, body
                    )));
                }
                let body = response.text().await.unwrap_or_default();
                let msg = sanitize_error(&format!(
                    "LLM returned {} (attempt {}/{}): {}",
                    status,
                    attempt + 1,
                    retry_policy.max_retries + 1,
                    body
                ));
                warn!("{}", msg);
                last_err = Some(msg);
            }
            Some(Err(e)) => {
                if e.is_timeout() || e.is_connect() || e.is_request() {
                    let msg = sanitize_error(&format!(
                        "LLM request error (attempt {}/{}): {}",
                        attempt + 1,
                        retry_policy.max_retries + 1,
                        e
                    ));
                    warn!("{}", msg);
                    last_err = Some(msg);
                } else {
                    return Err(sanitize_error(&format!("LLM request error: {}", e)));
                }
            }
            None => return Err("Summary generation was cancelled".to_string()),
        }

        if attempt < retry_policy.max_retries {
            let delay = backoff_ms(retry_policy, attempt);
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }

    Err(sanitize_error(&format!(
        "LLM call failed after {} retries: {}",
        retry_policy.max_retries + 1,
        last_err.unwrap_or_else(|| "unknown error".to_string())
    )))
}
/// Generates a summary using the specified LLM provider
///
/// # Arguments
/// * `client` - Reqwest HTTP client (reused for performance)
/// * `provider` - The LLM provider to use
/// * `model_name` - The specific model to use (e.g., "gpt-4", "claude-3-opus")
/// * `api_key` - API key for the provider (not needed for Ollama)
/// * `system_prompt` - System instructions for the LLM
/// * `user_prompt` - User query/content to process
/// * `ollama_endpoint` - Optional custom Ollama endpoint (defaults to localhost:11434)
/// * `custom_openai_endpoint` - Optional custom OpenAI-compatible endpoint
/// * `max_tokens` - Optional max tokens (for CustomOpenAI provider)
/// * `temperature` - Optional temperature (for CustomOpenAI provider)
/// * `top_p` - Optional top_p (for CustomOpenAI provider)
/// * `app_data_dir` - Optional app data directory (for BuiltInAI provider)
/// * `cancellation_token` - Optional token to cancel the request
///
/// # Returns
/// The generated summary text or an error message
pub async fn generate_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    ollama_endpoint: Option<&str>,
    custom_openai_endpoint: Option<&str>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    app_data_dir: Option<&PathBuf>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<String, String> {
    // Check if cancelled before starting
    if let Some(token) = cancellation_token {
        if token.is_cancelled() {
            return Err("Summary generation was cancelled".to_string());
        }
    }

    // Handle BuiltInAI provider separately (uses local sidecar, no HTTP API)
    if provider == &LLMProvider::BuiltInAI {
        let app_data_dir = app_data_dir
            .ok_or_else(|| "app_data_dir is required for BuiltInAI provider".to_string())?;

        return crate::summary::summary_engine::generate_with_builtin(
            app_data_dir,
            model_name,
            system_prompt,
            user_prompt,
            cancellation_token,
        )
        .await
        .map_err(|e| e.to_string());
    }
    let (api_url, mut headers) = match provider {
        LLMProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Groq => (
            "https://api.groq.com/openai/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Claude => (
            "https://api.anthropic.com/v1/messages".to_string(),
            {
                let mut h = header::HeaderMap::new();
                h.insert("x-api-key", header::HeaderValue::from_static(""));
                h.insert("anthropic-version", header::HeaderValue::from_static("2023-06-01"));
                h
            },
        ),
        LLMProvider::Ollama => (
            format!(
                "{}/v1/chat/completions",
                ollama_endpoint.unwrap_or("http://localhost:11434")
            ),
            header::HeaderMap::new(),
        ),
        LLMProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::CustomOpenAI => {
            let endpoint = custom_openai_endpoint
                .ok_or_else(|| "custom_openai_endpoint is required for CustomOpenAI provider".to_string())?;
            (format!("{}/v1/chat/completions", endpoint), header::HeaderMap::new())
        }
        LLMProvider::BuiltInAI => {
            // This case is handled earlier with early returns
            unreachable!("BuiltInAI is handled before this match statement")
        }
    };

    // Add authorization header for non-Claude providers
    if provider != &LLMProvider::Claude {
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", api_key)
                .parse()
                .map_err(|_| "Invalid authorization header".to_string())?,
        );
    }
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| "Invalid content type".to_string())?,
    );
    // Build request body based on provider
    let request_body = if provider != &LLMProvider::Claude {
        // For CustomOpenAI, apply optional parameters if provided
        let (max_tokens_val, temperature_val, top_p_val) = if provider == &LLMProvider::CustomOpenAI {
            (max_tokens, temperature, top_p)
        } else {
            (None, None, None)
        };

        serde_json::json!(ChatRequest {
            model: model_name.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                }
            ],
            max_tokens: max_tokens_val,
            temperature: temperature_val,
            top_p: top_p_val,
        })
    } else {
        serde_json::json!(ClaudeRequest {
            system: system_prompt.to_string(),
            model: model_name.to_string(),
            max_tokens: 2048,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }]
        })
    };

    info!("馃悶 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request with retry on transient failures (PR-31)
    let response = send_request_with_retry(
        client
            .post(api_url)
            .headers(headers)
            .json(&request_body)
            .timeout(REQUEST_TIMEOUT_DURATION),
        &RetryPolicy::default(),
        cancellation_token,
    )
    .await?;
    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = response
            .json::<ClaudeChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("馃悶 LLM Response received from Claude");

        let content = chat_response
            .content
            .get(0)
            .ok_or("No content in LLM response")?
            .text
            .trim();
        Ok(content.to_string())
    } else {
        let chat_response = response
            .json::<ChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("馃悶 LLM Response received from {}", provider_name(provider));

        let content = chat_response
            .choices
            .get(0)
            .ok_or("No content in LLM response")?
            .message
            .content
            .trim();
        Ok(content.to_string())
    }
}

/// Helper function to get provider name for logging
fn provider_name(provider: &LLMProvider) -> &str {
    match provider {
        LLMProvider::OpenAI => "OpenAI",
        LLMProvider::Claude => "Claude",
        LLMProvider::Groq => "Groq",
        LLMProvider::Ollama => "Ollama",
        LLMProvider::BuiltInAI => "Built-in AI",
        LLMProvider::OpenRouter => "OpenRouter",
        LLMProvider::CustomOpenAI => "Custom OpenAI",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn default_policy_has_three_retries() {
        let p = RetryPolicy::default();
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.initial_backoff_ms, 1000);
        assert_eq!(p.max_backoff_ms, 8000);
    }

    #[test]
    fn retryable_status_includes_server_errors_and_429() {
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
    }

    #[test]
    fn non_retryable_status_excludes_client_errors() {
        assert!(!is_retryable_status(StatusCode::OK));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn backoff_grows_exponentially_then_caps() {
        let p = RetryPolicy {
            max_retries: 5,
            initial_backoff_ms: 1000,
            max_backoff_ms: 8000,
        };
        let d0 = backoff_ms(&p, 0);
        assert!(d0 >= 800 && d0 <= 1200, "attempt 0 out of range: {}", d0);
        let d1 = backoff_ms(&p, 1);
        assert!(d1 >= 1600 && d1 <= 2400, "attempt 1 out of range: {}", d1);
        let d3 = backoff_ms(&p, 3);
        assert!(d3 >= 6400 && d3 <= 9600, "attempt 3 out of range: {}", d3);
        let d10 = backoff_ms(&p, 10);
        assert!(d10 >= 6400 && d10 <= 9600, "attempt 10 out of range: {}", d10);
    }

    #[test]
    fn sanitize_error_masks_bearer_tokens() {
        assert_eq!(sanitize_error("Bearer sk-abc123"), "Bearer *** sk-abc123");
        assert_eq!(sanitize_error("Authorization: Bearer xyz"), "Authorization: Bearer *** xyz");
        assert_eq!(sanitize_error("ok"), "ok");
    }

    #[test]
    fn sanitize_error_handles_empty() {
        assert_eq!(sanitize_error(""), "");
    }
    /// Helper: build a RequestBuilder pointing to a non-listening local port.
    /// All sends will fail with a connect error (retriable).
    fn unreachable_builder() -> RequestBuilder {
        Client::new()
            .post("http://127.0.0.1:1/")
            .header("content-type", "application/json")
            .body("{}")
            .timeout(Duration::from_millis(50))
    }

    #[tokio::test]
    async fn retry_exhausts_on_persistent_connect_failure() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_backoff_ms: 10,
            max_backoff_ms: 50,
        };
        let start = Instant::now();
        let result = send_request_with_retry(unreachable_builder(), &policy, None).await;
        let elapsed = start.elapsed();
        assert!(result.is_err(), "expected error after exhausting retries");
        let err = result.unwrap_err();
        assert!(err.contains("LLM call failed after 3 retries"), "unexpected error: {}", err);
        assert!(elapsed >= Duration::from_millis(20), "should have backed off: {:?}", elapsed);
    }

    #[tokio::test]
    async fn retry_respects_cancellation_token() {
        let policy = RetryPolicy {
            max_retries: 5,
            initial_backoff_ms: 10,
            max_backoff_ms: 20,
        };
        let token = CancellationToken::new();
        let token_clone = token.clone();
        let handle = tokio::spawn(async move {
            send_request_with_retry(unreachable_builder(), &policy, Some(&token_clone)).await
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        token.cancel();
        let result = handle.await.unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }

    #[tokio::test]
    async fn retry_does_not_retry_when_max_retries_zero() {
        let policy = RetryPolicy {
            max_retries: 0,
            initial_backoff_ms: 1,
            max_backoff_ms: 1,
        };
        let result = send_request_with_retry(unreachable_builder(), &policy, None).await;
        assert!(result.is_err(), "should fail without retrying");
        assert!(result.unwrap_err().contains("LLM call failed after 1 retries"));
    }

    #[tokio::test]
    async fn error_messages_do_not_echo_api_key() {
        let policy = RetryPolicy {
            max_retries: 1,
            initial_backoff_ms: 1,
            max_backoff_ms: 1,
        };
        // Build a request whose Authorization header contains the api_key.
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_static("Bearer sk-supersecret-key-do-not-leak"),
        );
        let req = Client::new()
            .post("http://127.0.0.1:1/")
            .headers(headers)
            .body("{}")
            .timeout(Duration::from_millis(50));
        let err = send_request_with_retry(req, &policy, None).await.unwrap_err();
        // The token value must be masked; "Bearer sk-supersecret..." must NOT appear verbatim.
        assert!(
            !err.contains("sk-supersecret"),
            "api_key leaked in error: {}",
            err
        );
        assert!(err.contains("Bearer ***"), "expected masked token marker: {}", err);
    }
}
