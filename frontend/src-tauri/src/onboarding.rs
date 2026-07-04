use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use log::{info, warn, error};
use anyhow::Result;

use crate::state::AppState;
use crate::database::repositories::setting::SettingsRepository;


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OnboardingStatus {
    pub version: String,
    pub completed: bool,
    pub current_step: u8,
    pub model_status: ModelStatus,
    /// Product focus chosen during onboarding: "general" | "clinician"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_focus: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelStatus {
    pub parakeet: String,  // "downloaded" | "not_downloaded" | "downloading"
    pub summary: String,   // Generic field for summary model (Qwen 3.5 or legacy Gemma variants)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_summary_model: Option<String>,
}

impl Default for OnboardingStatus {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            completed: false,
            current_step: 1,
            model_status: ModelStatus {
                parakeet: "not_downloaded".to_string(),
                summary: "not_downloaded".to_string(),  // Changed from gemma
                selected_summary_model: None,
            },
            product_focus: None,
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}


/// Load onboarding status from store
pub async fn load_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<OnboardingStatus> {
    // Try to load from Tauri store
    let store = match app.store("onboarding-status.json") {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access onboarding store: {}, using defaults", e);
            return Ok(OnboardingStatus::default());
        }
    };

    // Try to get the status from store
    let status = if let Some(value) = store.get("status") {
        match serde_json::from_value::<OnboardingStatus>(value.clone()) {
            Ok(s) => {
                info!("Loaded onboarding status from store - Step: {}, Completed: {}",
                      s.current_step, s.completed);
                s
            }
            Err(e) => {
                warn!("Failed to deserialize onboarding status: {}, using defaults", e);
                OnboardingStatus::default()
            }
        }
    } else {
        info!("No stored onboarding status found, using defaults");
        OnboardingStatus::default()
    };

    Ok(status)
}

/// Save onboarding status to store
pub async fn save_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
    status: &OnboardingStatus,
) -> Result<()> {
    info!("Saving onboarding status: step={}, completed={}",
          status.current_step, status.completed);

    // Get or create store
    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Update last_updated timestamp
    let mut status = status.clone();
    status.last_updated = chrono::Utc::now().to_rfc3339();

    // Serialize status to JSON value
    let status_value = serde_json::to_value(&status)
        .map_err(|e| anyhow::anyhow!("Failed to serialize onboarding status: {}", e))?;

    // Save to store
    store.set("status", status_value);

    // Persist to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store to disk: {}", e))?;

    info!("Successfully persisted onboarding status to disk");
    Ok(())
}

/// Load the persisted product focus ("general" | "clinician"), if any.
/// Used to gate focus-specific models (e.g. clinical models) in the UI.
pub async fn load_product_focus<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    load_onboarding_status(app)
        .await
        .ok()
        .and_then(|status| status.product_focus)
}

/// Reset onboarding status (delete from store)
pub async fn reset_onboarding_status<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<()> {
    info!("Resetting onboarding status");

    let store = app.store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Clear the status key
    store.delete("status");

    // Persist deletion to disk
    store.save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store after reset: {}", e))?;

    info!("Successfully reset onboarding status");
    Ok(())
}

/// Tauri commands for onboarding status
#[tauri::command]
pub async fn get_onboarding_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<OnboardingStatus>, String> {
    let status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    // Return None if it's the default (never saved before)
    // Check if we have any saved data by seeing if the store has the key
    let store = app.store("onboarding-status.json")
        .map_err(|e| format!("Failed to access store: {}", e))?;

    if store.get("status").is_none() {
        Ok(None)
    } else {
        Ok(Some(status))
    }
}

#[tauri::command]
pub async fn save_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
    status: OnboardingStatus,
) -> Result<(), String> {
    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save onboarding status: {}", e))
}

#[tauri::command]
pub async fn reset_onboarding_status_cmd<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    reset_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to reset onboarding status: {}", e))
}

/// Persist the product focus immediately (not subject to the frontend's
/// debounced auto-save, so a following step can rely on it).
#[tauri::command]
pub async fn set_product_focus<R: Runtime>(
    app: AppHandle<R>,
    focus: String,
) -> Result<(), String> {
    if focus != "general" && focus != "clinician" {
        return Err(format!("Invalid product focus: {}", focus));
    }

    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.product_focus = Some(focus);

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save product focus: {}", e))
}

#[tauri::command]
pub async fn get_product_focus<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    Ok(load_product_focus(&app).await)
}

#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    info!("Completing onboarding with builtin-ai model: {}", model);

    // Step 1: Save model configuration to SQLite database FIRST
    let pool = state.db_manager.pool();

    // Onboarding always uses builtin-ai (local LLM)
    if let Err(e) = SettingsRepository::save_model_config(
        pool,
        "builtin-ai",
        &model,
        "large-v3",
        None,
    ).await {
        error!("Failed to save builtin-ai model config: {}", e);
        return Err(format!("Failed to save builtin-ai model config: {}", e));
    }
    info!("Saved builtin-ai model config: model={}", model);

    // Save transcription model config (parakeet provider) - always parakeet
    if let Err(e) = SettingsRepository::save_transcript_config(
        pool,
        "parakeet",
        crate::config::DEFAULT_PARAKEET_MODEL,
    ).await {
        error!("Failed to save transcription model config: {}", e);
        return Err(format!("Failed to save transcription model config: {}", e));
    }
    info!("Saved transcription model config: provider=parakeet, model={}", crate::config::DEFAULT_PARAKEET_MODEL);

    // Step 2: Only NOW mark onboarding as complete (after DB operations succeed)
    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.completed = true;
    status.current_step = 5; // Max step (5 on macOS with permissions, 4 on other platforms)
    status.model_status.parakeet = "downloaded".to_string();
    status.model_status.summary = "downloaded".to_string();
    status.model_status.selected_summary_model = Some(model.clone());

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;

    info!("Onboarding completed successfully with model: {}", model);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_status_deserializes_without_selected_summary_model() {
        let status: OnboardingStatus = serde_json::from_str(
            r#"{
                "version": "1.0",
                "completed": true,
                "current_step": 4,
                "model_status": {
                    "parakeet": "downloaded",
                    "summary": "downloaded"
                },
                "last_updated": "2026-05-30T00:00:00Z"
            }"#,
        )
        .expect("old onboarding status should remain compatible");

        assert_eq!(status.model_status.selected_summary_model, None);
        assert_eq!(status.product_focus, None);
    }

    #[test]
    fn onboarding_status_round_trips_product_focus() {
        let mut status = OnboardingStatus::default();
        status.product_focus = Some("clinician".to_string());

        let json = serde_json::to_string(&status).expect("status should serialize");
        let restored: OnboardingStatus =
            serde_json::from_str(&json).expect("status should deserialize");

        assert_eq!(restored.product_focus, Some("clinician".to_string()));
    }
}
