use std::sync::{Arc, Mutex};
use anyhow::Result;
use log::{info, warn, error};
use tauri::{AppHandle, Runtime, Emitter};
use tokio::sync::mpsc;

use super::recording_state::{AudioChunk, DeviceType};
use super::recording_preferences::{
    load_recording_preferences, generate_recording_filename
};

// Removed unused constant - we now preserve original sample rates

/// Resample audio with linear interpolation to prevent stretching artifacts
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

// Audio chunks with sample rate tracking (timestamp removed for simple concatenation)
#[derive(Debug, Clone)]
struct AudioData {
    data: Vec<f32>,
    sample_rate: u32,
}

// Raw audio buffers with sample rate tracking
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

/// Audio saver that accumulates raw audio data and creates WAV files exactly like the old implementation
pub struct RecordingSaver {
    chunk_receiver: Option<mpsc::UnboundedReceiver<AudioChunk>>,
    is_saving: Arc<Mutex<bool>>,
}

impl RecordingSaver {
    pub fn new() -> Self {
        Self {
            chunk_receiver: None,
            is_saving: Arc::new(Mutex::new(false)),
        }
    }

    /// Start accumulating audio chunks for saving - initializes raw audio buffers
    pub fn start_accumulation(&mut self) -> mpsc::UnboundedSender<AudioChunk> {
        info!("Initializing raw audio buffers for recording");

        // Initialize timestamped audio chunk buffers
        unsafe {
            MIC_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
            SYSTEM_CHUNKS = Some(Arc::new(Mutex::new(Vec::new())));
        }

        // Create channel for receiving audio chunks
        let (sender, receiver) = mpsc::unbounded_channel::<AudioChunk>();
        self.chunk_receiver = Some(receiver);

        // Start the accumulation task - this will accumulate raw audio data
        let is_saving_clone = self.is_saving.clone();

        if let Some(mut receiver) = self.chunk_receiver.take() {
            tokio::spawn(async move {
                info!("Recording saver raw audio accumulation task started");

                while let Some(chunk) = receiver.recv().await {
                    // Check if we should still be saving
                    let should_continue = if let Ok(is_saving) = is_saving_clone.lock() {
                        *is_saving
                    } else {
                        false
                    };

                    if !should_continue {
                        break;
                    }

                    // Store audio chunks with sample rate info
                    match chunk.device_type {
                        DeviceType::Microphone => {
                            let audio_data = AudioData {
                                data: chunk.data,
                                sample_rate: chunk.sample_rate,
                            };
                            with_mic_chunks(|chunks| {
                                if let Ok(mut mic_chunks) = chunks.lock() {
                                    mic_chunks.push(audio_data);
                                }
                            });
                        }
                        DeviceType::System => {
                            let audio_data = AudioData {
                                data: chunk.data,
                                sample_rate: chunk.sample_rate,
                            };
                            with_system_chunks(|chunks| {
                                if let Ok(mut system_chunks) = chunks.lock() {
                                    system_chunks.push(audio_data);
                                }
                            });
                        }
                    }
                }

                info!("Recording saver raw audio accumulation task ended");
            });
        }

        // Set saving flag
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = true;
        }

