use crate::model_config::load_provider_models;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};

/// Groq model information returned to frontend
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GroqModel {
    pub id: String,
    pub owned_by: Option<String>,
}

/// API response model from Groq (OpenAI-compatible format)
#[derive(Debug, Deserialize)]
struct GroqApiModel {
    id: String,
    owned_by: Option<String>,
    #[allow(dead_code)]
    object: String,
}

/// API response wrapper from Groq
#[derive(Debug, Deserialize)]
struct GroqApiResponse {
    data: Vec<GroqApiModel>,
}

/// Cache entry for models
struct CacheEntry {
    models: Vec<GroqModel>,
    fetched_at: Instant,
}

/// Global cache for Groq models (5 minute TTL)
static MODELS_CACHE: RwLock<Option<CacheEntry>> = RwLock::new(None);

/// Cache TTL in seconds
const CACHE_TTL_SECS: u64 = 300;

/// Get configured fallback models as GroqModel vec.
fn get_fallback_models<R: Runtime>(app: &AppHandle<R>) -> Vec<GroqModel> {
    load_provider_models(Some(app), "groq")
        .unwrap_or_default()
        .into_iter()
        .map(|model| GroqModel {
            id: model.id,
            owned_by: model.owned_by,
        })
        .collect()
}

/// Check if model is a chat-capable model (filter out whisper, etc.)
fn is_chat_model(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
    // Exclude whisper, tool-use specific models, and embedding models
    !id.contains("whisper")
        && !id.contains("embed")
        && !id.contains("guard")
        && !id.contains("tool-use")
}

/// Fetch Groq models from API
///
/// # Arguments
/// * `api_key` - Groq API key
///
/// # Returns
/// Vector of available models, or fallback models on error
#[tauri::command]
pub async fn get_groq_models<R: Runtime>(
    app: AppHandle<R>,
    api_key: Option<String>,
) -> Result<Vec<GroqModel>, String> {
    // Return fallback if no API key provided
    let api_key = match api_key {
        Some(key) if !key.trim().is_empty() => key.trim().to_string(),
        _ => {
            log::info!("No Groq API key provided, returning fallback models");
            return Ok(get_fallback_models(&app));
        }
    };

    // Check cache first
    {
        let cache = MODELS_CACHE.read().map_err(|e| e.to_string())?;
        if let Some(entry) = cache.as_ref() {
            if entry.fetched_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                log::info!("Returning cached Groq models ({} models)", entry.models.len());
                return Ok(entry.models.clone());
            }
        }
    }

    // Fetch from API
    log::info!("Fetching Groq models from API...");
    let client = reqwest::Client::new();

    let response = match client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("Failed to fetch Groq models: {}. Using fallback.", e);
            return Ok(get_fallback_models(&app));
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        log::warn!(
            "Groq API returned status {}. Using fallback models.",
            status
        );
        return Ok(get_fallback_models(&app));
    }

    let api_response: GroqApiResponse = match response.json().await {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to parse Groq response: {}. Using fallback.", e);
            return Ok(get_fallback_models(&app));
        }
    };

    // Filter to only chat models and map to our struct
    let models: Vec<GroqModel> = api_response
        .data
        .into_iter()
        .filter(|m| is_chat_model(&m.id))
        .map(|m| GroqModel {
            id: m.id,
            owned_by: m.owned_by,
        })
        .collect();

    // If no models returned, use fallback
    if models.is_empty() {
        log::warn!("No chat models returned from Groq API. Using fallback.");
        return Ok(get_fallback_models(&app));
    }

    log::info!("Fetched {} Groq models from API", models.len());

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
        log::info!("Groq models cache cleared");
    }
}
