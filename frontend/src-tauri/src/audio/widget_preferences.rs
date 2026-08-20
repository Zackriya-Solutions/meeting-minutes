// audio/widget_preferences.rs
//
// Persistence for the floating recording widget (issue #718): whether the
// widget should be shown, and its last-known screen position. Mirrors
// recording_preferences.rs's store-backed load/save/command pattern.

use anyhow::Result;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "widget_preferences.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WidgetPreferences {
    pub show_widget: bool,
    #[serde(default)]
    pub position_x: Option<f64>,
    #[serde(default)]
    pub position_y: Option<f64>,
}

impl Default for WidgetPreferences {
    fn default() -> Self {
        Self {
            show_widget: false,
            position_x: None,
            position_y: None,
        }
    }
}

/// Load widget preferences from store
pub async fn load_widget_preferences<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<WidgetPreferences> {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access widget preferences store: {}, using defaults", e);
            return Ok(WidgetPreferences::default());
        }
    };

    let prefs = if let Some(value) = store.get("preferences") {
        match serde_json::from_value::<WidgetPreferences>(value.clone()) {
            Ok(p) => {
                info!("Loaded widget preferences from store");
                p
            }
            Err(e) => {
                warn!("Failed to deserialize widget preferences: {}, using defaults", e);
                WidgetPreferences::default()
            }
        }
    } else {
        info!("No stored widget preferences found, using defaults");
        WidgetPreferences::default()
    };

    Ok(prefs)
}

/// Save widget preferences to store
pub async fn save_widget_preferences<R: Runtime>(
    app: &AppHandle<R>,
    preferences: &WidgetPreferences,
) -> Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("Failed to access widget preferences store: {}", e))?;

    let prefs_value = serde_json::to_value(preferences)
        .map_err(|e| anyhow::anyhow!("Failed to serialize widget preferences: {}", e))?;

    store.set("preferences", prefs_value);

    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save widget preferences store to disk: {}", e))?;

    info!(
        "Saved widget preferences: show_widget={}, position=({:?}, {:?})",
        preferences.show_widget, preferences.position_x, preferences.position_y
    );

    Ok(())
}

/// Tauri command: get widget preferences (show/hide + last known position)
#[tauri::command]
pub async fn get_widget_preferences<R: Runtime>(
    app: AppHandle<R>,
) -> Result<WidgetPreferences, String> {
    load_widget_preferences(&app).await.map_err(|e| e.to_string())
}

/// Tauri command: persist widget preferences (show/hide + last known position)
#[tauri::command]
pub async fn set_widget_preferences<R: Runtime>(
    app: AppHandle<R>,
    preferences: WidgetPreferences,
) -> Result<(), String> {
    save_widget_preferences(&app, &preferences)
        .await
        .map_err(|e| e.to_string())
}
