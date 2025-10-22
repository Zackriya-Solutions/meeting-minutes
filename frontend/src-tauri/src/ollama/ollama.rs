use std::process::Command;
use std::sync::Arc;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, Emitter, Runtime};
use reqwest::Client;
use tokio::time::{timeout, Duration, sleep};
use tokio::sync::RwLock;
use futures_util::StreamExt;
use once_cell::sync::Lazy;
use crate::ollama::metadata::ModelMetadataCache;

// Global set to track models currently being downloaded
static DOWNLOADING_MODELS: Lazy<Arc<RwLock<HashSet<String>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(HashSet::new()))
});

// Global cache for model metadata (5 minute TTL)
static METADATA_CACHE: Lazy<ModelMetadataCache> = Lazy::new(|| {
    ModelMetadataCache::new(Duration::from_secs(300))
});

// Error categorization for better error handling and user feedback
#[derive(Debug)]
pub enum OllamaError {
    Timeout,
    NetworkError(String),
    InvalidEndpoint(String),
    ServerError(String),
    NoModelsFound,
    ParseError(String),
}

impl std::fmt::Display for OllamaError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OllamaError::Timeout => write!(f, "Request timed out after 5 seconds. Please check if the Ollama server is running."),
            OllamaError::NetworkError(msg) => write!(f, "Network error: {}. Please check your connection and endpoint URL.", msg),
            OllamaError::InvalidEndpoint(msg) => write!(f, "Invalid endpoint: {}. Please check the URL format.", msg),
            OllamaError::ServerError(msg) => write!(f, "Ollama server error: {}", msg),
            OllamaError::NoModelsFound => write!(f, "No models found on the Ollama server. Please pull models using 'ollama pull <model>'."),
            OllamaError::ParseError(msg) => write!(f, "Failed to parse server response: {}", msg),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub id: String,
    pub size: String,
    pub modified: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaApiResponse {
    models: Vec<OllamaApiModel>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaApiModel {
    name: String,
    model: String,
    modified_at: String,
    size: i64,
}

// Helper function to check if endpoint is localhost
fn is_localhost_endpoint(endpoint: Option<&str>) -> bool {
    match endpoint {
        None | Some("") => true,
        Some(url) => {
            url.contains("localhost") ||
            url.contains("127.0.0.1") ||
            url.contains("::1")
        }
    }
}

// Helper function to validate endpoint URL format
fn validate_endpoint_url(url: &str) -> Result<(), OllamaError> {
    if url.is_empty() {
        return Ok(()); // Empty is valid (uses default)
    }

    // Check if URL starts with http:// or https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(OllamaError::InvalidEndpoint(
            "URL must start with http:// or https://".to_string()
        ));
    }

    Ok(())
}

#[command]
pub async fn get_ollama_models(endpoint: Option<String>) -> Result<Vec<OllamaModel>, String> {
    // Validate endpoint format if provided
    if let Some(ref ep) = endpoint {
        if let Err(e) = validate_endpoint_url(ep) {
            return Err(e.to_string());
        }
    }

    // Add timeout wrapper (5 seconds max)
    match timeout(
        Duration::from_secs(5),
        get_models_via_http_with_retry(endpoint.as_deref())
    ).await {
        Ok(Ok(models)) => {
            if models.is_empty() {
                Err(OllamaError::NoModelsFound.to_string())
            } else {
                Ok(models)
            }
        }
        Ok(Err(http_err)) => {
            // Only fallback to CLI if endpoint is localhost/empty
            if is_localhost_endpoint(endpoint.as_deref()) {
                get_models_via_cli().map_err(|cli_err| {
                    format!("{}\n\nAlso tried CLI: {}", http_err, cli_err)
                })
            } else {
                Err(http_err)
            }
        }
        Err(_) => Err(OllamaError::Timeout.to_string()),
    }
}

