use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;

use super::audio_processing::create_meeting_folder;
use super::incremental_saver::IncrementalAudioSaver;
use super::recording_state::AudioChunk;

/// Structured transcript segment for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,         // Segment duration in seconds
    pub display_time: String,  // Formatted time for display like "[02:15]"
    pub confidence: f32,
    pub sequence_id: u64,
    /// Dominant audio channel during live capture: "mic" (local user) or "system"
    /// (remote participants). The post-meeting refinement pass later replaces this
    /// export with diarized per-person speaker names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

/// Cloneable persistence handle used by the transcript event listener. It deliberately does
/// not borrow the global recording manager, so transcript completions remain writable while
/// shutdown temporarily moves that manager out to stop streams and finalize files.
#[derive(Clone)]
pub struct TranscriptSink {
    segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    meeting_folder: Option<PathBuf>,
}

impl TranscriptSink {
    pub fn add(&self, segment: TranscriptSegment) {
        if let Ok(mut segments) = self.segments.lock() {
            if let Some(existing) = segments
                .iter_mut()
                .find(|existing| existing.sequence_id == segment.sequence_id)
            {
                *existing = segment.clone();
            } else {
                segments.push(segment.clone());
            }
            info!(
                "Persisted transcript segment {} (seq: {}) - total segments: {}",
                segment.id,
                segment.sequence_id,
                segments.len()
            );
        } else {
            error!("Failed to lock transcript segments for {}", segment.id);
            return;
        }

        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = write_transcripts_json_from_segments(folder, &self.segments) {
                warn!("Failed to write incremental transcript update: {e}");
            }
        }
    }
}

/// Meeting metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub version: String,
    pub meeting_id: Option<String>,
    pub meeting_name: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub duration_seconds: Option<f64>,
    pub devices: DeviceInfo,
    pub audio_file: String,
    pub transcript_file: String,
    pub sample_rate: u32,
    pub status: String, // "recording", "completed", "error"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub microphone: Option<String>,
    pub system_audio: Option<String>,
}

/// New recording saver using incremental saving strategy
pub struct RecordingSaver {
    incremental_saver: Option<IncrementalAudioSaver>,
    meeting_folder: Option<PathBuf>,
    meeting_name: Option<String>,
    metadata: Option<MeetingMetadata>,
    transcript_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
    chunk_receiver: Option<mpsc::Receiver<AudioChunk>>,
    accumulation_task: Option<tokio::task::JoinHandle<Option<IncrementalAudioSaver>>>,
    checkpoint_count: Arc<AtomicU32>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            incremental_saver: None,
            meeting_folder: None,
            meeting_name: None,
            metadata: None,
            transcript_segments: Arc::new(Mutex::new(Vec::new())),
            chunk_receiver: None,
            accumulation_task: None,
            checkpoint_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    /// Set device information in metadata
    pub fn set_device_info(&mut self, mic_name: Option<String>, sys_name: Option<String>) {
        if let Some(ref mut metadata) = self.metadata {
            metadata.devices.microphone = mic_name;
            metadata.devices.system_audio = sys_name;

            // Write updated metadata to disk if folder exists
            if let Some(folder) = &self.meeting_folder {
                let metadata_clone = metadata.clone();
                if let Err(e) = self.write_metadata(folder, &metadata_clone) {
                    warn!("Failed to update metadata with device info: {}", e);
                }
            }
        }
    }

    /// Add or update a structured transcript segment (upserts based on sequence_id)
    /// Also saves incrementally to disk
    pub fn add_transcript_segment(&self, segment: TranscriptSegment) {
        self.transcript_sink().add(segment);
    }

