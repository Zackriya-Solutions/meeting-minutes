use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;

use anyhow::Result;
#[cfg(target_os = "macos")]
use log::error;

#[cfg(target_os = "macos")]
use crate::audio::capture::AudioCaptureBackend;

static RECORDING_PREFERENCES_SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn serialize_save_transaction<T, F, Fut>(transaction: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let _guard = RECORDING_PREFERENCES_SAVE_LOCK.lock().await;
    transaction().await
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecordingPreferences {
    pub save_folder: PathBuf,
    pub auto_save: bool,
    pub file_format: String,
    #[serde(default)]
    pub preferred_mic_device: Option<String>,
    #[serde(default)]
    pub preferred_system_device: Option<String>,
    #[cfg(target_os = "macos")]
    #[serde(default)]
    pub system_audio_backend: Option<String>,
}

impl Default for RecordingPreferences {
    fn default() -> Self {
        Self {
            save_folder: get_default_recordings_folder(),
            auto_save: true,
            file_format: "mp4".to_string(),
            preferred_mic_device: None,
            preferred_system_device: None,
            #[cfg(target_os = "macos")]
            system_audio_backend: Some("coreaudio".to_string()),
        }
    }
}

/// Get the default recordings folder based on platform
pub fn get_default_recordings_folder() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Windows: %USERPROFILE%\Music\meetily-recordings
        if let Some(music_dir) = dirs::audio_dir() {
            music_dir.join("meetily-recordings")
        } else {
            // Fallback to Documents if Music folder is not available
            dirs::document_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("meetily-recordings")
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Movies/meetily-recordings
        if let Some(movies_dir) = dirs::video_dir() {
            movies_dir.join("meetily-recordings")
        } else {
            // Fallback to Documents if Movies folder is not available
            dirs::document_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("meetily-recordings")
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Linux/Others: ~/Documents/meetily-recordings
        dirs::document_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("meetily-recordings")
    }
}

/// Ensure the recordings directory exists
pub fn ensure_recordings_directory(path: &PathBuf) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            return Err(anyhow::anyhow!(
                "Recording save path is not a directory: {}",
                path.display()
            ));
        }
    } else {
        std::fs::create_dir_all(path)?;
        info!("Created recordings directory: {:?}", path);
    }
    Ok(())
}

fn persist_preferences_value(
    preferences: &RecordingPreferences,
    previous_value: Option<serde_json::Value>,
    mut update_store: impl FnMut(Option<serde_json::Value>),
    save_store: impl FnOnce() -> Result<()>,
) -> Result<()> {
    ensure_recordings_directory(&preferences.save_folder)?;
    let preferences_value = serde_json::to_value(preferences)
        .map_err(|error| anyhow::anyhow!("Failed to serialize preferences: {}", error))?;

    update_store(Some(preferences_value));
    if let Err(error) = save_store() {
        update_store(previous_value);
        return Err(error);
    }

    Ok(())
}

fn merge_recording_save_folder(
    mut preferences: RecordingPreferences,
    save_folder: PathBuf,
) -> RecordingPreferences {
    preferences.save_folder = save_folder;
    preferences
}

/// Generate a unique filename for a recording
pub fn generate_recording_filename(format: &str) -> String {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d_%H%M%S");
    format!("recording_{}.{}", timestamp, format)
}

/// Load recording preferences from store
pub async fn load_recording_preferences<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<RecordingPreferences> {
    // Try to load from Tauri store
    let store = match app.store("recording_preferences.json") {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to access store: {}, using defaults", e);
            return Ok(RecordingPreferences::default());
        }
    };

    // Try to get the preferences from store
    let prefs = if let Some(value) = store.get("preferences") {
        match serde_json::from_value::<RecordingPreferences>(value.clone()) {
            Ok(mut p) => {
                info!("Loaded recording preferences from store");
                // Update macOS backend to current value if needed
                #[cfg(target_os = "macos")]
                {
                    let backend = crate::audio::capture::get_current_backend();
                    p.system_audio_backend = Some(backend.to_string());
                }
                p
            }
            Err(e) => {
                warn!("Failed to deserialize preferences: {}, using defaults", e);
                RecordingPreferences::default()
            }
        }
    } else {
        info!("No stored preferences found, using defaults");
        RecordingPreferences::default()
    };

    info!("Loaded recording preferences: save_folder={:?}, auto_save={}, format={}, mic={:?}, system={:?}",
          prefs.save_folder, prefs.auto_save, prefs.file_format,
          prefs.preferred_mic_device, prefs.preferred_system_device);
    Ok(prefs)
}

