use std::sync::{Arc, Mutex};
use anyhow::Result;
use log::{info, warn, error};
use tauri::{AppHandle, Runtime, Emitter};
use tokio::sync::mpsc;

use super::recording_state::{AudioChunk, DeviceType};
use super::recording_preferences::load_recording_preferences;
use super::audio_processing::{write_audio_to_file_with_meeting_name, write_transcript_to_file};

/// Simple resample function for sample rate conversion
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let fraction = src_pos - src_idx as f64;

        if src_idx + 1 < samples.len() {
            // Linear interpolation between adjacent samples
            let sample1 = samples[src_idx];
            let sample2 = samples[src_idx + 1];
            let interpolated = sample1 + (sample2 - sample1) * fraction as f32;
            resampled.push(interpolated);
        } else if src_idx < samples.len() {
            // Use the last sample if we're at the end
            resampled.push(samples[src_idx]);
        }
    }

    resampled
}

// Simple audio data structure
#[derive(Debug, Clone)]
struct AudioData {
    data: Vec<f32>,
    sample_rate: u32,
}

// Session-scoped audio storage (improvement over unsafe static buffers)
#[derive(Debug, Clone)]
struct AudioSession {
    mic_chunks: Arc<Mutex<Vec<AudioData>>>,
    system_chunks: Arc<Mutex<Vec<AudioData>>>,
    session_id: uuid::Uuid,
}

impl AudioSession {
    fn new() -> Self {
        Self {
            mic_chunks: Arc::new(Mutex::new(Vec::new())),
            system_chunks: Arc::new(Mutex::new(Vec::new())),
            session_id: uuid::Uuid::new_v4(),
        }
    }

    fn clear(&self) {
        if let Ok(mut mic_chunks) = self.mic_chunks.lock() {
            mic_chunks.clear();
        }
        if let Ok(mut system_chunks) = self.system_chunks.lock() {
            system_chunks.clear();
        }
    }
}

impl Default for AudioSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple audio saver using proven concatenation approach
pub struct RecordingSaver {
    chunk_receiver: Option<mpsc::UnboundedReceiver<AudioChunk>>,
    is_saving: Arc<Mutex<bool>>,
    current_session: Option<AudioSession>,
    meeting_name: Option<String>,
    transcript_chunks: Arc<Mutex<Vec<String>>>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            chunk_receiver: None,
            is_saving: Arc::new(Mutex::new(false)),
            current_session: None,
            meeting_name: None,
            transcript_chunks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Set the meeting name for this recording session
    pub fn set_meeting_name(&mut self, name: Option<String>) {
        self.meeting_name = name;
    }

    /// Add a transcript chunk to be saved later
    pub fn add_transcript_chunk(&self, text: String) {
        if let Ok(mut chunks) = self.transcript_chunks.lock() {
            chunks.push(text);
        }
    }

    /// Force cleanup of any existing session
    pub fn force_cleanup(&mut self) {
        if let Some(session) = &self.current_session {
            info!("Force cleaning up recording session: {}", session.session_id);
            session.clear();
        }
        self.current_session = None;

        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = false;
        }

        // Clear transcript chunks
        if let Ok(mut chunks) = self.transcript_chunks.lock() {
            chunks.clear();
        }