    pub fn transcript_sink(&self) -> TranscriptSink {
        TranscriptSink {
            segments: Arc::clone(&self.transcript_segments),
            meeting_folder: self.meeting_folder.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn take_accumulation_task_for_test(
        &mut self,
    ) -> Option<tokio::task::JoinHandle<Option<IncrementalAudioSaver>>> {
        self.accumulation_task.take()
    }

    /// Legacy method for backward compatibility - converts text to basic segment
    pub fn add_transcript_chunk(&self, text: String) {
        let segment = TranscriptSegment {
            id: format!("seg_{}", chrono::Utc::now().timestamp_millis()),
            text,
            audio_start_time: 0.0,
            audio_end_time: 0.0,
            duration: 0.0,
            display_time: "[00:00]".to_string(),
            confidence: 1.0,
            sequence_id: 0,
            speaker: None,
        };
        self.add_transcript_segment(segment);
    }

    /// Start accumulation with optional incremental saving
    ///
    /// # Arguments
    /// * `auto_save` - If true, creates checkpoints and enables saving. If false, audio chunks are discarded.
    pub fn start_accumulation(&mut self, auto_save: bool) -> mpsc::Sender<AudioChunk> {
        if auto_save {
            info!("Initializing incremental audio saver for recording (auto-save ENABLED)");
        } else {
            info!(
                "Starting recording without audio saving (auto-save DISABLED - transcripts only)"
            );
        }

        // Create channel for receiving audio chunks
        // Enough for more than a minute of 600ms mixed windows while FFmpeg writes a
        // checkpoint, but bounded so a wedged encoder cannot exhaust process memory.
        let (sender, receiver) = mpsc::channel::<AudioChunk>(128);
        self.chunk_receiver = Some(receiver);

        // Initialize meeting folder and incremental saver ONLY if auto_save is enabled
        if auto_save {
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, true) {
                    Ok(()) => info!("Successfully initialized meeting folder with checkpoints"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                        // Continue anyway - will use fallback flat structure
                    }
                }
            }
        } else {
            // When auto_save is false, still create meeting folder for transcripts/metadata
            // but skip .checkpoints directory
            if let Some(name) = self.meeting_name.clone() {
                match self.initialize_meeting_folder(&name, false) {
                    Ok(()) => info!("Successfully initialized meeting folder (transcripts only)"),
                    Err(e) => {
                        error!("Failed to initialize meeting folder: {}", e);
                    }
                }
            }
        }

        // Start accumulation task
        let incremental_saver = self.incremental_saver.take();
        let checkpoint_count = Arc::clone(&self.checkpoint_count);
        let save_audio = auto_save;

        if let Some(mut receiver) = self.chunk_receiver.take() {
            // Checkpoint encoding launches FFmpeg and waits synchronously. Keep the entire
            // accumulation loop on the blocking pool so a 30-second checkpoint cannot stall
            // audio/VAD/transcript tasks on Tokio's async workers.
            self.accumulation_task = Some(tokio::task::spawn_blocking(move || {
                drain_audio_chunks(
                    &mut receiver,
                    incremental_saver,
                    save_audio,
                    checkpoint_count,
                )
            }));
        }

        sender
    }

    /// Initialize meeting folder structure and metadata
    ///
    /// # Arguments
    /// * `meeting_name` - Name of the meeting
    /// * `create_checkpoints` - Whether to create .checkpoints/ directory and IncrementalAudioSaver
    fn initialize_meeting_folder(
        &mut self,
        meeting_name: &str,
        create_checkpoints: bool,
    ) -> Result<()> {
        // Load preferences to get base recordings folder
        let base_folder = super::recording_preferences::get_default_recordings_folder();

        // Create meeting folder structure (with or without .checkpoints/ subdirectory)
        let meeting_folder = create_meeting_folder(&base_folder, meeting_name, create_checkpoints)?;

        // Only initialize incremental saver if checkpoints are needed (auto_save is true)
        if create_checkpoints {
            let incremental_saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000)?;
            self.checkpoint_count.store(0, Ordering::Relaxed);
            self.incremental_saver = Some(incremental_saver);
            info!(
                "✅ Incremental audio saver initialized for meeting: {}",
                meeting_name
            );
        } else {
            info!("⚠️  Skipped incremental audio saver (auto-save disabled)");
        }

        // Create initial metadata
        let metadata = MeetingMetadata {
            version: "1.0".to_string(),
            meeting_id: None, // Will be set by backend
            meeting_name: Some(meeting_name.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            duration_seconds: None,
            devices: DeviceInfo {
                microphone: None, // Could be enhanced to store actual device names
                system_audio: None,
            },
            audio_file: if create_checkpoints {
                "audio.mp4".to_string()
            } else {
                "".to_string()
            },
            transcript_file: "transcripts.json".to_string(),
            sample_rate: 48000,
            status: "recording".to_string(),
        };

        // Write initial metadata.json
        self.write_metadata(&meeting_folder, &metadata)?;

        self.meeting_folder = Some(meeting_folder);
        self.metadata = Some(metadata);

        Ok(())
    }

    /// Write metadata.json to disk (atomic write with temp file)
    fn write_metadata(&self, folder: &PathBuf, metadata: &MeetingMetadata) -> Result<()> {
        let metadata_path = folder.join("metadata.json");
        let temp_path = folder.join(".metadata.json.tmp");

        let json_string = serde_json::to_string_pretty(metadata)?;
        std::fs::write(&temp_path, json_string)?;
        std::fs::rename(&temp_path, &metadata_path)?; // Atomic

        Ok(())
    }

    /// Write transcripts.json to disk (atomic write with temp file and validation)
    fn write_transcripts_json(&self, folder: &PathBuf) -> Result<()> {
        write_transcripts_json_from_segments(folder, &self.transcript_segments)
    }

