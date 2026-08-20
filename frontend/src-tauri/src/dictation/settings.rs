// dictation/settings.rs
//
// Persistence for `DictationSettings`, mirroring
// `audio::recording_preferences`'s exact load/save shape via the shared
// `tauri-plugin-store` dependency (Fix 5 of the live-dictation plan).

use anyhow::Result;
use log::{info, warn};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

use super::types::DictationSettings;

const STORE_FILE: &str = "dictation_settings.json";
const STORE_KEY: &str = "settings";

/// Load dictation settings from the store, falling back to defaults if the
/// store is unavailable, empty, or fails to deserialize.
pub async fn load_dictation_settings<R: Runtime>(app: &AppHandle<R>) -> Result<DictationSettings> {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(e) => {
            warn!("Dictation: failed to access store: {}, using defaults", e);
            return Ok(DictationSettings::default());
        }
    };

    let settings = if let Some(value) = store.get(STORE_KEY) {
        match serde_json::from_value::<DictationSettings>(value.clone()) {
            Ok(s) => {
                info!("Dictation: loaded settings from store");
                s
            }
            Err(e) => {
                warn!("Dictation: failed to deserialize settings: {}, using defaults", e);
                DictationSettings::default()
            }
        }
    } else {
        info!("Dictation: no stored settings found, using defaults");
        DictationSettings::default()
    };

    Ok(settings)
}

/// Persist dictation settings to disk.
pub async fn save_dictation_settings<R: Runtime>(
    app: &AppHandle<R>,
    settings: &DictationSettings,
) -> Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("Failed to access store: {}", e))?;

    let value = serde_json::to_value(settings)
        .map_err(|e| anyhow::anyhow!("Failed to serialize dictation settings: {}", e))?;

    store.set(STORE_KEY, value);
    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save store to disk: {}", e))?;

    info!(
        "Dictation: persisted settings (enabled={}, hotkey={:?})",
        settings.enabled, settings.hotkey
    );
    Ok(())
}
