// Lightweight app preferences persisted via tauri-plugin-store (app-preferences.json).
// Currently holds the default summary template preference set during onboarding
// (clinician focus) or when the user picks a template in meeting details.

use log::{info, warn};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const PREFERENCES_STORE: &str = "app-preferences.json";
const DEFAULT_SUMMARY_TEMPLATE_KEY: &str = "default_summary_template";

#[tauri::command]
pub async fn get_default_summary_template<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    let store = match app.store(PREFERENCES_STORE) {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access preferences store: {}", e);
            return Ok(None);
        }
    };

    let template = store
        .get(DEFAULT_SUMMARY_TEMPLATE_KEY)
        .and_then(|value| value.as_str().map(|s| s.to_string()));

    Ok(template)
}

#[tauri::command]
pub async fn set_default_summary_template<R: Runtime>(
    app: AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    let store = app
        .store(PREFERENCES_STORE)
        .map_err(|e| format!("Failed to access preferences store: {}", e))?;

    store.set(DEFAULT_SUMMARY_TEMPLATE_KEY, serde_json::json!(template_id));

    store
        .save()
        .map_err(|e| format!("Failed to save preferences store: {}", e))?;

    info!("Saved default summary template: {}", template_id);
    Ok(())
}
