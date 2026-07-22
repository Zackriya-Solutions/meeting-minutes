use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use tauri::{AppHandle, Runtime};

/// Toggle the "auto-detect Google Meet calls" setting read by `spawn_meet_detection_task`'s
/// poll loop. Takes effect on the next poll — no app restart needed.
#[tauri::command]
pub async fn calendar_toggle_auto_detect<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    SettingsRepository::set_auto_detect_meet_enabled(state.db_manager.pool(), enabled)
        .await
        .map_err(|e| format!("Failed to update auto-detect setting: {}", e))
}

#[tauri::command]
pub async fn calendar_get_auto_detect_enabled<R: Runtime>(
    _app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    SettingsRepository::get_auto_detect_meet_enabled(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to read auto-detect setting: {}", e))
}
