use super::history::{self, DictationHistoryItem};
use crate::state::AppState;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn dictation_get_shortcut_status(
    status: tauri::State<'_, super::DictationShortcutStatusState>,
) -> super::DictationShortcutStatus {
    status.get()
}

#[tauri::command]
pub fn dictation_get_overlay_enabled(
    state: tauri::State<'_, super::DictationOverlayState>,
) -> bool {
    state.enabled()
}

#[tauri::command]
pub fn dictation_set_overlay_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    super::set_overlay_enabled(&app, enabled)
}

#[tauri::command]
pub fn dictation_set_overlay_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    super::set_overlay_expanded(&app, expanded)
}

#[tauri::command]
pub async fn dictation_list_history(
    app: AppHandle,
    limit: Option<i64>,
) -> Result<Vec<DictationHistoryItem>, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Dictation history is not available yet.".to_string())?;
    history::list_history(state.db_manager.pool(), limit.unwrap_or(50))
        .await
        .map_err(|error| format!("Could not load dictation history: {error}"))
}

#[tauri::command]
pub async fn dictation_copy_history(app: AppHandle, id: String) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "Dictation history is not available yet.".to_string())?;
    let text = history::text_by_id(state.db_manager.pool(), &id)
        .await
        .map_err(|error| format!("Could not read dictation history: {error}"))?
        .ok_or_else(|| "This dictation has no text to copy.".to_string())?;

    #[cfg(target_os = "windows")]
    {
        use super::ClipboardPort;
        let mut clipboard = super::WindowsClipboard;
        clipboard
            .set_text(&text)
            .map_err(|error| format!("Could not copy dictation: {error}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    Err("Copying dictation history is not implemented here.".into())
}
