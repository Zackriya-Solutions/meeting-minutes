use crate::state::AppState;
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use super::audio_processing::sanitize_filename;
use super::ffmpeg::find_ffmpeg_path;
use super::retranscription::find_audio_file;

async fn meeting_audio_source(
    state: &State<'_, AppState>,
    meeting_id: &str,
) -> Result<(PathBuf, String), String> {
    let row = sqlx::query_as::<_, (Option<String>, String)>(
        "SELECT folder_path, title FROM meetings WHERE id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(state.db_manager.pool())
    .await
    .map_err(|error| format!("Database error: {error}"))?
    .ok_or_else(|| "Meeting not found".to_string())?;

    let folder = row
        .0
        .map(PathBuf::from)
        .ok_or_else(|| "Recording folder path is not available for this meeting".to_string())?;
    if !folder.is_dir() {
        return Err("Recording folder was not found".to_string());
    }

    let audio = find_audio_file(&folder).map_err(|error| error.to_string())?;
    Ok((audio, row.1))
}

fn suggested_mp3_name(title: &str) -> String {
    let sanitized = sanitize_filename(title);
    let stem = if sanitized.is_empty() {
        "Memento recording"
    } else {
        sanitized.as_str()
    };
    format!("{stem}.mp3")
}

fn ensure_mp3_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
    {
        path
    } else {
        path.with_extension("mp3")
    }
}

/// Resolve the stored audio for a meeting. The path is discovered inside the
/// meeting folder rather than accepted from the webview.
#[tauri::command]
pub async fn get_meeting_audio_path<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    let (audio, _) = meeting_audio_source(&state, &meeting_id).await?;
    // Grant the asset protocol access to this exact database-owned recording,
    // not to the whole Movies/Documents directory. The webview can then stream
    // and seek without serialising a potentially multi-gigabyte file over IPC.
    app.asset_protocol_scope()
        .allow_file(&audio)
        .map_err(|error| format!("Cannot expose the meeting recording for playback: {error}"))?;
    Ok(audio.to_string_lossy().into_owned())
}

/// Export a copy of the meeting recording as a standard 192 kbps MP3.
/// The original AAC/MP4 recording remains untouched.
#[tauri::command]
pub async fn export_meeting_audio_mp3<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<String>, String> {
    let (source, title) = meeting_audio_source(&state, &meeting_id).await?;
    let default_name = suggested_mp3_name(&title);

    let selected = app
        .dialog()
        .file()
        .add_filter("MP3 audio", &["mp3"])
        .set_file_name(default_name)
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let output = ensure_mp3_extension(
        selected
            .into_path()
            .map_err(|error| format!("Invalid export path: {error}"))?,
    );

    if same_file_path(&source, &output) {
        return Err(
            "Choose a different file name so the original recording is not overwritten".to_string(),
        );
    }

    let ffmpeg = find_ffmpeg_path()
        .ok_or_else(|| "The bundled audio converter is unavailable".to_string())?;
    let source_for_task = source.clone();
    let output_for_task = output.clone();
    let result = tokio::task::spawn_blocking(move || {
        Command::new(ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .arg("-i")
            .arg(&source_for_task)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-codec:a",
                "libmp3lame",
                "-b:a",
                "192k",
                "-id3v2_version",
                "3",
            ])
            .arg(&output_for_task)
            .output()
    })
    .await
    .map_err(|error| format!("MP3 export task failed: {error}"))?
    .map_err(|error| format!("Failed to start MP3 export: {error}"))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
        warn!("MP3 export failed for meeting {meeting_id}: {stderr}");
        return Err(if stderr.is_empty() {
            "MP3 export failed".to_string()
        } else {
            format!("MP3 export failed: {stderr}")
        });
    }

    info!(
        "Exported meeting {} audio from {} to {}",
        meeting_id,
        source.display(),
        output.display()
    );
    Ok(Some(output.to_string_lossy().into_owned()))
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp3_name_is_safe_and_has_extension() {
        assert_eq!(
            suggested_mp3_name("Standup: API / release"),
            "Standup_ API _ release.mp3"
        );
        assert_eq!(suggested_mp3_name("   "), "Memento recording.mp3");
    }

    #[test]
    fn mp3_extension_is_added_or_preserved_case_insensitively() {
        assert_eq!(
            ensure_mp3_extension(PathBuf::from("meeting")),
            PathBuf::from("meeting.mp3")
        );
        assert_eq!(
            ensure_mp3_extension(PathBuf::from("meeting.wav")),
            PathBuf::from("meeting.mp3")
        );
        assert_eq!(
            ensure_mp3_extension(PathBuf::from("meeting.MP3")),
            PathBuf::from("meeting.MP3")
        );
    }
}
