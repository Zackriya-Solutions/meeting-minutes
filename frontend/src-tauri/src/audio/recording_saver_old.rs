use std::sync::{Arc, Mutex};
use anyhow::Result;
use log::{info, warn, error};
use tauri::{AppHandle, Runtime, Emitter};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};

use super::recording_state::{AudioChunk, ProcessedAudioChunk, DeviceType};
use super::recording_preferences::load_recording_preferences;
use super::audio_processing::write_audio_to_file_with_meeting_name;

/// Structured transcript segment for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: f64, // Seconds from recording start
    pub audio_end_time: f64,   // Seconds from recording start
    pub duration: f64,          // Segment duration in seconds
    pub display_time: String,   // Formatted time for display like "[02:15]"
    pub confidence: f32,
    pub sequence_id: u64,
}

// Simple audio data structure (NO TIMESTAMP - prevents sorting issues)
#[derive(Debug, Clone)]
struct AudioData {
    data: Vec<f32>,
    sample_rate: u32,
}

// Simple static buffers for audio accumulation (proven working approach)
static mut MIC_CHUNKS: Option<Arc<Mutex<Vec<AudioData>>>> = None;
static mut SYSTEM_CHUNKS: Option<Arc<Mutex<Vec<AudioData>>>> = None;

// Helper functions to safely access static buffers
fn with_mic_chunks<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Arc<Mutex<Vec<AudioData>>>) -> R,
{
    unsafe {
        let ptr = std::ptr::addr_of!(MIC_CHUNKS);
        (*ptr).as_ref().map(f)
    }
}

fn with_system_chunks<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Arc<Mutex<Vec<AudioData>>>) -> R,
{
    unsafe {
        let ptr = std::ptr::addr_of!(SYSTEM_CHUNKS);
        (*ptr).as_ref().map(f)
    }
}

/// Simple audio saver using proven concatenation approach
pub struct RecordingSaver {
    chunk_receiver: Option<mpsc::UnboundedReceiver<AudioChunk>>,
    is_saving: Arc<Mutex<bool>>,
    meeting_name: Option<String>,
    transcript_segments: Arc<Mutex<Vec<TranscriptSegment>>>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            chunk_receiver: None,
            is_saving: Arc::new(Mutex::new(false)),
            meeting_name: None,
            transcript_segments: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    /// Add or update a structured transcript segment (upserts based on sequence_id)
    pub fn add_transcript_segment(&self, segment: TranscriptSegment) {
        if let Ok(mut segments) = self.transcript_segments.lock() {
            // Check if segment with same sequence_id exists (update it)
            if let Some(existing) = segments.iter_mut().find(|s| s.sequence_id == segment.sequence_id) {
                *existing = segment.clone();
                info!("Updated transcript segment {} (seq: {}) - total segments: {}", segment.id, segment.sequence_id, segments.len());
            } else {
                // New segment, add it
                segments.push(segment.clone());
                info!("Added new transcript segment {} (seq: {}) - total segments: {}", segment.id, segment.sequence_id, segments.len());
            }
        } else {
            error!("Failed to lock transcript segments for adding segment {}", segment.id);
        }
    }

    /// Legacy method for backward compatibility - converts text to basic segment
    pub fn add_transcript_chunk(&self, text: String) {
        // Create a basic segment with minimal info for backward compatibility
        let segment = TranscriptSegment {
            id: format!("seg_{}", chrono::Utc::now().timestamp_millis()),
            text,
            audio_start_time: 0.0,
            audio_end_time: 0.0,
            duration: 0.0,
            display_time: "[00:00]".to_string(),
            confidence: 1.0,
            sequence_id: 0,
        };
        self.add_transcript_segment(segment);
    }

