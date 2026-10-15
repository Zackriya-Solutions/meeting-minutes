use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use log::{info, warn, error};
use super::encode::encode_lossless_checkpoint;
use super::recording_state::AudioChunk;
use serde::{Serialize, Deserialize};

use super::ffmpeg::find_ffmpeg_path;

/// metadata.json field that records how audio.mp4 was produced.
pub const AUDIO_LAYOUT_FIELD: &str = "audio_layout";
/// audio.mp4 is a single AAC stream: only the encoder's 1024-sample priming precedes the audio.
pub const AUDIO_LAYOUT_SINGLE_STREAM: &str = "single_stream";

/// Audio data without device type (we only store mixed audio)
#[derive(Clone)]
struct AudioData {
    data: Vec<f32>,
    // sample_rate: u32,
}

/// Checkpoints written by this version: lossless, so the recording is encoded to AAC once.
pub const CHECKPOINT_EXTENSION: &str = "flac";
/// AAC checkpoints left by older versions; still merged when recovering a crashed recording.
pub const LEGACY_CHECKPOINT_EXTENSION: &str = "mp4";

/// Incremental audio saver that writes lossless checkpoints every 30 seconds
/// to minimize memory usage and enable crash recovery
pub struct IncrementalAudioSaver {
    checkpoint_buffer: Vec<AudioData>,
    checkpoint_interval_samples: usize,  // 30s at 48kHz = 1,440,000 samples
    checkpoint_count: u32,
    checkpoints_dir: PathBuf,
    meeting_folder: PathBuf,
    sample_rate: u32,
}

impl IncrementalAudioSaver {
    /// Create a new incremental saver
    ///
    /// # Arguments
    /// * `meeting_folder` - Path to the meeting folder (contains .checkpoints/)
    /// * `sample_rate` - Sample rate of audio (typically 48000)
    pub fn new(meeting_folder: PathBuf, sample_rate: u32) -> Result<Self> {
        let checkpoints_dir = meeting_folder.join(".checkpoints");

        // Verify checkpoints directory exists
        if !checkpoints_dir.exists() {
            return Err(anyhow!("Checkpoints directory does not exist: {}", checkpoints_dir.display()));
        }

        Ok(Self {
            checkpoint_buffer: Vec::new(),
            checkpoint_interval_samples: sample_rate as usize * 30, // 30 seconds
            checkpoint_count: 0,
            checkpoints_dir,
            meeting_folder,
            sample_rate,
        })
    }

    /// Add an audio chunk to the buffer
    /// Automatically saves a checkpoint when buffer reaches 30 seconds
    pub fn add_chunk(&mut self, chunk: AudioChunk) -> Result<()> {
        let audio_data = AudioData {
            data: chunk.data,
            // sample_rate: chunk.sample_rate,
        };

        self.checkpoint_buffer.push(audio_data);

        // Calculate total samples in buffer
        let total_samples: usize = self.checkpoint_buffer
            .iter()
            .map(|c| c.data.len())
            .sum();

        // Save checkpoint when buffer reaches threshold (30 seconds)
        if total_samples >= self.checkpoint_interval_samples {
            self.save_checkpoint()?;
            self.checkpoint_buffer.clear();
        }

        Ok(())
    }

    /// Save current buffer as a checkpoint file
    fn save_checkpoint(&mut self) -> Result<()> {
        // Concatenate all chunks in buffer
        let audio_data: Vec<f32> = self.checkpoint_buffer
            .iter()
            .flat_map(|c| &c.data)
            .cloned()
            .collect();

        if audio_data.is_empty() {
            warn!("Attempted to save empty checkpoint, skipping");
            return Ok(());
        }

        // Generate checkpoint filename
        let checkpoint_path = self.checkpoints_dir
            .join(format!("audio_chunk_{:03}.{}", self.checkpoint_count, CHECKPOINT_EXTENSION));

        // Lossless checkpoint; finalize encodes the whole recording once.
        encode_lossless_checkpoint(
            bytemuck::cast_slice(&audio_data),
            self.sample_rate,
            1,  // mono
            &checkpoint_path
        )?;

        let duration_seconds = audio_data.len() as f32 / self.sample_rate as f32;
        self.checkpoint_count += 1;

        info!("Saved checkpoint {}: {:.2}s of audio ({} samples)",
              self.checkpoint_count,
              duration_seconds,
              audio_data.len());

        Ok(())
    }