/// Save recording preferences to store
pub async fn save_recording_preferences<R: Runtime>(
    app: &AppHandle<R>,
    preferences: &RecordingPreferences,
) -> Result<()> {
    info!("Saving recording preferences: save_folder={:?}, auto_save={}, format={}, mic={:?}, system={:?}",
          preferences.save_folder, preferences.auto_save, preferences.file_format,
          preferences.preferred_mic_device, preferences.preferred_system_device);

    serialize_save_transaction(|| async {
        let store = app
            .store("recording_preferences.json")
            .map_err(|e| anyhow::anyhow!("Failed to access store: {}", e))?;
        let previous_value = store.get("preferences");

        persist_preferences_value(
            preferences,
            previous_value,
            |value| match value {
                Some(value) => store.set("preferences", value),
                None => {
                    store.delete("preferences");
                }
            },
            || {
                store
                    .save()
                    .map_err(|error| anyhow::anyhow!("Failed to save store to disk: {}", error))
            },
        )?;

        if let Err(error) = app.emit("recording-preferences-updated", preferences) {
            warn!("Failed to emit recording preferences update: {}", error);
        }
        Ok(())
    })
    .await?;

    info!("Successfully persisted recording preferences to disk");

    // Save backend preference to global config
    #[cfg(target_os = "macos")]
    if let Some(backend_str) = &preferences.system_audio_backend {
        if let Some(backend) = AudioCaptureBackend::from_string(backend_str) {
            info!("Setting audio capture backend to: {:?}", backend);
            crate::audio::capture::set_current_backend(backend);
        }
    }

    Ok(())
}

/// Tauri commands for recording preferences
#[tauri::command]
pub async fn get_recording_preferences<R: Runtime>(
    app: AppHandle<R>,
) -> Result<RecordingPreferences, String> {
    load_recording_preferences(&app)
        .await
        .map_err(|e| format!("Failed to load recording preferences: {}", e))
}

#[tauri::command]
pub async fn set_recording_preferences<R: Runtime>(
    app: AppHandle<R>,
    preferences: RecordingPreferences,
) -> Result<(), String> {
    save_recording_preferences(&app, &preferences)
        .await
        .map_err(|e| format!("Failed to save recording preferences: {}", e))
}

#[tauri::command]
pub async fn set_recording_save_folder<R: Runtime>(
    app: AppHandle<R>,
    save_folder: String,
) -> Result<RecordingPreferences, String> {
    serialize_save_transaction(|| async {
        let store = app
            .store("recording_preferences.json")
            .map_err(|e| anyhow::anyhow!("Failed to access store: {}", e))?;
        let previous_value = store.get("preferences");
        let latest_preferences = match previous_value.as_ref() {
            Some(value) => serde_json::from_value::<RecordingPreferences>(value.clone())
                .map_err(|e| anyhow::anyhow!("Failed to deserialize preferences: {}", e))?,
            None => RecordingPreferences::default(),
        };
        let preferences =
            merge_recording_save_folder(latest_preferences, PathBuf::from(save_folder));

        persist_preferences_value(
            &preferences,
            previous_value,
            |value| match value {
                Some(value) => store.set("preferences", value),
                None => {
                    store.delete("preferences");
                }
            },
            || {
                store
                    .save()
                    .map_err(|error| anyhow::anyhow!("Failed to save store to disk: {}", error))
            },
        )?;

        if let Err(error) = app.emit("recording-preferences-updated", &preferences) {
            warn!("Failed to emit recording preferences update: {}", error);
        }

        Ok(preferences)
    })
    .await
    .map_err(|e| format!("Failed to save recording folder: {}", e))
}

#[tauri::command]
pub async fn get_default_recordings_folder_path() -> Result<String, String> {
    let path = get_default_recordings_folder();
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn open_recordings_folder<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let preferences = load_recording_preferences(&app)
        .await
        .map_err(|e| format!("Failed to load preferences: {}", e))?;

    // Ensure directory exists before trying to open it
    ensure_recordings_directory(&preferences.save_folder)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    let folder_path = preferences.save_folder.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    info!("Opened recordings folder: {}", folder_path);
    Ok(())
}

#[tauri::command]
pub async fn select_recording_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    let start_dir = load_recording_preferences(&app)
        .await
        .map(|preferences| preferences.save_folder)
        .unwrap_or_else(|error| {
            warn!(
                "Failed to load recording preferences before folder selection: {}",
                error
            );
            get_default_recordings_folder()
        });

    let selected = tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        if start_dir.is_dir() {
            dialog.set_directory(start_dir).blocking_pick_folder()
        } else {
            dialog.blocking_pick_folder()
        }
    })
    .await
    .map_err(|error| format!("Folder dialog task failed: {}", error))?;

    selected
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Selected folder is not a filesystem path: {}", error))
                .and_then(|path| {
                    path.into_os_string().into_string().map_err(|_| {
                        "Selected folder path contains unsupported characters".to_string()
                    })
                })
        })
        .transpose()
}

