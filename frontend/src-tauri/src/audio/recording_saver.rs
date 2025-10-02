use std::sync::{Arc, Mutex};
use anyhow::Result;
use log::{info, warn, error};
use tauri::{AppHandle, Runtime, Emitter};
use tokio::sync::mpsc;

use super::recording_state::{AudioChunk, ProcessedAudioChunk, DeviceType};
use super::recording_preferences::load_recording_preferences;
use super::audio_processing::{write_audio_to_file_with_meeting_name, write_transcript_to_file};

/// Improved resample function with anti-aliasing (adapted from VAD processor)
/// Prevents aliasing artifacts and distortion for better audio quality
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut resampled = Vec::with_capacity(output_len);

    // Apply simple low-pass filter before downsampling to reduce aliasing
    let cutoff_freq = 0.4; // Normalized frequency (0.4 * Nyquist)
    let mut filtered_samples = Vec::with_capacity(samples.len());

    // Simple moving average filter (basic low-pass)
    let filter_size = (from_rate as f64 / (cutoff_freq * from_rate as f64)) as usize;
    let filter_size = std::cmp::max(1, std::cmp::min(filter_size, 5)); // Limit filter size

    for i in 0..samples.len() {
        let start = if i >= filter_size { i - filter_size } else { 0 };
        let end = std::cmp::min(i + filter_size + 1, samples.len());
        let sum: f32 = samples[start..end].iter().sum();
        filtered_samples.push(sum / (end - start) as f32);
    }

    // Linear interpolation downsampling
    for i in 0..output_len {
        let source_pos = i as f64 * ratio;
        let source_index = source_pos as usize;
        let fraction = source_pos - source_index as f64;

        if source_index + 1 < filtered_samples.len() {
            // Linear interpolation
            let sample1 = filtered_samples[source_index];
            let sample2 = filtered_samples[source_index + 1];
            let interpolated = sample1 + (sample2 - sample1) * fraction as f32;
            resampled.push(interpolated);
        } else if source_index < filtered_samples.len() {
            resampled.push(filtered_samples[source_index]);
        }
    }

    resampled
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
    transcript_chunks: Arc<Mutex<Vec<String>>>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            chunk_receiver: None,
            is_saving: Arc::new(Mutex::new(false)),
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

        // Extract chunks from static buffers
        let mic_chunks = with_mic_chunks(|chunks| {
            if let Ok(guard) = chunks.lock() {
                guard.clone()
            } else {
                Vec::new()
            }
        }).unwrap_or_default();

        let system_chunks = with_system_chunks(|chunks| {
            if let Ok(guard) = chunks.lock() {
                guard.clone()
            } else {
                Vec::new()
            }
        }).unwrap_or_default();

        info!("Processing {} mic chunks and {} system chunks", mic_chunks.len(), system_chunks.len());

        if mic_chunks.is_empty() && system_chunks.is_empty() {
            error!("No audio data captured");
            unsafe {
                MIC_CHUNKS = None;
                SYSTEM_CHUNKS = None;
            }
            return Err("No audio data captured".to_string());
        }

        // CRITICAL FIX: Use direct concatenation WITHOUT sorting by timestamp
        // Timestamps can be unreliable and sorting causes audio sync issues
        // Trust the order that chunks arrive in - it's guaranteed by the audio callback
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

        // Resample ONCE before mixing
        let mic_resampled = if mic_sample_rate != target_sample_rate && !mic_data.is_empty() {
            info!("Resampling mic audio from {}Hz to {}Hz", mic_sample_rate, target_sample_rate);
            resample_audio(&mic_data, mic_sample_rate, target_sample_rate)
        } else {
            mic_data
        };

        let system_resampled = if system_sample_rate != target_sample_rate && !system_data.is_empty() {
            info!("Resampling system audio from {}Hz to {}Hz", system_sample_rate, target_sample_rate);
            resample_audio(&system_data, system_sample_rate, target_sample_rate)
        } else {
            system_data
        };

        // Calculate RMS levels for adaptive mixing
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

        info!("Audio levels - Mic RMS: {:.6}, System RMS: {:.6}", mic_rms, system_rms);

        // FIXED: Smart ducking mix matching transcription pipeline exactly
        // Use min() to avoid zero-padding artifacts that cause echo/reverb
        let min_len = mic_resampled.len().min(system_resampled.len());
        let mut mixed_data = Vec::with_capacity(min_len);

        // Mix only the overlapping portion
        for i in 0..min_len {
            let mic_sample = mic_resampled[i];
            let system_sample = system_resampled[i];

            // Smart ducking (IDENTICAL to transcription pipeline)
            // When system audio is active (> 0.01), mix with ducked mic (0.6 mic + 0.9 system)
            // When only mic, use full strength
            let mixed_sample = if system_sample.abs() > 0.01 {
                // System audio active: duck mic (0.6x), boost system (0.9x)
                ((mic_sample * 0.6) + (system_sample * 0.9)).clamp(-1.0, 1.0)
            } else {
                // Only mic: full strength
                mic_sample
            };

            mixed_data.push(mixed_sample);
        }

        // Handle any remaining samples from the longer buffer
        // This prevents losing audio while avoiding echo artifacts
        if mic_resampled.len() > min_len {
            info!("Appending {} remaining mic samples", mic_resampled.len() - min_len);
            mixed_data.extend_from_slice(&mic_resampled[min_len..]);
        } else if system_resampled.len() > min_len {
            info!("Appending {} remaining system samples", system_resampled.len() - min_len);
            mixed_data.extend_from_slice(&system_resampled[min_len..]);
        }

        info!("Mixed {} samples at {}Hz with smart ducking (matched transcription)", mixed_data.len(), target_sample_rate);

        // NO NORMALIZATION NEEDED: Audio is already VAD-processed and clean
        // The VAD output is speech-only with proper levels
        // Only apply safety normalization if RMS is extremely low
        let mixed_data = if !mixed_data.is_empty() {
            let current_rms = (mixed_data.iter().map(|x| x * x).sum::<f32>() / mixed_data.len() as f32).sqrt();
            info!("Final mixed audio RMS: {:.6} (no normalization - VAD output is clean)", current_rms);

            // Safety: only normalize if extremely quiet (< 0.05 RMS)
            if current_rms < 0.05 && current_rms > 0.0 {
                warn!("Audio extremely quiet ({:.6}), applying safety normalization", current_rms);
                super::audio_processing::normalize_v2(&mixed_data)
            } else {
                mixed_data
            }
        } else {
            mixed_data
        };

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

        // Clean up static buffers and transcript chunks
        unsafe {
            MIC_CHUNKS = None;
            SYSTEM_CHUNKS = None;
        }
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