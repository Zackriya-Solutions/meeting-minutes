use super::history::{self, DictationHistoryItem};
use crate::state::AppState;
use std::str::FromStr;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

#[tauri::command]
pub fn dictation_get_shortcut_status(
    status: tauri::State<'_, super::DictationShortcutStatusState>,
) -> super::DictationShortcutStatus {
    status.get()
}

#[tauri::command]
pub fn dictation_set_shortcut(
    app: AppHandle,
    shortcut: String,
) -> Result<super::DictationShortcutStatus, String> {
    let shortcut = shortcut.trim().to_owned();
    let parsed = Shortcut::from_str(&shortcut)
        .map_err(|error| format!("That shortcut is not supported: {error}"))?;
    if parsed.mods.is_empty() {
        return Err("Choose at least one modifier, such as Ctrl, Alt, Shift, or Cmd.".into());
    }

    let status_state = app.state::<super::DictationShortcutStatusState>();
    let previous = status_state.get().shortcut;
    if previous.as_deref() == Some(shortcut.as_str()) {
        return Ok(status_state.get());
    }

    if let Some(previous) = previous.as_deref() {
        let previous_shortcut = Shortcut::from_str(previous)
            .map_err(|error| format!("The active shortcut cannot be removed: {error}"))?;
        app.global_shortcut()
            .unregister(previous_shortcut)
            .map_err(|error| format!("Could not release the active shortcut: {error}"))?;
    }

    if let Err(error) = app.global_shortcut().register(parsed) {
        if let Some(previous) = previous.as_deref() {
            if let Ok(previous_shortcut) = Shortcut::from_str(previous) {
                let _ = app.global_shortcut().register(previous_shortcut);
            }
        }
        return Err(format!(
            "That shortcut is already in use or unavailable: {error}"
        ));
    }

    if let Err(error) = super::save_shortcut(&app, &shortcut) {
        let _ = app.global_shortcut().unregister(parsed);
        if let Some(previous) = previous.as_deref() {
            if let Ok(previous_shortcut) = Shortcut::from_str(previous) {
                let _ = app.global_shortcut().register(previous_shortcut);
            }
        }
        return Err(error);
    }

    status_state.registered(&shortcut);
    Ok(status_state.get())
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
