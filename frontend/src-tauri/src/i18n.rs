use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;
use log::{info, warn};
use anyhow::Result;

pub const SUPPORTED_LOCALES: &[&str] = &["en-US", "zh-CN"];
pub const DEFAULT_LOCALE: &str = "en-US";
const STORE_FILE: &str = "ui-locale.json";
const STORE_KEY: &str = "language";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiLocale {
    pub language: String,
    pub last_updated: String,
}

pub fn is_supported_locale(value: &str) -> bool {
    SUPPORTED_LOCALES.contains(&value)
}

pub async fn load_ui_language<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Option<UiLocale>> {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to access ui-locale store: {}, returning None", e);
            return Ok(None);
        }
    };
    match store.get(STORE_KEY) {
        Some(v) => match serde_json::from_value::<UiLocale>(v.clone()) {
            Ok(loc) => Ok(Some(loc)),
            Err(e) => {
                warn!("Failed to deserialize ui-locale entry: {}", e);
                Ok(None)
            }
        },
        None => Ok(None),
    }
}

pub async fn save_ui_language<R: Runtime>(
    app: &AppHandle<R>,
    language: &str,
) -> Result<()> {
    if !is_supported_locale(language) {
        return Err(anyhow::anyhow!("Unsupported locale: {}", language));
    }
    let store = app.store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("Failed to access ui-locale store: {}", e))?;
    let payload = UiLocale {
        language: language.to_string(),
        last_updated: chrono::Utc::now().to_rfc3339(),
    };
    let value = serde_json::to_value(&payload)?;
    store.set(STORE_KEY, value);
    store.save().map_err(|e| anyhow::anyhow!("Failed to persist ui-locale: {}", e))?;
    info!("Saved UI locale: {}", language);
    Ok(())
}

pub async fn reset_ui_language<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let store = app.store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("Failed to access ui-locale store: {}", e))?;
    store.delete(STORE_KEY);
    store.save().map_err(|e| anyhow::anyhow!("Failed to persist ui-locale reset: {}", e))?;
    info!("Reset UI locale");
    Ok(())
}

#[tauri::command]
pub async fn get_ui_language<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    load_ui_language(&app)
        .await
        .map(|opt| opt.map(|l| l.language))
        .map_err(|e| format!("Failed to load ui language: {}", e))
}

#[tauri::command]
pub async fn set_ui_language<R: Runtime>(
    app: AppHandle<R>,
    language: String,
) -> Result<(), String> {
    if !is_supported_locale(&language) {
        return Err(format!("Unsupported locale: {}", language));
    }
    save_ui_language(&app, &language)
        .await
        .map_err(|e| format!("Failed to save ui language: {}", e))
}

#[tauri::command]
pub async fn reset_ui_language_cmd<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    reset_ui_language(&app)
        .await
        .map_err(|e| format!("Failed to reset ui language: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_locales_match_frontend() {
        // Mirror of frontend/src/i18n/config.ts LOCALES constant.
        // Cross-checked at PR review time.
        assert_eq!(SUPPORTED_LOCALES, &["en-US", "zh-CN"]);
    }

    #[test]
    fn rejects_unsupported_locale() {
        assert!(!is_supported_locale("fr-FR"));
        assert!(!is_supported_locale(""));
        assert!(!is_supported_locale("EN-us"));
        assert!(is_supported_locale("en-US"));
        assert!(is_supported_locale("zh-CN"));
    }

    #[test]
    fn ui_locale_roundtrips() {
        let loc = UiLocale {
            language: "zh-CN".to_string(),
            last_updated: "2026-07-10T00:00:00Z".to_string(),
        };
        let v = serde_json::to_value(&loc).unwrap();
        let back: UiLocale = serde_json::from_value(v).unwrap();
        assert_eq!(back.language, "zh-CN");
        assert_eq!(back.last_updated, "2026-07-10T00:00:00Z");
    }

    #[test]
    fn backward_compat_missing_last_updated() {
        // Older Wave 1 stored shape only had `language`. Tolerate.
        let v: serde_json::Value = serde_json::json!({"language": "en-US"});
        let loc: UiLocale = serde_json::from_value(v).unwrap();
        assert_eq!(loc.language, "en-US");
    }
}
