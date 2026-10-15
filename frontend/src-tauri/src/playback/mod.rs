//! Playing a meeting's recording in the app: the file for the asset protocol, time table and WAV clips.
pub mod clip;

use crate::audio::decoder::{aac_decoded_frames, container_duration_s};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::diarization::timing::{
    read_metadata, recording_layout, recording_time_map, TimeMap, AAC_PRIMING_SAMPLES, CHECKPOINT_FRAMES_48K,
    CHECKPOINT_SAMPLES_48K,
};
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

/// What the player needs to play a meeting's recording.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlaybackSource {
    /// The audio file, canonical (the path the asset scope allows); the webview loads it
    /// through `convertFileSrc`.
    pub path: String,
    /// Length in seconds of container time, which is the recording clock.
    pub duration_s: f64,
    /// (clock_s, file_s) points the player interpolates; see `playback_time_table`.
    pub time_table: Vec<[f64; 2]>,
}

const NO_RECORDING: &str = "This meeting has no recording";

/// The meeting's audio file, found in the folder stored for it, with symlinks resolved.
pub(crate) async fn meeting_audio_path(pool: &SqlitePool, meeting_id: &str) -> Result<PathBuf, String> {
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to read the meeting: {}", e))?;
    let folder = meeting
        .ok_or_else(|| "Meeting not found".to_string())?
        .folder_path
        .filter(|f| !f.trim().is_empty())
        .ok_or_else(|| NO_RECORDING.to_string())?;
    let audio =
        crate::audio::retranscription::find_audio_file(Path::new(&folder)).map_err(|_| NO_RECORDING.to_string())?;
    // The asset protocol checks its scope against the resolved path, so grant and serve that one.
    std::fs::canonicalize(&audio).map_err(|_| NO_RECORDING.to_string())
}

/// Transcript clock → playback position. The player seeks the `<audio>` element and the WAV
/// clips in container time, and container time is the recording clock: joined 30 s checkpoints
/// advance the container by exactly 30 s each. Rows of a live recording count that clock, so their
/// table is the identity. Rows timed from the decoded file (`rows` is the identity, as after a
/// retranscription) count decoded frames, which a checkpoint-joined file has 1792 more of per
/// checkpoint; their table maps each checkpoint's decoded audio back onto its 30 s. A single-stream
/// file is off by its 21 ms of priming at most, which the identity leaves.
fn playback_time_table(duration_s: f64, rows: TimeMap, layout: TimeMap, decoded_s: f64) -> Vec<[f64; 2]> {
    if rows != TimeMap::Identity || layout != TimeMap::Checkpoints {
        return vec![[0.0, 0.0], [duration_s, duration_s]];
    }
    let mut table = vec![[0.0, 0.0]];
    for k in 0.. {
        let audio_start = k * CHECKPOINT_FRAMES_48K + AAC_PRIMING_SAMPLES;
        let start_s = audio_start as f64 / 48_000.0;
        if start_s >= decoded_s {
            break;
        }
        let end_s = ((audio_start + CHECKPOINT_SAMPLES_48K) as f64 / 48_000.0).min(decoded_s);
        table.push([start_s, layout.clock_s(start_s)]);
        table.push([end_s, layout.clock_s(end_s)]);
    }
    table
}

/// Source for `audio`, whose folder's metadata.json is `metadata`. Reads the file's packet table
/// but does not decode it.
pub(crate) fn playback_source(audio: &Path, metadata: Option<&Value>) -> anyhow::Result<PlaybackSource> {
    let duration_s = container_duration_s(audio)?;
    let time_table = match aac_decoded_frames(audio) {
        Ok(Some((rate, frames))) => playback_time_table(
            duration_s,
            recording_time_map(metadata, rate, frames),
            recording_layout(metadata, rate, frames),
            frames as f64 / rate as f64,
        ),
        _ => playback_time_table(duration_s, TimeMap::Unknown, TimeMap::Unknown, duration_s),
    };
    Ok(PlaybackSource { path: audio.to_string_lossy().into_owned(), duration_s, time_table })
}

/// Lets the webview load exactly this meeting's audio file and returns how to play it.
#[tauri::command]
pub async fn api_prepare_meeting_playback<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<PlaybackSource, String> {
    let audio = meeting_audio_path(state.db_manager.pool(), &meeting_id).await?;
    app.asset_protocol_scope()
        .allow_file(&audio)
        .map_err(|e| format!("Failed to allow playback of the recording: {}", e))?;
    let metadata = audio.parent().and_then(read_metadata);
    tokio::task::spawn_blocking(move || playback_source(&audio, metadata.as_ref()))
        .await
        .map_err(|e| format!("Reading the recording failed: {}", e))?
        .map_err(|e| format!("Failed to read the recording: {:#}", e))
}