    /// Start accumulating audio chunks - simple proven approach
    pub fn start_accumulation(&mut self) -> mpsc::UnboundedSender<AudioChunk> {
        info!("Initializing simple audio buffers for recording");

        // Initialize static audio buffers
        unsafe {
            MIC_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
            SYSTEM_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
        }

        // Create channel for receiving audio chunks
        let (sender, receiver) = mpsc::unbounded_channel::<AudioChunk>();
        self.chunk_receiver = Some(receiver);

        // Start simple accumulation task
        let is_saving_clone = self.is_saving.clone();

        if let Some(mut receiver) = self.chunk_receiver.take() {
            tokio::spawn(async move {
                info!("Recording saver accumulation task started");

                while let Some(chunk) = receiver.recv().await {
                    // Check if we should continue saving
                    let should_continue = if let Ok(is_saving) = is_saving_clone.lock() {
                        *is_saving
                    } else {
                        false
                    };

                    if !should_continue {
                        break;
                    }

                    // Simple chunk storage - no filtering, no processing, NO TIMESTAMP
                    let audio_data = AudioData {
                        data: chunk.data,
                        sample_rate: chunk.sample_rate,
                    };

                    match chunk.device_type {
                        DeviceType::Microphone => {
                            with_mic_chunks(|chunks| {
                                if let Ok(mut mic_chunks) = chunks.lock() {
                                    mic_chunks.push(audio_data);
                                }
                            });
                        }
                        DeviceType::System => {
                            with_system_chunks(|chunks| {
                                if let Ok(mut system_chunks) = chunks.lock() {
                                    system_chunks.push(audio_data);
                                }
                            });
                        }
                    }
                }

                info!("Recording saver accumulation task ended");
            });
        }

        // Set saving flag
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = true;
        }

