use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use reqwest::Client;
use regex::Regex;
use once_cell::sync::Lazy;

/// Model metadata containing context size and other details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    pub name: String,
    pub context_size: usize,
    pub parameter_count: String,
    pub family: String,
}

/// Response structure from Ollama /api/show endpoint
#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    modelfile: String,
    #[serde(default)]
    details: ModelDetails,
    #[serde(default)]
    model_info: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct ModelDetails {
    #[serde(default)]
    family: String,
    #[serde(default)]
    parameter_size: String,
}

/// Cache entry with timestamp for TTL management
struct CacheEntry {
    metadata: ModelMetadata,
    fetched_at: Instant,
}

/// Thread-safe cache for model metadata with TTL
pub struct ModelMetadataCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    ttl: Duration,
}

impl ModelMetadataCache {
    /// Create a new metadata cache with the specified TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
        }
    }

    /// Get metadata from cache or fetch from API
    ///
    /// # Arguments
    /// * `model_name` - Name of the model (e.g., "llama3.2:1b")
    /// * `endpoint` - Optional custom Ollama endpoint
    ///
    /// # Returns
    /// ModelMetadata on success, error message on failure
    pub async fn get_or_fetch(
        &self,
        model_name: &str,
        endpoint: Option<&str>,
    ) -> Result<ModelMetadata, String> {
        let cache_key = format!("{}::{}", model_name, endpoint.unwrap_or("default"));

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                // Check if entry is still valid (within TTL)
                if entry.fetched_at.elapsed() < self.ttl {
                    tracing::debug!(
                        "Cache hit for model {}: context_size={}",
                        model_name,
                        entry.metadata.context_size
                    );
                    return Ok(entry.metadata.clone());
                }
            }
        }

        // Cache miss or expired - fetch from API
        tracing::info!("Fetching metadata for model: {}", model_name);
        let metadata = fetch_model_info(model_name, endpoint).await?;

        // Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                cache_key,
                CacheEntry {
                    metadata: metadata.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(metadata)
    }

    /// Clear all cached entries (useful for testing or manual refresh)
    #[allow(dead_code)]
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        tracing::info!("Model metadata cache cleared");
    }
}

/// Default context sizes for common model families (fallback when API fails)
const DEFAULT_CONTEXT_SIZES: &[(&str, usize)] = &[
    ("llama", 4096),
    ("mistral", 8192),
    ("phi", 2048),
    ("qwen", 8192),
    ("gemma", 8192),
    ("codellama", 16384),
    ("deepseek", 16384),
    ("neural-chat", 4096),
];

/// Ultimate fallback context size when model family is unknown
const ULTIMATE_FALLBACK: usize = 4000;

/// Fetch model information from Ollama API
///
/// # Arguments
/// * `model_name` - Name of the model
/// * `endpoint` - Optional custom Ollama endpoint
///
/// # Returns
/// ModelMetadata on success, error string on failure
async fn fetch_model_info(
    model_name: &str,
    endpoint: Option<&str>,
) -> Result<ModelMetadata, String> {
    let client = Client::new();
    let base_url = endpoint.unwrap_or("http://localhost:11434");
    let url = format!("{}/api/show", base_url);

    let payload = serde_json::json!({
        "name": model_name,
        "verbose": true
    });

    let response = client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("Request timed out while fetching metadata for {}", model_name)
            } else if e.is_connect() {
                format!("Cannot connect to {}. Ollama server may not be running.", base_url)
            } else {
                format!("Network error: {}", e)
            }
        })?;

    if !response.status().is_success() {
        // Try fallback based on model name
        return Ok(get_fallback_metadata(model_name));
    }

    let show_response: OllamaShowResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    // Try to get context size from model_info (verbose mode) first
    let mut context_size = extract_context_from_model_info(&show_response.model_info, &show_response.details.family);

    // If not found in model_info, try parsing modelfile
    if context_size == ULTIMATE_FALLBACK {
        context_size = parse_num_ctx_from_modelfile(&show_response.modelfile);
    }

    // If still not found, try family-based fallback
    if context_size == ULTIMATE_FALLBACK {
        let family = if !show_response.details.family.is_empty() {
            &show_response.details.family
        } else {
            model_name
        };

        // Check if this model family has a known context size
        if let Some((_, size)) = DEFAULT_CONTEXT_SIZES
            .iter()
            .find(|(fam, _)| family.to_lowercase().contains(fam))
        {
            tracing::info!(
                "No num_ctx in modelfile for {}, using family-based default: {} tokens",
                model_name,
                size
            );
            context_size = *size;
        }
    }

    Ok(ModelMetadata {
        name: model_name.to_string(),
        context_size,
        parameter_count: show_response.details.parameter_size,
        family: show_response.details.family,
    })
}

