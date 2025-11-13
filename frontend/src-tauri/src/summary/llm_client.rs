use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use tracing::info;

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
            _ => Err(format!("Unsupported LLM provider: {}", s)),
        }
    }
}

/// Generates a summary using the specified LLM provider
///
/// # Detailed Documentation - Date: 13/11/2025 - Author: Luiz
///
/// This method implements a unified HTTP client for communication with 5 different
/// LLM providers. It abstracts API differences between providers and provides
/// a consistent interface for summary generation.
///
/// # Supported Providers and Their Specifics:
///
/// **1. OpenAI** (api.openai.com)
/// - Format: OpenAI Chat Completions API
/// - Auth: Bearer token via Authorization header
/// - Models: gpt-4, gpt-4-turbo, gpt-3.5-turbo
///
/// **2. Claude** (api.anthropic.com)
/// - Format: Anthropic Messages API (DIFFERENT from OpenAI)
/// - Auth: x-api-key header + anthropic-version header
/// - Max tokens: Fixed at 2048 for summaries
/// - Models: claude-3-opus, claude-3-sonnet, claude-3-haiku
/// - Peculiarity: System prompt is separate field, not a message
///
/// **3. Groq** (api.groq.com)
/// - Format: OpenAI-compatible API
/// - Auth: Bearer token
/// - Models: llama-3.1-70b-versatile, mixtral-8x7b-32768
///
/// **4. Ollama** (localhost:11434 or custom endpoint)
/// - Format: OpenAI-compatible API
/// - Auth: None (local execution)
/// - Configurable endpoint via settings
/// - Models: Any locally installed model
///
/// **5. OpenRouter** (openrouter.ai)
/// - Format: OpenAI-compatible API
/// - Auth: Bearer token
/// - Models: Access to multiple providers via proxy
///
/// # Processing Flow:
///
/// **STEP 1: Endpoint and Headers Configuration (lines 199-237)**
/// - Match on provider to determine base URL
/// - Claude: Adds x-api-key and anthropic-version headers
/// - Others: Adds Authorization: Bearer {api_key}
/// - Ollama: Uses custom endpoint or localhost:11434
///
/// **STEP 2: Request Body Construction (lines 256-280)**
/// - OpenAI-compatible providers (OpenAI, Groq, Ollama, OpenRouter):
///   ```json
///   {
///     "model": "gpt-4",
///     "messages": [
///       {"role": "system", "content": "..."},
///       {"role": "user", "content": "..."}
///     ]
///   }
///   ```
/// - Claude (different format):
///   ```json
///   {
///     "model": "claude-3-opus",
///     "max_tokens": 2048,
///     "system": "...",
///     "messages": [
///       {"role": "user", "content": "..."}
///     ]
///   }
///   ```
///
/// **STEP 3: Request Sending (lines 285-299)**
/// - POST to api_url with headers and JSON body
/// - Default reqwest::Client timeout
/// - Returns error if HTTP status ≠ 2xx
///
/// **STEP 4: Response Parsing (lines 302-334)**
/// - Claude: Uses ClaudeChatResponse → content[0].text
/// - Others: Uses ChatResponse → choices[0].message.content
/// - Trim to remove whitespace
///
/// # Usage Example:
///
/// ```rust
/// let summary = generate_summary(
///     &client,
///     &LLMProvider::OpenAI,
///     "gpt-4",
///     "sk-...",
///     "You are an expert meeting summarizer.",
///     "Summarize this transcript: ...",
///     None
/// ).await?;
/// ```
///
/// # Error Handling:
///
/// - Network failure → "Failed to send request to LLM: {error}"
/// - HTTP status ≠ 2xx → "LLM API request failed: {response_body}"
/// - JSON parsing error → "Failed to parse LLM response: {error}"
/// - Empty response → "No content in LLM response"
///
/// # Arguments
/// * `client` - Reqwest HTTP client (reused for performance)
/// * `provider` - The LLM provider to use
/// * `model_name` - The specific model to use (e.g., "gpt-4", "claude-3-opus")
/// * `api_key` - API key for the provider (not needed for Ollama)
/// * `system_prompt` - System instructions for the LLM
/// * `user_prompt` - User query/content to process
/// * `ollama_endpoint` - Optional custom Ollama endpoint (defaults to localhost:11434)
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
) -> Result<String, String> {
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

    info!("🐞 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request
    let response = client
        .post(api_url)
        .headers(headers)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request to LLM: {}", e))?;

    if !response.status().is_success() {
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("LLM API request failed: {}", error_body));
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = response
            .json::<ClaudeChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

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
        LLMProvider::OpenRouter => "OpenRouter",
    }
}
