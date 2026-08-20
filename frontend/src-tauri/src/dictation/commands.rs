// dictation/commands.rs
//
// Tauri commands exposed to the frontend for the live dictation feature.

use log::{info, warn};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use super::manager::DictationManagerState;
use super::settings;
use super::types::{DictationSettings, DictationState};

/// (Re)apply the global hotkey registration to match `settings`. This app
/// only ever registers a single global shortcut (dictation's own toggle), so
/// it is always safe to clear every registered shortcut before registering
/// the current one.
pub async fn apply_hotkey<R: Runtime>(app: &AppHandle<R>, settings: &DictationSettings) {
    let global_shortcut = app.global_shortcut();
    if let Err(e) = global_shortcut.unregister_all() {
        warn!("Dictation: failed to unregister existing hotkey(s): {}", e);
    }

    if !settings.enabled {
        return;
    }

    let Some(hotkey_str) = settings.hotkey.as_deref() else {
        return;
    };

    match tauri_plugin_global_shortcut::Shortcut::try_from(hotkey_str) {
        Ok(shortcut) => match global_shortcut.register(shortcut) {
            Ok(()) => info!("Dictation: registered global hotkey '{}'", hotkey_str),
            Err(e) => warn!(
                "Dictation: failed to register hotkey '{}' (the global-shortcut plugin may not be \
                 supported on this desktop session): {}",
                hotkey_str, e
            ),
        },
        Err(e) => warn!("Dictation: invalid hotkey string '{}': {}", hotkey_str, e),
    }
}

#[tauri::command]
pub async fn start_dictation(
    manager: State<'_, DictationManagerState<tauri::Wry>>,
) -> Result<(), String> {
    manager.start().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_dictation(
    manager: State<'_, DictationManagerState<tauri::Wry>>,
) -> Result<(), String> {
    manager.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn get_dictation_state(
    manager: State<'_, DictationManagerState<tauri::Wry>>,
) -> Result<DictationState, String> {
    Ok(manager.state().await)
}

#[tauri::command]
pub async fn get_dictation_settings(app: AppHandle<tauri::Wry>) -> Result<DictationSettings, String> {
    settings::load_dictation_settings(&app)
        .await
        .map_err(|e| format!("Failed to load dictation settings: {}", e))
}

#[tauri::command]
pub async fn set_dictation_settings(
    app: AppHandle<tauri::Wry>,
    manager: State<'_, DictationManagerState<tauri::Wry>>,
    settings: DictationSettings,
) -> Result<(), String> {
    settings::save_dictation_settings(&app, &settings)
        .await
        .map_err(|e| format!("Failed to save dictation settings: {}", e))?;

    apply_hotkey(&app, &settings).await;

    // Keep the manager's live state in sync with the saved `enabled` flag so
    // the Settings toggle behaves like a real on/off switch, not just a
    // preference for next launch.
    if settings.enabled && !manager.is_active() {
        manager.start().await.map_err(|e| e.to_string())?;
    } else if !settings.enabled && manager.is_active() {
        manager.stop().await;
    }

    Ok(())
}