/// A WAV clip (16 kHz mono) of the meeting's recording from container time `start_file_s`, for
/// webviews that cannot play the recording itself. Raw bytes: an ArrayBuffer in JS.
#[tauri::command]
pub async fn api_render_playback_clip(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    start_file_s: f64,
    seconds: f64,
) -> Result<tauri::ipc::Response, String> {
    if !start_file_s.is_finite() || !seconds.is_finite() {
        return Err("Invalid clip range".into());
    }
    log::info!("Rendering a {:.1}s playback clip at {:.1}s of meeting {}", seconds, start_file_s, meeting_id);
    let audio = meeting_audio_path(state.db_manager.pool(), &meeting_id).await?;
    let wav = tokio::task::spawn_blocking(move || clip::render_clip(&audio, start_file_s, seconds))
        .await
        .map_err(|e| format!("Rendering the clip failed: {}", e))?
        .map_err(|e| format!("Failed to render the clip: {:#}", e))?;
    Ok(tauri::ipc::Response::new(wav))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::decoder::test_audio::{joined_checkpoints, write_wav};
    use crate::database::test_support::{migrated_pool, seed_meeting};

    async fn set_folder(pool: &SqlitePool, meeting_id: &str, folder: &Path) {
        sqlx::query("UPDATE meetings SET folder_path = ? WHERE id = ?")
            .bind(folder.to_string_lossy().to_string())
            .bind(meeting_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn audio_path_comes_from_the_meeting_folder() {
        let dir = tempfile::tempdir().unwrap();
        write_wav(&dir.path().join("audio.wav"), 16_000, 1, &[0.0; 1600]);
        let pool = migrated_pool().await;
        seed_meeting(&pool, "m", &[]).await;
        set_folder(&pool, "m", dir.path()).await;
        assert_eq!(meeting_audio_path(&pool, "m").await.unwrap(), dir.path().join("audio.wav"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn audio_path_is_resolved_through_symlinks() {
        let real = tempfile::tempdir().unwrap();
        write_wav(&real.path().join("audio.wav"), 16_000, 1, &[0.0; 1600]);
        let links = tempfile::tempdir().unwrap();
        let link = links.path().join("link");
        std::os::unix::fs::symlink(real.path(), &link).unwrap();
        let pool = migrated_pool().await;
        seed_meeting(&pool, "m", &[]).await;
        set_folder(&pool, "m", &link).await;
        let expected = std::fs::canonicalize(real.path().join("audio.wav")).unwrap();
        assert_eq!(meeting_audio_path(&pool, "m").await.unwrap(), expected);
    }

    #[tokio::test]
    async fn meeting_without_audio_is_an_error() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "no-folder", &[]).await;
        assert_eq!(meeting_audio_path(&pool, "no-folder").await.unwrap_err(), "This meeting has no recording");
        let empty = tempfile::tempdir().unwrap();
        seed_meeting(&pool, "no-file", &[]).await;
        set_folder(&pool, "no-file", empty.path()).await;
        assert_eq!(meeting_audio_path(&pool, "no-file").await.unwrap_err(), "This meeting has no recording");
        assert_eq!(meeting_audio_path(&pool, "missing").await.unwrap_err(), "Meeting not found");
    }

    #[test]
    fn live_recording_plays_on_the_container_clock() {
        let dir = tempfile::tempdir().unwrap();
        let audio = joined_checkpoints(dir.path(), 2, |_| false);
        let source = playback_source(&audio, None).unwrap();
        // Two 30 s checkpoints plus the first checkpoint's 1024 frames of priming.
        let d = 60.0 + 1024.0 / 48_000.0;
        assert_eq!(source.path, audio.to_string_lossy());
        assert!((source.duration_s - d).abs() < 1e-6, "duration {}", source.duration_s);
        assert_eq!(source.time_table, vec![[0.0, 0.0], [source.duration_s, source.duration_s]]);
    }

    #[test]
    fn retranscribed_live_recording_plays_each_checkpoint_on_the_container_clock() {
        use crate::diarization::timing::{TimeMap, AAC_PRIMING_SAMPLES, CHECKPOINT_FRAMES_48K};
        let dir = tempfile::tempdir().unwrap();
        let audio = joined_checkpoints(dir.path(), 3, |_| false);
        // A retranscription times its rows by decoded frames, which every checkpoint's priming
        // and padding push 1792 frames further from the container clock.
        let source = playback_source(&audio, Some(&serde_json::json!({ "retranscribed_at": "2026-10-05T19:28:56Z" }))).unwrap();
        let file_at = |clock_s: f64| {
            let t = &source.time_table;
            let i = t.iter().rposition(|p| p[0] <= clock_s).unwrap();
            if i + 1 == t.len() || t[i + 1][0] == t[i][0] {
                return t[i][1];
            }
            t[i][1] + (clock_s - t[i][0]) * (t[i + 1][1] - t[i][1]) / (t[i + 1][0] - t[i][0])
        };
        for k in 0..3 {
            let decoded_s = (k * CHECKPOINT_FRAMES_48K + AAC_PRIMING_SAMPLES) as f64 / 48_000.0;
            for into in [0.0, 12.5, 29.9] {
                let expected = TimeMap::Checkpoints.clock_s(decoded_s + into);
                assert!((file_at(decoded_s + into) - expected).abs() < 1e-6, "checkpoint {k} at {into} s");
                assert!((expected - (k as f64 * 30.0 + into)).abs() < 1e-6);
            }
        }
        // A live recording's rows already count the container clock.
        assert_eq!(playback_source(&audio, None).unwrap().time_table, vec![[0.0, 0.0], [source.duration_s, source.duration_s]]);
    }

    #[test]
    fn imported_meeting_source_is_identity() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("audio.wav");
        write_wav(&audio, 16_000, 1, &vec![0.0; 32_000]);
        let source = playback_source(&audio, Some(&serde_json::json!({ "source": "import" }))).unwrap();
        assert_eq!(source, PlaybackSource { path: audio.to_string_lossy().into_owned(), duration_s: 2.0, time_table: vec![[0.0, 0.0], [2.0, 2.0]] });
    }

    #[test]
    fn csp_allows_recording_playback() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))).unwrap();
        assert_eq!(conf["app"]["security"]["csp"]["media-src"], "'self' asset: http://asset.localhost blob:");
        // Clips arrive as raw bytes only over the IPC protocol; without it Tauri falls back to
        // postMessage, which delivers them as a JSON array of numbers.
        let connect = conf["app"]["security"]["csp"]["connect-src"].as_str().unwrap();
        assert!(connect.split_whitespace().any(|s| s == "ipc:"), "{connect}");
        assert!(connect.split_whitespace().any(|s| s == "http://ipc.localhost"), "{connect}");
    }
}