// HTTP request with retry logic and exponential backoff
async fn get_models_via_http_with_retry(endpoint: Option<&str>) -> Result<Vec<OllamaModel>, String> {
    const MAX_RETRIES: u32 = 2;
    const INITIAL_BACKOFF_MS: u64 = 300;

    let mut last_error = String::new();

    for attempt in 0..=MAX_RETRIES {
        match get_models_via_http_async(endpoint).await {
            Ok(models) => return Ok(models),
            Err(e) => {
                last_error = e.clone();

                // Don't retry on certain errors
                if e.contains("Invalid endpoint") || e.contains("404") {
                    return Err(e);
                }

                // If not the last attempt, wait with exponential backoff
                if attempt < MAX_RETRIES {
                    let backoff_duration = INITIAL_BACKOFF_MS * 2_u64.pow(attempt);
                    sleep(Duration::from_millis(backoff_duration)).await;
                }
            }
        }
    }

    Err(format!("Failed after {} retries: {}", MAX_RETRIES, last_error))
}

async fn get_models_via_http_async(endpoint: Option<&str>) -> Result<Vec<OllamaModel>, String> {
    let client = Client::new();
    let base_url = endpoint.unwrap_or("http://localhost:11434");
    let url = format!("{}/api/tags", base_url);

    let response = client
        .get(&url)
        .timeout(Duration::from_secs(3)) // Per-request timeout
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                OllamaError::NetworkError("Connection timed out".to_string()).to_string()
            } else if e.is_connect() {
                OllamaError::NetworkError(format!("Cannot connect to {}. Please check if the server is running.", base_url)).to_string()
            } else {
                OllamaError::NetworkError(e.to_string()).to_string()
            }
        })?;

    if !response.status().is_success() {
        return Err(OllamaError::ServerError(
            format!("HTTP {}: Server returned an error", response.status())
        ).to_string());
    }

    let api_response: OllamaApiResponse = response
        .json()
        .await
        .map_err(|e| OllamaError::ParseError(e.to_string()).to_string())?;

    Ok(api_response.models.into_iter().map(|m| OllamaModel {
        name: m.name,
        id: m.model,
        size: format_size(m.size),
        modified: m.modified_at,
    }).collect())
}

fn get_models_via_cli() -> Result<Vec<OllamaModel>, String> {
    let output = Command::new("ollama")
        .arg("list")
        .output()
        .map_err(|e| {
            OllamaError::NetworkError(
                format!("Ollama CLI not found or not in PATH: {}", e)
            ).to_string()
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OllamaError::ServerError(
            format!("Ollama CLI error: {}", stderr)
        ).to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();

    // Skip the header line
    for line in output_str.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            models.push(OllamaModel {
                name: parts[0].to_string(),
                id: parts[1].to_string(),
                size: format!("{} {}", parts[2], parts[3]),
                modified: parts[4..].join(" "),
            });
        }
    }

    if models.is_empty() {
        return Err(OllamaError::NoModelsFound.to_string());
    }

    Ok(models)
}

