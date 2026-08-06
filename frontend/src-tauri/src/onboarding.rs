use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

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
    pub parakeet: String, // "downloaded" | "not_downloaded" | "downloading"
    pub summary: String,  // Generic field for summary model (Qwen 3.5 or legacy Gemma variants)
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
                summary: "not_downloaded".to_string(), // Changed from gemma
                selected_summary_model: None,
            },
            last_updated: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Load onboarding status from store
pub async fn load_onboarding_status<R: Runtime>(app: &AppHandle<R>) -> Result<OnboardingStatus> {
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
                info!(
                    "Loaded onboarding status from store - Step: {}, Completed: {}",
                    s.current_step, s.completed
                );
                s
            }
            Err(e) => {
                warn!(
                    "Failed to deserialize onboarding status: {}, using defaults",
                    e
                );
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
    info!(
        "Saving onboarding status: step={}, completed={}",
        status.current_step, status.completed
    );

    // Get or create store
    let store = app
        .store("onboarding-status.json")
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
    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store to disk: {}", e))?;

    info!("Successfully persisted onboarding status to disk");
    Ok(())
}

/// Set by a reset, cleared by a completion. An explicit request to see setup again, which
/// [`onboarding_should_run`] honours even on an install full of real meetings.
const RESET_REQUESTED_KEY: &str = "reset_requested";

/// Reset onboarding status: forget the saved progress and ask for the flow to run again.
///
/// Deleting the saved status is not enough on its own — an install with real meetings is
/// treated as already set up, so a reset would appear to do nothing. The explicit flag is
/// what makes "reset" mean "show me setup again".
pub async fn reset_onboarding_status<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    info!("Resetting onboarding status");

    let store = app
        .store("onboarding-status.json")
        .map_err(|e| anyhow::anyhow!("Failed to access onboarding store: {}", e))?;

    // Clear the status key
    store.delete("status");
    store.set(RESET_REQUESTED_KEY, serde_json::Value::Bool(true));

    // Persist deletion to disk
    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save onboarding store after reset: {}", e))?;

    info!("Successfully reset onboarding status");
    Ok(())
}

fn reset_requested<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.store("onboarding-status.json")
        .ok()
        .and_then(|store| store.get(RESET_REQUESTED_KEY))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn clear_reset_request<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(store) = app.store("onboarding-status.json") {
        store.delete(RESET_REQUESTED_KEY);
        if let Err(e) = store.save() {
            warn!("Could not clear the onboarding reset request: {}", e);
        }
    }
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
    let store = app
        .store("onboarding-status.json")
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
pub async fn reset_onboarding_status_cmd<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    reset_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to reset onboarding status: {}", e))
}

/// Whether the onboarding flow should be shown at all.
///
/// A saved `completed` flag is the primary answer, but installs that predate the flow being
/// re-enabled have no saved status and already hold real meetings. Treating an existing
/// meeting as consent avoids walking a working install back through setup.
#[tauri::command]
pub async fn onboarding_should_run<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    if reset_requested(&app) {
        return Ok(true);
    }

    if let Ok(status) = load_onboarding_status(&app).await {
        if status.completed {
            return Ok(false);
        }
    }

    Ok(!has_real_meetings(state.db_manager.pool()).await)
}

/// Whether the database holds a meeting the user made, as opposed to the seeded example.
///
/// This is the rule that decides whether an install predating the saved-status era gets walked
/// through setup, so it must not count the example meeting (onboarding creates it) and must
/// fail safe: an unreadable database is treated as "there is real work here", because showing
/// setup over a working install is the worse mistake.
async fn has_real_meetings(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM meetings WHERE id != ?")
        .bind(crate::demo_meeting::DEMO_MEETING_ID)
        .fetch_one(pool)
        .await
        .map(|count| count > 0)
        .unwrap_or(true)
}

