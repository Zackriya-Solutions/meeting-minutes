//! Path resolution shared with the main Memento app.
//!
//! These deliberately mirror the main app's conventions so recordings land where
//! Memento expects them and register into the same database.

use std::path::{Path, PathBuf};

/// Bundle identifier of the MAIN app (not this detector). The main app's Tauri
/// data/config directory and SQLite DB live under this id.
pub const MAIN_APP_BUNDLE_ID: &str = "com.meetily.ai";

/// `~/Library/Application Support/com.meetily.ai` on macOS.
///
/// This is where the main app keeps its SQLite DB and Tauri store files. Tauri's
/// `app_data_dir()`/`app_config_dir()` both resolve here on macOS.
pub fn main_app_support_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(MAIN_APP_BUNDLE_ID))
}

/// Path to the main app's SQLite database (`meeting_minutes.sqlite`).
pub fn database_path() -> Option<PathBuf> {
    main_app_support_dir().map(|d| d.join("meeting_minutes.sqlite"))
}

/// Path to the main app's recording-preferences Tauri store file.
fn recording_preferences_store() -> Option<PathBuf> {
    main_app_support_dir().map(|d| d.join("recording_preferences.json"))
}

/// Resolve the recordings folder the main app writes to.
///
/// Prefers the user's configured `save_folder` from the Tauri store, falling back
/// to the branded default (`~/Movies/memento-recordings`, or the legacy
/// `~/Movies/meetily-recordings` if it already exists).
pub fn recordings_folder() -> PathBuf {
    if let Some(folder) = save_folder_from_store() {
        return folder;
    }
    default_recordings_folder()
}

/// Read `preferences.save_folder` out of the Tauri store JSON, if present.
fn save_folder_from_store() -> Option<PathBuf> {
    let store = recording_preferences_store()?;
    let bytes = std::fs::read(&store).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // tauri-plugin-store persists a flat map of key -> value; the app stores its
    // RecordingPreferences under the "preferences" key.
    let save = json
        .get("preferences")
        .and_then(|p| p.get("save_folder"))
        .and_then(|v| v.as_str())?;
    let path = PathBuf::from(save);
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Branded default recordings folder, matching the main app's
/// `branded_recordings_folder()` logic.
fn default_recordings_folder() -> PathBuf {
    let parent = if cfg!(target_os = "macos") {
        dirs::video_dir()
    } else if cfg!(target_os = "windows") {
        dirs::audio_dir()
    } else {
        dirs::document_dir()
    }
    .unwrap_or_else(|| PathBuf::from("."));

    let legacy = parent.join("meetily-recordings");
    if legacy.exists() {
        legacy
    } else {
        parent.join("memento-recordings")
    }
}

/// Locate an ffmpeg executable, matching the strategy the main app uses.
///
/// Order: explicit override env var, next to our own executable (bundled .app),
/// the app bundle's Resources dir, the main app's source-tree sidecar (dev), then
/// common system locations / PATH.
pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MEMENTO_DETECTOR_FFMPEG") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("ffmpeg");
            if bundled.is_file() {
                return Some(bundled);
            }
            let resources = dir.join("../Resources/ffmpeg");
            if resources.is_file() {
                return Some(resources);
            }
        }
    }

    // Development: reuse the main app's bundled ffmpeg sidecar from the source tree.
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let dev_sidecar = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/src-tauri/binaries")
        .join(format!("ffmpeg-{arch}-apple-darwin"));
    if dev_sidecar.is_file() {
        return Some(dev_sidecar);
    }

    for candidate in [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ] {
        let pb = PathBuf::from(candidate);
        if pb.is_file() {
            return Some(pb);
        }
    }

    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = Path::new(&dir).join("ffmpeg");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }

    None
}
