use crate::model_config::load_provider_models;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};

/// Anthropic (Claude) model information returned to frontend
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnthropicModel {
    pub id: String,
    pub display_name: Option<String>,
}

/// API response model from Anthropic
#[derive(Debug, Deserialize)]
struct AnthropicApiModel {
    id: String,
    display_name: Option<String>,
    #[allow(dead_code)]
    created_at: Option<String>,
}

/// API response wrapper from Anthropic
#[derive(Debug, Deserialize)]
struct AnthropicApiResponse {
    data: Vec<AnthropicApiModel>,
}

/// Cache entry for models
struct CacheEntry {
    models: Vec<AnthropicModel>,
    fetched_at: Instant,
}

/// Global cache for Anthropic models (5 minute TTL)
static MODELS_CACHE: RwLock<Option<CacheEntry>> = RwLock::new(None);

/// Cache TTL in seconds
const CACHE_TTL_SECS: u64 = 300;

/// Get configured fallback models as AnthropicModel vec.
fn get_fallback_models<R: Runtime>(app: &AppHandle<R>) -> Vec<AnthropicModel> {
    load_provider_models(Some(app), "anthropic")
        .unwrap_or_default()
        .into_iter()
        .map(|model| AnthropicModel {
            id: model.id,
            display_name: model.display_name,
        })
        .collect()
}

/// Check if model is a chat-capable model
fn is_chat_model(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    // Include Claude models only
    id.starts_with("claude-")
}

/// Fetch Anthropic models from API
///
/// # Arguments
/// * `api_key` - Anthropic API key
///
/// # Returns
/// Vector of available models, or fallback models on error
#[tauri::command]
pub async fn get_anthropic_models<R: Runtime>(
    app: AppHandle<R>,
    api_key: Option<String>,
) -> Result<Vec<AnthropicModel>, String> {
    // Return fallback if no API key provided
    let api_key = match api_key {
        Some(key) if !key.trim().is_empty() => key.trim().to_string(),
        _ => {
            log::info!("No Anthropic API key provided, returning fallback models");
            return Ok(get_fallback_models(&app));
        }
    };

    // Check cache first
    {
        let cache = MODELS_CACHE.read().map_err(|e| e.to_string())?;
        if let Some(entry) = cache.as_ref() {
            if entry.fetched_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                log::info!(
                    "Returning cached Anthropic models ({} models)",
                    entry.models.len()
                );
                return Ok(entry.models.clone());
            }
        }
    }

    // Fetch from API
    log::info!("Fetching Anthropic models from API...");
    let client = reqwest::Client::new();

    let response = match client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("Failed to fetch Anthropic models: {}. Using fallback.", e);
            return Ok(get_fallback_models(&app));
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        log::warn!(
            "Anthropic API returned status {}. Using fallback models.",
            status
        );
        return Ok(get_fallback_models(&app));
    }

    let api_response: AnthropicApiResponse = match response.json().await {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to parse Anthropic response: {}. Using fallback.", e);
            return Ok(get_fallback_models(&app));
        }
    };

    // Filter to only chat models and map to our struct
    let models: Vec<AnthropicModel> = api_response
        .data
        .into_iter()
        .filter(|m| is_chat_model(&m.id))
        .map(|m| AnthropicModel {
            id: m.id,
            display_name: m.display_name,
        })
        .collect();

    // If no models returned, use fallback
    if models.is_empty() {
        log::warn!("No chat models returned from Anthropic API. Using fallback.");
        return Ok(get_fallback_models(&app));
    }

    log::info!("Fetched {} Anthropic models from API", models.len());

    // Update cache
    {
        let mut cache = MODELS_CACHE.write().map_err(|e| e.to_string())?;
        *cache = Some(CacheEntry {
            models: models.clone(),
            fetched_at: Instant::now(),
        });
    }

    Ok(models)
}

/// Clear the models cache (useful when API key changes)
pub fn clear_cache() {
    if let Ok(mut cache) = MODELS_CACHE.write() {
        *cache = None;
        log::info!("Anthropic models cache cleared");
    }
}
