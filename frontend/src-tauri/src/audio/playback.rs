// Meeting recording playback support
//
// Exposes the stored audio recording of a meeting to the webview so the
// frontend can play it back (and sync it with transcript timestamps).
// Recordings live in the user-configurable recordings folder, which is
// outside the static asset protocol scope, so the located file is allowed
// into the scope at runtime before its path is returned.

use log::{info, warn};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

/// Locate the audio recording inside a meeting folder and make it playable.
///
/// Returns the absolute path of the audio file (e.g. `audio.mp4`) after
/// adding it to the asset protocol scope, so the frontend can stream it via
/// `convertFileSrc`. Returns `None` when the meeting has no folder on disk
/// or the folder contains no audio file (e.g. legacy/folderless meetings).
#[tauri::command]
pub async fn get_meeting_audio_path<R: Runtime>(
    app: AppHandle<R>,
    meeting_folder_path: String,
) -> Result<Option<String>, String> {
    let folder = PathBuf::from(&meeting_folder_path);
    if !folder.is_dir() {
        warn!(
            "Playback: meeting folder does not exist: {}",
            meeting_folder_path
        );
        return Ok(None);
    }

    let audio_path = match super::retranscription::find_audio_file(&folder) {
        Ok(path) => path,
        Err(e) => {
            info!(
                "Playback: no audio recording found in {}: {}",
                meeting_folder_path, e
            );
            return Ok(None);
        }
    };

    app.asset_protocol_scope()
        .allow_file(&audio_path)
        .map_err(|e| format!("Failed to allow audio file for playback: {}", e))?;

    info!("Playback: serving audio file {}", audio_path.display());
    Ok(Some(audio_path.to_string_lossy().to_string()))
}
