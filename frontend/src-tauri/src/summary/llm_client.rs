use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

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
    pub max_completion_tokens: Option<u32>,
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
    // Only `text` blocks carry this field. Models that enable thinking by
    // default (Sonnet 5, Opus 5) also return `thinking` blocks, which don't.
    pub text: Option<String>,
}

impl ClaudeChatResponse {
    /// First block that carries text. With thinking enabled the leading block
    /// is a `thinking` block, so `content[0]` is not necessarily the answer.
    fn first_text(&self) -> Option<&str> {
        self.content.iter().find_map(|block| block.text.as_deref())
    }
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

fn model_matches_family(model: &str, family: &str) -> bool {
    model == family
        || model
            .strip_prefix(family)
            .is_some_and(|suffix| suffix.starts_with('-'))
}

fn is_reasoning_style_model(model: &str) -> bool {
    let newer_gpt5_model = ["gpt-5.1", "gpt-5.2"]
        .iter()
        .any(|family| model_matches_family(model, family));
    let gpt5_reasoning_model = model == "gpt-5"
        || ["gpt-5-mini", "gpt-5-nano", "gpt-5-pro"]
            .iter()
            .any(|family| model_matches_family(model, family));
    let reasoning_model = ["o1", "o3", "o4"]
        .iter()
        .any(|prefix| model_matches_family(model, prefix));

    newer_gpt5_model || gpt5_reasoning_model || reasoning_model
}

fn uses_max_completion_tokens(model_name: &str) -> bool {
    let model = model_name.trim().to_ascii_lowercase();
    let newer_gpt4_model = ["gpt-4.1", "gpt-4.2"]
        .iter()
        .any(|family| model_matches_family(&model, family));

    newer_gpt4_model || is_reasoning_style_model(&model)
}

fn omits_sampling_parameters(model_name: &str) -> bool {
    let model = model_name.trim().to_ascii_lowercase();

    is_reasoning_style_model(&model)
}

pub(crate) fn build_openai_chat_request(
    model_name: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
) -> ChatRequest {
    let uses_completion_tokens = uses_max_completion_tokens(model_name);
    let (max_tokens, max_completion_tokens) = if max_tokens.is_some() && uses_completion_tokens {
        (None, max_tokens)
    } else {
        (max_tokens, None)
    };
    let (temperature, top_p) = if omits_sampling_parameters(model_name) {
        (None, None)
    } else {
        (temperature, top_p)
    };

    ChatRequest {
        model: model_name.to_string(),
        messages,
        max_tokens,
        max_completion_tokens,
        temperature,
        top_p,
    }
}

pub(crate) fn build_openai_chat_request_with_max_completion_tokens(
    model_name: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
) -> ChatRequest {
    ChatRequest {
        model: model_name.to_string(),
        messages,
        max_tokens: None,
        max_completion_tokens: max_tokens,
        temperature: None,
        top_p: None,
    }
}

pub(crate) fn build_openai_chat_request_with_max_tokens(
    model_name: &str,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    top_p: Option<f32>,
) -> ChatRequest {
    ChatRequest {
        model: model_name.to_string(),
        messages,
        max_tokens,
        max_completion_tokens: None,
        temperature,
        top_p,
    }
}

fn is_unsupported_parameter_error(error_body: &str, parameter: &str) -> bool {
    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(error_body) {
        if let Some(error) = payload
            .get("error")
            .or_else(|| payload.is_object().then_some(&payload))
        {
            let rejected_parameter = error.get("param").and_then(|value| value.as_str());
            let unsupported_code = error
                .get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| {
                    code.eq_ignore_ascii_case("unsupported_parameter")
                        || code.eq_ignore_ascii_case("unknown_parameter")
                });
            let unsupported_message = error
                .get("message")
                .and_then(|value| value.as_str())
                .is_some_and(|message| text_explicitly_rejects_parameter(message, parameter));

            return match rejected_parameter {
                Some(rejected) if rejected.eq_ignore_ascii_case(parameter) => {
                    unsupported_code || unsupported_message
                }
                Some(_) => false,
                None => unsupported_code && unsupported_message,
            };
        }
    }