// Backend selection commands

/// Get available audio capture backends for the current platform
#[tauri::command]
pub async fn get_available_audio_backends() -> Result<Vec<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let backends = crate::audio::capture::get_available_backends();
        Ok(backends.iter().map(|b| b.to_string()).collect())
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Only ScreenCaptureKit available on non-macOS
        Ok(vec!["screencapturekit".to_string()])
    }
}

/// Get current audio capture backend
#[tauri::command]
pub async fn get_current_audio_backend() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let backend = crate::audio::capture::get_current_backend();
        Ok(backend.to_string())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok("screencapturekit".to_string())
    }
}

/// Set audio capture backend
#[tauri::command]
pub async fn set_audio_backend(backend: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio::capture::AudioCaptureBackend;
        use crate::audio::permissions::{
            check_screen_recording_permission, request_screen_recording_permission,
        };

        let backend_enum = AudioCaptureBackend::from_string(&backend)
            .ok_or_else(|| format!("Invalid backend: {}", backend))?;

        // If switching to Core Audio, log information about Audio Capture permission
        if backend_enum == AudioCaptureBackend::CoreAudio {
            info!("🔐 Core Audio backend requires Audio Capture permission (macOS 14.4+)");
            info!("📍 Permission dialog will appear automatically when recording starts");

            // Check if permission is already granted (this is informational only)
            if !check_screen_recording_permission() {
                warn!("⚠️  Audio Capture permission may not be granted");

                // Attempt to open System Settings (opens System Settings)
                if let Err(e) = request_screen_recording_permission() {
                    error!("Failed to open System Settings: {}", e);
                }

                return Err(
                    "Core Audio requires Audio Capture permission. \
                    The permission dialog will appear when you start recording. \
                    If already denied, enable it in System Settings → Privacy & Security → Audio Capture, \
                    then restart the app.".to_string()
                );
            }

            info!(
                "✅ Core Audio backend selected - permission check will occur at recording start"
            );
        }

        info!("Setting audio backend to: {:?}", backend_enum);
        crate::audio::capture::set_current_backend(backend_enum);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        if backend != "screencapturekit" {
            return Err(format!(
                "Backend {} not available on this platform",
                backend
            ));
        }
        Ok(())
    }
}

/// Get backend information (name and description)
#[derive(Serialize)]
pub struct BackendInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn get_audio_backend_info() -> Result<Vec<BackendInfo>, String> {
    #[cfg(target_os = "macos")]
    {
        use crate::audio::capture::AudioCaptureBackend;

        let backends = vec![
            BackendInfo {
                id: AudioCaptureBackend::ScreenCaptureKit.to_string(),
                name: AudioCaptureBackend::ScreenCaptureKit.name().to_string(),
                description: AudioCaptureBackend::ScreenCaptureKit
                    .description()
                    .to_string(),
            },
            BackendInfo {
                id: AudioCaptureBackend::CoreAudio.to_string(),
                name: AudioCaptureBackend::CoreAudio.name().to_string(),
                description: AudioCaptureBackend::CoreAudio.description().to_string(),
            },
        ];
        Ok(backends)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![BackendInfo {
            id: "screencapturekit".to_string(),
            name: "ScreenCaptureKit".to_string(),
            description: "Default system audio capture".to_string(),
        }])
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::{
        merge_recording_save_folder, persist_preferences_value, serialize_save_transaction,
        RecordingPreferences,
    };
    use serde_json::{json, Value};
    use std::cell::{Cell, RefCell};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::sync::{Mutex, Notify};

    #[test]
    fn folder_only_update_preserves_latest_preferences() {
        let mut latest = RecordingPreferences::default();
        latest.auto_save = false;
        latest.file_format = "wav".to_string();
        latest.preferred_mic_device = Some("Latest microphone".to_string());
        latest.preferred_system_device = Some("Latest system audio".to_string());
        let selected_folder = std::env::temp_dir().join("selected-recordings");

        let updated = merge_recording_save_folder(latest, selected_folder.clone());

        assert_eq!(updated.save_folder, selected_folder);
        assert!(!updated.auto_save);
        assert_eq!(updated.file_format, "wav");
        assert_eq!(
            updated.preferred_mic_device.as_deref(),
            Some("Latest microphone")
        );
        assert_eq!(
            updated.preferred_system_device.as_deref(),
            Some("Latest system audio")
        );
    }