        sender
    }

    /// Stop accumulating and save the recorded audio exactly like the old implementation
    pub async fn stop_and_save<R: Runtime>(&mut self, app: &AppHandle<R>) -> Result<Option<String>, String> {
        info!("Stopping recording saver and saving audio with old implementation logic");

        // Stop accumulation
        if let Ok(mut is_saving) = self.is_saving.lock() {
            *is_saving = false;
        }

        // Give a moment for any final chunks to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Load recording preferences first
        let preferences = match load_recording_preferences(app).await {
            Ok(prefs) => prefs,
            Err(e) => {
                warn!("Failed to load recording preferences: {}", e);
                return Err(format!("Failed to load recording preferences: {}", e));
            }
        };

        if !preferences.auto_save {
            info!("Auto-save disabled in preferences, skipping save");
            // Clean up buffers
            unsafe {
                MIC_CHUNKS = None;
                SYSTEM_CHUNKS = None;
            }
            return Ok(None);
        }

        // Extract timestamped audio chunks and synchronize them
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

        // Get sample rates from first chunks
        let mic_sample_rate = mic_chunks.first().map(|c| c.sample_rate).unwrap_or(48000);
        let system_sample_rate = system_chunks.first().map(|c| c.sample_rate).unwrap_or(48000);

        info!("Audio sample rates - Mic: {}Hz, System: {}Hz", mic_sample_rate, system_sample_rate);

        // Use the higher sample rate for better quality
        let target_sample_rate = mic_sample_rate.max(system_sample_rate);

        // Extract and resample audio data using simple concatenation (fixes timing issues)
        let mic_data: Vec<f32> = mic_chunks.iter().flat_map(|chunk| &chunk.data).cloned().collect();
        let system_data: Vec<f32> = system_chunks.iter().flat_map(|chunk| &chunk.data).cloned().collect();

        // Resample to common rate if needed
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

        // Improved audio mixing with level balancing
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

        // Improved mic boost logic - boost when mic is 2x quieter (more realistic)
        let mic_boost = if mic_rms > 0.0 && system_rms > 0.0 && mic_rms < system_rms * 0.5 {
            (system_rms / mic_rms * 0.3).min(3.0) // Max 3x boost, more conservative
        } else {
            1.0
        };

        info!("Audio levels - Mic RMS: {:.6}, System RMS: {:.6}, Mic boost: {:.2}x",
              mic_rms, system_rms, mic_boost);

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

            // Fixed mixing levels: 60% mic + 40% system = 100% max (no over-driving)
            let mixed_sample = mic_sample * 0.6 + system_sample * 0.4;

            // Simple hard clipping instead of aggressive soft limiting
            let clipped_sample = mixed_sample.max(-1.0).min(1.0);

            mixed_data.push(clipped_sample);
        }

        if mixed_data.is_empty() {
            error!("No audio data captured");
            unsafe {
                MIC_CHUNKS = None;
                SYSTEM_CHUNKS = None;
            }
            return Err("No audio data captured".to_string());
        }

        // Automatic gain control - normalize to target level (-12dB = 0.25 peak)
        let target_level = 0.25f32;
        let current_peak = mixed_data.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        if current_peak > 0.0 {
            let gain_factor = (target_level / current_peak).min(2.0); // Max 2x gain
            info!("Applying AGC: peak={:.3}, gain={:.2}x", current_peak, gain_factor);

            for sample in &mut mixed_data {
                *sample *= gain_factor;
            }
        }

        info!("Mixed and normalized {} audio samples at {}Hz", mixed_data.len(), target_sample_rate);

        // DO NOT resample for saving! Keep original quality for the WAV file
        // Only resample for transcription, not for the saved recording

        // Convert to 16-bit PCM samples
        let mut bytes = Vec::with_capacity(mixed_data.len() * 2);
        for &sample in mixed_data.iter() {
            let value = (sample.max(-1.0).min(1.0) * 32767.0) as i16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        info!("Converted to {} bytes of PCM data", bytes.len());

        // Create WAV header using the actual sample rate (not 16kHz!)
        let data_size = bytes.len() as u32;
        let file_size = 36 + data_size;
        let sample_rate = target_sample_rate; // Use the actual sample rate!
        let channels = 1u16; // Mono
        let bits_per_sample = 16u16;
        let block_align = channels * (bits_per_sample / 8);
        let byte_rate = sample_rate * block_align as u32;

        let mut wav_file = Vec::with_capacity(44 + bytes.len());

        // RIFF header
        wav_file.extend_from_slice(b"RIFF");
        wav_file.extend_from_slice(&file_size.to_le_bytes());
        wav_file.extend_from_slice(b"WAVE");

        // fmt chunk
        wav_file.extend_from_slice(b"fmt ");
        wav_file.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        wav_file.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
        wav_file.extend_from_slice(&channels.to_le_bytes()); // num channels
        wav_file.extend_from_slice(&sample_rate.to_le_bytes()); // sample rate
        wav_file.extend_from_slice(&byte_rate.to_le_bytes()); // byte rate
        wav_file.extend_from_slice(&block_align.to_le_bytes()); // block align
        wav_file.extend_from_slice(&bits_per_sample.to_le_bytes()); // bits per sample

        // data chunk
        wav_file.extend_from_slice(b"data");
        wav_file.extend_from_slice(&data_size.to_le_bytes());
        wav_file.extend_from_slice(&bytes);

        info!("Created WAV file with {} bytes total", wav_file.len());

        // Generate filename and create save directory
        let filename = generate_recording_filename(&preferences.file_format);
        let file_path = preferences.save_folder.join(&filename);

        // Create the save directory if it doesn't exist
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                info!("Creating directory: {:?}", parent);
                if let Err(e) = std::fs::create_dir_all(parent) {
                    let err_msg = format!("Failed to create save directory: {}", e);
                    error!("{}", err_msg);
                    unsafe {
                        MIC_CHUNKS = None;
                        SYSTEM_CHUNKS = None;
                    }
                    return Err(err_msg);
                }
            }
        }

        // Save the recording
        info!("Saving recording to: {}", file_path.display());
        match std::fs::write(&file_path, wav_file) {
            Ok(_) => {
                let file_path_str = file_path.to_string_lossy().to_string();
                info!("Successfully saved recording");

                // Clean up buffers
                unsafe {
                    MIC_CHUNKS = None;
                    SYSTEM_CHUNKS = None;
                }

                // Emit success event to frontend with correct sample rate and duration
                if let Err(e) = app.emit("recording-saved", serde_json::json!({
                    "file_path": file_path_str,
                    "size_samples": mixed_data.len(),
                    "sample_rate": target_sample_rate,
                    "duration_seconds": mixed_data.len() as f64 / target_sample_rate as f64,
                    "message": "Recording saved successfully"
                })) {
                    warn!("Failed to emit recording-saved event: {}", e);
                }

                Ok(Some(file_path_str))
            }
            Err(e) => {
                let err_msg = format!("Failed to save recording: {}", e);
                error!("{}", err_msg);
                unsafe {
                    MIC_CHUNKS = None;
                    SYSTEM_CHUNKS = None;
                }
                Err(err_msg)
            }
        }
    }

    // Removed complex timestamp synchronization - using simple concatenation for reliability

    /// Get current accumulated audio stats
    pub fn get_stats(&self) -> (usize, u32) {
        let mic_len = with_mic_chunks(|chunks| {
            if let Ok(chunks_guard) = chunks.lock() {
                chunks_guard.iter().map(|chunk| chunk.data.len()).sum()
            } else {
                0
            }
        }).unwrap_or(0);

        let system_len = with_system_chunks(|chunks| {
            if let Ok(chunks_guard) = chunks.lock() {
                chunks_guard.iter().map(|chunk| chunk.data.len()).sum()
            } else {
                0
            }
        }).unwrap_or(0);

        let total_len = mic_len.max(system_len); // Return the length of the longer buffer
        (total_len, 48000) // Use default sample rate
    }
}

impl Default for RecordingSaver {
    fn default() -> Self {
        Self::new()
    }
}