/// Extract context size from model_info (verbose mode)
///
/// # Arguments
/// * `model_info` - The model_info HashMap from /api/show with verbose=true
/// * `family` - The model family name
///
/// # Returns
/// Context size in tokens, or ULTIMATE_FALLBACK if not found
fn extract_context_from_model_info(
    model_info: &std::collections::HashMap<String, serde_json::Value>,
    family: &str,
) -> usize {
    // Try to find context_length key with family prefix
    // Examples: "gemma3.context_length", "llama.context_length", etc.
    let possible_keys = vec![
        format!("{}.context_length", family),
        format!("{}.context_size", family),
        "context_length".to_string(),
        "context_size".to_string(),
    ];

    for key in possible_keys {
        if let Some(value) = model_info.get(&key) {
            if let Some(ctx) = value.as_u64() {
                tracing::info!("Found context size in model_info[{}]: {} tokens", key, ctx);
                return ctx as usize;
            }
        }
    }

    ULTIMATE_FALLBACK
}

/// Parse num_ctx parameter from Ollama modelfile
///
/// # Arguments
/// * `modelfile` - The modelfile string from /api/show response
///
/// # Returns
/// Context size in tokens, defaults to 4000 if not found
fn parse_num_ctx_from_modelfile(modelfile: &str) -> usize {
    // Regex to match: PARAMETER num_ctx <number>
    static RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"PARAMETER\s+num_ctx\s+(\d+)").expect("Invalid regex pattern")
    });

    RE.captures(modelfile)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or_else(|| {
            tracing::debug!(
                "num_ctx not found in modelfile, using default {}",
                ULTIMATE_FALLBACK
            );
            ULTIMATE_FALLBACK
        })
}

/// Get fallback metadata based on model name pattern matching
///
/// # Arguments
/// * `model_name` - Name of the model
///
/// # Returns
/// ModelMetadata with estimated context size
fn get_fallback_metadata(model_name: &str) -> ModelMetadata {
    let model_lower = model_name.to_lowercase();

    // Try to match against known model families
    let context_size = DEFAULT_CONTEXT_SIZES
        .iter()
        .find(|(family, _)| model_lower.contains(family))
        .map(|(_, size)| *size)
        .unwrap_or(ULTIMATE_FALLBACK);

    // Extract family name from model name (first part before colon or hyphen)
    let family = model_name
        .split(':')
        .next()
        .or_else(|| model_name.split('-').next())
        .unwrap_or("unknown")
        .to_string();

    tracing::warn!(
        "Using fallback metadata for {}: context_size={}",
        model_name,
        context_size
    );

    ModelMetadata {
        name: model_name.to_string(),
        context_size,
        parameter_count: "unknown".to_string(),
        family,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_num_ctx_standard() {
        let modelfile = "FROM /path/to/model\nPARAMETER num_ctx 8192\nPARAMETER temperature 0.7";
        assert_eq!(parse_num_ctx_from_modelfile(modelfile), 8192);
    }

    #[test]
    fn test_parse_num_ctx_with_spaces() {
        let modelfile = "PARAMETER   num_ctx   16384";
        assert_eq!(parse_num_ctx_from_modelfile(modelfile), 16384);
    }

    #[test]
    fn test_parse_num_ctx_missing() {
        let modelfile = "PARAMETER temperature 0.7\nPARAMETER top_p 0.9";
        assert_eq!(parse_num_ctx_from_modelfile(modelfile), ULTIMATE_FALLBACK);
    }

    #[test]
    fn test_parse_num_ctx_multiple_params() {
        let modelfile = "PARAMETER temperature 0.7\nPARAMETER num_ctx 32768\nPARAMETER top_k 40";
        assert_eq!(parse_num_ctx_from_modelfile(modelfile), 32768);
    }

    #[test]
    fn test_fallback_metadata_llama() {
        let metadata = get_fallback_metadata("llama3.2:1b");
        assert_eq!(metadata.context_size, 4096);
        assert_eq!(metadata.name, "llama3.2:1b");
    }

    #[test]
    fn test_fallback_metadata_mistral() {
        let metadata = get_fallback_metadata("mistral:7b");
        assert_eq!(metadata.context_size, 8192);
    }

    #[test]
    fn test_fallback_metadata_unknown() {
        let metadata = get_fallback_metadata("unknown-model:latest");
        assert_eq!(metadata.context_size, ULTIMATE_FALLBACK);
    }

    #[test]
    fn test_fallback_metadata_phi() {
        let metadata = get_fallback_metadata("phi4:latest");
        assert_eq!(metadata.context_size, 2048);
    }
}