/// Finish onboarding: persist the chosen providers, then seed the example meeting.
///
/// `model` is the summary model the user picked — a DeepSeek v4 tier. Anything else is
/// coerced to the default rather than written through, so a stale frontend cannot leave the
/// install pointing at a provider onboarding no longer offers.
///
/// Returns the id of the example meeting when one was seeded, so the caller can open it —
/// an empty app explains nothing about what recording a meeting produces.
#[tauri::command]
pub async fn complete_onboarding<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    model: String,
) -> Result<Option<String>, String> {
    let summary_model = crate::llm::providers::deepseek::normalize_model(&model);
    let transcription_model = crate::gigaam_engine::commands::selected_model_label(&app);
    info!(
        "Completing onboarding: summary=deepseek/{}, transcription=gigaam/{}",
        summary_model, transcription_model
    );

    // Step 1: Save model configuration to SQLite database FIRST
    let pool = state.db_manager.pool();

    // This also mirrors the tier into `app_settings_kv['deepseek.model']`, which is where the
    // analytics report reads its model from.
    if let Err(e) =
        SettingsRepository::save_model_config(pool, "deepseek", summary_model, "large-v3", None)
            .await
    {
        error!("Failed to save the DeepSeek summary config: {}", e);
        return Err(format!("Failed to save the DeepSeek summary config: {}", e));
    }

    // Transcription is local: the downloaded GigaAM variant, never a cloud engine.
    if let Err(e) =
        SettingsRepository::save_transcript_config(pool, "gigaam", &transcription_model).await
    {
        error!("Failed to save transcription model config: {}", e);
        return Err(format!("Failed to save transcription model config: {}", e));
    }

    // Step 2: Only NOW mark onboarding as complete (after DB operations succeed)
    let mut status = load_onboarding_status(&app)
        .await
        .map_err(|e| format!("Failed to load onboarding status: {}", e))?;

    status.completed = true;
    status.current_step = 3; // Max step (3 on macOS with permissions, 2 on other platforms)
    status.model_status.summary = "configured".to_string();
    status.model_status.selected_summary_model = Some(summary_model.to_string());

    save_onboarding_status(&app, &status)
        .await
        .map_err(|e| format!("Failed to save completed onboarding status: {}", e))?;
    clear_reset_request(&app);

    // Step 3: The example meeting. A failure here is not a setup failure — the app is
    // configured and usable, it just starts empty.
    let demo_meeting_id = match crate::demo_meeting::seed(&app, pool).await {
        Ok(id) => id,
        Err(error) => {
            warn!("Could not seed the example meeting: {}", error);
            None
        }
    };

    info!("Onboarding completed successfully with model: {}", model);
    Ok(demo_meeting_id)
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

    /// Onboarding writes whatever the picker sends. A build that still remembers a retired
    /// alias — or a local model from before the DeepSeek-only rule — must not be able to
    /// leave the install pointing at something the gateway rejects.
    #[test]
    fn only_offered_deepseek_tiers_survive_completion() {
        use crate::llm::providers::deepseek::{normalize_model, DEFAULT_MODEL, FLASH_MODEL};

        assert_eq!(normalize_model("deepseek-v4-flash"), FLASH_MODEL);
        assert_eq!(normalize_model(" deepseek-v4-pro "), DEFAULT_MODEL);
        assert_eq!(normalize_model("deepseek-chat"), DEFAULT_MODEL);
        assert_eq!(normalize_model("qwen3.5:2b"), DEFAULT_MODEL);
        assert_eq!(normalize_model(""), DEFAULT_MODEL);
    }

    /// The example meeting is created BY onboarding, so it must never be the reason onboarding
    /// is skipped — and one real meeting must be enough to leave a working install alone.
    #[tokio::test]
    async fn only_meetings_the_user_made_count_as_prior_use() {
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        assert!(!has_real_meetings(&pool).await, "a fresh install has none");

        let insert = |id: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO meetings(id, title, created_at, updated_at) \
                     VALUES(?, 'x', datetime('now'), datetime('now'))",
                )
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            }
        };

        insert(crate::demo_meeting::DEMO_MEETING_ID).await;
        assert!(
            !has_real_meetings(&pool).await,
            "the seeded example is not prior use"
        );

        insert("meeting-real").await;
        assert!(has_real_meetings(&pool).await, "a real meeting is");
    }
}