    // in frontend/src-tauri/src/audio/recording_saver.rs
    pub fn get_stats(&self) -> (usize, u32) {
        (
            self.checkpoint_count.load(Ordering::Relaxed) as usize,
            48000,
        )
    }

    /// Stop and save using incremental saving approach
    ///
    /// # Arguments
    /// * `app` - Tauri app handle for emitting events
    /// * `recording_duration` - Actual recording duration in seconds (from RecordingState)
    pub async fn stop_and_save<R: Runtime>(
        &mut self,
        app: &AppHandle<R>,
        recording_duration: Option<f64>,
    ) -> Result<Option<String>, String> {
        info!("Stopping recording saver");

        // The audio pipeline has already stopped and dropped its sender. Wait until the
        // accumulator observes channel closure and drains every queued mixed chunk before
        // finalizing checkpoints. The former boolean + 200ms sleep could discard the tail,
        // or even let the receiver exit immediately due to a startup race.
        if let Some(task) = self.accumulation_task.take() {
            match task.await {
                Ok(saver) => self.incremental_saver = saver,
                Err(e) => {
                    return Err(format!("Recording accumulation task failed: {e}"));
                }
            }
        }

        // Check if incremental saver exists (indicates auto_save was enabled)
        let should_save_audio = self.incremental_saver.is_some();

        if !should_save_audio {
            info!("⚠️  No audio saver initialized (auto-save was disabled) - skipping audio finalization");
            info!("✅ Transcripts and metadata already saved incrementally");
            return Ok(None);
        }

        // Finalize incremental saver (merge checkpoints into final audio.mp4)
        let final_audio_path = if let Some(mut saver) = self.incremental_saver.take() {
            match saver.finalize().await {
                Ok(path) => {
                    info!("✅ Successfully finalized audio: {}", path.display());
                    path
                }
                Err(e) => {
                    error!("❌ Failed to finalize incremental saver: {}", e);
                    return Err(format!("Failed to finalize audio: {}", e));
                }
            }
        } else {
            error!("No incremental saver initialized - cannot save recording");
            return Err("No incremental saver initialized".to_string());
        };

        // Save final transcripts.json with validation
        if let Some(folder) = &self.meeting_folder {
            if let Err(e) = self.write_transcripts_json(folder) {
                error!("❌ Failed to write final transcripts: {}", e);
                return Err(format!("Failed to save transcripts: {}", e));
            }

            // Verify transcripts were written correctly
            let transcript_path = folder.join("transcripts.json");
            if !transcript_path.exists() {
                error!(
                    "❌ Transcript file was not created at: {}",
                    transcript_path.display()
                );
                return Err("Transcript file verification failed".to_string());
            }
            info!(
                "✅ Transcripts saved and verified at: {}",
                transcript_path.display()
            );
        }

        // Update metadata to completed status with actual recording duration
        if let (Some(folder), Some(mut metadata)) = (&self.meeting_folder, self.metadata.clone()) {
            metadata.status = "completed".to_string();
            metadata.completed_at = Some(chrono::Utc::now().to_rfc3339());

            // Use actual recording duration from RecordingState (more accurate than transcript segments)
            // Falls back to last transcript segment if duration not provided
            metadata.duration_seconds = recording_duration.or_else(|| {
                if let Ok(segments) = self.transcript_segments.lock() {
                    segments.last().map(|seg| seg.audio_end_time)
                } else {
                    None
                }
            });

            if let Err(e) = self.write_metadata(folder, &metadata) {
                error!("❌ Failed to update metadata to completed: {}", e);
                return Err(format!("Failed to update metadata: {}", e));
            }

            info!(
                "✅ Metadata updated with duration: {:?}s",
                metadata.duration_seconds
            );
        }

        // Emit save event with audio and transcript paths
        let save_event = serde_json::json!({
            "audio_file": final_audio_path.to_string_lossy(),
            "transcript_file": self.meeting_folder.as_ref()
                .map(|f| f.join("transcripts.json").to_string_lossy().to_string()),
            "meeting_name": self.meeting_name,
            "meeting_folder": self.meeting_folder.as_ref()
                .map(|f| f.to_string_lossy().to_string())
        });

        if let Err(e) = app.emit("recording-saved", &save_event) {
            warn!("Failed to emit recording-saved event: {}", e);
        }

        // Clean up transcript segments
        if let Ok(mut segments) = self.transcript_segments.lock() {
            segments.clear();
        }

        Ok(Some(final_audio_path.to_string_lossy().to_string()))
    }

    /// Get the meeting folder path (for passing to backend)
    pub fn get_meeting_folder(&self) -> Option<&PathBuf> {
        self.meeting_folder.as_ref()
    }

