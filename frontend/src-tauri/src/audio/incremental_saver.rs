use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use log::{info, warn, error};
use super::encode::encode_single_audio;
use super::recording_state::AudioChunk;
use serde::{Serialize, Deserialize};

use super::ffmpeg::find_ffmpeg_path;

/// Audio data without device type (we only store mixed audio)
#[derive(Clone)]
struct AudioData {
    data: Vec<f32>,
    // sample_rate: u32,
}

/// Incremental audio saver that writes checkpoints every 30 seconds
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
            .join(format!("audio_chunk_{:03}.mp4", self.checkpoint_count));

        // Encode and save checkpoint
        encode_single_audio(
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

        // Merge all checkpoints using FFmpeg concat
        let final_audio_path = self.meeting_folder.join("audio.mp4");
        self.merge_checkpoints(&final_audio_path).await?;

        // Clean up checkpoints directory
        info!("Cleaning up {} checkpoint files", self.checkpoint_count);
        if let Err(e) = std::fs::remove_dir_all(&self.checkpoints_dir) {
            warn!("Failed to clean up checkpoints directory: {}", e);
            // Non-fatal - user can manually delete
        }

        info!("Finalized recording: {}", final_audio_path.display());

        Ok(final_audio_path)
    }

    /// Merge all checkpoint files into final audio.mp4 using FFmpeg concat
    /// Uses concat demuxer for fast merging without re-encoding
    async fn merge_checkpoints(&self, output: &PathBuf) -> Result<()> {
        info!("Merging {} checkpoints into final audio file...", self.checkpoint_count);

        // Create concat list file for FFmpeg
        let list_file = self.checkpoints_dir.join("concat_list.txt");
        let mut list_content = String::new();

        for i in 0..self.checkpoint_count {
            let checkpoint_path = self.checkpoints_dir
                .join(format!("audio_chunk_{:03}.mp4", i));

            // Verify checkpoint exists
            if !checkpoint_path.exists() {
                return Err(anyhow!("Checkpoint file missing: {}", checkpoint_path.display()));
            }

            // Use absolute path for FFmpeg (required for safe mode)
            let abs_path = checkpoint_path.canonicalize()?;
            list_content.push_str(&format!("file '{}'\n", abs_path.display()));
        }

        std::fs::write(&list_file, list_content)?;

        let ffmpeg_path = find_ffmpeg_path()
            .ok_or_else(|| anyhow!("FFmpeg not found. Please install FFmpeg to finalize recordings."))?;
        info!("Using FFmpeg at: {:?}", ffmpeg_path);

        // Run FFmpeg concat command
        // Using concat demuxer with copy codec for fast merging (no re-encoding)
        
        let mut command = std::process::Command::new(ffmpeg_path);
        
        command.args(&[
            "-f", "concat",          // Use concat demuxer
            "-safe", "0",            // Allow absolute paths
            "-i", list_file.to_str().unwrap(),
            "-c", "copy",            // Copy codec - no re-encoding!
            "-y",                    // Overwrite output file
            output.to_str().unwrap()
        ]);

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
            error!("FFmpeg merge failed: {}", stderr);
            return Err(anyhow!("FFmpeg concat failed: {}", stderr));
        }

        // Verify output file was created
        if !output.exists() {
            return Err(anyhow!("Merged audio file was not created: {}", output.display()));
        }

        info!("Successfully merged {} checkpoints → {}",
              self.checkpoint_count, output.display());

        Ok(())
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

/// Audio recovery status for transcript recovery feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecoveryStatus {
    pub status: String, // "success" | "partial" | "failed" | "none"
    pub chunk_count: u32,
    pub estimated_duration_seconds: f64,
    pub audio_file_path: Option<String>,
    pub message: String,
}

fn existing_recording_path(meeting_folder: &Path) -> Option<PathBuf> {
    let audio_path = meeting_folder.join("audio.mp4");
    let metadata = std::fs::metadata(&audio_path).ok()?;

    if metadata.is_file() && metadata.len() > 0 {
        Some(audio_path)
    } else {
        None
    }
}

fn has_checkpoint_files(meeting_folder: &Path) -> Result<bool, String> {
    let checkpoints_dir = meeting_folder.join(".checkpoints");
    if !checkpoints_dir.exists() {
        return Ok(false);
    }

    std::fs::read_dir(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))
        .map(|entries| {
            entries.filter_map(|entry| entry.ok()).any(|entry| {
                entry.path().extension().and_then(|extension| extension.to_str()) == Some("mp4")
            })
        })
}

