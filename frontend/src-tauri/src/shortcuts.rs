use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;

pub const DEFAULT_SHORTCUT: &str = "Control+F8";
const SHORTCUT_STORE_KEY: &str = "recording_shortcut";
const STORE_FILE: &str = "shortcuts.json";

pub fn normalize_shortcut(s: &str) -> String {
    let mut parts: Vec<String> = s.split('+').map(|p| p.trim().to_string()).collect();
    if parts.is_empty() {
        return DEFAULT_SHORTCUT.to_string();
    }
    let last = parts.pop().unwrap();
    parts.sort();
    let mut sorted: Vec<String> = parts.into_iter().map(|p| normalize_modifier(&p)).collect();
    sorted.push(last);
    sorted.join("+")
}

fn normalize_modifier(m: &str) -> String {
    match m.to_lowercase().as_str() {
        "ctrl" | "control" => "Control".to_string(),
        "alt" | "option" => "Alt".to_string(),
        "shift" => "Shift".to_string(),
        "meta" | "super" | "cmd" | "command" => "Meta".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
    }
}

pub async fn load_shortcut<R: Runtime>(app: &AppHandle<R>) -> String {
    match app.store(STORE_FILE) {
        Ok(store) => {
            if let Some(val) = store.get(SHORTCUT_STORE_KEY) {
                if let Some(s) = val.as_str() {
                    if !s.is_empty() {
                        return s.to_string();
                    }
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open shortcuts store: {}", e);
        }
    }
    DEFAULT_SHORTCUT.to_string()
}

pub async fn save_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| format!("Failed to open shortcuts store: {}", e))?;
    store.set(SHORTCUT_STORE_KEY, serde_json::json!(shortcut));
    store
        .save()
        .map_err(|e| format!("Failed to save shortcuts store: {}", e))?;
    Ok(())
}

pub async fn register_shortcut<R: Runtime>(
    app: &AppHandle<R>,
    shortcut_str: &str,
) -> Result<(), String> {
    let app_clone = app.clone();
    let shortcut_str_owned = shortcut_str.to_string();
    app.global_shortcut()
        .on_shortcut(shortcut_str, move |_app, shortcut, event| {
            if event.state == ShortcutState::Pressed {
                log::info!("Global shortcut triggered: {:?}", shortcut);
                let inner = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    crate::tray::toggle_recording_via_shortcut(&inner).await;
                });
            }
        })
        .map_err(|e| {
            format!(
                "Failed to register shortcut '{}': {}",
                shortcut_str_owned, e
            )
        })?;
    log::info!("Registered global shortcut: {}", shortcut_str_owned);
    Ok(())
}

pub fn unregister_all<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.global_shortcut().unregister_all() {
        log::warn!("Failed to unregister all shortcuts: {}", e);
    }
}

pub async fn init<R: Runtime>(app: &AppHandle<R>) {
    let shortcut = load_shortcut(app).await;
    if let Err(e) = register_shortcut(app, &shortcut).await {
        log::error!("Failed to register initial shortcut '{}': {}", shortcut, e);
    }
}

#[cfg(target_os = "macos")]
pub fn check_accessibility_permission() -> bool {
    use std::process::Command;
    Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get name of every process",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn check_accessibility_permission() -> bool {
    true
}

#[tauri::command]
pub async fn get_recording_shortcut<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    Ok(load_shortcut(&app).await)
}

#[tauri::command]
pub async fn set_recording_shortcut<R: Runtime>(
    app: AppHandle<R>,
    shortcut: String,
) -> Result<(), String> {
    if shortcut.trim().is_empty() {
        return Err("Shortcut cannot be empty".to_string());
    }
    let normalized = normalize_shortcut(&shortcut);
    unregister_all(&app);
    save_shortcut(&app, &normalized).await?;
    register_shortcut(&app, &normalized).await?;
    Ok(())
}

#[tauri::command]
pub fn check_shortcut_permission() -> Result<bool, String> {
    Ok(check_accessibility_permission())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ctrl_f8() {
        let result = normalize_shortcut("ctrl+f8");
        assert_eq!(result, "Control+f8");
    }

    #[test]
    fn normalize_already_canonical() {
        let result = normalize_shortcut("Control+F8");
        assert_eq!(result, "Control+F8");
    }

    #[test]
    fn normalize_multiple_modifiers() {
        let result = normalize_shortcut("shift+ctrl+a");
        assert!(result.contains("Control"));
        assert!(result.contains("Shift"));
        assert!(result.ends_with("+a"));
    }
}
