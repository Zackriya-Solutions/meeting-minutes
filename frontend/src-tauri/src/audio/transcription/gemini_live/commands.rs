// audio/transcription/gemini_live/commands.rs
//
// Tauri commands for the Gemini Live settings panel.

use super::connection::LiveConnection;
use super::settings::{self, GeminiLiveSettings};
use crate::state::AppState;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiLiveSettingsView {
    #[serde(flatten)]
    pub settings: GeminiLiveSettings,
    /// A transcription-specific key is saved.
    pub has_api_key: bool,
    /// Last 4 characters of the saved key, for display.
    pub api_key_hint: Option<String>,
    /// A Gemini key is saved for summaries and will be used if no key is set here.
    pub has_summary_api_key: bool,
}

#[tauri::command]
pub async fn gemini_live_get_settings(state: tauri::State<'_, AppState>) -> Result<GeminiLiveSettingsView, String> {
    let pool = state.db_manager.pool();
    let (settings, api_key) = settings::load(pool).await.map_err(|e| e.to_string())?;
    Ok(GeminiLiveSettingsView {
        settings,
        has_api_key: api_key.is_some(),
        api_key_hint: api_key.map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()),
        has_summary_api_key: settings::summary_gemini_key(pool).await.is_some(),
    })
}

/// `api_key`: a new key, "" to remove the saved key, or null to keep it.
#[tauri::command]
pub async fn gemini_live_save_settings(
    state: tauri::State<'_, AppState>,
    settings: GeminiLiveSettings,
    api_key: Option<String>,
) -> Result<GeminiLiveSettings, String> {
    let settings = settings.normalized();
    settings::save(state.db_manager.pool(), &settings, api_key.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    log::info!("Gemini Live settings saved");
    Ok(settings)
}

/// Open a session with the given key (or the saved one) to check the key and network.
#[tauri::command]
pub async fn gemini_live_test_connection(
    state: tauri::State<'_, AppState>,
    api_key: Option<String>,
) -> Result<String, String> {
    let pool = state.db_manager.pool();
    let key = match api_key.filter(|k| !k.trim().is_empty()) {
        Some(k) => k.trim().to_string(),
        None => super::resolve_api_key(pool)
            .await?
            .ok_or("No API key saved yet. Paste your Gemini API key first.")?,
    };
    let (settings, _) = settings::load(pool).await.map_err(|e| e.to_string())?;
    let conn = LiveConnection::connect(&key, &settings.setup_options())
        .await
        .map_err(|e| e.to_string())?;
    conn.close_in_background();
    Ok(format!("Connected to {} successfully.", super::protocol::MODEL))
}