fn format_size(size: i64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

#[command]
pub async fn pull_ollama_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
    endpoint: Option<String>,
) -> Result<(), String> {
    // Check if model is already being downloaded
    {
        let downloading = DOWNLOADING_MODELS.read().await;
        if downloading.contains(&model_name) {
            log::warn!("Model {} is already being downloaded, ignoring duplicate request", model_name);
            return Err(format!("Model {} is already being downloaded", model_name));
        }
    }

    // Mark model as downloading
    {
        let mut downloading = DOWNLOADING_MODELS.write().await;
        downloading.insert(model_name.clone());
        log::info!("Started download tracking for model: {}", model_name);
    }

    let client = Client::new();
    let base_url = endpoint.as_deref().unwrap_or("http://localhost:11434");
    let url = format!("{}/api/pull", base_url);

    let payload = serde_json::json!({
        "name": model_name,
        "stream": true
    });

    let response = client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(600)) // 10 minutes timeout for pulling
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("Download timed out. The model may be large, please try using the Ollama CLI: ollama pull {}", model_name)
            } else if e.is_connect() {
                format!("Cannot connect to {}. Please check if the Ollama server is running.", base_url)
            } else {
                format!("Failed to download model: {}", e)
            }
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        // Remove from downloading set on error
        {
            let mut downloading = DOWNLOADING_MODELS.write().await;
            downloading.remove(&model_name);
        }

        // Emit error event
        let _ = app_handle.emit(
            "ollama-model-download-error",
            serde_json::json!({
                "modelName": model_name,
                "error": format!("HTTP {}: {}", status, error_text)
            }),
        );

        return Err(format!("Failed to pull model (HTTP {}): {}", status, error_text));
    }

    // Process streaming response (NDJSON format)
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut last_progress = 0u8;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            let error_msg = format!("Failed to read stream: {}", e);

            // Remove from downloading set on stream error
            let model_name_clone = model_name.clone();
            tokio::spawn(async move {
                let mut downloading = DOWNLOADING_MODELS.write().await;
                downloading.remove(&model_name_clone);
            });

            let _ = app_handle.emit(
                "ollama-model-download-error",
                serde_json::json!({
                    "modelName": model_name,
                    "error": error_msg
                }),
            );
            error_msg
        })?;

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            // Parse JSON line
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                // Extract progress if available
                if let (Some(completed), Some(total)) = (
                    json.get("completed").and_then(|v| v.as_u64()),
                    json.get("total").and_then(|v| v.as_u64()),
                ) {
                    if total > 0 {
                        let progress = ((completed as f64 / total as f64) * 100.0) as u8;

                        // Only emit if progress changed significantly (reduces event spam)
                        if progress != last_progress && (progress - last_progress >= 1 || progress == 100) {
                            log::info!("Ollama download progress for {}: {}%", model_name, progress);

                            let _ = app_handle.emit(
                                "ollama-model-download-progress",
                                serde_json::json!({
                                    "modelName": model_name,
                                    "progress": progress
                                }),
                            );

                            last_progress = progress;
                        }
                    }
                }

                // Check for error status
                if let Some(error) = json.get("error").and_then(|v| v.as_str()) {
                    let error_msg = format!("Ollama error: {}", error);

                    // Remove from downloading set on Ollama error
                    {
                        let mut downloading = DOWNLOADING_MODELS.write().await;
                        downloading.remove(&model_name);
                    }

                    let _ = app_handle.emit(
                        "ollama-model-download-error",
                        serde_json::json!({
                            "modelName": model_name,
                            "error": error_msg
                        }),
                    );
                    return Err(error_msg);
                }
            }
        }
    }

    // Remove from downloading set before emitting completion
    {
        let mut downloading = DOWNLOADING_MODELS.write().await;
        downloading.remove(&model_name);
        log::info!("Removed {} from downloading set", model_name);
    }

    // Emit completion event
    let _ = app_handle.emit(
        "ollama-model-download-complete",
        serde_json::json!({
            "modelName": model_name
        }),
    );

    log::info!("Ollama model {} downloaded successfully", model_name);

    Ok(())
}

#[command]
pub async fn delete_ollama_model(
    model_name: String,
    endpoint: Option<String>,
) -> Result<(), String> {
    let client = Client::new();
    let base_url = endpoint.as_deref().unwrap_or("http://localhost:11434");
    let url = format!("{}/api/delete", base_url);

    let payload = serde_json::json!({
        "name": model_name
    });

    log::info!("Deleting Ollama model: {}", model_name);

    let response = client
        .delete(&url)
        .json(&payload)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("Delete request timed out for model: {}", model_name)
            } else if e.is_connect() {
                format!("Cannot connect to {}. Please check if the Ollama server is running.", base_url)
            } else {
                format!("Failed to delete model: {}", e)
            }
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to delete model (HTTP {}): {}", status, error_text));
    }

    log::info!("Successfully deleted Ollama model: {}", model_name);

    Ok(())
}

/// Get the context size for a specific Ollama model
///
/// This command fetches model metadata and returns the context size.
/// Results are cached for 5 minutes to avoid repeated API calls.
///
/// # Arguments
/// * `model_name` - Name of the model (e.g., "llama3.2:1b")
/// * `endpoint` - Optional custom Ollama endpoint
///
/// # Returns
/// Context size in tokens, or error message
#[command]
pub async fn get_ollama_model_context(
    model_name: String,
    endpoint: Option<String>,
) -> Result<usize, String> {
    log::info!("Fetching context size for model: {}", model_name);

    match METADATA_CACHE.get_or_fetch(&model_name, endpoint.as_deref()).await {
        Ok(metadata) => {
            log::info!(
                "Model {} context size: {} tokens",
                model_name,
                metadata.context_size
            );
            Ok(metadata.context_size)
        }
        Err(e) => {
            log::warn!(
                "Failed to fetch context for {}: {}. Returning default 4000",
                model_name,
                e
            );
            // Return default instead of error for better UX
            Ok(4000)
        }
    }
}