        self.chunk_receiver = None;
    }

    /// Start accumulating audio chunks - simple approach
    pub fn start_accumulation(&mut self) -> mpsc::UnboundedSender<AudioChunk> {
        // Force cleanup any existing session first
        self.force_cleanup();

        // Create new clean session
        let session = AudioSession::new();
        info!("Starting new recording session: {}", session.session_id);

        let session_clone = session.clone();
        self.current_session = Some(session);

        // Create channel for receiving audio chunks
        let (sender, receiver) = mpsc::unbounded_channel::<AudioChunk>();
        self.chunk_receiver = Some(receiver);

        // Start simple accumulation task
        let is_saving_clone = self.is_saving.clone();

        if let Some(mut receiver) = self.chunk_receiver.take() {
            tokio::spawn(async move {
                info!("Recording saver started for session: {}", session_clone.session_id);

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

                    // Simple chunk storage - no filtering, no processing
                    let audio_data = AudioData {
                        data: chunk.data,
                        sample_rate: chunk.sample_rate,
                    };

                    match chunk.device_type {
                        DeviceType::Microphone => {
                            if let Ok(mut mic_chunks) = session_clone.mic_chunks.lock() {
                                mic_chunks.push(audio_data);
                            }
                        }
                        DeviceType::System => {
                            if let Ok(mut system_chunks) = session_clone.system_chunks.lock() {
                                system_chunks.push(audio_data);
                            }
                        }
                    }
                }

                info!("Recording saver accumulation ended for session: {}", session_clone.session_id);
            });
        }

        // Set saving flag
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = true;
        }

        sender
    }

    /// Get recording statistics
    pub fn get_stats(&self) -> (usize, u32) {
        if let Some(session) = &self.current_session {
            let mic_count = session.mic_chunks.lock().map(|chunks| chunks.len()).unwrap_or(0);
            let system_count = session.system_chunks.lock().map(|chunks| chunks.len()).unwrap_or(0);
            (mic_count + system_count, 48000) // Default sample rate
        } else {
            (0, 48000)
        }
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
            if let Some(session) = &self.current_session {
                session.clear();
            }
            self.current_session = None;
            return Ok(None);
        }

        // Get current session
        let session = self.current_session.as_ref().ok_or_else(|| {
            "No active recording session found".to_string()
        })?;

        info!("Saving session: {}", session.session_id);

        // Extract chunks from session
        let mic_chunks = if let Ok(guard) = session.mic_chunks.lock() {
            guard.clone()
        } else {
            warn!("Failed to lock mic chunks");
            Vec::new()
        };

        let system_chunks = if let Ok(guard) = session.system_chunks.lock() {
            guard.clone()
        } else {
            warn!("Failed to lock system chunks");
            Vec::new()
        };

        info!("Processing {} mic chunks and {} system chunks", mic_chunks.len(), system_chunks.len());

        if mic_chunks.is_empty() && system_chunks.is_empty() {
            error!("No audio data captured");
            session.clear();
            self.current_session = None;
            return Err("No audio data captured".to_string());
        }

        // Simple concatenation approach - what was working before
        let mic_data: Vec<f32> = mic_chunks.iter().flat_map(|chunk| &chunk.data).cloned().collect();
        let system_data: Vec<f32> = system_chunks.iter().flat_map(|chunk| &chunk.data).cloned().collect();

        info!("Raw audio data - Mic: {} samples, System: {} samples", mic_data.len(), system_data.len());

        // Get sample rates
        let mic_sample_rate = mic_chunks.first().map(|c| c.sample_rate).unwrap_or(48000);
        let system_sample_rate = system_chunks.first().map(|c| c.sample_rate).unwrap_or(48000);

        // Use higher sample rate for better quality
        let target_sample_rate = mic_sample_rate.max(system_sample_rate);

        info!("Sample rates - Mic: {}Hz, System: {}Hz, Target: {}Hz",
              mic_sample_rate, system_sample_rate, target_sample_rate);

        // Simple resampling if needed
        let mic_resampled = if mic_sample_rate != target_sample_rate && !mic_data.is_empty() {
            info!("Resampling mic audio");
            resample_audio(&mic_data, mic_sample_rate, target_sample_rate)
        } else {
            mic_data
        };

        let system_resampled = if system_sample_rate != target_sample_rate && !system_data.is_empty() {
            info!("Resampling system audio");
            resample_audio(&system_data, system_sample_rate, target_sample_rate)
        } else {
            system_data
        };

        // FIXED: Improved mixing to prevent overlapping/stretching
        let max_len = mic_resampled.len().max(system_resampled.len());
        let mut mixed_data = Vec::with_capacity(max_len);

        // Calculate RMS levels for balancing
        let mic_rms = if !mic_resampled.is_empty() {
            (mic_resampled.iter().map(|x| x * x).sum::<f32>() / mic_resampled.len() as f32).sqrt()
        } else {
            0.0
        };

        let system_rms = if !system_resampled.is_empty() {
            (system_resampled.iter().map(|x| x * x).sum::<f32>() / system_resampled.len() as f32).sqrt()
        } else {
            0.0
        };

        // Conservative mic boost only when significantly quieter
        let mic_boost = if mic_rms > 0.0 && system_rms > 0.0 && mic_rms < system_rms * 0.3 {
            (system_rms / mic_rms * 0.4).min(2.5) // Max 2.5x boost
        } else {
            1.0
        };

        info!("Audio levels - Mic RMS: {:.6}, System RMS: {:.6}, Mic boost: {:.2}x",
              mic_rms, system_rms, mic_boost);

        // FIXED: Balanced mixing to prevent overlapping issues
        for i in 0..max_len {
            let mic_sample = if i < mic_resampled.len() {
                mic_resampled[i] * mic_boost
            } else {
                0.0
            };

            let system_sample = if i < system_resampled.len() {
                system_resampled[i]
            } else {
                0.0
            };

            // FIXED: Better mixing ratio - 70% mic + 30% system to prevent overlapping
            let mixed_sample = mic_sample * 0.7 + system_sample * 0.3;

            // Simple clipping to prevent distortion
            let clipped_sample = mixed_sample.max(-0.95).min(0.95);
            mixed_data.push(clipped_sample);
        }

        info!("Mixed audio: {} samples at {}Hz", mixed_data.len(), target_sample_rate);

        // Use the new audio writing function with meeting name
        let filename = write_audio_to_file_with_meeting_name(
            &mixed_data,
            target_sample_rate,
            &preferences.save_folder,
            "recording",
            false, // Don't skip encoding
            self.meeting_name.as_deref(),
        ).map_err(|e| format!("Failed to write audio file: {}", e))?;

        info!("✅ Recording saved: {} ({} samples, {:.2}s)",
              filename, mixed_data.len(), mixed_data.len() as f64 / target_sample_rate as f64);

        // Save transcript if we have any transcript chunks
        let transcript_filename = if let Ok(chunks) = self.transcript_chunks.lock() {
            if !chunks.is_empty() {
                let combined_transcript = chunks.join("\n");
                match write_transcript_to_file(
                    &combined_transcript,
                    &preferences.save_folder,
                    self.meeting_name.as_deref(),
                ) {
                    Ok(transcript_path) => {
                        info!("✅ Transcript saved: {}", transcript_path);
                        Some(transcript_path)
                    }
                    Err(e) => {
                        warn!("Failed to save transcript: {}", e);
                        None
                    }
                }
            } else {
                info!("No transcript chunks to save");
                None
            }
        } else {
            warn!("Failed to lock transcript chunks");
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

        // Clean up session and transcript chunks
        session.clear();
        self.current_session = None;
        if let Ok(mut chunks) = self.transcript_chunks.lock() {
            chunks.clear();
        }

        Ok(Some(filename))
    }
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}