        sender
    }

    /// NEW: Start accumulation with processed (VAD-filtered) audio
    /// This receives clean speech-only audio from the pipeline
    pub fn start_accumulation_with_processed(&mut self, mut receiver: mpsc::UnboundedReceiver<ProcessedAudioChunk>) {
        info!("Initializing processed audio buffers for recording");

        // Initialize static audio buffers
        unsafe {
            MIC_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
            SYSTEM_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
        }

        // Start accumulation task for processed audio
        let is_saving_clone = self.is_saving.clone();

        tokio::spawn(async move {
            info!("Recording saver (processed audio) accumulation task started");

            while let Some(chunk) = receiver.recv().await {
                // Check if we should continue saving
                let should_continue = if let Ok(is_saving) = is_saving_clone.lock() {
                    *is_saving
                } else {
                    false
                };

                if !should_continue {
                    break;
                }

                // Store processed audio chunk
                let audio_data = AudioData {
                    data: chunk.data,
                    sample_rate: chunk.sample_rate,
                };

                match chunk.device_type {
                    DeviceType::Microphone => {
                        with_mic_chunks(|chunks| {
                            if let Ok(mut mic_chunks) = chunks.lock() {
                                mic_chunks.push(audio_data);
                            }
                        });
                    }
                    DeviceType::System => {
                        with_system_chunks(|chunks| {
                            if let Ok(mut system_chunks) = chunks.lock() {
                                system_chunks.push(audio_data);
                            }
                        });
                    }
                }
            }

            info!("Recording saver (processed audio) accumulation task ended");
        });

        // Set saving flag
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = true;
        }
    }

    /// Get recording statistics
    pub fn get_stats(&self) -> (usize, u32) {
        let mic_count = with_mic_chunks(|chunks| {
            chunks.lock().map(|c| c.len()).unwrap_or(0)
        }).unwrap_or(0);

        let system_count = with_system_chunks(|chunks| {
            chunks.lock().map(|c| c.len()).unwrap_or(0)
        }).unwrap_or(0);

        (mic_count + system_count, 48000)
    }

    /// Stop and save using simple concatenation approach
    pub async fn stop_and_save<R: Runtime>(&mut self, app: &AppHandle<R>) -> Result<Option<String>, String> {
        info!("Stopping recording saver - using simple concatenation approach");

        // Stop accumulation
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = false;
        }

        // Give time for final chunks
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Load recording preferences
        let preferences = match load_recording_preferences(app).await {
            Ok(prefs) => prefs,
            Err(e) => {
                warn!("Failed to load recording preferences: {}", e);
                return Err(format!("Failed to load recording preferences: {}", e));
            }
        };

        if !preferences.auto_save {
            info!("Auto-save disabled, skipping save");
            // Clean up buffers
            unsafe {
                MIC_CHUNKS = None;
                SYSTEM_CHUNKS = None;
            }
            return Ok(None);
        }

        // Extract PRE-MIXED audio chunks from pipeline
        // The pipeline professionally mixes mic + system audio and sends unified chunks
        let mixed_chunks = with_mic_chunks(|chunks| {
            if let Ok(guard) = chunks.lock() {
                guard.clone()
            } else {
                Vec::new()
            }
        }).unwrap_or_default();

        info!("Processing {} pre-mixed audio chunks from pipeline", mixed_chunks.len());

        if mixed_chunks.is_empty() {
            error!("No audio data captured");
            unsafe {
                MIC_CHUNKS = None;
                SYSTEM_CHUNKS = None;
            }
            return Err("No audio data captured".to_string());
        }

        // Concatenate pre-mixed audio (already contains both mic AND system audio)
        let mixed_data: Vec<f32> = mixed_chunks.iter().flat_map(|chunk| &chunk.data).cloned().collect();
        let target_sample_rate = mixed_chunks.first().map(|c| c.sample_rate).unwrap_or(48000);

        info!("Saving pre-mixed audio: {} samples at {}Hz (includes mic + system)", mixed_data.len(), target_sample_rate);

        // Calculate RMS for logging
        let current_rms = if !mixed_data.is_empty() {
            (mixed_data.iter().map(|x| x * x).sum::<f32>() / mixed_data.len() as f32).sqrt()
        } else {
            0.0
        };
        info!("Pre-mixed audio RMS: {:.6} (should be >0 if system audio present)", current_rms);

        // Use the new audio writing function with meeting name
        let filename = write_audio_to_file_with_meeting_name(
            &mixed_data,
            target_sample_rate,
            &preferences.save_folder,
            "recording",
            false, // Don't skip encoding
            self.meeting_name.as_deref(),
        ).map_err(|e| format!("Failed to write audio file: {}", e))?;

        let recording_duration = mixed_data.len() as f64 / target_sample_rate as f64;
        info!("✅ Recording saved: {} ({} samples, {:.2}s)",
              filename, mixed_data.len(), recording_duration);

        // Save transcript with NEW structured JSON format (includes timestamps for sync)
        info!("Attempting to save transcript JSON...");
        let transcript_filename = if let Ok(segments) = self.transcript_segments.lock() {
            info!("Locked transcript segments successfully, count: {}", segments.len());
            if !segments.is_empty() {
                // Extract just the filename from the full path for JSON reference
                let audio_filename = std::path::Path::new(&filename)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("recording.mp4");

                match super::audio_processing::write_transcript_json_to_file(
                    &segments,
                    &preferences.save_folder,
                    self.meeting_name.as_deref(),
                    audio_filename,
                    recording_duration,
                ) {
                    Ok(transcript_path) => {
                        info!("✅ Structured transcript saved: {} ({} segments with timestamps)",
                              transcript_path, segments.len());
                        Some(transcript_path)
                    }
                    Err(e) => {
                        error!("❌ Failed to save structured transcript JSON: {}", e);
                        error!("   Transcript segments: {}", segments.len());
                        error!("   Save folder: {}", preferences.save_folder.display());
                        error!("   Meeting name: {:?}", self.meeting_name);
                        None
                    }
                }
            } else {
                info!("No transcript segments to save");
                None
            }
        } else {
            warn!("Failed to lock transcript segments");
            None
        };

        // Emit save event with both audio and transcript paths
        let save_event = serde_json::json!({
            "audio_file": filename,
            "transcript_file": transcript_filename,
            "meeting_name": self.meeting_name
        });

        if let Err(e) = app.emit("recording-saved", &save_event) {
            warn!("Failed to emit recording-saved event: {}", e);
        }

        // Clean up static buffers and transcript segments
        unsafe {
            MIC_CHUNKS = None;
            SYSTEM_CHUNKS = None;
        }
        if let Ok(mut segments) = self.transcript_segments.lock() {
            segments.clear();
        }

        Ok(Some(filename))
    }
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}