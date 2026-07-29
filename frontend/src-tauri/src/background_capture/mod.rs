//! Background auto-recording — the meeting-detector companion's behaviour, hosted
//! inside the main app.
//!
//! When a recognized meeting client takes over the microphone, this captures the
//! call silently: mic + system audio are mixed to `audio.mp4` in a new meeting
//! folder, and when the call signal ends the meeting is registered in the library
//! with audio but no transcript. The user transcribes it later with
//! Enhance/Retranscribe.
//!
//! How it differs from `meeting_detection`'s auto-listening, which shares the same
//! detector:
//!
//! | | auto-listening | background auto-recording |
//! |---|---|---|
//! | recording path | the interactive session (live transcription) | private capture, audio only |
//! | UI | navigates to the recorder, shows transcripts | nothing; the app is untouched |
//! | needs a loaded model | yes | no |
//! | clients | native only (browser/Telegram confirm first) | every recognized client |
//!
//! Auto-listening wins when both are enabled, so a call is never captured twice.
//! Short captures (a voice message that briefly grabbed the mic) are discarded
//! instead of registered.

pub mod recorder;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tokio::sync::Mutex as AsyncMutex;

use crate::audio::audio_processing::create_meeting_folder;
use crate::audio::recording_preferences::get_default_recordings_folder;
use crate::audio::recording_saver::{DeviceInfo, MeetingMetadata};
use crate::notifications::commands::NotificationManagerState;
use crate::state::AppState;
use recorder::{BackgroundRecorder, BackgroundRecording};

/// Captures shorter than this are treated as false positives (a voice message, a
/// device test) and discarded rather than added to the library.
const MIN_MEETING_SECONDS: f64 = 60.0;

/// A capture currently in progress.
struct ActiveCapture {
    recorder: BackgroundRecorder,
    folder: PathBuf,
    title: String,
    meeting_id: String,
    started: DateTime<Utc>,
}

/// Owns the in-progress capture, if any. Registered as Tauri managed state.
#[derive(Default)]
pub struct BackgroundCaptureState {
    active: AsyncMutex<Option<ActiveCapture>>,
    /// Mirrors `active.is_some()` for cheap synchronous reads from the detector
    /// loop and from `RunEvent::Exit`.
    is_active: Arc<AtomicBool>,
}