    #[test]
    fn invalid_directory_is_rejected_before_store_mutation() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let file_path = temp_dir.path().join("not-a-directory");
        std::fs::write(&file_path, b"file").expect("create file root");
        let preferences = RecordingPreferences {
            save_folder: file_path,
            ..RecordingPreferences::default()
        };
        let mutations = RefCell::new(Vec::<Option<Value>>::new());
        let save_called = Cell::new(false);

        let result = persist_preferences_value(
            &preferences,
            None,
            |value| mutations.borrow_mut().push(value),
            || {
                save_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_err());
        assert!(mutations.borrow().is_empty());
        assert!(!save_called.get());
    }

    #[test]
    fn save_failure_restores_previous_in_memory_value() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let preferences = RecordingPreferences {
            save_folder: temp_dir.path().join("selected"),
            ..RecordingPreferences::default()
        };
        let previous = json!({"save_folder": "/tmp/previous"});
        let mutations = RefCell::new(Vec::<Option<Value>>::new());

        let result = persist_preferences_value(
            &preferences,
            Some(previous.clone()),
            |value| mutations.borrow_mut().push(value),
            || Err(anyhow::anyhow!("simulated disk save failure")),
        );

        assert!(result.is_err());
        let mutations = mutations.into_inner();
        assert_eq!(mutations.len(), 2);
        assert_eq!(
            mutations[0],
            Some(serde_json::to_value(&preferences).expect("serialize preferences"))
        );
        assert_eq!(mutations[1], Some(previous));
    }

    #[test]
    fn save_failure_removes_new_value_when_store_was_empty() {
        let temp_dir = tempfile::tempdir().expect("temporary directory");
        let preferences = RecordingPreferences {
            save_folder: temp_dir.path().join("selected"),
            ..RecordingPreferences::default()
        };
        let mutations = RefCell::new(Vec::<Option<Value>>::new());

        let result = persist_preferences_value(
            &preferences,
            None,
            |value| mutations.borrow_mut().push(value),
            || Err(anyhow::anyhow!("simulated disk save failure")),
        );

        assert!(result.is_err());
        let mutations = mutations.into_inner();
        assert_eq!(mutations.len(), 2);
        assert_eq!(
            mutations[0],
            Some(serde_json::to_value(&preferences).expect("serialize preferences"))
        );
        assert_eq!(mutations[1], None);
    }

    #[tokio::test]
    async fn concurrent_save_failure_cannot_rollback_a_successful_transaction() {
        #[derive(Default)]
        struct SimulatedStore {
            cache: Option<&'static str>,
            durable: Option<&'static str>,
        }

        let store = Arc::new(Mutex::new(SimulatedStore::default()));
        let first_entered = Arc::new(Notify::new());
        let release_first = Arc::new(Notify::new());
        let second_attempting = Arc::new(Notify::new());
        let second_entered = Arc::new(AtomicBool::new(false));

        let successful = {
            let store = store.clone();
            let first_entered = first_entered.clone();
            let release_first = release_first.clone();
            tokio::spawn(async move {
                serialize_save_transaction(|| async move {
                    store.lock().await.cache = Some("successful");
                    first_entered.notify_one();
                    release_first.notified().await;
                    store.lock().await.durable = Some("successful");
                    Ok::<_, anyhow::Error>("successful")
                })
                .await
            })
        };

        first_entered.notified().await;
        let failed = {
            let store = store.clone();
            let second_attempting = second_attempting.clone();
            let second_entered = second_entered.clone();
            tokio::spawn(async move {
                second_attempting.notify_one();
                serialize_save_transaction(|| async move {
                    second_entered.store(true, Ordering::SeqCst);
                    let previous = store.lock().await.cache;
                    store.lock().await.cache = Some("failed");
                    store.lock().await.cache = previous;
                    Err::<&'static str, _>(anyhow::anyhow!("save failed for failed payload"))
                })
                .await
            })
        };

        second_attempting.notified().await;
        tokio::task::yield_now().await;
        assert!(!second_entered.load(Ordering::SeqCst));
        release_first.notify_one();

        assert_eq!(
            successful.await.expect("successful task").unwrap(),
            "successful"
        );
        let failed_error = failed
            .await
            .expect("failed task")
            .expect_err("second transaction must fail");
        assert!(failed_error.to_string().contains("failed payload"));
        let store = store.lock().await;
        assert_eq!(store.cache, Some("successful"));
        assert_eq!(store.durable, Some("successful"));
    }
}