/// Recover audio from checkpoint files
/// This is called by the transcript recovery system to merge audio chunks after a crash
#[tauri::command]
pub async fn recover_audio_from_checkpoints(
    meeting_folder: String,
    _sample_rate: u32
) -> Result<AudioRecoveryStatus, String> {
    info!("Starting audio recovery for folder: {}", meeting_folder);

    let folder_path = PathBuf::from(&meeting_folder);

    // A clean recording stop merges checkpoints into audio.mp4 and removes the
    // checkpoint directory. IndexedDB recovery metadata can still remain when
    // the app is interrupted immediately afterwards, so the finalized file is
    // itself a valid (and preferred) recovery source.
    if let Some(audio_path) = existing_recording_path(&folder_path) {
        let audio_path = audio_path.to_string_lossy().to_string();
        info!("Using existing finalized recording: {}", audio_path);
        return Ok(AudioRecoveryStatus {
            status: "success".to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: Some(audio_path),
            message: "Existing finalized recording found".to_string(),
        });
    }

    let checkpoints_dir = folder_path.join(".checkpoints");

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        info!("No checkpoints directory found at: {}", checkpoints_dir.display());
        return Ok(AudioRecoveryStatus {
            status: "none".to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: None,
            message: "No audio checkpoints found".to_string(),
        });
    }

    // Scan for checkpoint files
    let mut checkpoint_files: Vec<_> = std::fs::read_dir(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().and_then(|s| s.to_str()) == Some("mp4")
        })
        .collect();

    if checkpoint_files.is_empty() {
        info!("No checkpoint files found in: {}", checkpoints_dir.display());
        return Ok(AudioRecoveryStatus {
            status: "none".to_string(),
            chunk_count: 0,
            estimated_duration_seconds: 0.0,
            audio_file_path: None,
            message: "No audio checkpoint files found".to_string(),
        });
    }

    // Sort by filename (audio_chunk_000.mp4, audio_chunk_001.mp4, etc.)
    checkpoint_files.sort_by_key(|entry| entry.path());

    let chunk_count = checkpoint_files.len() as u32;
    let estimated_duration = (chunk_count as f64) * 30.0; // 30 seconds per chunk

    info!("Found {} checkpoint files, estimated duration: {:.2}s", chunk_count, estimated_duration);

    // Create FFmpeg concat file
    let concat_file_path = checkpoints_dir.join("concat_list.txt");
    let mut concat_content = String::new();

    for entry in &checkpoint_files {
        let path = entry.path().canonicalize()
            .map_err(|e| format!("Failed to canonicalize path: {}", e))?;
        concat_content.push_str(&format!("file '{}'\n", path.display()));
    }

    std::fs::write(&concat_file_path, concat_content)
        .map_err(|e| format!("Failed to write concat file: {}", e))?;

    // Run FFmpeg to merge chunks
    let output_path = folder_path.join("audio.mp4");
    let output_path_str = output_path.to_str()
        .ok_or("Invalid output path")?
        .to_string();

    let ffmpeg_path = find_ffmpeg_path()
        .ok_or_else(|| "FFmpeg not found. Please install FFmpeg to recover audio.".to_string())?;
    info!("Using FFmpeg at: {:?}", ffmpeg_path);

    let mut command = std::process::Command::new(ffmpeg_path);

    command.args(&[
        "-f", "concat",
        "-safe", "0",
        "-i", concat_file_path.to_str().unwrap(),
        "-c", "copy",
        "-y", // Overwrite if exists
        &output_path_str
    ]);

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let ffmpeg_result = command.output();

    match ffmpeg_result {
        Ok(output) if output.status.success() => {
            // Clean up concat file
            let _ = std::fs::remove_file(concat_file_path);

            info!("Successfully recovered audio: {}", output_path_str);

            Ok(AudioRecoveryStatus {
                status: "success".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: Some(output_path_str),
                message: format!("Successfully recovered {} audio chunks", chunk_count),
            })
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            error!("FFmpeg recovery failed: {}", error);
            Ok(AudioRecoveryStatus {
                status: "failed".to_string(),
                chunk_count,
                estimated_duration_seconds: estimated_duration,
                audio_file_path: None,
                message: format!("FFmpeg failed: {}", error),
            })
        }
        Err(e) => {
            error!("Failed to run FFmpeg: {}", e);
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
    info!("Cleaning up checkpoints for folder: {}", meeting_folder);

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
/// Returns true if .checkpoints/ directory exists and contains .mp4 files
#[tauri::command]
pub async fn has_audio_checkpoints(meeting_folder: String) -> Result<bool, String> {
    let folder_path = PathBuf::from(&meeting_folder);
    has_checkpoint_files(&folder_path)
}

/// Check whether a meeting can recover audio from either the finalized
/// recording or incremental checkpoint files.
#[tauri::command]
pub async fn has_recoverable_audio(meeting_folder: String) -> Result<bool, String> {
    let folder_path = PathBuf::from(&meeting_folder);
    if existing_recording_path(&folder_path).is_some() {
        return Ok(true);
    }

    has_checkpoint_files(&folder_path)
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
                speaker: None,
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
    async fn finalized_audio_is_recoverable_without_checkpoints() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Finalized_Meeting");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::write(meeting_folder.join("audio.mp4"), b"recorded audio").unwrap();

        assert!(has_recoverable_audio(meeting_folder.to_string_lossy().to_string())
            .await
            .unwrap());
        assert!(!has_audio_checkpoints(meeting_folder.to_string_lossy().to_string())
            .await
            .unwrap());

        let status = recover_audio_from_checkpoints(
            meeting_folder.to_string_lossy().to_string(),
            48_000,
        )
        .await
        .unwrap();

        assert_eq!(status.status, "success");
        assert_eq!(status.chunk_count, 0);
        assert_eq!(
            status.audio_file_path,
            Some(meeting_folder.join("audio.mp4").to_string_lossy().to_string())
        );
    }

    #[tokio::test]
    async fn empty_finalized_audio_is_not_recoverable() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Empty_Finalized_Meeting");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::File::create(meeting_folder.join("audio.mp4")).unwrap();

        assert!(!has_recoverable_audio(meeting_folder.to_string_lossy().to_string())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn checkpoint_audio_remains_recoverable() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Checkpoint_Meeting");
        let checkpoints = meeting_folder.join(".checkpoints");
        std::fs::create_dir_all(&checkpoints).unwrap();
        std::fs::write(checkpoints.join("audio_chunk_000.mp4"), b"checkpoint").unwrap();

        assert!(has_recoverable_audio(meeting_folder.to_string_lossy().to_string())
            .await
            .unwrap());
        assert!(has_audio_checkpoints(meeting_folder.to_string_lossy().to_string())
            .await
            .unwrap());
    }
}
