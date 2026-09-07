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
    pub last_updated: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelStatus {
    pub parakeet: String,  // "downloaded" | "not_downloaded" | "downloading" | "skipped"
    pub summary: String,   // Generic field for summary model (Qwen 3.5 or legacy Gemma variants)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_summary_model: Option<String>,
    /// Transcription backend picked during onboarding: "parakeet" (local, the
    /// default) or "remoteWhisper" (a self-hosted OpenAI-compatible server).
    /// `None` on statuses written before remote transcription existed, which is
    /// read as "parakeet".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription_provider: Option<String>,
    /// Base URL of the remote transcription server, set only when
    /// `transcription_provider` is "remoteWhisper".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_transcription_url: Option<String>,
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
                transcription_provider: None,
                remote_transcription_url: None,
            },
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

/// Transcription backend identifier persisted by onboarding when the user opts
/// into a self-hosted server instead of the bundled local engine.
const REMOTE_WHISPER_PROVIDER: &str = "remoteWhisper";

#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    model: Option<String>,
    transcription_provider: Option<String>,
    remote_transcription_url: Option<String>,
) -> Result<(), String> {
    let pool = state.db_manager.pool();

    // ---- Summarization -----------------------------------------------------
    // `model` is `None` when the user chose to bring their own summary provider
    // (OpenAI, Claude, Ollama, ...). In that case onboarding must not write a
    // builtin-ai config, otherwise the app would try to run a model that was
    // never downloaded. The provider is configured later in Settings.
    let summary_status = match model.as_deref() {
        Some(model_name) => {
            info!("Completing onboarding with builtin-ai model: {}", model_name);
            if let Err(e) =
                SettingsRepository::save_model_config(pool, "builtin-ai", model_name, "large-v3", None)
                    .await
            {
                error!("Failed to save builtin-ai model config: {}", e);
                return Err(format!("Failed to save builtin-ai model config: {}", e));
            }
            info!("Saved builtin-ai model config: model={}", model_name);
            "downloaded"
        }
        None => {
            info!("Completing onboarding without a local summary model (external provider)");
            "skipped"
        }
    };

    // ---- Transcription -----------------------------------------------------
    // Default stays local Parakeet; "remoteWhisper" reuses the `model` column of
    // `transcript_settings` to carry the server base URL (see RemoteWhisperProvider).
    let provider = transcription_provider.as_deref().unwrap_or("parakeet");
    let (transcript_provider, transcript_model, parakeet_status) = if provider == REMOTE_WHISPER_PROVIDER
    {
        let url = remote_transcription_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .ok_or_else(|| {
                "Remote transcription selected but no server URL was provided".to_string()
            })?;
        (REMOTE_WHISPER_PROVIDER, url.to_string(), "skipped")
    } else {
        (
            "parakeet",
            crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
            "downloaded",
        )
    };

    if let Err(e) =
        SettingsRepository::save_transcript_config(pool, transcript_provider, &transcript_model).await
    {
        error!("Failed to save transcription model config: {}", e);
        return Err(format!("Failed to save transcription model config: {}", e));
    }
    info!(
        "Saved transcription model config: provider={}, model={}",
        transcript_provider, transcript_model
    );

    // ---- Mark complete (only after the DB writes succeeded) ----------------
    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.completed = true;
    status.current_step = 4; // Max step (4 on macOS with permissions, 3 on other platforms)
    status.model_status.parakeet = parakeet_status.to_string();
    status.model_status.summary = summary_status.to_string();
    status.model_status.selected_summary_model = model.clone();
    status.model_status.transcription_provider = Some(transcript_provider.to_string());
    status.model_status.remote_transcription_url = if transcript_provider == REMOTE_WHISPER_PROVIDER {
        Some(transcript_model.clone())
    } else {
        None
    };

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;

    info!(
        "Onboarding completed successfully (transcription={}, summary={})",
        transcript_provider, summary_status
    );
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
    }

    #[test]
    fn onboarding_status_deserializes_without_transcription_fields() {
        // Statuses written before remote transcription existed must keep loading:
        // a missing provider is read as "local Parakeet", not as a hard error.
        let status: OnboardingStatus = serde_json::from_str(
            r#"{
                "version": "1.0",
                "completed": true,
                "current_step": 4,
                "model_status": {
                    "parakeet": "downloaded",
                    "summary": "downloaded",
                    "selected_summary_model": "qwen3.5:4b"
                },
                "last_updated": "2026-05-30T00:00:00Z"
            }"#,
        )
        .expect("pre-remote onboarding status should remain compatible");

        assert_eq!(status.model_status.transcription_provider, None);
        assert_eq!(status.model_status.remote_transcription_url, None);
    }

    #[test]
    fn onboarding_status_roundtrips_remote_transcription() {
        let status: OnboardingStatus = serde_json::from_str(
            r#"{
                "version": "1.0",
                "completed": true,
                "current_step": 4,
                "model_status": {
                    "parakeet": "skipped",
                    "summary": "skipped",
                    "transcription_provider": "remoteWhisper",
                    "remote_transcription_url": "http://192.168.1.100:8093"
                },
                "last_updated": "2026-05-30T00:00:00Z"
            }"#,
        )
        .expect("remote onboarding status should deserialize");

        assert_eq!(
            status.model_status.transcription_provider.as_deref(),
            Some("remoteWhisper")
        );
        assert_eq!(status.model_status.selected_summary_model, None);

        // Absent optional fields must not be re-emitted as nulls.
        let json = serde_json::to_string(&status).expect("serializes");
        assert!(!json.contains("selected_summary_model"));
        assert!(json.contains("remoteWhisper"));
    }
}