    text_explicitly_rejects_parameter(error_body, parameter)
}

fn text_explicitly_rejects_parameter(error: &str, parameter: &str) -> bool {
    let error = error.to_ascii_lowercase().replace(['\'', '"', '`'], "");
    let parameter = parameter.to_ascii_lowercase();

    error.contains(&format!("unsupported parameter: {parameter}"))
        || error.contains(&format!("unsupported parameter {parameter}"))
        || error.contains(&format!("unknown parameter: {parameter}"))
        || error.contains(&format!("unknown parameter {parameter}"))
        || error.contains(&format!("parameter {parameter} is not supported"))
        || error.contains(&format!("{parameter} parameter is not supported"))
        || error.contains(&format!("{parameter} is not supported"))
        || error.contains(&format!("does not support {parameter}"))
}

pub(crate) fn is_max_tokens_unsupported_error(error_body: &str) -> bool {
    is_unsupported_parameter_error(error_body, "max_tokens")
}

pub(crate) fn is_max_completion_tokens_unsupported_error(error_body: &str) -> bool {
    is_unsupported_parameter_error(error_body, "max_completion_tokens")
}

fn openai_chat_messages(system_prompt: &str, user_prompt: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_prompt.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt.to_string(),
        },
    ]
}

async fn send_chat_request(
    client: &Client,
    api_url: &str,
    headers: &header::HeaderMap,
    request_body: &serde_json::Value,
    cancellation_token: Option<&CancellationToken>,
) -> Result<reqwest::Response, String> {
    let request_future = client
        .post(api_url)
        .headers(headers.clone())
        .json(request_body)
        .timeout(REQUEST_TIMEOUT_DURATION)
        .send();

    if let Some(token) = cancellation_token {
        tokio::select! {
            result = request_future => {
                result.map_err(|e| {
                    if e.is_timeout() {
                        format!("LLM request timed out after 60 seconds")
                    } else {
                        format!("Failed to send request to LLM: {}", e)
                    }
                })
            }
            _ = token.cancelled() => {
                Err("Summary generation was cancelled".to_string())
            }
        }
    } else {
        request_future.await.map_err(|e| {
            if e.is_timeout() {
                format!("LLM request timed out after 60 seconds")
            } else {
                format!("Failed to send request to LLM: {}", e)
            }
        })
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

    // For CustomOpenAI, apply optional parameters if provided
    let (max_tokens_val, temperature_val, top_p_val) = if provider == &LLMProvider::CustomOpenAI {
        (max_tokens, temperature, top_p)
    } else {
        (None, None, None)
    };

    // Build request body based on provider
    let mut request_body = if provider != &LLMProvider::Claude {
        serde_json::json!(build_openai_chat_request(
            model_name,
            openai_chat_messages(system_prompt, user_prompt),
            max_tokens_val,
            temperature_val,
            top_p_val,
        ))
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

    let mut response = send_chat_request(
        client,
        &api_url,
        &headers,
        &request_body,
        cancellation_token,
    )
    .await?;

    if !response.status().is_success() {
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());

        let rejected_sampling_parameter = (request_body.get("temperature").is_some()
            && is_unsupported_parameter_error(&error_body, "temperature"))
            || (request_body.get("top_p").is_some()
                && is_unsupported_parameter_error(&error_body, "top_p"));

        if provider == &LLMProvider::CustomOpenAI && rejected_sampling_parameter {
            info!("Retrying Custom OpenAI request without sampling parameters");

            if let Some(request) = request_body.as_object_mut() {
                request.remove("temperature");
                request.remove("top_p");
            }

            response = send_chat_request(
                client,
                &api_url,
                &headers,
                &request_body,
                cancellation_token,
            )
            .await?;

            if !response.status().is_success() {
                let retry_error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("LLM API request failed: {}", retry_error_body));
            }
        } else if provider == &LLMProvider::CustomOpenAI
            && request_body.get("max_tokens").is_some()
            && is_max_tokens_unsupported_error(&error_body)
        {
            info!("Retrying Custom OpenAI request with max_completion_tokens");

            let retry_body =
                serde_json::json!(build_openai_chat_request_with_max_completion_tokens(
                    model_name,
                    openai_chat_messages(system_prompt, user_prompt),
                    max_tokens_val,
                ));

            response =
                send_chat_request(client, &api_url, &headers, &retry_body, cancellation_token)
                    .await?;

            if !response.status().is_success() {
                let retry_error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("LLM API request failed: {}", retry_error_body));
            }
        } else if provider == &LLMProvider::CustomOpenAI
            && request_body.get("max_completion_tokens").is_some()
            && is_max_completion_tokens_unsupported_error(&error_body)
        {
            info!("Retrying Custom OpenAI request with max_tokens");

            let retry_body = serde_json::json!(build_openai_chat_request_with_max_tokens(
                model_name,
                openai_chat_messages(system_prompt, user_prompt),
                max_tokens_val,
                temperature_val,
                top_p_val,
            ));

            response =
                send_chat_request(client, &api_url, &headers, &retry_body, cancellation_token)
                    .await?;

            if !response.status().is_success() {
                let retry_error_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("LLM API request failed: {}", retry_error_body));
            }
        } else {
            return Err(format!("LLM API request failed: {}", error_body));
        }
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = response
            .json::<ClaudeChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

        let content = chat_response
            .first_text()
            .ok_or("No text content in LLM response")?
            .trim();
        Ok(content.to_string())
    } else {
        let chat_response = response
            .json::<ChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from {}", provider_name(provider));

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
pub(crate) mod test_support {
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    pub(crate) async fn spawn_chat_server(
        responses: Vec<(u16, Value)>,
    ) -> (
        String,
        mpsc::UnboundedReceiver<Value>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_sender, request_receiver) = mpsc::unbounded_channel();

        let server = tokio::spawn(async move {
            for (status, response_body) in responses {
                let (mut socket, _) =
                    tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept())
                        .await
                        .expect("expected another chat request")
                        .unwrap();
                let request_body = read_request_body(&mut socket).await;
                request_sender.send(request_body).unwrap();

                let response_body = response_body.to_string();
                let reason = match status {
                    200 => "OK",
                    400 => "Bad Request",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    _ => "Response",
                };
                let headers = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                socket.write_all(headers.as_bytes()).await.unwrap();
                socket.write_all(response_body.as_bytes()).await.unwrap();
                socket.flush().await.unwrap();
            }
        });

        (format!("http://{address}"), request_receiver, server)
    }

    async fn read_request_body(socket: &mut tokio::net::TcpStream) -> Value {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];

        loop {
            let bytes_read = socket.read(&mut buffer).await.unwrap();
            assert!(bytes_read > 0, "request ended before its JSON body arrived");
            request.extend_from_slice(&buffer[..bytes_read]);

            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let body_start = header_end + 4;
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);

            if request.len() >= body_start + content_length {
                return serde_json::from_slice(&request[body_start..body_start + content_length])
                    .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use test_support::spawn_chat_server;

    fn request_json_for_model(model: &str, max_tokens: Option<u32>) -> serde_json::Value {
        serde_json::to_value(build_openai_chat_request(
            model,
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            max_tokens,
            None,
            None,
        ))
        .unwrap()
    }

    #[test]
    fn newer_azure_chat_models_use_max_completion_tokens() {
        for model in [
            "gpt-5",
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-5-pro",
            "gpt-5.1-chat",
            "gpt-5.2-chat",
            "gpt-4.1-mini",
            "o3-mini",
        ] {
            let payload = request_json_for_model(model, Some(512));

            assert_eq!(payload["max_completion_tokens"], 512);
            assert!(
                payload.get("max_tokens").is_none(),
                "{model} still used max_tokens"
            );
        }
    }

    #[test]
    fn legacy_custom_models_keep_max_tokens() {
        for model in [
            "gpt-5-chat",
            "llama-3.1-local",
            "custom-summary-deployment",
            "legacy-gpt-5.1-shim",
            "gpt-4.10-deployment",
        ] {
            let payload = request_json_for_model(model, Some(512));

            assert_eq!(payload["max_tokens"], 512);
            assert!(
                payload.get("max_completion_tokens").is_none(),
                "{model} unexpectedly used max_completion_tokens"
            );
        }
    }

    #[test]
    fn missing_token_limit_omits_both_token_fields() {
        let payload = request_json_for_model("gpt-5.1-chat", None);

        assert!(payload.get("max_tokens").is_none());
        assert!(payload.get("max_completion_tokens").is_none());
    }

    #[test]
    fn forced_completion_token_payload_handles_arbitrary_deployment_names() {
        let payload = serde_json::to_value(build_openai_chat_request_with_max_completion_tokens(
            "prod-summary-deployment",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
        ))
        .unwrap();

        assert_eq!(payload["max_completion_tokens"], 512);
        assert!(payload.get("max_tokens").is_none());
    }

    #[test]
    fn reasoning_model_payloads_omit_unsupported_sampling_parameters() {
        let payload = serde_json::to_value(build_openai_chat_request(
            "gpt-5.1-chat",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
            Some(0.7),
            Some(0.9),
        ))
        .unwrap();

        assert_eq!(payload["max_completion_tokens"], 512);
        assert!(payload.get("temperature").is_none());
        assert!(payload.get("top_p").is_none());
    }

    #[test]
    fn gpt_4_completion_payloads_keep_sampling_parameters() {
        let payload = serde_json::to_value(build_openai_chat_request(
            "gpt-4.1-mini",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
            Some(0.7),
            Some(0.9),
        ))
        .unwrap();

        assert_eq!(payload["max_completion_tokens"], 512);
        assert_eq!(payload["temperature"], 0.7);
        assert_eq!(payload["top_p"], 0.9);
    }

    #[test]
    fn completion_token_retry_omits_unsupported_sampling_parameters() {
        let payload = serde_json::to_value(build_openai_chat_request_with_max_completion_tokens(
            "prod-summary-deployment",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
        ))
        .unwrap();

        assert_eq!(payload["max_completion_tokens"], 512);
        assert!(payload.get("temperature").is_none());
        assert!(payload.get("top_p").is_none());
    }

    #[test]
    fn forced_max_tokens_payload_handles_legacy_compatible_endpoints() {
        let payload = serde_json::to_value(build_openai_chat_request_with_max_tokens(
            "gpt-4.1-mini",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
            None,
            None,
        ))
        .unwrap();

        assert_eq!(payload["max_tokens"], 512);
        assert!(payload.get("max_completion_tokens").is_none());
    }

    #[test]
    fn legacy_max_token_payloads_keep_sampling_parameters() {
        let payload = serde_json::to_value(build_openai_chat_request(
            "gpt-5-chat",
            vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            Some(512),
            Some(0.7),
            Some(0.9),
        ))
        .unwrap();

        assert_eq!(payload["max_tokens"], 512);
        assert_eq!(payload["temperature"], 0.7);
        assert_eq!(payload["top_p"], 0.9);
    }

    #[test]
    fn detects_max_tokens_unsupported_errors() {
        assert!(is_max_tokens_unsupported_error(
            r#"{"error":{"code":"unsupported_parameter","param":"max_tokens"}}"#
        ));
        assert!(is_max_tokens_unsupported_error(
            "The max_tokens parameter is not supported by this model"
        ));
        assert!(is_max_tokens_unsupported_error(
            r#""The max_tokens parameter is not supported by this model""#
        ));
        assert!(!is_max_tokens_unsupported_error(
            r#"{"error":{"code":"invalid_api_key"}}"#
        ));
    }

    #[test]
    fn detects_max_completion_tokens_unsupported_errors() {
        assert!(is_max_completion_tokens_unsupported_error(
            r#"{"error":{"code":"unsupported_parameter","param":"max_completion_tokens"}}"#
        ));
        assert!(is_max_completion_tokens_unsupported_error(
            "The max_completion_tokens parameter is not supported by this endpoint"
        ));
        assert!(!is_max_completion_tokens_unsupported_error(
            r#"{"error":{"code":"unsupported_parameter","param":"max_tokens"}}"#
        ));
    }

    #[test]
    fn ignores_unrelated_unsupported_parameters_that_mention_token_fields() {
        assert!(!is_max_tokens_unsupported_error(
            r#"{"error":{"code":"unsupported_parameter","param":"temperature","message":"temperature is unsupported; max_tokens remains allowed"}}"#
        ));
        assert!(!is_max_completion_tokens_unsupported_error(
            r#"{"error":{"code":"unsupported_parameter","param":"top_p","message":"top_p is unsupported; max_completion_tokens remains allowed"}}"#
        ));
    }

    fn successful_chat_response(content: &str) -> serde_json::Value {
        json!({"choices": [{"message": {"content": content}}]})
    }

    async fn custom_summary(
        endpoint: &str,
        model: &str,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
    ) -> Result<String, String> {
        generate_summary(
            &Client::new(),
            &LLMProvider::CustomOpenAI,
            model,
            "test-key",
            "system",
            "user",
            None,
            Some(endpoint),
            max_tokens,
            temperature,
            top_p,
            None,
            None,
        )
        .await
    }

    #[tokio::test]
    async fn summary_retries_max_tokens_with_sampling_omitted() {
        let (endpoint, mut requests, server) = spawn_chat_server(vec![
            (
                400,
                json!({"error": {"code": "unsupported_parameter", "param": "max_tokens"}}),
            ),
            (200, successful_chat_response("retried")),
        ])
        .await;

        let summary = custom_summary(
            &endpoint,
            "azure-deployment",
            Some(512),
            Some(0.7),
            Some(0.9),
        )
        .await
        .unwrap();
        server.await.unwrap();

        assert_eq!(summary, "retried");
        let first = requests.recv().await.unwrap();
        assert_eq!(first["max_tokens"], 512);
        assert_eq!(first["temperature"], 0.7);
        assert_eq!(first["top_p"], 0.9);
        let second = requests.recv().await.unwrap();
        assert_eq!(second["max_completion_tokens"], 512);
        assert!(second.get("max_tokens").is_none());
        assert!(second.get("temperature").is_none());
        assert!(second.get("top_p").is_none());
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn summary_retries_without_sampling_while_preserving_max_tokens() {
        let (endpoint, mut requests, server) = spawn_chat_server(vec![
            (
                400,
                json!({"error": {"code": "unsupported_parameter", "param": "temperature"}}),
            ),
            (200, successful_chat_response("sampling-free retry")),
        ])
        .await;

        let summary = custom_summary(
            &endpoint,
            "azure-deployment",
            Some(512),
            Some(0.7),
            Some(0.9),
        )
        .await
        .unwrap();
        server.await.unwrap();

        assert_eq!(summary, "sampling-free retry");
        let first = requests.recv().await.unwrap();
        assert_eq!(first["max_tokens"], 512);
        assert_eq!(first["temperature"], 0.7);
        assert_eq!(first["top_p"], 0.9);
        let second = requests.recv().await.unwrap();
        assert_eq!(second["max_tokens"], 512);
        assert!(second.get("max_completion_tokens").is_none());
        assert!(second.get("temperature").is_none());
        assert!(second.get("top_p").is_none());
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn summary_retries_without_sampling_while_preserving_completion_tokens() {
        let (endpoint, mut requests, server) = spawn_chat_server(vec![
            (
                400,
                json!({"error": {"code": "unsupported_parameter", "param": "top_p"}}),
            ),
            (200, successful_chat_response("sampling-free GPT-4 retry")),
        ])
        .await;

        let summary = custom_summary(&endpoint, "gpt-4.1-mini", Some(512), Some(0.7), Some(0.9))
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(summary, "sampling-free GPT-4 retry");
        let first = requests.recv().await.unwrap();
        assert_eq!(first["max_completion_tokens"], 512);
        assert_eq!(first["temperature"], 0.7);
        assert_eq!(first["top_p"], 0.9);
        let second = requests.recv().await.unwrap();
        assert_eq!(second["max_completion_tokens"], 512);
        assert!(second.get("max_tokens").is_none());
        assert!(second.get("temperature").is_none());
        assert!(second.get("top_p").is_none());
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn summary_retries_max_completion_tokens_with_sampling_restored() {
        let (endpoint, mut requests, server) = spawn_chat_server(vec![
            (
                400,
                json!({"error": {"code": "unsupported_parameter", "param": "max_completion_tokens"}}),
            ),
            (200, successful_chat_response("legacy retry")),
        ])
        .await;

        let summary = custom_summary(&endpoint, "gpt-5.1-chat", Some(512), Some(0.7), Some(0.9))
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(summary, "legacy retry");
        let first = requests.recv().await.unwrap();
        assert_eq!(first["max_completion_tokens"], 512);
        assert!(first.get("temperature").is_none());
        assert!(first.get("top_p").is_none());
        let second = requests.recv().await.unwrap();
        assert_eq!(second["max_tokens"], 512);
        assert!(second.get("max_completion_tokens").is_none());
        assert_eq!(second["temperature"], 0.7);
        assert_eq!(second["top_p"], 0.9);
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn summary_does_not_retry_an_unrelated_unsupported_parameter() {
        let original_error = json!({
            "error": {
                "code": "unsupported_parameter",
                "param": "frequency_penalty",
                "message": "frequency_penalty is unsupported; max_tokens remains allowed"
            }
        });
        let (endpoint, mut requests, server) =
            spawn_chat_server(vec![(400, original_error.clone())]).await;

        let error = custom_summary(&endpoint, "azure-deployment", Some(512), Some(0.7), None)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert_eq!(error, format!("LLM API request failed: {original_error}"));
        assert!(requests.recv().await.is_some());
        assert!(requests.try_recv().is_err());
    }

    #[tokio::test]
    async fn summary_returns_the_terminal_retry_failure_without_a_third_request() {
        let retry_error = json!({"error": {"code": "rate_limit", "message": "retry failed"}});
        let (endpoint, mut requests, server) = spawn_chat_server(vec![
            (
                400,
                json!({"error": {"code": "unsupported_parameter", "param": "max_tokens"}}),
            ),
            (429, retry_error.clone()),
        ])
        .await;

        let error = custom_summary(&endpoint, "azure-deployment", Some(512), None, None)
            .await
            .unwrap_err();
        server.await.unwrap();

        assert_eq!(error, format!("LLM API request failed: {retry_error}"));
        assert!(requests.recv().await.is_some());
        assert!(requests.recv().await.is_some());
        assert!(requests.try_recv().is_err());
    }

    #[test]
    fn claude_response_skips_leading_thinking_block() {
        // Shape returned by models with thinking enabled by default,
        // e.g. claude-sonnet-5 / claude-opus-5.
        let response: ClaudeChatResponse = serde_json::from_value(json!({
            "content": [
                {"type": "thinking", "thinking": "", "signature": "abc"},
                {"type": "text", "text": "Meeting summary."}
            ]
        }))
        .expect("thinking blocks must not fail deserialization");

        assert_eq!(response.first_text(), Some("Meeting summary."));
    }

    #[test]
    fn claude_response_reads_plain_text_block() {
        let response: ClaudeChatResponse = serde_json::from_value(json!({
            "content": [{"type": "text", "text": "Meeting summary."}]
        }))
        .unwrap();

        assert_eq!(response.first_text(), Some("Meeting summary."));
    }

    #[test]
    fn claude_response_without_text_block_returns_none() {
        let response: ClaudeChatResponse = serde_json::from_value(json!({
            "content": [{"type": "thinking", "thinking": "", "signature": "abc"}]
        }))
        .unwrap();

        assert_eq!(response.first_text(), None);
    }
}