    /// Finalize the recording: save final checkpoint, merge all checkpoints, cleanup
    ///
    /// Returns the path to the final merged audio.mp4 file
    pub async fn finalize(&mut self) -> Result<PathBuf> {
        info!("Finalizing incremental recording...");

        // Save final buffer if not empty
        if !self.checkpoint_buffer.is_empty() {
            info!("Saving final checkpoint with remaining {} chunks", self.checkpoint_buffer.len());
            self.save_checkpoint()?;
            self.checkpoint_buffer.clear();
        }

        if self.checkpoint_count == 0 {
            return Err(anyhow!("No audio checkpoints to merge - recording may have failed"));
        }

        // Encode all checkpoints into one AAC stream
        let final_audio_path = self.meeting_folder.join("audio.mp4");
        let files: Vec<PathBuf> = (0..self.checkpoint_count)
            .map(|i| self.checkpoints_dir.join(format!("audio_chunk_{:03}.{}", i, CHECKPOINT_EXTENSION)))
            .collect();
        if let Some(missing) = files.iter().find(|p| !p.exists()) {
            return Err(anyhow!("Checkpoint file missing: {}", missing.display()));
        }
        let list_file = self.checkpoints_dir.join("concat_list.txt");
        let output = final_audio_path.clone();
        let started = std::time::Instant::now();
        // Encoding a long meeting takes a while; keep it off the async runtime threads.
        tokio::task::spawn_blocking(move || merge_checkpoint_files(&files, &list_file, &output))
            .await
            .map_err(|e| anyhow!("Merge task failed: {}", e))??;
        info!("Merged {} checkpoints in {:.1}s", self.checkpoint_count, started.elapsed().as_secs_f64());

        // Clean up checkpoints directory
        info!("Cleaning up {} checkpoint files", self.checkpoint_count);
        if let Err(_) = std::fs::remove_dir_all(&self.checkpoints_dir) {
            warn!("Failed to clean up checkpoints");
            // Non-fatal - user can manually delete
        }

        info!("Finalized recording");

        Ok(final_audio_path)
    }

    /// Get the meeting folder path
    pub fn get_meeting_folder(&self) -> &PathBuf {
        &self.meeting_folder
    }

    /// Get current checkpoint count
    pub fn get_checkpoint_count(&self) -> u32 {
        self.checkpoint_count
    }
}

/// Checkpoint files in `dir`, sorted by name: this version's lossless files, or the legacy AAC
/// files of a recording made by an older version.
pub(crate) fn list_checkpoint_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut lossless = Vec::new();
    let mut legacy = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some(CHECKPOINT_EXTENSION) => lossless.push(path),
            Some(LEGACY_CHECKPOINT_EXTENSION) => legacy.push(path),
            _ => {}
        }
    }
    let mut files = if lossless.is_empty() { legacy } else { lossless };
    files.sort();
    Ok(files)
}

/// Merge checkpoint files into `output` with ffmpeg's concat demuxer. Lossless checkpoints are
/// encoded once to AAC-LC, so the file is continuous with a single encoder priming; legacy AAC
/// checkpoints are joined without re-encoding (each keeps its own priming and padding).
pub(crate) fn merge_checkpoint_files(files: &[PathBuf], list_file: &Path, output: &Path) -> Result<()> {
    if files.is_empty() {
        return Err(anyhow!("No audio checkpoints to merge"));
    }
    let mut list_content = String::new();
    for file in files {
        // Absolute paths (required with -safe 0)
        list_content.push_str(&format!("file '{}'\n", file.canonicalize()?.display()));
    }
    std::fs::write(list_file, list_content)?;

    let ffmpeg_path = find_ffmpeg_path()
        .ok_or_else(|| anyhow!("FFmpeg not found. Please install FFmpeg to finalize recordings."))?;
    let lossless = files
        .iter()
        .all(|f| f.extension().and_then(|e| e.to_str()) == Some(CHECKPOINT_EXTENSION));

    let mut command = std::process::Command::new(ffmpeg_path);
    command.args(["-f", "concat", "-safe", "0", "-i"]).arg(list_file);
    if lossless {
        command.args(["-c:a", "aac", "-b:a", "192k", "-profile:a", "aac_low", "-movflags", "+faststart", "-f", "mp4"]);
    } else {
        command.args(["-c", "copy"]);
    }
    command.arg("-y").arg(output);

    // Hide console window on Windows to prevent CMD popup during finalization
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let ffmpeg_output = command.output()?;
    if !ffmpeg_output.status.success() {
        let stderr = String::from_utf8_lossy(&ffmpeg_output.stderr);
        error!("FFmpeg merge failed");
        return Err(anyhow!("FFmpeg concat failed: {}", stderr));
    }
    if !output.exists() {
        return Err(anyhow!("Merged audio file was not created: {}", output.display()));
    }
    Ok(())
}

