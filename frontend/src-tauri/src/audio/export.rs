use crate::state::AppState;
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use super::audio_processing::sanitize_filename;
use super::ffmpeg::find_ffmpeg_path;
use super::retranscription::find_audio_file;

#[derive(serde::Serialize)]
pub struct MeetingAudioPlaybackInfo {
    pub path: String,
    pub duration_seconds: f64,
}

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

fn source_cache_key(source: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(source)
        .map_err(|error| format!("Cannot inspect the meeting recording: {error}"))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or(0);
    Ok(format!("{}-{modified}", metadata.len()))
}

fn prepare_playback_file(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.is_file() {
        return Ok(());
    }
    let ffmpeg = find_ffmpeg_path()
        .ok_or_else(|| "The bundled audio converter is unavailable".to_string())?;
    let destination_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("meeting-audio.m4a");
    let temporary = destination.with_file_name(format!(
        ".{destination_name}.{}.part",
        uuid::Uuid::new_v4()
    ));

    // A remux is effectively instant even for long recordings. It fixes legacy files
    // containing raw ADTS AAC under an `.m4a` name and gives audio-only MP4 a MIME type
    // handled consistently by WKWebView. Re-encode only for incompatible source codecs.
    let run = |codec: &str| {
        Command::new(&ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(source)
            .args(["-map", "0:a:0", "-vn", "-c:a", codec])
            .args(["-movflags", "+faststart", "-f", "mp4"])
            .arg(&temporary)
            .output()
            .map_err(|error| format!("Failed to start the audio converter: {error}"))
    };
    let first = run("copy")?;
    let output = if first.status.success() {
        first
    } else {
        let _ = std::fs::remove_file(&temporary);
        run("aac")?
    };
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "Could not prepare recording for playback: {}",
            detail.trim()
        ));
    }
    match std::fs::rename(&temporary, destination) {
        Ok(()) => Ok(()),
        // Another concurrent request may have atomically published its own valid remux.
        Err(_) if destination.is_file() => {
            let _ = std::fs::remove_file(&temporary);
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!("Could not cache the playable recording: {error}"))
        }
    }
}

/// Remove all derived playback copies for a deleted meeting. The source recording is
/// handled separately by the delete command; cached audio must never outlive the meeting.
pub fn remove_meeting_audio_playback_cache<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
) -> Result<(), String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Cannot resolve the playback cache: {error}"))?
        .join("meeting-audio");
    if !cache_dir.is_dir() {
        return Ok(());
    }

    let prefix = format!("{}-", sanitize_filename(meeting_id));
    let temporary_prefix = format!(".{prefix}");
    for entry in std::fs::read_dir(&cache_dir)
        .map_err(|error| format!("Cannot inspect the playback cache: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Cannot inspect a cached recording: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (name.starts_with(&prefix) || name.starts_with(&temporary_prefix))
            && entry.path().is_file()
        {
            std::fs::remove_file(entry.path())
                .map_err(|error| format!("Cannot remove a cached recording: {error}"))?;
        }
    }
    Ok(())
}

fn prune_stale_meeting_playback_files(
    cache_dir: &Path,
    meeting_id: &str,
    keep: &Path,
) -> Result<(), String> {
    let prefix = format!("{}-", sanitize_filename(meeting_id));
    for entry in std::fs::read_dir(cache_dir)
        .map_err(|error| format!("Cannot inspect the playback cache: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Cannot inspect a cached recording: {error}"))?;
        let path = entry.path();
        if path != keep
            && path.is_file()
            && entry.file_name().to_string_lossy().starts_with(&prefix)
        {
            std::fs::remove_file(path)
                .map_err(|error| format!("Cannot prune a stale cached recording: {error}"))?;
        }
    }
    Ok(())
}

/// Return a seekable WebView-compatible path and native duration without changing the
/// original recording.
#[tauri::command]
pub async fn get_meeting_audio_playback_info<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingAudioPlaybackInfo, String> {
    let (source, _) = meeting_audio_source(&state, &meeting_id).await?;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Cannot resolve the playback cache: {error}"))?
        .join("meeting-audio");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("Cannot create the playback cache: {error}"))?;
    let destination = cache_dir.join(format!(
        "{}-{}.m4a",
        sanitize_filename(&meeting_id),
        source_cache_key(&source)?
    ));
    prune_stale_meeting_playback_files(&cache_dir, &meeting_id, &destination)?;
    let source_for_task = source.clone();
    let destination_for_task = destination.clone();
    tokio::task::spawn_blocking(move || {
        prepare_playback_file(&source_for_task, &destination_for_task)
    })
    .await
    .map_err(|error| format!("Audio preparation task failed: {error}"))??;
    let meeting_still_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM meetings WHERE id = ?)",
    )
    .bind(&meeting_id)
    .fetch_one(state.db_manager.pool())
    .await
    .map_err(|error| format!("Database error: {error}"))?;
    if !meeting_still_exists {
        let _ = remove_meeting_audio_playback_cache(&app, &meeting_id);
        return Err("Meeting was deleted while preparing its recording".to_string());
    }
    let duration_seconds = super::import::extract_duration_from_metadata(&destination)
        .map_err(|error| format!("Cannot read recording duration: {error}"))?;
    app.asset_protocol_scope()
        .allow_file(&destination)
        .map_err(|error| format!("Cannot expose the meeting recording for playback: {error}"))?;
    Ok(MeetingAudioPlaybackInfo {
        path: destination.to_string_lossy().into_owned(),
        duration_seconds,
    })
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
