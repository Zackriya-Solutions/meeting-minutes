use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

const REQUEST_TIMEOUT_DURATION: Duration = Duration::from_secs(300);

async fn await_or_cancel<T>(
    operation: impl Future<Output = T>,
    cancellation_token: Option<&CancellationToken>,
) -> Result<T, String> {
    let Some(token) = cancellation_token else {
        return Ok(operation.await);
    };

    tokio::select! {
        biased;
        _ = token.cancelled() => Err("Summary generation was cancelled".to_string()),
        result = operation => Ok(result),
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<&'static str>,
}

/// Build OpenAI-compat JSON body.
pub fn build_openai_compat_chat_body(
    provider: &LLMProvider,
    model_name: &str,
    system_prompt: &str,
    user_prompt: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
) -> serde_json::Value {
    let (max_tokens_val, temperature_val, top_p_val) = if *provider == LLMProvider::CustomOpenAI {
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
        reasoning_effort: (*provider == LLMProvider::Ollama).then_some("none"),
    })
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
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
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
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: Option<String>,
}

impl ClaudeChatResponse {
    fn completion(&self) -> Option<LlmCompletion> {
        let content = self
            .content
            .iter()
            .find(|block| block.block_type == "text")
            .and_then(|block| block.text.as_deref())?
            .trim()
            .to_string();
        Some(LlmCompletion {
            content,
            reasoning_stripped: self.content.iter().any(|block| {
                matches!(block.block_type.as_str(), "thinking" | "redacted_thinking")
            }),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlmCompletion {
    pub content: String,
    pub reasoning_stripped: bool,
}

impl ChatResponse {
    fn completion(&self) -> Result<LlmCompletion, String> {
        let message = &self
            .choices
            .first()
            .ok_or("No content in LLM response")?
            .message;
        Ok(LlmCompletion {
            content: message.content.trim().to_string(),
            reasoning_stripped: [message.reasoning.as_deref(), message.reasoning_content.as_deref()]
                .into_iter()
                .flatten()
                .any(|reasoning| !reasoning.trim().is_empty()),
        })
    }
}

pub(crate) fn ollama_rejects_reasoning_effort(status: reqwest::StatusCode, body: &str) -> bool {
    if !matches!(status.as_u16(), 400 | 422) {
        return false;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    let matches_field = |value: Option<&serde_json::Value>| {
        value
            .and_then(serde_json::Value::as_str)
            .is_some_and(|field| field.eq_ignore_ascii_case("reasoning_effort"))
    };
    if matches_field(value.get("param"))
        || matches_field(value.get("field"))
        || matches_field(value.get("error").and_then(|error| error.get("param")))
        || matches_field(value.get("error").and_then(|error| error.get("field")))
    {
        return true;
    }

    let matches_message = [
        value.as_str(),
        value.get("error").and_then(serde_json::Value::as_str),
        value.get("message").and_then(serde_json::Value::as_str),
        value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str),
    ]
    .into_iter()
    .flatten()
    .any(|message| {
        let message = message.to_ascii_lowercase();
        message.contains("reasoning_effort")
            || (message.contains("think value")
                && message.contains("none")
                && (message.contains("not supported") || message.contains("invalid")))
    });
    matches_message
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
/// The generated visible content and whether private reasoning was removed.
pub(crate) async fn generate_summary(
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
) -> Result<LlmCompletion, String> {
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
        .map(|content| LlmCompletion {
            content,
            reasoning_stripped: false,
        })
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
        LLMProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Ollama => {
            let host = ollama_endpoint
                .map(|s| s.to_string())
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            (
                format!("{}/v1/chat/completions", host),
                header::HeaderMap::new(),
            )
        }
        LLMProvider::CustomOpenAI => {
            let endpoint = custom_openai_endpoint
                .ok_or_else(|| "Custom OpenAI endpoint not configured".to_string())?;
            (
                format!("{}/chat/completions", endpoint.trim_end_matches('/')),
                header::HeaderMap::new(),
            )
        }
        LLMProvider::Claude => {
            let mut header_map = header::HeaderMap::new();
            header_map.insert(
                "x-api-key",
                api_key
                    .parse()
                    .map_err(|_| "Invalid API key format".to_string())?,
            );
            header_map.insert(
                "anthropic-version",
                "2023-06-01"
                    .parse()
                    .map_err(|_| "Invalid anthropic version".to_string())?,
            );
            ("https://api.anthropic.com/v1/messages".to_string(), header_map)
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
        build_openai_compat_chat_body(
            provider,
            model_name,
            system_prompt,
            user_prompt,
            max_tokens,
            temperature,
            top_p,
        )
    } else {
        serde_json::json!(ClaudeRequest {
            system: system_prompt.to_string(),
            model: model_name.to_string(),
            // Shared budget: on models with thinking enabled by default this
            // covers thinking tokens as well as the answer.
            max_tokens: 8192,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }]
        })
    };

    info!("🐞 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request with timeout and cancellation support
    let request_future = client
        .post(api_url.clone())
        .headers(headers.clone())
        .json(&request_body)
        .timeout(REQUEST_TIMEOUT_DURATION)
        .send();

    // Use tokio::select to race between cancellation and request completion
    let response = if let Some(token) = cancellation_token {
        tokio::select! {
            result = request_future => {
                result.map_err(|e| {
                    if e.is_timeout() {
                        format!(
                            "LLM request timed out after {} seconds",
                            REQUEST_TIMEOUT_DURATION.as_secs()
                        )
                    } else {
                        format!("Failed to send request to LLM: {}", e)
                    }
                })?
            }
            _ = token.cancelled() => {
                return Err("Summary generation was cancelled".to_string());
            }
        }
    } else {
        request_future.await.map_err(|e| {
            if e.is_timeout() {
                format!(
                    "LLM request timed out after {} seconds",
                    REQUEST_TIMEOUT_DURATION.as_secs()
                )
            } else {
                format!("Failed to send request to LLM: {}", e)
            }
        })?
    };

    let response = if response.status().is_success() {
        response
    } else {
        let status = response.status();
        let error_body = await_or_cancel(response.text(), cancellation_token)
            .await?
            .unwrap_or_else(|_| "Unknown error".to_string());
        if provider != &LLMProvider::Ollama || !ollama_rejects_reasoning_effort(status, &error_body) {
            return Err(format!(
                "LLM API request failed with status {}: {}",
                status, error_body
            ));
        }

        warn!("Ollama rejected reasoning_effort; retrying once without it");
        let mut retry_body = request_body;
        retry_body
            .as_object_mut()
            .ok_or_else(|| "Failed to prepare Ollama compatibility retry".to_string())?
            .remove("reasoning_effort");
        let retry_future = client
            .post(api_url)
            .headers(headers)
            .json(&retry_body)
            .timeout(REQUEST_TIMEOUT_DURATION)
            .send();
        await_or_cancel(retry_future, cancellation_token)
            .await?
            .map_err(|e| {
                if e.is_timeout() {
                    format!(
                        "LLM retry request timed out after {} seconds",
                        REQUEST_TIMEOUT_DURATION.as_secs()
                    )
                } else {
                    format!("Failed to send retry request to LLM: {}", e)
                }
            })?
    };

    if !response.status().is_success() {
        let status = response.status();
        let error_body = await_or_cancel(response.text(), cancellation_token)
            .await?
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!(
            "LLM API request failed with status {}: {}",
            status, error_body
        ));
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = await_or_cancel(
            response.json::<ClaudeChatResponse>(),
            cancellation_token,
        )
        .await?
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

        let completion = chat_response
            .completion()
            .ok_or("No text content in LLM response")?;
        Ok(completion)
    } else {
        let chat_response = await_or_cancel(
            response.json::<ChatResponse>(),
            cancellation_token,
        )
        .await?
        .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from {}", provider_name(provider));
        chat_response.completion()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::Cell, task::Poll};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
        time::{timeout, Duration},
    };

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];

        loop {
            let bytes_read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(bytes_read, 0, "connection closed before completing request");
            request.extend_from_slice(&buffer[..bytes_read]);

            let Some(headers_end) = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| position + 4)
            else {
                continue;
            };
            let content_length = std::str::from_utf8(&request[..headers_end])
                .unwrap()
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .expect("request should have Content-Length");
            if request.len() >= headers_end + content_length {
                return request;
            }
        }
    }

    fn request_json(request: &[u8]) -> serde_json::Value {
        let headers_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|position| position + 4)
            .expect("request should contain complete headers");
        serde_json::from_slice(&request[headers_end..]).expect("request body should be valid JSON")
    }

    #[test]
    fn only_ollama_disables_reasoning_and_custom_sampling_is_preserved() {
        for provider in [
            LLMProvider::OpenAI,
            LLMProvider::Groq,
            LLMProvider::OpenRouter,
            LLMProvider::CustomOpenAI,
        ] {
            assert!(build_openai_compat_chat_body(&provider, "model", "sys", "user", None, None, None)
                .get("reasoning_effort")
                .is_none());
        }
        let ollama = build_openai_compat_chat_body(
            &LLMProvider::Ollama, "model", "sys", "user", None, None, None,
        );
        assert_eq!(ollama["reasoning_effort"], "none");
        let custom = build_openai_compat_chat_body(
            &LLMProvider::CustomOpenAI, "model", "sys", "user", Some(12), Some(0.3), Some(0.8),
        );
        assert_eq!(custom["max_tokens"], 12);
        assert_eq!(custom["temperature"].as_f64(), Some(0.3_f32 as f64));
        assert_eq!(custom["top_p"].as_f64(), Some(0.8_f32 as f64));
    }

    #[test]
    fn compatible_reasoning_is_separate_from_visible_content() {
        let response: ChatResponse = serde_json::from_value(json!({
            "choices": [{"message": {"reasoning_content": "private"}}]
        }))
        .unwrap();
        assert_eq!(
            response.completion().unwrap(),
            LlmCompletion {
                content: String::new(),
                reasoning_stripped: true,
            }
        );
    }

    #[test]
    fn legacy_ollama_rejection_requires_compatible_error() {
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"param":"reasoning_effort"}}"#,
        ));
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            r#"{"message":"Unsupported parameter 'ReAsOnInG_EfFoRt'."}"#,
        ));
        assert!(ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":"think value \"none\" is not supported for this model"}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"message":"invalid model"}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"param":"reasoning_effort"}}"#,
        ));
        assert!(!ollama_rejects_reasoning_effort(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":"think value \"none\" is not supported"}"#,
        ));
    }

    #[tokio::test]
    async fn cancellation_prevents_compatibility_io_from_being_polled() {
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();
        let polled = Cell::new(false);

        let result = await_or_cancel(
            std::future::poll_fn(|_| {
                polled.set(true);
                Poll::<()>::Pending
            }),
            Some(&cancellation_token),
        )
        .await;

        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
        assert!(!polled.get());
    }

    #[tokio::test]
    async fn cancellation_during_failed_legacy_ollama_retry_body_returns_promptly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (retry_headers_sent, mut retry_headers_ready) = oneshot::channel();
        let (release_retry_body, retry_body_released) = oneshot::channel();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            stream
                .write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 4\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            stream.flush().await.unwrap();
            let _ = retry_headers_sent.send(());
            let _ = retry_body_released.await;
            let _ = stream.write_all(b"fail").await;
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let generation = generate_summary(
            &client,
            &LLMProvider::Ollama,
            "model",
            "",
            "system",
            "user",
            Some(&endpoint),
            None,
            None,
            None,
            None,
            None,
            Some(&cancellation_token),
        );
        tokio::pin!(generation);

        tokio::select! {
            biased;
            result = &mut generation => panic!("generation ended before retry headers: {result:?}"),
            _ = &mut retry_headers_ready => {}
        }

        cancellation_token.cancel();
        let completion = timeout(Duration::from_secs(1), &mut generation).await;
        let _ = release_retry_body.send(());
        let released_completion = if completion.is_err() {
            Some(timeout(Duration::from_secs(1), &mut generation).await)
        } else {
            None
        };
        server.await.unwrap();

        let result = completion.unwrap_or_else(|_| {
            released_completion
                .expect("generation should be awaited after releasing the retry body")
                .expect("generation should finish after releasing the retry body")
        });
        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
    }

    #[tokio::test]
    async fn cancellation_during_successful_legacy_ollama_retry_body_returns_promptly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (retry_headers_sent, mut retry_headers_ready) = oneshot::channel();
        let (release_retry_body, retry_body_released) = oneshot::channel();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            let completion_body =
                br#"{"choices":[{"message":{"content":"Meeting summary."}}]}"#;
            let completion_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                completion_body.len()
            );
            stream.write_all(completion_headers.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = retry_headers_sent.send(());
            let _ = retry_body_released.await;
            let _ = stream.write_all(completion_body).await;
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let generation = generate_summary(
            &client,
            &LLMProvider::Ollama,
            "model",
            "",
            "system",
            "user",
            Some(&endpoint),
            None,
            None,
            None,
            None,
            None,
            Some(&cancellation_token),
        );
        tokio::pin!(generation);

        tokio::select! {
            biased;
            result = &mut generation => panic!("generation ended before retry headers: {result:?}"),
            _ = &mut retry_headers_ready => {}
        }

        cancellation_token.cancel();
        let completion = timeout(Duration::from_secs(1), &mut generation).await;
        let _ = release_retry_body.send(());
        if completion.is_err() {
            let _ = timeout(Duration::from_secs(1), &mut generation).await;
        }
        server.await.unwrap();

        let result =
            completion.expect("generation should observe cancellation before retry body release");
        assert_eq!(result, Err("Summary generation was cancelled".to_string()));
    }

    #[tokio::test]
    async fn legacy_ollama_retry_without_reasoning_effort_returns_completion() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert_eq!(body["reasoning_effort"], "none");

            let rejection_body = br#"{"error":{"param":"reasoning_effort"}}"#;
            let rejection_headers = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                rejection_body.len()
            );
            stream.write_all(rejection_headers.as_bytes()).await.unwrap();
            stream.write_all(rejection_body).await.unwrap();
            stream.flush().await.unwrap();
            drop(stream);

            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let body = request_json(&request);
            assert!(body.get("reasoning_effort").is_none());

            let completion_body =
                br#"{"choices":[{"message":{"content":"Meeting summary."}}]}"#;
            let completion_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                completion_body.len()
            );
            stream.write_all(completion_headers.as_bytes()).await.unwrap();
            stream.write_all(completion_body).await.unwrap();
            stream.flush().await.unwrap();
        });

        let client = Client::new();
        let cancellation_token = CancellationToken::new();
        let endpoint = format!("http://{address}");
        let completion = timeout(
            Duration::from_secs(1),
            generate_summary(
                &client,
                &LLMProvider::Ollama,
                "model",
                "",
                "system",
                "user",
                Some(&endpoint),
                None,
                None,
                None,
                None,
                None,
                Some(&cancellation_token),
            ),
        )
        .await;
        server.await.unwrap();

        assert_eq!(
            completion
                .expect("generation should finish after the compatibility retry")
                .unwrap(),
            LlmCompletion {
                content: "Meeting summary.".to_string(),
                reasoning_stripped: false,
            }
        );
    }

    #[test]
    fn claude_response_uses_exact_block_types() {
        let response: ClaudeChatResponse = serde_json::from_value(json!({
            "content": [
                {"type": "analysis", "text": "not visible"},
                {"type": "thinking", "thinking": "private"},
                {"type": "text", "text": "Meeting summary."}
            ]
        }))
        .unwrap();
        assert_eq!(
            response.completion(),
            Some(LlmCompletion {
                content: "Meeting summary.".to_string(),
                reasoning_stripped: true,
            })
        );
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