/// Record in metadata.json that audio.mp4 is a single AAC stream; other fields are kept.
pub(crate) fn mark_single_stream_layout(folder: &Path) -> Result<()> {
    let path = folder.join("metadata.json");
    let mut value: serde_json::Value = match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e.into()),
    };
    value
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata.json root must be an object"))?
        .insert(AUDIO_LAYOUT_FIELD.to_string(), serde_json::json!(AUDIO_LAYOUT_SINGLE_STREAM));
    let temp = folder.join(format!(".metadata.json.{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temp, serde_json::to_string_pretty(&value)?)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

/// Audio recovery status for transcript recovery feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecoveryStatus {
    pub status: String, // "success" | "partial" | "failed" | "none"
    pub chunk_count: u32,
    pub estimated_duration_seconds: f64,
    pub audio_file_path: Option<String>,
    pub message: String,
}

/// Recover audio from checkpoint files
/// This is called by the transcript recovery system to merge audio chunks after a crash
#[tauri::command]
pub async fn recover_audio_from_checkpoints(
    meeting_folder: String,
    _sample_rate: u32
) -> Result<AudioRecoveryStatus, String> {
    info!("Starting audio recovery");

    let folder_path = PathBuf::from(&meeting_folder);
    let checkpoints_dir = folder_path.join(".checkpoints");

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        info!("No checkpoints directory found");
        return Ok(AudioRecoveryStatus {
            status: "none".to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: None,
            message: "No audio checkpoints found".to_string(),
        });
    }

    // Lossless checkpoints from this version, or AAC checkpoints from an older one
    let checkpoint_files = list_checkpoint_files(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?;

    if checkpoint_files.is_empty() {
        info!("No checkpoint files found");
        return Ok(AudioRecoveryStatus {
            status: "none".to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: None,
            message: "No audio checkpoint files found".to_string(),
        });
    }

    let chunk_count = checkpoint_files.len() as u32;
    let estimated_duration = (chunk_count as f64) * 30.0; // 30 seconds per chunk

    info!("Found {} checkpoint files, estimated duration: {:.2}s", chunk_count, estimated_duration);

    let output_path = folder_path.join("audio.mp4");
    let output_path_str = output_path.to_str()
        .ok_or("Invalid output path")?
        .to_string();

    find_ffmpeg_path()
        .ok_or_else(|| "FFmpeg not found. Please install FFmpeg to recover audio.".to_string())?;
    info!("FFmpeg selected for recovery");

    let lossless = checkpoint_files
        .iter()
        .all(|f| f.extension().and_then(|e| e.to_str()) == Some(CHECKPOINT_EXTENSION));
    let concat_file_path = checkpoints_dir.join("concat_list.txt");
    let (files, list, output) = (checkpoint_files.clone(), concat_file_path.clone(), output_path.clone());
    let merged = tokio::task::spawn_blocking(move || merge_checkpoint_files(&files, &list, &output)).await;

    match merged {
        Ok(Ok(())) => {
            // Clean up concat file
            let _ = std::fs::remove_file(&concat_file_path);
            if lossless {
                if let Err(e) = mark_single_stream_layout(&folder_path) {
                    warn!("Failed to record the audio layout in metadata.json: {}", e);
                }
            }

            info!("Successfully recovered audio from {} checkpoints", chunk_count);

            Ok(AudioRecoveryStatus {
                status: "success".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: Some(output_path_str),
                message: format!("Successfully recovered {} audio chunks", chunk_count),
            })
        }
        Ok(Err(e)) => {
            error!("FFmpeg recovery failed");
            Ok(AudioRecoveryStatus {
                status: "failed".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: None,
                message: format!("FFmpeg failed: {}", e),
            })
        }
        Err(e) => {
            error!("Failed to run FFmpeg");
            Ok(AudioRecoveryStatus {
                status: "failed".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: None,
                message: format!("Failed to run FFmpeg: {}", e),
            })
        }
    }
}

/// Clean up checkpoint files after successful recording or recovery
/// This command is called by the frontend after successful save to clean up checkpoint files
#[tauri::command]
pub async fn cleanup_checkpoints(meeting_folder: String) -> Result<(), String> {
    info!("Cleaning up checkpoints");

    let folder_path = PathBuf::from(&meeting_folder);
    let checkpoints_dir = folder_path.join(".checkpoints");

    if checkpoints_dir.exists() {
        std::fs::remove_dir_all(&checkpoints_dir)
            .map_err(|e| format!("Failed to remove checkpoints directory: {}", e))?;
        info!("Successfully cleaned up checkpoints directory");
    } else {
        info!("No checkpoints directory to clean up");
    }

    Ok(())
}

/// Check if a meeting folder has audio checkpoint files
/// Returns true if .checkpoints/ directory exists and contains checkpoint files
#[tauri::command]
pub async fn has_audio_checkpoints(meeting_folder: String) -> Result<bool, String> {
    let folder_path = PathBuf::from(&meeting_folder);
    let checkpoints_dir = folder_path.join(".checkpoints");

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        return Ok(false);
    }

    // Lossless (current) or AAC (older version) checkpoint files
    let files = list_checkpoint_files(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?;
    Ok(!files.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use super::super::recording_state::DeviceType;

    #[tokio::test]
    async fn test_checkpoint_creation() {
        // Create temp meeting folder
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Test_Meeting");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(
            meeting_folder.clone(),
            48000
        ).unwrap();

        // Add 60 seconds worth of audio (should create 2 checkpoints)
        for i in 0..120 {  // 120 chunks of 0.5s each
            let chunk = AudioChunk {
                data: vec![0.5f32; 24000],  // 0.5s at 48kHz
                sample_rate: 48000,
                timestamp: i as f64 * 0.5,  // timestamp in seconds
                chunk_id: i as u64,
                device_type: DeviceType::Microphone,
            };
            saver.add_chunk(chunk).unwrap();
        }

        // Verify 2 checkpoints created
        assert_eq!(saver.checkpoint_count, 2);

        // Finalize and verify merge
        let final_path = saver.finalize().await.unwrap();
        assert!(final_path.exists());

        // Verify checkpoints directory deleted
        assert!(!meeting_folder.join(".checkpoints").exists());
    }

    #[tokio::test]
    async fn test_empty_recording() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Empty_Test");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(
            meeting_folder.clone(),
            48000
        ).unwrap();

        // Try to finalize without adding any chunks
        let result = saver.finalize().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No audio checkpoints"));
    }

    #[tokio::test]
    async fn finalize_output_is_sample_continuous() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Aligned");
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();
        let mut saver = IncrementalAudioSaver::new(meeting_folder.clone(), 48000).unwrap();

        const TOTAL: usize = 3 * 1_440_000;
        const IMPULSES: [usize; 3] = [0, 1_440_000, 3_600_000];
        let mut samples = vec![0.0f32; TOTAL];
        for &i in &IMPULSES {
            samples[i] = 1.0;
        }
        for (n, chunk) in samples.chunks(28_800).enumerate() {
            saver
                .add_chunk(AudioChunk {
                    data: chunk.to_vec(),
                    sample_rate: 48000,
                    timestamp: n as f64 * 0.6,
                    chunk_id: n as u64,
                    device_type: DeviceType::Microphone,
                })
                .unwrap();
        }
        let path = saver.finalize().await.unwrap();
        let decoded = crate::audio::decoder::decode_audio_file(&path).unwrap().to_whisper_format();

        // One AAC encode: only the encoder's 1024-sample priming (341 samples at 16 kHz) comes first.
        let priming_16k = 1024 / 3;
        let expected_len = TOTAL / 3 + priming_16k;
        assert!(
            (decoded.len() as i64 - expected_len as i64).abs() <= 1024,
            "decoded {} samples, expected about {}",
            decoded.len(),
            expected_len
        );
        for &i in &IMPULSES {
            let expected = i / 3 + priming_16k;
            let (lo, hi) = (expected.saturating_sub(800), (expected + 800).min(decoded.len()));
            let peak = (lo..hi)
                .max_by(|&a, &b| decoded[a].abs().partial_cmp(&decoded[b].abs()).unwrap())
                .unwrap();
            // Within 10 ms at 16 kHz.
            assert!((peak as i64 - expected as i64).abs() <= 160, "impulse at {i}: peak {peak}, expected {expected}");
        }
        assert!(!meeting_folder.join(".checkpoints").exists());
    }

    #[tokio::test]
    async fn recovery_merges_lossless_checkpoints_and_marks_the_layout() {
        let temp_dir = tempdir().unwrap();
        let folder = temp_dir.path().join("Crashed");
        let checkpoints = folder.join(".checkpoints");
        std::fs::create_dir_all(&checkpoints).unwrap();
        std::fs::write(folder.join("metadata.json"), r#"{"status":"recording"}"#).unwrap();
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        for k in 0..2 {
            crate::audio::encode::encode_lossless_checkpoint(
                bytemuck::cast_slice(&tone),
                48000,
                1,
                &checkpoints.join(format!("audio_chunk_{:03}.flac", k)),
            )
            .unwrap();
        }
        let status = recover_audio_from_checkpoints(folder.to_string_lossy().to_string(), 48000).await.unwrap();
        assert_eq!(status.status, "success", "{}", status.message);
        assert!(folder.join("audio.mp4").exists());
        let metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(folder.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata[AUDIO_LAYOUT_FIELD], AUDIO_LAYOUT_SINGLE_STREAM);
        assert_eq!(metadata["status"], "recording");
        assert!(has_audio_checkpoints(folder.to_string_lossy().to_string()).await.unwrap());
    }
}