#[derive(Debug, Serialize)]
pub struct BackgroundCaptureStatus {
    pub enabled: bool,
    pub supported: bool,
    pub capturing: bool,
    pub minimum_capture_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundCaptureSavedEvent {
    pub meeting_id: String,
    pub title: String,
    pub duration_seconds: f64,
    pub registered: bool,
}

impl BackgroundCaptureState {
    pub fn is_capturing(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    /// Begin capturing a detected call. `label` names the detected client(s) and is
    /// only used to title the meeting.
    pub async fn start(&self, app: &AppHandle<Wry>, label: &str) -> Result<(), String> {
        let mut active = self.active.lock().await;
        if active.is_some() {
            return Err("a background capture is already in progress".to_string());
        }

        let started = Utc::now();
        let title = auto_meeting_title(label);
        let meeting_id = format!("meeting-{}", uuid::Uuid::new_v4());

        // `true` also creates `.checkpoints/`, which the incremental saver needs.
        let folder = create_meeting_folder(&get_default_recordings_folder(), &title, true)
            .map_err(|error| format!("could not create the meeting folder: {error}"))?;

        // Write both marker files up front: a crash leaves a folder that still looks
        // like a Memento meeting, and the discard path can delete it safely.
        write_metadata(&folder, &meeting_id, &title, started, "recording", None, None)?;
        write_empty_transcripts(&folder, started)?;

        let recorder = match BackgroundRecorder::start(folder.clone()).await {
            Ok(recorder) => recorder,
            Err(error) => {
                discard_folder(&folder);
                return Err(format!("could not start background capture: {error}"));
            }
        };

        *active = Some(ActiveCapture {
            recorder,
            folder,
            title: title.clone(),
            meeting_id,
            started,
        });
        self.is_active.store(true, Ordering::SeqCst);
        drop(active);

        let _ = app.emit("background-capture-started", &title);
        notify_recording_started(app, &title).await;
        log::info!("Background capture started for “{title}”");
        Ok(())
    }

    /// Stop the capture, then register it unless it was too short to be a meeting.
    pub async fn stop_and_finalize(&self, app: &AppHandle<Wry>) {
        let Some(capture) = self.active.lock().await.take() else {
            return;
        };
        self.is_active.store(false, Ordering::SeqCst);

        let ActiveCapture {
            recorder,
            folder,
            title,
            meeting_id,
            started,
        } = capture;

        let recording = match recorder.stop().await {
            Ok(recording) => recording,
            Err(error) => {
                log::error!("Background capture “{title}” could not be finalized: {error:#}");
                // Merging can fail with the captured checkpoints still on disk. Keep
                // the folder in that case — the audio is recoverable and deleting it
                // would be worse than leaving a folder behind.
                if has_recoverable_audio(&folder) {
                    log::warn!(
                        "Keeping the unfinalized capture at {} — its audio can still be recovered",
                        folder.display()
                    );
                } else {
                    discard_folder(&folder);
                }
                let _ = app.emit("background-capture-discarded", &title);
                return;
            }
        };

        if recording.duration_secs < MIN_MEETING_SECONDS {
            log::info!(
                "Discarding short background capture “{title}” ({:.1}s < {:.0}s)",
                recording.duration_secs,
                MIN_MEETING_SECONDS
            );
            discard_folder(&folder);
            let _ = app.emit("background-capture-discarded", &title);
            return;
        }

        if let Err(error) = write_metadata(
            &folder,
            &meeting_id,
            &title,
            started,
            "completed",
            Some(recording.duration_secs),
            Some(&recording),
        ) {
            log::warn!("Could not update metadata for “{title}”: {error}");
        }

        let registered = match register_meeting(app, &meeting_id, &title, &folder).await {
            Ok(()) => true,
            Err(error) => {
                // The folder is still a valid Memento recording, so the user can
                // import it manually. Never delete audio because a write failed.
                log::error!(
                    "Background capture “{title}” was saved to disk but not registered: {error}"
                );
                false
            }
        };

        let _ = app.emit(
            "background-capture-saved",
            BackgroundCaptureSavedEvent {
                meeting_id,
                title: title.clone(),
                duration_seconds: recording.duration_secs,
                registered,
            },
        );
        notify_recording_stopped(app).await;
        log::info!(
            "Background capture “{title}” finished ({:.0}s, registered: {registered})",
            recording.duration_secs
        );
    }
}

/// Privacy-safe status for Settings: no process names, no capture history.
#[tauri::command]
pub async fn get_background_capture_status(
    capture: State<'_, BackgroundCaptureState>,
    notifications: State<'_, NotificationManagerState<Wry>>,
) -> Result<BackgroundCaptureStatus, String> {
    let enabled = match notifications.read().await.as_ref() {
        Some(manager) => manager.get_settings().await.background_auto_recording,
        None => false,
    };
    Ok(BackgroundCaptureStatus {
        enabled,
        // Detection depends on the OS microphone-session signal.
        supported: cfg!(target_os = "macos"),
        capturing: capture.is_capturing(),
        minimum_capture_seconds: MIN_MEETING_SECONDS,
    })
}

/// Title a captured call, following the interactive recorder's `Auto meeting
/// DD_MM_YY_HH_MM_SS` convention (see `useRecordingStart`) and naming the detected
/// client so the meeting is recognizable in the library.
fn auto_meeting_title(label: &str) -> String {
    let stamp = chrono::Local::now().format("%d_%m_%y_%H_%M_%S");
    if label.trim().is_empty() {
        format!("Auto meeting {stamp}")
    } else {
        format!("Auto meeting {label} {stamp}")
    }
}

/// Insert the meeting so it appears in the library with audio and no transcript.
async fn register_meeting(
    app: &AppHandle<Wry>,
    meeting_id: &str,
    title: &str,
    folder: &Path,
) -> Result<(), String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "application database state is unavailable".to_string())?;
    let pool = state.db_manager.pool();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(meeting_id)
    .bind(title)
    .bind(now)
    .bind(now)
    .bind(folder.to_string_lossy().to_string())
    .execute(pool)
    .await
    .map_err(|error| format!("could not insert the meeting row: {error}"))?;

    // Automatically captured meetings enter the reviewable Inbox first. A
    // classifier failure must never make an otherwise-saved meeting fail.
    if let Err(error) = crate::learning::classification::prepare_saved_meeting(pool, meeting_id).await
    {
        log::warn!("Could not prepare background capture {meeting_id} for classification: {error}");
    }
    Ok(())
}