    /// Get accumulated transcript segments (for reload sync)
    pub fn get_transcript_segments(&self) -> Vec<TranscriptSegment> {
        if let Ok(segments) = self.transcript_segments.lock() {
            segments.clone()
        } else {
            Vec::new()
        }
    }

    /// Get meeting name (for reload sync)
    pub fn get_meeting_name(&self) -> Option<String> {
        self.meeting_name.clone()
    }
}

fn drain_audio_chunks(
    receiver: &mut mpsc::Receiver<AudioChunk>,
    mut incremental_saver: Option<IncrementalAudioSaver>,
    save_audio: bool,
    checkpoint_count: Arc<AtomicU32>,
) -> Option<IncrementalAudioSaver> {
    info!(
        "Recording saver accumulation task started (save_audio: {})",
        save_audio
    );

    // Drain through channel closure. The pipeline owns the sender and drops it only after
    // all mixed audio has been forwarded, which gives shutdown a real completion signal.
    while let Some(chunk) = receiver.blocking_recv() {
        if !save_audio {
            continue;
        }

        let Some(saver) = incremental_saver.as_mut() else {
            error!("Incremental saver not available while accumulating");
            continue;
        };
        if let Err(e) = saver.add_chunk(chunk) {
            error!("Failed to add chunk to incremental saver: {e}");
        }
        checkpoint_count.store(saver.get_checkpoint_count(), Ordering::Relaxed);
    }

    info!("Recording saver accumulation task ended");
    incremental_saver
}

fn write_transcripts_json_from_segments(
    folder: &PathBuf,
    segments: &Arc<Mutex<Vec<TranscriptSegment>>>,
) -> Result<()> {
    // Clone segments to avoid holding the mutex during serialization and file I/O.
    let segments_clone = segments
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock transcript segments"))?
        .clone();

    let transcript_path = folder.join("transcripts.json");
    let temp_path = folder.join(".transcripts.json.tmp");
    let json = serde_json::json!({
        "version": "1.0",
        "segments": segments_clone,
        "last_updated": chrono::Utc::now().to_rfc3339(),
        "total_segments": segments_clone.len()
    });
    let json_string = serde_json::to_string_pretty(&json)
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))?;

    std::fs::write(&temp_path, json_string)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", temp_path.display()))?;
    std::fs::rename(&temp_path, &transcript_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to replace transcript file {}: {e}",
            transcript_path.display()
        )
    })?;

    Ok(())
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(sequence_id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("seg_{sequence_id}"),
            text: text.to_string(),
            audio_start_time: sequence_id as f64,
            audio_end_time: sequence_id as f64 + 1.0,
            duration: 1.0,
            display_time: "00:00:00".to_string(),
            confidence: 0.9,
            sequence_id,
            speaker: Some("mic".to_string()),
        }
    }

    #[test]
    fn transcript_sink_persists_and_upserts_without_recording_manager() {
        let temp = tempfile::tempdir().unwrap();
        let segments = Arc::new(Mutex::new(Vec::new()));
        let sink = TranscriptSink {
            segments: Arc::clone(&segments),
            meeting_folder: Some(temp.path().to_path_buf()),
        };

        sink.add(segment(7, "first"));
        sink.add(segment(7, "corrected"));
        sink.add(segment(8, "next"));

        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(temp.path().join("transcripts.json")).unwrap())
                .unwrap();
        assert_eq!(persisted["total_segments"], 2);
        assert_eq!(persisted["segments"][0]["text"], "corrected");
        assert_eq!(persisted["segments"][1]["text"], "next");
        assert_eq!(segments.lock().unwrap().len(), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accumulation_is_safe_on_a_current_thread_runtime() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".checkpoints")).unwrap();
        let saver = IncrementalAudioSaver::new(temp.path().to_path_buf(), 48_000).unwrap();
        let checkpoint_count = Arc::new(AtomicU32::new(0));
        let task_checkpoint_count = Arc::clone(&checkpoint_count);
        let (sender, mut receiver) = mpsc::channel(2);

        let task = tokio::task::spawn_blocking(move || {
            drain_audio_chunks(&mut receiver, Some(saver), true, task_checkpoint_count)
        });
        sender
            .send(AudioChunk {
                data: vec![0.25; 4_800],
                sample_rate: 48_000,
                timestamp: 0.0,
                chunk_id: 1,
                device_type: super::super::recording_state::DeviceType::Microphone,
                speaker: None,
            })
            .await
            .unwrap();
        drop(sender);

        let returned_saver = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("accumulator must not deadlock")
            .expect("accumulator must not panic")
            .expect("accumulator must return saver ownership");
        assert_eq!(returned_saver.get_checkpoint_count(), 0);
        assert_eq!(checkpoint_count.load(Ordering::Relaxed), 0);
    }
}