fn write_metadata(
    folder: &Path,
    meeting_id: &str,
    title: &str,
    started: DateTime<Utc>,
    status: &str,
    duration_secs: Option<f64>,
    recording: Option<&BackgroundRecording>,
) -> Result<(), String> {
    let metadata = MeetingMetadata {
        version: "1.0".to_string(),
        meeting_id: Some(meeting_id.to_string()),
        meeting_name: Some(title.to_string()),
        created_at: started.to_rfc3339(),
        completed_at: (status == "completed").then(|| Utc::now().to_rfc3339()),
        duration_seconds: duration_secs,
        devices: DeviceInfo {
            microphone: recording.and_then(|recording| recording.microphone.clone()),
            system_audio: recording.and_then(|recording| recording.system_audio.clone()),
        },
        audio_file: "audio.mp4".to_string(),
        transcript_file: "transcripts.json".to_string(),
        sample_rate: 48_000,
        status: status.to_string(),
    };
    write_json_atomically(folder, "metadata.json", &metadata)
}

fn write_empty_transcripts(folder: &Path, started: DateTime<Utc>) -> Result<(), String> {
    let transcripts = serde_json::json!({
        "version": "1.0",
        "segments": [],
        "last_updated": started.to_rfc3339(),
        "total_segments": 0,
    });
    write_json_atomically(folder, "transcripts.json", &transcripts)
}

fn write_json_atomically<T: Serialize>(
    folder: &Path,
    file_name: &str,
    value: &T,
) -> Result<(), String> {
    let path = folder.join(file_name);
    let temp_path = folder.join(format!(".{file_name}.tmp"));
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("could not serialize {file_name}: {error}"))?;
    std::fs::write(&temp_path, json)
        .map_err(|error| format!("could not write {file_name}: {error}"))?;
    std::fs::rename(&temp_path, &path)
        .map_err(|error| format!("could not replace {file_name}: {error}"))?;
    Ok(())
}

/// Whether the folder still holds audio worth keeping: a merged `audio.mp4`, or any
/// unmerged checkpoint (the app can rebuild a recording from those).
fn has_recoverable_audio(folder: &Path) -> bool {
    if folder.join("audio.mp4").is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(folder.join(".checkpoints")) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
    })
}

fn discard_folder(folder: &Path) {
    if let Err(error) = crate::api::api::delete_recording_folder(folder) {
        log::warn!(
            "Could not remove the discarded capture folder {}: {error}",
            folder.display()
        );
    }
}

async fn notify_recording_started(app: &AppHandle<Wry>, title: &str) {
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    if let Some(manager) = manager.as_ref() {
        let _ = manager.show_recording_started(Some(title.to_string())).await;
    }
}

async fn notify_recording_stopped(app: &AppHandle<Wry>) {
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    if let Some(manager) = manager.as_ref() {
        let _ = manager.show_recording_stopped().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_follows_the_recorder_convention_and_names_the_client() {
        let title = auto_meeting_title("Zoom");
        assert!(title.starts_with("Auto meeting Zoom "), "got {title}");
        // Trailing DD_MM_YY_HH_MM_SS stamp, matching `useRecordingStart`.
        let stamp = title.rsplit(' ').next().unwrap();
        assert_eq!(stamp.split('_').count(), 6, "got {title}");
        assert!(stamp.split('_').all(|part| part.len() == 2), "got {title}");
    }

    #[test]
    fn title_stays_valid_without_a_recognized_client() {
        let title = auto_meeting_title("   ");
        assert!(title.starts_with("Auto meeting "), "got {title}");
        assert!(!title.contains("  "), "got {title}");
    }

    #[test]
    fn recoverable_audio_covers_merged_output_and_unmerged_checkpoints() {
        let root = std::env::temp_dir().join(format!("memento-bg-{}", uuid::Uuid::new_v4()));
        let checkpoints = root.join(".checkpoints");
        std::fs::create_dir_all(&checkpoints).unwrap();
        assert!(!has_recoverable_audio(&root));

        std::fs::write(checkpoints.join("audio_chunk_000.mp4"), b"x").unwrap();
        assert!(has_recoverable_audio(&root));

        std::fs::remove_dir_all(&checkpoints).unwrap();
        assert!(!has_recoverable_audio(&root));

        std::fs::write(root.join("audio.mp4"), b"x").unwrap();
        assert!(has_recoverable_audio(&root));

        std::fs::remove_dir_all(&root).unwrap();
    }
}
