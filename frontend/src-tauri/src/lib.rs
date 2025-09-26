use std::fs;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::Duration;
use std::collections::VecDeque;
use serde::{Deserialize, Serialize};
use tauri_plugin_notification::NotificationExt;

// Declare audio module
pub mod audio;
pub mod ollama;
pub mod analytics;
pub mod api;
pub mod utils;
pub mod console_utils;
pub mod tray;
pub mod whisper_engine;
pub mod openrouter;

use audio::{
    default_input_device, default_output_device, AudioStream, list_audio_devices, parse_audio_device,
    encode_single_audio,AudioDevice, DeviceType
};
use audio::vad::extract_speech_16k;
use ollama::{OllamaModel};
use analytics::{AnalyticsClient, AnalyticsConfig};
use utils::format_timestamp;
use tauri::{Runtime, AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use log::{info as log_info, error as log_error, debug as log_debug,warn as log_warn};
use reqwest::multipart::{Form, Part};
use tokio::sync::mpsc;
use whisper_engine::{WhisperEngine, ModelInfo, ModelStatus};

static RECORDING_FLAG: AtomicBool = AtomicBool::new(false);
static SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static CHUNK_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
static DROPPED_CHUNK_COUNTER: AtomicU64 = AtomicU64::new(0);
static mut MIC_BUFFER: Option<Arc<Mutex<Vec<f32>>>> = None;
static mut SYSTEM_BUFFER: Option<Arc<Mutex<Vec<f32>>>> = None;
static mut AUDIO_CHUNK_QUEUE: Option<Arc<Mutex<VecDeque<AudioChunk>>>> = None;
static mut MIC_STREAM: Option<Arc<AudioStream>> = None;
static mut SYSTEM_STREAM: Option<Arc<AudioStream>> = None;
static mut IS_RUNNING: Option<Arc<AtomicBool>> = None;
static mut RECORDING_START_TIME: Option<std::time::Instant> = None;
static mut TRANSCRIPTION_TASK: Option<tokio::task::JoinHandle<()>> = None;
static mut AUDIO_COLLECTION_TASK: Option<tokio::task::JoinHandle<()>> = None;
static mut ANALYTICS_CLIENT: Option<Arc<AnalyticsClient>> = None;
static mut ERROR_EVENT_EMITTED: bool = false;
static mut WHISPER_ENGINE: Option<Arc<WhisperEngine>> = None;
static LAST_TRANSCRIPTION_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static ACTIVE_WORKERS: AtomicU64 = AtomicU64::new(0);

// Audio configuration constants
const CHUNK_DURATION_MS: u32 = 30000; // 30 seconds per chunk for better sentence processing
const WHISPER_SAMPLE_RATE: u32 = 16000; // Whisper's required sample rate
const WAV_SAMPLE_RATE: u32 = 44100; // WAV file sample rate
const WAV_CHANNELS: u16 = 2; // Stereo for WAV files
const WHISPER_CHANNELS: u16 = 1; // Mono for Whisper API
const SENTENCE_TIMEOUT_MS: u64 = 1000; // Emit incomplete sentence after 1 second of silence
const MIN_CHUNK_DURATION_MS: u32 = 2000; // Minimum duration before sending chunk
const MIN_RECORDING_DURATION_MS: u64 = 2000; // 2 seconds minimum
const MAX_AUDIO_QUEUE_SIZE: usize = 50; // Maximum number of chunks in queue


// VAD and silence detection thresholds - BALANCED for better speech preservation
const VAD_SILENCE_THRESHOLD: f32 = 0.003; // Reduced threshold for detecting silence in individual audio samples
const VAD_RMS_SILENCE_THRESHOLD: f32 = 0.002; // Reduced RMS energy threshold for silence detection
const CHUNK_SILENCE_THRESHOLD: f32 = 0.002; // Reduced RMS threshold for chunk-level silence detection
const CHUNK_AVG_SILENCE_THRESHOLD: f32 = 0.003; // Reduced average level threshold for chunk-level silence detection

// Server configuration constants
const TRANSCRIPT_SERVER_URL: &str = "http://127.0.0.1:8178";

#[derive(Debug, Deserialize)]
struct RecordingArgs {
    save_path: String,
}

#[derive(Debug, Serialize, Clone)]
struct TranscriptionStatus {
    chunks_in_queue: usize,
    is_processing: bool,
    last_activity_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
struct TranscriptUpdate {
    text: String,
    timestamp: String,
    source: String,
    sequence_id: u64,
    chunk_start_time: f64,
    is_partial: bool,
}

#[derive(Debug, Clone)]
struct AudioChunk {
    samples: Vec<f32>,
    timestamp: f64,
    chunk_id: u64,
    start_time: std::time::Instant,
    recording_start_time: std::time::Instant,
}

#[derive(Debug, Deserialize)]
struct TranscriptSegment {
    text: String,
    t0: f32,
    t1: f32,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    segments: Vec<TranscriptSegment>,
    buffer_size_ms: i32,
}

// Helper struct to accumulate transcript segments
#[derive(Debug)]
struct TranscriptAccumulator {
    current_sentence: String,
    sentence_start_time: f32,
    last_update_time: std::time::Instant,
    last_segment_hash: u64,
    current_chunk_id: u64,
    current_chunk_start_time: f64,
    recording_start_time: Option<std::time::Instant>,
}

impl TranscriptAccumulator {
    fn new() -> Self {
        Self {
            current_sentence: String::new(),
            sentence_start_time: 0.0,
            last_update_time: std::time::Instant::now(),
            last_segment_hash: 0,
            current_chunk_id: 0,
            current_chunk_start_time: 0.0,
            recording_start_time: None,
        }
    }

    fn set_chunk_context(&mut self, chunk_id: u64, chunk_start_time: f64, recording_start_time: std::time::Instant) {
        self.current_chunk_id = chunk_id;
        self.current_chunk_start_time = chunk_start_time;
        // Store recording start time for calculating actual elapsed times
        self.recording_start_time = Some(recording_start_time);
    }

    fn add_segment(&mut self, segment: &TranscriptSegment) -> Option<TranscriptUpdate> {
        log_info!("Processing new transcript segment: {:?}", segment);
        
        // Update the last update time
        self.last_update_time = std::time::Instant::now();

        // Clean up the text (remove [BLANK_AUDIO], [AUDIO OUT] and trim)
        let clean_text = segment.text
            .replace("[BLANK_AUDIO]", "")
            .replace("[AUDIO OUT]", "")
            .trim()
            .to_string();
            
        if !clean_text.is_empty() {
            log_info!("Clean transcript text: {}", clean_text);
        }

        // Skip empty segments or very short segments (less than 1 second)
        if clean_text.is_empty() || (segment.t1 - segment.t0) < 1.0 {
            return None;
        }

        // Calculate hash of this segment to detect duplicates
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        segment.text.hash(&mut hasher);
        segment.t0.to_bits().hash(&mut hasher);
        segment.t1.to_bits().hash(&mut hasher);
        self.current_chunk_id.hash(&mut hasher); // Include chunk ID to avoid cross-chunk duplicates
        let segment_hash = hasher.finish();

        // Skip if this is a duplicate segment
        if segment_hash == self.last_segment_hash {
            log_info!("Skipping duplicate segment: {}", clean_text);
            return None;
        }
        self.last_segment_hash = segment_hash;

        // If this is the start of a new sentence, store the start time
        if self.current_sentence.is_empty() {
            self.sentence_start_time = segment.t0;
        }

        // Add the new text with proper spacing
        if !self.current_sentence.is_empty() && !self.current_sentence.ends_with(' ') {
            self.current_sentence.push(' ');
        }
        self.current_sentence.push_str(&clean_text);

        // Check if we have a complete sentence (including common sentence endings)
        let has_sentence_ending = clean_text.ends_with('.') || clean_text.ends_with('?') || clean_text.ends_with('!') ||
                                  clean_text.ends_with("...") || clean_text.ends_with(".\"") || clean_text.ends_with(".'");
        
        if has_sentence_ending {
            let sentence = std::mem::take(&mut self.current_sentence);
            let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
            
            // Calculate actual elapsed time from recording start
            let (start_elapsed, end_elapsed) = if let Some(recording_start) = self.recording_start_time {
                // Calculate when this sentence actually started and ended relative to recording start
                let sentence_start_elapsed = self.current_chunk_start_time + (self.sentence_start_time as f64 / 1000.0);
                let sentence_end_elapsed = self.current_chunk_start_time + (segment.t1 as f64 / 1000.0);
                (sentence_start_elapsed.max(0.0), sentence_end_elapsed.max(0.0))
            } else {
                // Fallback to chunk-relative times if recording start time not available
                let sentence_start_elapsed = self.current_chunk_start_time + (self.sentence_start_time as f64 / 1000.0);
                let sentence_end_elapsed = self.current_chunk_start_time + (segment.t1 as f64 / 1000.0);
                (sentence_start_elapsed.max(0.0), sentence_end_elapsed.max(0.0))
            };
            
            let update = TranscriptUpdate {
                text: sentence.trim().to_string(),
                timestamp: format!("{}", format_timestamp(start_elapsed)),
                source: "Mixed Audio".to_string(),
                sequence_id,
                chunk_start_time: self.current_chunk_start_time,
                is_partial: false,
            };
            log_info!("Generated transcript update: {:?}", update);
            Some(update)
        } else {
            None
        }
    }

    fn check_timeout(&mut self) -> Option<TranscriptUpdate> {
        if !self.current_sentence.is_empty() && 
           self.last_update_time.elapsed() > Duration::from_millis(SENTENCE_TIMEOUT_MS) {
            let sentence = std::mem::take(&mut self.current_sentence);
            let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
            
            // Calculate actual elapsed time from recording start for timeout
            let (start_elapsed, end_elapsed) = if let Some(recording_start) = self.recording_start_time {
                // For timeout, we know the sentence started at sentence_start_time and is timing out now
                let sentence_start_elapsed = self.current_chunk_start_time + (self.sentence_start_time as f64 / 1000.0);
                let sentence_end_elapsed = sentence_start_elapsed + (SENTENCE_TIMEOUT_MS as f64 / 1000.0);
                (sentence_start_elapsed.max(0.0), sentence_end_elapsed.max(0.0))
            } else {
                // Fallback to chunk-relative times
                let sentence_start_elapsed = self.current_chunk_start_time + (self.sentence_start_time as f64 / 1000.0);
                let sentence_end_elapsed = sentence_start_elapsed + (SENTENCE_TIMEOUT_MS as f64 / 1000.0);
                (sentence_start_elapsed.max(0.0), sentence_end_elapsed.max(0.0))
            };
            
            let update = TranscriptUpdate {
                text: sentence.trim().to_string(),
                timestamp: format!("{}", format_timestamp(start_elapsed)),
                source: "Mixed Audio".to_string(),
                sequence_id,
                chunk_start_time: self.current_chunk_start_time,
                is_partial: true,
            };
            Some(update)
        } else {
            None
        }
    }
}


async fn audio_collection_task<R: Runtime>(
    mic_stream: Arc<AudioStream>,
    system_stream: Option<Arc<AudioStream>>,
    is_running: Arc<AtomicBool>,
    sample_rate: u32,
    recording_start_time: std::time::Instant,
    app_handle: AppHandle<R>,
) -> Result<(), String> {
    log_info!("Audio collection task started");
    
    let mut mic_receiver = mic_stream.subscribe().await;   
    let mut system_receiver = match &system_stream {
        Some(stream) => Some(stream.subscribe().await),
        None => {
            log_info!("No system audio stream available, using mic only");
            None
        }
    };
    
    if system_receiver.is_some() {
        log_info!("🔊 System audio receiver created successfully");
    }
    
    // Calculate samples based on the actual input sample rate, not Whisper's target rate
    let chunk_samples = (sample_rate as f32 * (CHUNK_DURATION_MS as f32 / 1000.0)) as usize;
    let min_samples = (sample_rate as f32 * (MIN_CHUNK_DURATION_MS as f32 / 1000.0)) as usize;
    log_info!("Audio chunking: target {}ms chunk = {} samples, min {}ms = {} samples @ {}Hz", 
              CHUNK_DURATION_MS, chunk_samples, MIN_CHUNK_DURATION_MS, min_samples, sample_rate);
    let mut current_chunk: Vec<f32> = Vec::with_capacity(chunk_samples);
    let mut last_chunk_time = std::time::Instant::now();
    let chunk_start_time = std::time::Instant::now();
    
    let mut iteration_count = 0;
    let mut last_reconnection_attempt = std::time::Instant::now();
    let mut system_audio_failure_count = 0;
    
    // Ensure audio chunk queue is initialized before starting processing
    while unsafe { AUDIO_CHUNK_QUEUE.is_none() } {
        log_info!("Waiting for audio chunk queue initialization...");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
    
    while is_running.load(Ordering::SeqCst) {
        iteration_count += 1;
        if iteration_count % 5000 == 0 {
            log_info!("🔄 Audio collection task iteration {} (still running)", iteration_count);
        }
        
        if system_receiver.is_none() && system_stream.is_some() {
            let now = std::time::Instant::now();
            if now.duration_since(last_reconnection_attempt).as_secs() >= 5 {
                log_info!("🔄 Attempting to reconnect system audio stream...");
                last_reconnection_attempt = now;

                // Create new receiver while ensuring we don't drop all receivers simultaneously
                // This prevents the broadcast channel from closing due to no active receivers
                if let Some(stream) = system_stream.as_ref() {
                    let new_receiver = stream.subscribe().await;
                    system_receiver = Some(new_receiver);
                    system_audio_failure_count = 0;
                    log_info!("✅ System audio reconnected successfully");
                } else {
                    log_error!("❌ System stream reference is None during reconnection");
                }

                if let Err(e) = app_handle.emit(
                    "system-audio-reconnected",
                    serde_json::json!({
                        "message": "System audio capture restored",
                        "timestamp": now.elapsed().as_secs_f64()
                    }),
                ) {
                    log_warn!("Failed to emit system audio reconnected event: {}", e);
                }
            }
        }
        
        // Collect audio samples
        let mut new_samples = Vec::new();
        let mut mic_samples = Vec::new();
        let mut system_samples = Vec::new();
        
        // Get microphone samples
        let mut mic_chunks_received = 0;
        while let Ok(chunk) = mic_receiver.try_recv() {
            mic_samples.extend(chunk);
            mic_chunks_received += 1;
        }
        
        // 🔧 Microphone Stream Recovery Logic
        if mic_chunks_received == 0 {
            // Simple static counter for microphone failures
            static mut MIC_FAILURE_COUNT: u32 = 0;
            static mut LAST_MIC_RECOVERY_ATTEMPT: u64 = 0;
            
            unsafe {
                MIC_FAILURE_COUNT += 1;
                
                // Log warning every 100 iterations to avoid spam
                if MIC_FAILURE_COUNT % 100 == 0 {
                    log_warn!("⚠️ No microphone chunks received for {} consecutive iterations", MIC_FAILURE_COUNT);
                }
                
                // Attempt microphone stream recovery every 10 seconds
                let now = std::time::Instant::now();
                let current_time = now.elapsed().as_secs();
                
                if current_time - LAST_MIC_RECOVERY_ATTEMPT >= 10 {
                    LAST_MIC_RECOVERY_ATTEMPT = current_time;
                    log_info!("🔄 Attempting microphone stream recovery...");

                    // Create new receiver while ensuring we don't drop all receivers simultaneously
                    // This prevents the broadcast channel from closing due to no active receivers
                    let new_receiver = mic_stream.subscribe().await;
                    mic_receiver = new_receiver;

                    MIC_FAILURE_COUNT = 0;
                    log_info!("✅ Microphone stream recovered successfully");

                    // Emit recovery event
                    if let Err(e) = app_handle.emit("microphone-recovered", serde_json::json!({
                        "message": "Microphone stream restored",
                        "timestamp": now.elapsed().as_secs_f64()
                    })) {
                        log_warn!("Failed to emit microphone recovery event: {}", e);
                    }
                }
            }
        } else {
            // Reset failure count when we receive data
            unsafe {
                static mut MIC_FAILURE_COUNT: u32 = 0;
                MIC_FAILURE_COUNT = 0;
            }
        }
        
        // Get system audio samples (if available)
        if let Some(ref mut receiver) = system_receiver {
            while let Ok(chunk) = receiver.try_recv() {
                system_samples.extend(chunk);
            }
        } else {
            log_debug!("No system audio receiver available");
        }
        
        // 💓 Heartbeat Monitoring for Audio Streams
        if iteration_count % 10000 == 0 {
            // Check microphone stream health (simple check based on recent activity)
            // We can't clone the receiver, so we'll use the failure count as a proxy
            unsafe {
                static mut MIC_FAILURE_COUNT: u32 = 0;
                if MIC_FAILURE_COUNT > 0 {
                    log_warn!("⚠️ Microphone receiver appears inactive ({} failures), will attempt recovery", MIC_FAILURE_COUNT);
                } else {
                    log_debug!("💓 Microphone heartbeat: healthy");
                }
            }
            
            // Check system audio receiver health
            if let Some(ref mut receiver) = system_receiver {
                match receiver.try_recv() {
                    Ok(_) => {
                        // Receiver is working, reset failure count
                        system_audio_failure_count = 0;
                        log_debug!("💓 System audio heartbeat: healthy");
                    }
                    Err(_) => {
                        // Receiver might be inactive, increment failure count
                        system_audio_failure_count += 1;
                        if system_audio_failure_count >= 5 {
                            log_warn!("⚠️ System audio receiver appears inactive ({} failures), will attempt reconnection", system_audio_failure_count);
                            // Force reconnection attempt on next iteration
                            system_receiver = None;
                        }
                    }
                }
            }
        }
        
        // 📊 Audio Stream Status Monitoring and Logging
        if iteration_count % 5000 == 0 {
            // Check microphone status
            let mic_status = if mic_chunks_received > 0 {
                "active"
            } else {
                "inactive"
            };
            
            // Check system audio status
            let system_audio_status = if system_receiver.is_some() {
                "active"
            } else if system_stream.is_some() {
                "disconnected"
            } else {
                "unavailable"
            };
            
            let status_data = serde_json::json!({
                "microphone": {
                    "status": mic_status,
                    "chunks_received": mic_chunks_received
                },
                "system_audio": {
                    "status": system_audio_status,
                    "failure_count": system_audio_failure_count
                },
                "iteration": iteration_count,
                "timestamp": std::time::Instant::now().elapsed().as_secs_f64()
            });
            
            // Emit audio status event
            if let Err(e) = app_handle.emit("audio-status", status_data) {
                log_debug!("Failed to emit audio status event: {}", e);
            }
            
            // Log detailed status
            log_info!("📊 Audio Status - Mic: {} ({} chunks) | System: {} (failures: {}) | Iteration: {}", 
                     mic_status, mic_chunks_received, system_audio_status, system_audio_failure_count, iteration_count);
        }
        
        // Debug audio levels every 1000 iterations to avoid log spam
        if iteration_count % 1000 == 0 && (!mic_samples.is_empty() || !system_samples.is_empty()) {
            let mic_max = mic_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
            let sys_max = system_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
            log_info!("Audio levels - Mic: {} samples, max: {:.4} | System: {} samples, max: {:.4}", 
                     mic_samples.len(), mic_max, system_samples.len(), sys_max);
        }
        
        // 🎵 Smart Audio Processing: Separate handling for mic vs system audio
        let mut processed_mic_samples = Vec::new();
        let mut processed_system_samples = Vec::new();
        
        // Process microphone audio - use BALANCED VAD for better speech preservation
        if !mic_samples.is_empty() {
            // Log mic audio levels for debugging
            let mic_max = mic_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
            let mic_avg = mic_samples.iter().map(|&x| x.abs()).sum::<f32>() / mic_samples.len() as f32;
            
            if iteration_count % 1000 == 0 {
                log_info!("🎤 Mic audio: {} samples, max: {:.6}, avg: {:.6}", mic_samples.len(), mic_max, mic_avg);
            }
            
            // Apply balanced VAD to microphone audio
            match extract_speech_16k(&mic_samples) {
                Ok(speech_samples) if !speech_samples.is_empty() => {
                    processed_mic_samples = speech_samples.clone();
                    log_debug!("🎤 VAD: Mic {} -> {} speech samples", mic_samples.len(), speech_samples.len());
                }
                Ok(_) => {
                    // VAD detected no speech, check if we have actual audio content with balanced threshold
                    if mic_avg > VAD_SILENCE_THRESHOLD {
                        // There's actual audio content, include it despite VAD
                        log_debug!("🔇 VAD: No speech detected but audio present ({:.6}), including mic audio", mic_avg);
                        processed_mic_samples = mic_samples.clone();
                    } else {
                        // Genuine silence, skip it
                        log_debug!("🔇 VAD: Genuine silence detected ({:.6}), skipping mic audio", mic_avg);
                    }
                }
                Err(e) => {
                    log_warn!("⚠️ VAD error on mic audio: {}, using original mic samples", e);
                    processed_mic_samples = mic_samples.clone();
                }
            }
        }
        
        // Process system audio with BALANCED VAD filtering for better quality
        if !system_samples.is_empty() {
            // Log system audio levels for debugging
            let sys_max = system_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
            let sys_avg = system_samples.iter().map(|&x| x.abs()).sum::<f32>() / system_samples.len() as f32;
            
            if iteration_count % 1000 == 0 {
                log_info!("🔊 System audio: {} samples, max: {:.6}, avg: {:.6}", system_samples.len(), sys_max, sys_avg);
            }
            
            // Apply balanced VAD to system audio to filter out silence while preserving content
            match extract_speech_16k(&system_samples) {
                Ok(speech_samples) if !speech_samples.is_empty() => {
                    processed_system_samples = speech_samples.clone();
                    log_debug!("🔊 VAD: System {} -> {} speech samples", system_samples.len(), speech_samples.len());
                }
                Ok(_) => {
                    // VAD detected no speech in system audio, check if we have actual content
                    if sys_avg > VAD_SILENCE_THRESHOLD { // Same threshold as mic audio
                        // There's actual system audio content, include it despite VAD
                        log_debug!("🔇 VAD: No speech detected in system audio but content present ({:.6}), including system audio", sys_avg);
                        processed_system_samples = system_samples.clone();
                    } else {
                        // Genuine silence in system audio, skip it
                        log_debug!("🔇 VAD: Genuine silence detected in system audio ({:.6}), skipping system audio", sys_avg);
                    }
                }
                Err(e) => {
                    log_warn!("⚠️ VAD error on system audio: {}, using original system samples", e);
                    processed_system_samples = system_samples.clone();
                }
            }
        }
        
        // Smart mixing: prioritize system audio, mix with mic speech
        if !processed_system_samples.is_empty() || !processed_mic_samples.is_empty() {
            let max_len = processed_mic_samples.len().max(processed_system_samples.len());
            
            for i in 0..max_len {
                let mic_sample = if i < processed_mic_samples.len() { processed_mic_samples[i] } else { 0.0 };
                let system_sample = if i < processed_system_samples.len() { processed_system_samples[i] } else { 0.0 };
                
                // Smart mixing: system audio gets higher priority, mic speech is mixed in
                let mixed = if system_sample.abs() > 0.01 {
                    // System audio is active, mix with mic speech
                    (mic_sample * 0.6 + system_sample * 0.9).clamp(-1.0, 1.0)
                } else {
                    // Only mic audio, use it as-is
                    mic_sample
                };
                new_samples.push(mixed);
            }
            
            // Log mixing details for debugging
            if iteration_count % 1000 == 0 {
                let mic_max = processed_mic_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
                let sys_max = processed_system_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
                let mixed_max = new_samples.iter().fold(0.0f32, |acc, &x| acc.max(x.abs()));
                log_info!("🎵 Smart Audio Mixing - Mic: {} samples (max: {:.4}) | System: {} samples (max: {:.4}) | Mixed: {} samples (max: {:.4})", 
                         processed_mic_samples.len(), mic_max, processed_system_samples.len(), sys_max, new_samples.len(), mixed_max);
            }
        } else {
            // Fallback: if no processed samples, check if we have original samples
            if !mic_samples.is_empty() {
                log_warn!("⚠️ No processed mic samples, using original mic samples as fallback");
                new_samples.extend(mic_samples.clone());
            }
            if !system_samples.is_empty() {
                log_warn!("⚠️ No processed system samples, using original system samples as fallback");
                new_samples.extend(system_samples.clone());
            }
        }
        
        // Add processed samples to current chunk
        for sample in new_samples {
            current_chunk.push(sample);
        }
        
        // Check if we should create a chunk
        let should_create_chunk = current_chunk.len() >= chunk_samples || 
                                (current_chunk.len() >= min_samples && 
                                 last_chunk_time.elapsed() >= Duration::from_millis(CHUNK_DURATION_MS as u64));
        
        // SMART: Quick silence detection for faster response
        if should_create_chunk && !current_chunk.is_empty() {
            // Calculate audio energy to determine if chunk contains actual speech
            let chunk_rms_energy = current_chunk.iter().map(|&x| x * x).sum::<f32>() / current_chunk.len() as f32;
            let chunk_rms = chunk_rms_energy.sqrt();
            let chunk_avg_level = current_chunk.iter().map(|&x| x.abs()).sum::<f32>() / current_chunk.len() as f32;
            
            // QUICK SILENCE DETECTION: Skip chunks that are clearly silence for faster response
            if chunk_rms < CHUNK_SILENCE_THRESHOLD * 0.3 && chunk_avg_level < CHUNK_AVG_SILENCE_THRESHOLD * 0.3 {
                // Chunk is clearly silence, skip it immediately for faster response
                log_debug!("🔇 Quick silence detection - skipping chunk: RMS: {:.6}, Avg: {:.6} (well below thresholds)", 
                         chunk_rms, chunk_avg_level);
                current_chunk.clear();
                last_chunk_time = std::time::Instant::now();
                continue; // Skip to next iteration
            }
            
            let chunk_duration_ms = (current_chunk.len() as f32 / sample_rate as f32 * 1000.0) as u32;
            log_info!("📦 Creating audio chunk with {} samples (~{}ms, target: {}ms) - RMS: {:.6}, Avg: {:.6}", 
                     current_chunk.len(), chunk_duration_ms, CHUNK_DURATION_MS, chunk_rms, chunk_avg_level);
            
            // Process chunk for Whisper API (VAD already applied to both mic and system audio)
            let whisper_samples = if sample_rate != WHISPER_SAMPLE_RATE {
                log_debug!("Resampling audio from {} to {}", sample_rate, WHISPER_SAMPLE_RATE);
                resample_audio(&current_chunk, sample_rate, WHISPER_SAMPLE_RATE)
            } else {
                current_chunk.clone()
            };

            // ✅ VAD already applied during audio collection - no need to filter again
            log_debug!("📊 Audio chunk ready: {} samples (VAD pre-processed, speech content confirmed)", whisper_samples.len());
            
            // Create audio chunk
            let chunk_id = CHUNK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
            let chunk_timestamp = chunk_start_time.elapsed().as_secs_f64();
            
            // Emit first audio detected event
            static FIRST_AUDIO_EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !FIRST_AUDIO_EMITTED.load(Ordering::SeqCst) {
                FIRST_AUDIO_EMITTED.store(true, Ordering::SeqCst);
                if let Err(e) = app_handle.emit("first-audio-detected", serde_json::json!({
                    "message": "Audio detected - processing for transcription...",
                    "chunk_size": current_chunk.len(),
                    "timestamp": chunk_timestamp
                })) {
                    log_error!("Failed to emit first-audio-detected event: {}", e);
                }
                log_info!("🔊 First audio chunk detected and queued for transcription");
            }
            let audio_chunk = AudioChunk {
                samples: whisper_samples,
                timestamp: chunk_timestamp,
                chunk_id,
                start_time: std::time::Instant::now(),
                recording_start_time,
            };
            
            // Add to queue (with overflow protection)
            unsafe {
                if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                    if let Ok(mut queue_guard) = queue.lock() {
                        // Remove oldest chunks if queue is full
                        while queue_guard.len() >= MAX_AUDIO_QUEUE_SIZE {
                            if let Some(dropped_chunk) = queue_guard.pop_front() {
                                let drop_count = DROPPED_CHUNK_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
                                log_info!("Dropped old audio chunk {} due to queue overflow (total drops: {})", dropped_chunk.chunk_id, drop_count);
                                
                                // // Emit warning event every 10th drop
                                // if drop_count % 10 == 0 {
                                if drop_count == 1 {
                                    let warning_message = format!("Transcription process is very slow. Audio chunk {} was dropped. Please choose a smaller model, or run whisper natively.", dropped_chunk.chunk_id);
                                    log_info!("Emitting chunk-drop-warning event: {}", warning_message);
                                    
                                    if let Err(e) = app_handle.emit("chunk-drop-warning", &warning_message) {
                                        log_error!("Failed to emit chunk-drop-warning event: {}", e);
                                    }
                                }
                            }
                        }
                        queue_guard.push_back(audio_chunk);
                        log_info!("Added chunk {} to queue (queue size: {})", chunk_id, queue_guard.len());
                    }
                }
            }
            
            // Reset for next chunk
            current_chunk.clear();
            last_chunk_time = std::time::Instant::now();
        }
        
        // Small sleep to prevent busy waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

    }

    // Check if recording stopped due to audio channel closure
    if RECORDING_FLAG.load(Ordering::SeqCst) {
        log_error!("⚠️ Audio collection stopped unexpectedly while recording flag is still active!");
        log_error!("This is likely due to audio channel closure after extended operation.");

        // Emit error to frontend to inform user
        if let Err(e) = app_handle.emit("recording-error", "Audio stream disconnected after extended operation. Please restart recording.".to_string()) {
            log_error!("Failed to emit recording error: {}", e);
        }

        // Set recording flag to false to stop showing false recording activity
        RECORDING_FLAG.store(false, Ordering::SeqCst);
    }

    log_info!("Audio collection task ended");
    Ok(())
}

async fn send_audio_chunk(chunk: Vec<f32>, client: &reqwest::Client, stream_url: &str) -> Result<TranscriptResponse, String> {
    log_debug!("Preparing to send audio chunk of size: {}", chunk.len());
    
    // Convert f32 samples to bytes
    let bytes: Vec<u8> = chunk.iter()
        .flat_map(|&sample| {
            let clamped = sample.max(-1.0).min(1.0);
            clamped.to_le_bytes().to_vec()
        })
        .collect();
    
    // Retry configuration
    let max_retries = 3;
    let mut retry_count = 0;
    let mut last_error = String::new();

    while retry_count <= max_retries {
        if retry_count > 0 {
            // Exponential backoff: wait 2^retry_count * 100ms
            let delay = Duration::from_millis(100 * (2_u64.pow(retry_count as u32)));
            log::info!("Retry attempt {} of {}. Waiting {:?} before retry...", 
                      retry_count, max_retries, delay);
            tokio::time::sleep(delay).await;
        }

        // Create fresh multipart form for each attempt since Form can't be reused
        let part = Part::bytes(bytes.clone())
            .file_name("audio.raw")
            .mime_str("audio/x-raw")
            .unwrap();
        let form = Form::new().part("audio", part);

        match client.post(stream_url)
            .multipart(form)
            .send()
            .await {
                Ok(response) => {
                    match response.json::<TranscriptResponse>().await {
                        Ok(transcript) => return Ok(transcript),
                        Err(e) => {
                            last_error = e.to_string();
                            log::error!("Failed to parse response: {}", last_error);
                        }
                    }
                }
                Err(e) => {
                    last_error = e.to_string();
                    log::error!("Request failed: {}", last_error);
                }
            }

        retry_count += 1;
    }

    Err(format!("Failed after {} retries. Last error: {}", max_retries, last_error))
}

async fn transcribe_audio_chunk_whisper_rs(chunk: Vec<f32>) -> Result<TranscriptResponse, String> {
    log_info!("Transcribing audio chunk of size: {} using whisper-rs", chunk.len());
    
    // Use whisper-rs directly for transcription
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            // Ensure model is loaded
            if !engine.is_model_loaded().await {
                log_info!("No whisper model loaded");
                return Err("No whisper model loaded".to_string());
            }
            
            log_debug!("Whisper model is loaded, resampling audio...");
            
            // The audio should already be resampled to 16kHz in audio_collection_task
            // But let's verify and resample if needed
        
        // Check audio levels to help debug silence issues
        let max_amplitude = chunk.iter().map(|&x| x.abs()).fold(0.0_f32, f32::max);
        let avg_amplitude = chunk.iter().map(|&x| x.abs()).sum::<f32>() / chunk.len() as f32;
        log_debug!("Audio levels - Max: {:.6}, Avg: {:.6}", max_amplitude, avg_amplitude);
        
        if max_amplitude < 0.001 {
            log_info!("⚠️ Very low audio levels detected - check microphone input or speak louder");
        }
            
            // For whisper, we need at least 1 second of audio (16000 samples at 16kHz)
            let final_chunk = if chunk.len() < 16000 {
                log_info!("Audio chunk too short ({} samples = {}ms), padding to 1 second", 
                         chunk.len(), (chunk.len() as f32 / 16000.0 * 1000.0) as u32);
                
                // Pad with silence to reach minimum 1 second
                let mut padded_chunk = chunk.clone();
                padded_chunk.resize(16000, 0.0); // Pad with silence
                padded_chunk
            } else {
                log_info!("Audio chunk has {} samples ({}ms) - sufficient for whisper", 
                         chunk.len(), (chunk.len() as f32 / 16000.0 * 1000.0) as u32);
                chunk
            };
            
            // Transcribe using whisper-rs with final audio chunk
            match engine.transcribe_audio(final_chunk).await {
                Ok(text) => {
                    log_info!("Whisper-rs transcription result: {}", text);
                    
                    // Convert to the expected TranscriptResponse format
                    let transcript_response = TranscriptResponse {
                        segments: vec![TranscriptSegment {
                            text: text.clone(),
                            t0: 0.0,
                            t1: 1.0, // Set duration to 1 second to pass the filter
                        }],
                        buffer_size_ms: 1000, // Default buffer size
                    };
                    
                    Ok(transcript_response)
                },
                Err(e) => {
                    log_error!("Whisper-rs transcription failed: {}", e);
                    Err(format!("Whisper transcription failed: {}", e))
                }
            }
        } else {
            log_error!("Whisper engine not initialized");
            Err("Whisper engine not initialized".to_string())
        }
    }
}

async fn transcription_worker<R: Runtime>(
    client: reqwest::Client,
    stream_url: String,
    app_handle: AppHandle<R>,
    worker_id: usize,
) {
    log_info!("Transcription worker {} started", worker_id);
    let mut accumulator = TranscriptAccumulator::new();
    
    // Increment active worker count
    ACTIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
    
    // Worker continues until both recording is stopped AND queue is empty
    loop {
        let is_running = unsafe { 
            if let Some(is_running) = &IS_RUNNING {
                is_running.load(Ordering::SeqCst)
            } else {
                false
            }
        };
        
        let queue_has_chunks = unsafe {
            if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                if let Ok(queue_guard) = queue.lock() {
                    !queue_guard.is_empty()
                } else {
                    false
                }
            } else {
                false
            }
        };
        
        // Continue if recording is active OR if there are still chunks to process
        if !is_running && !queue_has_chunks {
            log_info!("Worker {}: Recording stopped and no more chunks to process, exiting", worker_id);
            break;
        }
        // Check for timeout on current sentence
        if let Some(update) = accumulator.check_timeout() {
            log_info!("Worker {}: Emitting timeout transcript-update event with sequence_id: {}", worker_id, update.sequence_id);
            
            if let Err(e) = app_handle.emit("transcript-update", &update) {
                log_error!("Worker {}: Failed to send timeout transcript update: {}", worker_id, e);
            } else {
                log_info!("Worker {}: Successfully emitted timeout transcript-update event", worker_id);
            }
        }
        
        // Try to get a chunk from the queue
        let audio_chunk = unsafe {
            if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                if let Ok(mut queue_guard) = queue.lock() {
                    queue_guard.pop_front()
                } else {
                    None
                }
            } else {
                None
            }
        };
        
        if let Some(chunk) = audio_chunk {
            log_info!("Worker {}: Processing chunk {} with {} samples", 
                     worker_id, chunk.chunk_id, chunk.samples.len());
            
            // Update last activity timestamp
            LAST_TRANSCRIPTION_ACTIVITY.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                Ordering::SeqCst
            );
            
            // Set chunk context in accumulator
            accumulator.set_chunk_context(chunk.chunk_id, chunk.timestamp, chunk.recording_start_time);
            
            // Send chunk for transcription
            match send_audio_chunk(chunk.samples, &client, &stream_url).await {
                Ok(response) => {
                    log_info!("Worker {}: Received {} transcript segments for chunk {}", 
                             worker_id, response.segments.len(), chunk.chunk_id);
                    
                    for segment in response.segments {
                        log_info!("Worker {}: Processing segment: {} ({} - {})", 
                                 worker_id, segment.text.trim(), format_timestamp(segment.t0 as f64), format_timestamp(segment.t1 as f64));
                        
                        // Add segment to accumulator and check for complete sentence
                        if let Some(update) = accumulator.add_segment(&segment) {
                            log_info!("Worker {}: Emitting transcript-update event with sequence_id: {}", worker_id, update.sequence_id);
                            
                            // Emit the update
                            if let Err(e) = app_handle.emit("transcript-update", &update) {
                                log_error!("Worker {}: Failed to emit transcript update: {}", worker_id, e);
                            } else {
                                log_info!("Worker {}: Successfully emitted transcript-update event", worker_id);
                            }
                        }
                    }
                }
                Err(e) => {
                    log_error!("Worker {}: Transcription error for chunk {}: {}", 
                              worker_id, chunk.chunk_id, e);
                    
                    // Handle error similar to original logic
                    static mut ERROR_COUNT: u32 = 0;
                    static mut LAST_ERROR_TIME: Option<std::time::Instant> = None;
                    
                    unsafe {
                        let now = std::time::Instant::now();
                        if let Some(last_time) = LAST_ERROR_TIME {
                            if now.duration_since(last_time).as_secs() < 30 {
                                ERROR_COUNT += 1;
                            } else {
                                ERROR_COUNT = 1;
                            }
                        } else {
                            ERROR_COUNT = 1;
                        }
                        LAST_ERROR_TIME = Some(now);
                        
                        if ERROR_COUNT == 1 && !ERROR_EVENT_EMITTED {
                            log_error!("Worker {}: Too many transcription errors, stopping recording", worker_id);
                            let error_msg = if e.contains("Failed to connect") || e.contains("Connection refused") {
                                "Transcription service is not available. Please check if the server is running.".to_string()
                            } else if e.contains("timeout") {
                                "Transcription service is not responding. Please check your connection.".to_string()
                            } else {
                                format!("Transcription service error: {}", e)
                            };
                            
                            if let Err(emit_err) = app_handle.emit("transcript-error", error_msg) {
                                log_error!("Worker {}: Failed to emit transcript error: {}", worker_id, emit_err);
                            }
                            
                            ERROR_EVENT_EMITTED = true;
                            RECORDING_FLAG.store(false, Ordering::SeqCst);
                            if let Some(is_running) = &IS_RUNNING {
                                is_running.store(false, Ordering::SeqCst);
                            }
                            ERROR_COUNT = 0;
                            LAST_ERROR_TIME = None;
                            
                            // Clean up audio streams when stopping due to errors
                            tokio::spawn(async {
                                unsafe {
                                    // Stop mic stream if it exists
                                    if let Some(mic_stream) = &MIC_STREAM {
                                        log_info!("Cleaning up microphone stream after transcription error...");
                                        if let Err(e) = mic_stream.stop().await {
                                            log_error!("Error stopping mic stream: {}", e);
                                        } else {
                                            log_info!("Microphone stream cleaned up successfully");
                                        }
                                    }
                                    
                                    // Stop system stream if it exists
                                    if let Some(system_stream) = &SYSTEM_STREAM {
                                        log_info!("Cleaning up system stream after transcription error...");
                                        if let Err(e) = system_stream.stop().await {
                                            log_error!("Error stopping system stream: {}", e);
                                        } else {
                                            log_info!("System stream cleaned up successfully");
                                        }
                                    }
                                    
                                    // Clear the stream references
                                    MIC_STREAM = None;
                                    SYSTEM_STREAM = None;
                                    IS_RUNNING = None;
                                    TRANSCRIPTION_TASK = None;
                                    AUDIO_COLLECTION_TASK = None;
                                    AUDIO_CHUNK_QUEUE = None;
                                }
                            });
                            
                            return;
                        }
                    }
                }
            }
        } else {
            // No chunks available, sleep briefly
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    
    // Emit any remaining transcript when worker stops
    if let Some(update) = accumulator.check_timeout() {
        log_info!("Worker {}: Emitting final transcript-update event with sequence_id: {}", worker_id, update.sequence_id);
        
        if let Err(e) = app_handle.emit("transcript-update", &update) {
            log_error!("Worker {}: Failed to send final transcript update: {}", worker_id, e);
        } else {
            log_info!("Worker {}: Successfully emitted final transcript-update event", worker_id);
        }
    }
    
    // Also flush any partial sentence that might not have been emitted
    if !accumulator.current_sentence.is_empty() {
        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let update = TranscriptUpdate {
            text: accumulator.current_sentence.trim().to_string(),
            timestamp: format!("{}", format_timestamp(accumulator.current_chunk_start_time + (accumulator.sentence_start_time as f64 / 1000.0))),
            source: "Mixed Audio".to_string(),
            sequence_id,
            chunk_start_time: accumulator.current_chunk_start_time,
            is_partial: true,
        };
        log_info!("Worker {}: Flushing final partial sentence: {} with sequence_id: {}", worker_id, update.text, update.sequence_id);
        
        if let Err(e) = app_handle.emit("transcript-update", &update) {
            log_error!("Worker {}: Failed to send final partial transcript: {}", worker_id, e);
        } else {
            log_info!("Worker {}: Successfully emitted final partial transcript-update event", worker_id);
        }
    }
    
    // Decrement active worker count
    ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
    
    // Check if this was the last active worker and emit completion event
    if ACTIVE_WORKERS.load(Ordering::SeqCst) == 0 {
        let should_emit = unsafe {
            if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                if let Ok(queue_guard) = queue.lock() {
                    queue_guard.is_empty()
                } else {
                    false
                }
            } else {
                false
            }
        };
        
        if should_emit {
            log_info!("All workers finished and queue is empty, waiting for pending segments...");
            
            // Wait a bit to ensure all pending segments are emitted
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            log_info!("Emitting transcription-complete event");
            if let Err(e) = app_handle.emit("transcription-complete", ()) {
                log_error!("Failed to emit transcription-complete event: {}", e);
            }
        }
    }
    
    log_info!("Transcription worker {} ended", worker_id);
}

async fn whisper_rs_transcription_worker<R: Runtime>(
    app_handle: AppHandle<R>,
    worker_id: usize,
) {
    log_info!("Whisper-rs transcription worker {} started", worker_id);
    let mut accumulator = TranscriptAccumulator::new();
    
    // Increment active worker count
    ACTIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
    
    loop {
        // Check if recording is still active
        if !is_recording() {
            log_info!("Worker {}: Recording stopped, checking for remaining chunks...", worker_id);
            
            // Process any remaining chunks in the queue
            let chunks_in_queue = unsafe {
                AUDIO_CHUNK_QUEUE.as_ref().map_or(0, |queue| {
                    queue.lock().unwrap().len()
                })
            };
            
            if chunks_in_queue == 0 {
                log_info!("Worker {}: No more chunks to process, shutting down", worker_id);
                break;
            }
        }
        
        // Check for timeout on current sentence (this handles both timeout and partial emissions)
        if let Some(update) = accumulator.check_timeout() {
            log_info!("Whisper-rs Worker {}: Emitting timeout transcript-update event with sequence_id: {}", worker_id, update.sequence_id);
            
            if let Err(e) = app_handle.emit("transcript-update", &update) {
                log_error!("Whisper-rs Worker {}: Failed to send timeout transcript update: {}", worker_id, e);
            } else {
                log_info!("Whisper-rs Worker {}: Successfully emitted timeout transcript-update event", worker_id);
            }
        }
        
        // Get chunk from queue
        let chunk = unsafe {
            AUDIO_CHUNK_QUEUE.as_ref().and_then(|queue| {
                let mut queue_lock = queue.lock().unwrap();
                queue_lock.pop_front()
            })
        };
        
        if let Some(chunk) = chunk {
            log_info!("Worker {}: Processing audio chunk {} with {} samples", 
                     worker_id, chunk.chunk_id, chunk.samples.len());
            
            // Emit first processing event
            static FIRST_PROCESSING_EMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !FIRST_PROCESSING_EMITTED.load(Ordering::SeqCst) {
                FIRST_PROCESSING_EMITTED.store(true, Ordering::SeqCst);
                if let Err(e) = app_handle.emit("transcription-started", serde_json::json!({
                    "message": "Transcription started - processing your first audio chunk...",
                    "worker_id": worker_id,
                    "chunk_id": chunk.chunk_id
                })) {
                    log_error!("Failed to emit transcription-started event: {}", e);
                }
                log_info!("🎯 First transcription started by worker {}", worker_id);
            }
            
            // Update last activity timestamp
            LAST_TRANSCRIPTION_ACTIVITY.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                Ordering::SeqCst
            );
            
            // Set chunk context in accumulator
            accumulator.set_chunk_context(chunk.chunk_id, chunk.timestamp, chunk.recording_start_time);
            
            // Send chunk for transcription using whisper-rs
            match transcribe_audio_chunk_whisper_rs(chunk.samples).await {
                Ok(response) => {
                    log_info!("Worker {}: Received {} transcript segments for chunk {}", 
                             worker_id, response.segments.len(), chunk.chunk_id);
                    
                    for segment in response.segments {
                        log_info!("Worker {}: Processing segment: {} ({} - {})", 
                                 worker_id, segment.text.trim(), format_timestamp(segment.t0 as f64), format_timestamp(segment.t1 as f64));
                        
                        // Add segment to accumulator and check for complete sentence
                        if let Some(update) = accumulator.add_segment(&segment) {
                            log_info!("Worker {}: Emitting transcript-update event with sequence_id: {}", worker_id, update.sequence_id);
                            
                            // Emit the update
                            if let Err(e) = app_handle.emit("transcript-update", &update) {
                                log_error!("Worker {}: Failed to emit transcript update: {}", worker_id, e);
                            } else {
                                log_info!("Worker {}: Successfully emitted transcript-update event", worker_id);
                            }
                        }
                    }
                }
                Err(e) => {
                    log_error!("Worker {}: Whisper-rs transcription error for chunk {}: {}", 
                              worker_id, chunk.chunk_id, e);
                    
                    // Handle error similar to original logic but for whisper-rs
                    static mut ERROR_COUNT: u32 = 0;
                    static mut LAST_ERROR_TIME: Option<std::time::Instant> = None;
                    
                    unsafe {
                        let now = std::time::Instant::now();
                        if let Some(last_time) = LAST_ERROR_TIME {
                            if now.duration_since(last_time).as_secs() < 30 {
                                ERROR_COUNT += 1;
                            } else {
                                ERROR_COUNT = 1;
                            }
                        } else {
                            ERROR_COUNT = 1;
                        }
                        LAST_ERROR_TIME = Some(now);
                        
                        if ERROR_COUNT >= 5 && !ERROR_EVENT_EMITTED {
                            log_error!("Worker {}: Too many whisper-rs transcription errors, stopping recording", worker_id);
                            
                            let error_msg = "Local whisper transcription failed multiple times. Please check the model.".to_string();
                            
                            if let Err(e) = app_handle.emit("recording-error", error_msg) {
                                log_error!("Worker {}: Failed to emit recording error: {}", worker_id, e);
                            }
                            ERROR_EVENT_EMITTED = true;
                            break;
                        }
                    }
                }
            }
        } else {
            // No chunks available, wait briefly
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    // Also flush any partial sentence that might not have been emitted
    if !accumulator.current_sentence.is_empty() {
        let sequence_id = SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let update = TranscriptUpdate {
            text: accumulator.current_sentence.trim().to_string(),
            timestamp: format!("{}", format_timestamp(accumulator.current_chunk_start_time + (accumulator.sentence_start_time as f64 / 1000.0))),
            source: "Mixed Audio".to_string(),
            sequence_id,
            chunk_start_time: accumulator.current_chunk_start_time,
            is_partial: true,
        };
        log_info!("Worker {}: Flushing final partial sentence: {} with sequence_id: {}", worker_id, update.text, update.sequence_id);
        
        if let Err(e) = app_handle.emit("transcript-update", &update) {
            log_error!("Worker {}: Failed to send final partial transcript: {}", worker_id, e);
        } else {
            log_info!("Worker {}: Successfully emitted final partial transcript-update event", worker_id);
        }
    }
    
    // Decrement active worker count
    ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
    
    // Check if this was the last active worker and emit completion event
    if ACTIVE_WORKERS.load(Ordering::SeqCst) == 0 {
        let should_emit = unsafe {
            if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                if let Ok(queue_guard) = queue.lock() {
                    queue_guard.is_empty()
                } else {
                    false
                }
            } else {
                false
            }
        };
        
        if should_emit {
            log_info!("All whisper-rs workers finished and queue is empty, waiting for pending segments...");
            
            // Wait a bit to ensure all pending segments are emitted
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            log_info!("Emitting transcription-complete event");
            if let Err(e) = app_handle.emit("transcription-complete", ()) {
                log_error!("Failed to emit transcription-complete event: {}", e);
            }
        }
    }
    
    log_info!("Whisper-rs transcription worker {} ended", worker_id);
}

#[tauri::command]
async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    log_info!("Attempting to start recording...");

    if let Err(e) = app.emit("recording-startup-progress", serde_json::json!({ "stage": "initializing", "message": "Starting...", "progress": 0 })) {
        log_error!("Failed to emit progress event: {}", e);
    }

    if is_recording() {
        return Err("Recording already in progress".to_string());
    }

    // INITIALIZE STATE AND BUFFERS (NON-BLOCKING)
    DROPPED_CHUNK_COUNTER.store(0, Ordering::SeqCst);
    RECORDING_FLAG.store(true, Ordering::SeqCst);
    unsafe {
        ERROR_EVENT_EMITTED = false;
        RECORDING_START_TIME = Some(std::time::Instant::now());
        MIC_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        SYSTEM_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        AUDIO_CHUNK_QUEUE = Some(Arc::new(Mutex::new(VecDeque::new())));
    }
    LAST_TRANSCRIPTION_ACTIVITY.store(0, Ordering::SeqCst);
    ACTIVE_WORKERS.store(0, Ordering::SeqCst);
    tray::update_tray_menu(&app);
    log_info!("Initialized recording state and buffers.");

    // LOAD CONFIGURATION AND MODELS
    app.emit("recording-startup-progress", serde_json::json!({ "stage": "loading-config", "message": "Loading configuration...", "progress": 20 })).ok();
    
    let transcript_config_result = api::api_get_transcript_config(app.clone(), None).await;
    let (use_local_whisper, whisper_model) = match transcript_config_result {
        Ok(Some(config)) => {
            let is_local = config.provider == "localWhisper";
            let model = if config.model.is_empty() { "small".to_string() } else { config.model };
            (is_local, model)
        },
        _ => {
            log_info!("Could not get transcript config, defaulting to localWhisper");
            (true, "small".to_string())
        }
    };
    log_info!("🔧 Transcription provider decision - use_local_whisper: {}", use_local_whisper);

    if use_local_whisper {
        app.emit("recording-startup-progress", serde_json::json!({ "stage": "loading-model", "message": format!("Loading {} model...", whisper_model), "progress": 40 })).ok();
        
        unsafe {
            let engine = WHISPER_ENGINE.as_ref().ok_or("Whisper engine not initialized")?;
            if !engine.is_model_loaded().await {
                log_info!("Loading {} model for transcription...", whisper_model);
                // TODO:Calling discover_models as workaround for updating the available_models, whihch is used in
                // load_model;
                engine.discover_models().await;

                engine.load_model(&whisper_model).await.map_err(|e| {
                    log_error!("Failed to load whisper model {}: {}", whisper_model, e);
                    format!("Failed to load whisper model: {}", e)
                })?;
                log_info!("✅ Loaded {} model for transcription...", whisper_model);
            } else {
                // If model is loaeded then ensure it is the model from the config 
                if let Some(current_loaded_model) = engine.get_current_model().await {
                    if current_loaded_model != whisper_model{
                        engine.load_model(&whisper_model).await.map_err(|e| {
                                log_error!("Failed to switch whisper model {}: {}", whisper_model, e);
                                format!("Failed to switch whisper model: {}", e)
                            })?;
                        log_info!("Model switched to {}",whisper_model);
                    }
                }
            }
        }
    }
// else {
//         let mut server_url = match transcript_config_result {
//             Ok(Some(config)) => {
//                 config.
//             }
//         }
//     }

    

    // Let the producers porduce first; due to failed to send audio data bug
    // INITIALIZE REAL-TIME AUDIO STREAMS (PRODUCERS) 
    app.emit("recording-startup-progress", serde_json::json!({ "stage": "detecting-devices", "message": "Detecting audio devices...", "progress": 80 })).ok();

    let mic_device = Arc::new(default_input_device().map_err(|e| format!("Failed to get default input device: {}", e))?);
    let system_device = default_output_device().ok(); // Treat system audio as optional

    let is_running = Arc::new(AtomicBool::new(true));

    let mic_stream = Arc::new(AudioStream::from_device(mic_device.clone(), is_running.clone()).await.map_err(|e| format!("Failed to create microphone stream: {}", e))?);
    let system_stream = if let Some(dev) = system_device {
        match AudioStream::from_device(Arc::new(dev), is_running.clone()).await {
            Ok(stream) => Some(Arc::new(stream)),
            Err(e) => {
                log_warn!("Failed to create system audio stream, continuing without it: {}", e);
                None
            }
        }
    } else {
        None
    };
    log_info!("✅ Audio streams created successfully.");

    // SPAWN CONSUMER AND WORKER TASKS
    let sample_rate = mic_stream.device_config.sample_rate().0;
    let recording_start_time = unsafe { RECORDING_START_TIME.unwrap_or_else(std::time::Instant::now) };

    // Spawn audio collection task (THE CONSUMER)
    let audio_collection_handle = tokio::spawn({
        let mic_stream_clone = mic_stream.clone();
        let system_stream_clone = system_stream.clone();
        let is_running_clone = is_running.clone();
        let app_handle_clone = app.clone();
        async move {
            if let Err(e) = audio_collection_task(mic_stream_clone, system_stream_clone, is_running_clone, sample_rate, recording_start_time, app_handle_clone).await {
                log_error!("Audio collection task error: {}", e);
            }
        }
    });

    // Spawn transcription workers
    const NUM_WORKERS: usize = 3;
    let mut worker_handles = Vec::new();
    if use_local_whisper {
        for worker_id in 0..NUM_WORKERS {
            worker_handles.push(tokio::spawn(whisper_rs_transcription_worker(app.clone(), worker_id)));
        }
    } else {
        let client = reqwest::Client::new();
        let stream_url = format!("{}/stream", TRANSCRIPT_SERVER_URL);
        for worker_id in 0..NUM_WORKERS {
            worker_handles.push(tokio::spawn(transcription_worker(client.clone(), stream_url.clone(), app.clone(), worker_id)));
        }
    }

    // Store all handles and streams in the global state.
    unsafe {
        MIC_STREAM = Some(mic_stream);
        SYSTEM_STREAM = system_stream.clone(); // Keep a clone for the event payload
        IS_RUNNING = Some(is_running);
        AUDIO_COLLECTION_TASK = Some(audio_collection_handle);
        if let Some(first_worker) = worker_handles.into_iter().next() {
            TRANSCRIPTION_TASK = Some(first_worker);
        }
    }
    
    app.emit("recording-startup-progress", serde_json::json!({ "stage": "ready", "message": "Recording started!", "progress": 100 })).ok();

    let mut devices = vec![format!("🎤 {}", mic_device.name)];
    if system_stream.is_some() {
        devices.push("🔊 System Audio".to_string());
    }
    app.emit("recording-started", serde_json::json!({
        "devices": devices,
        "provider": if use_local_whisper { "local" } else { "http" },
        "model": whisper_model,
        "message": "Recording is now active."
    })).ok();
    
    log_info!("🎯 Recording started successfully with {} devices", devices.len());
    
    let _ = app.notification().builder()
        .title("Meetily")
        .body("Recording has started. Please inform others in the meeting.")
        .show();
    
    Ok(())
}

#[tauri::command]
async fn stop_recording<R: Runtime>(app: AppHandle<R>, args: RecordingArgs) -> Result<(), String> {
    log_info!("Attempting to stop recording...");
    
    // Only check recording state if we haven't already started stopping
    if !RECORDING_FLAG.load(Ordering::SeqCst) {
        log_info!("Recording is already stopped");
        return Ok(());
    }

    // Check minimum recording duration
    let elapsed_ms = unsafe {
        RECORDING_START_TIME
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0)
    };

    if elapsed_ms < MIN_RECORDING_DURATION_MS {
        let remaining = MIN_RECORDING_DURATION_MS - elapsed_ms;
        log_info!("Waiting for minimum recording duration ({} ms remaining)...", remaining);
        tokio::time::sleep(Duration::from_millis(remaining)).await;
    }

    // First set the recording flag to false to prevent new data from being processed
    RECORDING_FLAG.store(false, Ordering::SeqCst);
    log_info!("Recording flag set to false");
    
    tray::update_tray_menu(&app);
    
    unsafe {
        // Stop the running flag for audio streams first
        if let Some(is_running) = &IS_RUNNING {
            // Set running flag to false first to stop the tokio task
            is_running.store(false, Ordering::SeqCst);
            log_info!("Set recording flag to false, waiting for streams to stop...");
            
            // Stop the audio collection task
            if let Some(task) = AUDIO_COLLECTION_TASK.take() {
                log_info!("Stopping audio collection task...");
                task.abort();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            
            // Wait for transcription workers to complete processing remaining chunks
            if TRANSCRIPTION_TASK.is_some() {
                log_info!("Waiting for transcription workers to complete...");
                
                // Wait for all workers to finish processing remaining chunks
                let mut wait_time = 0;
                const MAX_WAIT_TIME: u64 = 30000; // 30 seconds max
                const CHECK_INTERVAL: u64 = 100; // Check every 100ms
                
                while wait_time < MAX_WAIT_TIME {
                    let active_count = ACTIVE_WORKERS.load(Ordering::SeqCst);
                    let queue_size = unsafe {
                        if let Some(queue) = &AUDIO_CHUNK_QUEUE {
                            if let Ok(queue_guard) = queue.lock() {
                                queue_guard.len()
                            } else {
                                0
                            }
                        } else {
                            0
                        }
                    };
                    
                    log_info!("Worker cleanup status: {} active workers, {} chunks in queue", active_count, queue_size);
                    
                    // If no active workers and queue is empty, we're done
                    if active_count == 0 && queue_size == 0 {
                        log_info!("All workers completed and queue is empty");
                        break;
                    }
                    
                    tokio::time::sleep(Duration::from_millis(CHECK_INTERVAL)).await;
                    wait_time += CHECK_INTERVAL;
                }
                
                if wait_time >= MAX_WAIT_TIME {
                    log_error!("Transcription worker cleanup timeout after {} seconds", MAX_WAIT_TIME / 1000);
                }
                
                // Now stop the transcription task
                if let Some(task) = TRANSCRIPTION_TASK.take() {
                    log_info!("Stopping transcription task...");
                    task.abort();
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
            
            // Give the tokio task time to finish and release its references
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Stop mic stream if it exists
            if let Some(mic_stream) = &MIC_STREAM {
                log_info!("Stopping microphone stream...");
                if let Err(e) = mic_stream.stop().await {
                    log_error!("Error stopping mic stream: {}", e);
                } else {
                    log_info!("Microphone stream stopped successfully");
                }
            }
            
            // Stop system stream if it exists
            if let Some(system_stream) = &SYSTEM_STREAM {
                log_info!("Stopping system stream...");
                if let Err(e) = system_stream.stop().await {
                    log_error!("Error stopping system stream: {}", e);
                } else {
                    log_info!("System stream stopped successfully");
                }
            }
            
            // Clear the stream references
            MIC_STREAM = None;
            SYSTEM_STREAM = None;
            IS_RUNNING = None;
            TRANSCRIPTION_TASK = None;
            AUDIO_COLLECTION_TASK = None;
            AUDIO_CHUNK_QUEUE = None;
            
            // Give streams time to fully clean up
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    // Get final buffers
    let mic_data = unsafe {
        if let Some(buffer) = &MIC_BUFFER {
            if let Ok(guard) = buffer.lock() {
                guard.clone()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };
    
    let system_data = unsafe {
        if let Some(buffer) = &SYSTEM_BUFFER {
            if let Ok(guard) = buffer.lock() {
                guard.clone()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };
    /*
    // Mix the audio and convert to 16-bit PCM
    let max_len = mic_data.len().max(system_data.len());
    let mut mixed_data = Vec::with_capacity(max_len);
    
    for i in 0..max_len {
        let mic_sample = if i < mic_data.len() { mic_data[i] } else { 0.0 };
        let system_sample = if i < system_data.len() { system_data[i] } else { 0.0 };
        mixed_data.push((mic_sample + system_sample) * 0.5);
    }

    if mixed_data.is_empty() {
        log_error!("No audio data captured");
        return Err("No audio data captured".to_string());
    }
    
    log_info!("Mixed {} audio samples", mixed_data.len());
    
    // Resample the audio to 16kHz for Whisper compatibility
    let original_sample_rate = 48000; // Assuming original sample rate is 48kHz
    if original_sample_rate != WHISPER_SAMPLE_RATE {
        log_info!("Resampling audio from {} Hz to {} Hz for Whisper compatibility", 
                 original_sample_rate, WHISPER_SAMPLE_RATE);
        mixed_data = resample_audio(&mixed_data, original_sample_rate, WHISPER_SAMPLE_RATE);
        log_info!("Resampled to {} samples", mixed_data.len());
    }
    
    // Convert to 16-bit PCM samples
    let mut bytes = Vec::with_capacity(mixed_data.len() * 2);
    for &sample in mixed_data.iter() {
        let value = (sample.max(-1.0).min(1.0) * 32767.0) as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    
    log_info!("Converted to {} bytes of PCM data", bytes.len());

    // Create WAV header
    let data_size = bytes.len() as u32;
    let file_size = 36 + data_size;
    let sample_rate = WHISPER_SAMPLE_RATE; // Use Whisper's required sample rate (16000 Hz)
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
    
    log_info!("Created WAV file with {} bytes total", wav_file.len());
    */
    // Create the save directory if it doesn't exist
    if let Some(parent) = std::path::Path::new(&args.save_path).parent() {
        if !parent.exists() {
            log_info!("Creating directory: {:?}", parent);
            if let Err(e) = std::fs::create_dir_all(parent) {
                let err_msg = format!("Failed to create save directory: {}", e);
                log_error!("{}", err_msg);
                return Err(err_msg);
            }
        }
    }

    /*
    // Save the recording
    log_info!("Saving recording to: {}", args.save_path);
    match fs::write(&args.save_path, wav_file) {
        Ok(_) => log_info!("Successfully saved recording"),
        Err(e) => {
            let err_msg = format!("Failed to save recording: {}", e);
            log_error!("{}", err_msg);
            return Err(err_msg);
        }
    }
    */
    
    // Clean up
    unsafe {
        MIC_BUFFER = None;
        SYSTEM_BUFFER = None;
        MIC_STREAM = None;
        SYSTEM_STREAM = None;
        IS_RUNNING = None;
        RECORDING_START_TIME = None;
        TRANSCRIPTION_TASK = None;
        AUDIO_COLLECTION_TASK = None;
        AUDIO_CHUNK_QUEUE = None;
        let engine = WHISPER_ENGINE.as_ref().ok_or("Whisper engine not initialized")?;
        if  engine.unload_model().await {
            log_info!("Model is unloaded successfully on Stop");
        }
        
    }

    // Send a system notification indicating recording has stopped
    let _ = app.notification().builder().title("Meetily").body("Recording stopped").show();
    
    Ok(())
}

#[tauri::command]
fn is_recording() -> bool {
    RECORDING_FLAG.load(Ordering::SeqCst)
}

#[tauri::command]
fn get_transcription_status() -> TranscriptionStatus {
    let chunks_in_queue = unsafe {
        if let Some(queue) = &AUDIO_CHUNK_QUEUE {
            if let Ok(queue_guard) = queue.lock() {
                queue_guard.len()
            } else {
                0
            }
        } else {
            0
        }
    };
    
    let is_processing = ACTIVE_WORKERS.load(Ordering::SeqCst) > 0 || chunks_in_queue > 0;
    
    let last_activity_ms = LAST_TRANSCRIPTION_ACTIVITY.load(Ordering::SeqCst);
    let current_time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let elapsed_since_activity = if last_activity_ms > 0 {
        current_time_ms.saturating_sub(last_activity_ms)
    } else {
        u64::MAX
    };
    
    TranscriptionStatus {
        chunks_in_queue,
        is_processing,
        last_activity_ms: elapsed_since_activity,
    }
}

#[tauri::command]
fn read_audio_file(file_path: String) -> Result<Vec<u8>, String> {
    match std::fs::read(&file_path) {
        Ok(data) => Ok(data),
        Err(e) => Err(format!("Failed to read audio file: {}", e))
    }
}

#[tauri::command]
async fn save_transcript(file_path: String, content: String) -> Result<(), String> {
    log::info!("Saving transcript to: {}", file_path);

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }
    }

    // Write content to file
    std::fs::write(&file_path, content)
        .map_err(|e| format!("Failed to write transcript: {}", e))?;

    log::info!("Transcript saved successfully");
    Ok(())
}

// Analytics commands
#[tauri::command]
async fn init_analytics() -> Result<(), String> {
    let config = AnalyticsConfig {
        api_key:"phc_cohhHPgfQfnNWl33THRRpCftuRtWx2k5svtKrkpFb04".to_string(),
        host: Some("https://us.i.posthog.com".to_string()),
        enabled: true ,
    };
    
    let client = Arc::new(AnalyticsClient::new(config).await);
    
    unsafe {
        ANALYTICS_CLIENT = Some(client);
    }
    
    Ok(())
}

#[tauri::command]
async fn disable_analytics() -> Result<(), String> {
    unsafe {
        ANALYTICS_CLIENT = None;
    }
    Ok(())
}


#[tauri::command]
async fn track_event(event_name: String, properties: Option<std::collections::HashMap<String, String>>) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_event(&event_name, properties).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn identify_user(user_id: String, properties: Option<std::collections::HashMap<String, String>>) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.identify(user_id, properties).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_meeting_started(meeting_id: String, meeting_title: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_meeting_started(&meeting_id, &meeting_title).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_recording_started(meeting_id: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_recording_started(&meeting_id).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_recording_stopped(meeting_id: String, duration_seconds: Option<u64>) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_recording_stopped(&meeting_id, duration_seconds).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_meeting_deleted(meeting_id: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_meeting_deleted(&meeting_id).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_search_performed(query: String, results_count: usize) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_search_performed(&query, results_count).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_settings_changed(setting_type: String, new_value: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_settings_changed(&setting_type, &new_value).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_feature_used(feature_name: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_feature_used(&feature_name).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn is_analytics_enabled() -> bool {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.is_enabled()
        } else {
            false
        }
    }
}

// Enhanced analytics commands for Phase 1
#[tauri::command]
async fn start_analytics_session(user_id: String) -> Result<String, String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.start_session(user_id).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn end_analytics_session() -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.end_session().await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}



#[tauri::command]
async fn track_daily_active_user() -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_daily_active_user().await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_user_first_launch() -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_user_first_launch().await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

// Summary generation analytics commands
#[tauri::command]
async fn track_summary_generation_started(model_provider: String, model_name: String, transcript_length: usize) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_summary_generation_started(&model_provider, &model_name, transcript_length).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_summary_generation_completed(model_provider: String, model_name: String, success: bool, duration_seconds: Option<u64>, error_message: Option<String>) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_summary_generation_completed(&model_provider, &model_name, success, duration_seconds, error_message.as_deref()).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_summary_regenerated(model_provider: String, model_name: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_summary_regenerated(&model_provider, &model_name).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_model_changed(old_provider: String, old_model: String, new_provider: String, new_model: String) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_model_changed(&old_provider, &old_model, &new_provider, &new_model).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn track_custom_prompt_used(prompt_length: usize) -> Result<(), String> {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.track_custom_prompt_used(prompt_length).await
        } else {
            Err("Analytics client not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn is_analytics_session_active() -> bool {
    unsafe {
        if let Some(client) = &ANALYTICS_CLIENT {
            client.is_session_active().await
        } else {
            false
        }
    }
}

// Helper function to convert stereo to mono
fn stereo_to_mono(stereo: &[i16]) -> Vec<i16> {
    let mut mono = Vec::with_capacity(stereo.len() / 2);
    for chunk in stereo.chunks_exact(2) {
        let left = chunk[0] as i32;
        let right = chunk[1] as i32;
        let combined = ((left + right) / 2) as i16;
        mono.push(combined);
    }
    mono
}

pub fn run() {
    log::set_max_level(log::LevelFilter::Info);
    
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|_app| {
            log::info!("Application setup complete");
            
            // Initialize system tray
            if let Err(e) = tray::create_tray(_app.handle()) {
                log::error!("Failed to create system tray: {}", e);
            }

            // Trigger microphone permission request on startup
            if let Err(e) = audio::core::trigger_audio_permission() {
                log::error!("Failed to trigger audio permission: {}", e);
            }
            
            // Initialize Whisper engine on startup
            tauri::async_runtime::spawn(async {
                if let Err(e) = whisper_init().await {
                    log::error!("Failed to initialize Whisper engine on startup: {}", e);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_recording,
            stop_recording,
            is_recording,
            get_transcription_status,
            read_audio_file,
            save_transcript,
            init_analytics,
            disable_analytics,
            track_event,
            identify_user,
            track_meeting_started,
            track_recording_started,
            track_recording_stopped,
            track_meeting_deleted,
            track_search_performed,
            track_settings_changed,
            track_feature_used,
            is_analytics_enabled,
            start_analytics_session,
            end_analytics_session,
            track_daily_active_user,
            track_user_first_launch,
            is_analytics_session_active,
            track_summary_generation_started,
            track_summary_generation_completed,
            track_summary_regenerated,
            track_model_changed,
            track_custom_prompt_used,
            
            whisper_init,
            whisper_get_available_models,
            whisper_load_model,
            whisper_get_current_model,
            whisper_is_model_loaded,
            whisper_transcribe_audio,
            whisper_get_models_directory,
            whisper_download_model,
            whisper_cancel_download,
            
            ollama::get_ollama_models,
            api::api_get_meetings,
            api::api_search_transcripts,
            api::api_get_profile,
            api::api_save_profile,
            api::api_update_profile,
            api::api_get_model_config,
            api::api_save_model_config,
            api::api_get_api_key,
            api::api_get_transcript_config,
            api::api_save_transcript_config,
            api::api_get_transcript_api_key,
            api::api_delete_meeting,
            api::api_get_meeting,
            api::api_save_meeting_title,
            api::api_save_meeting_summary,
            api::api_get_summary,
            api::api_save_transcript,
            api::api_process_transcript,
    
            api::test_backend_connection,
            api::debug_backend_connection,
            api::open_external_url,
            openrouter::get_openrouter_models,
            console_utils::show_console,
            console_utils::hide_console,
            console_utils::toggle_console,
        ])
        .plugin(tauri_plugin_store::Builder::new().build())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Helper function to resample audio
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }
    
    let ratio = to_rate as f32 / from_rate as f32;
    let new_len = (samples.len() as f32 * ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);
    
    for i in 0..new_len {
        let src_idx = (i as f32 / ratio) as usize;
        if src_idx < samples.len() {
            resampled.push(samples[src_idx]);
        }
    }
    
    resampled
}



// Whisper model management commands
#[tauri::command]
async fn whisper_init() -> Result<(), String> {
    unsafe {
        if WHISPER_ENGINE.is_some() {
            return Ok(());
        }
        
        let engine = WhisperEngine::new()
            .map_err(|e| format!("Failed to initialize whisper engine: {}", e))?;
        WHISPER_ENGINE = Some(Arc::new(engine));
        log_info!("Whisper engine initialized successfully");
        Ok(())
    }
}

#[tauri::command]
async fn whisper_get_available_models() -> Result<Vec<ModelInfo>, String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            engine.discover_models().await
                .map_err(|e| format!("Failed to discover models: {}", e))
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_load_model(model_name: String) -> Result<(), String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            engine.load_model(&model_name).await
                .map_err(|e| format!("Failed to load model: {}", e))
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_get_current_model() -> Result<Option<String>, String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            Ok(engine.get_current_model().await)
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_is_model_loaded() -> Result<bool, String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            Ok(engine.is_model_loaded().await)
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_transcribe_audio(audio_data: Vec<f32>) -> Result<String, String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            engine.transcribe_audio(audio_data).await
                .map_err(|e| format!("Transcription failed: {}", e))
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command] 
async fn whisper_get_models_directory() -> Result<String, String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            let path = engine.get_models_directory().await;
            Ok(path.to_string_lossy().to_string())
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_download_model(app_handle: tauri::AppHandle, model_name: String) -> Result<(), String> {
    use tauri::Manager;
    
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            // Create progress callback that emits events
            let app_handle_clone = app_handle.clone();
            let model_name_clone = model_name.clone();
            
            let progress_callback = Box::new(move |progress: u8| {
                log_info!("Download progress for {}: {}%", model_name_clone, progress);
                
                // Emit download progress event
                if let Err(e) = app_handle_clone.emit("model-download-progress", serde_json::json!({
                    "modelName": model_name_clone,
                    "progress": progress
                })) {
                    log_error!("Failed to emit download progress event: {}", e);
                }
            });
            
            let result = engine.download_model(&model_name, Some(progress_callback)).await;
            
            match result {
                Ok(()) => {
                    // Emit completion event
                    if let Err(e) = app_handle.emit("model-download-complete", serde_json::json!({
                        "modelName": model_name
                    })) {
                        log_error!("Failed to emit download complete event: {}", e);
                    }
                    Ok(())
                },
                Err(e) => {
                    // Emit error event
                    if let Err(emit_e) = app_handle.emit("model-download-error", serde_json::json!({
                        "modelName": model_name,
                        "error": e.to_string()
                    })) {
                        log_error!("Failed to emit download error event: {}", emit_e);
                    }
                    Err(format!("Failed to download model: {}", e))
                }
            }
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn whisper_cancel_download(model_name: String) -> Result<(), String> {
    unsafe {
        if let Some(engine) = &WHISPER_ENGINE {
            engine.cancel_download(&model_name).await
                .map_err(|e| format!("Failed to cancel download: {}", e))
        } else {
            Err("Whisper engine not initialized".to_string())
        }
    }
}

#[tauri::command]
async fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
    list_audio_devices().await.map_err(|e| format!("Failed to list audio devices: {}", e))
}

#[tauri::command]
async fn start_recording_with_devices(
    mic_device_name: Option<String>, 
    system_audio_enabled: bool,
    save_path: String
) -> Result<String, String> {
    log_info!("Starting recording with custom devices - Mic: {:?}, System: {}", mic_device_name, system_audio_enabled);
    
    // Get devices based on user selection
    let mic_device = if let Some(name) = mic_device_name {
        log_info!("Using selected mic device: {}", name);
        parse_audio_device(&name)
            .map_err(|e| format!("Failed to get selected mic device '{}': {}", name, e))?
    } else {
        log_info!("Using default mic device");
        default_input_device()
            .map_err(|e| format!("Failed to get default mic device: {}", e))?
    };
    
    let system_device = if system_audio_enabled {
        match default_output_device() {
            Ok(device) => {
                log_info!("✅ System audio enabled: {} (type: {:?})", device.name, device.device_type);
                Some(device)
            }
            Err(e) => {
                log_error!("⚠️ System audio requested but failed to get device: {}", e);
                None
            }
        }
    } else {
        log_info!("❌ System audio disabled by user");
        None
    };
    
    start_recording_with_custom_devices(mic_device, system_device, save_path).await
}

async fn start_recording_with_custom_devices(
    mic_device: AudioDevice,
    system_device: Option<AudioDevice>,
    save_path: String
) -> Result<String, String> {
    use std::sync::atomic::Ordering;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use std::time::Duration;
    
    // For now, set up recording with the selected devices
    // This mirrors the logic from the main recording function but with custom devices
    
    let mic_device_arc = Arc::new(mic_device);
    let system_device_arc = system_device.map(Arc::new);
    
    log_info!("🎙️ Starting custom recording with mic: {} | system: {:?}", 
              mic_device_arc.name, 
              system_device_arc.as_ref().map(|d| d.name.as_str()));
              
    // Set up the recording session similar to existing implementation
    unsafe {
        if RECORDING_FLAG.load(Ordering::SeqCst) {
            return Err("Recording already in progress".to_string());
        }
        
        RECORDING_FLAG.store(true, Ordering::SeqCst);
        SEQUENCE_COUNTER.store(0, Ordering::SeqCst);
        CHUNK_ID_COUNTER.store(0, Ordering::SeqCst);
        RECORDING_START_TIME = Some(std::time::Instant::now());
        
        // Initialize buffers
        MIC_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        SYSTEM_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        AUDIO_CHUNK_QUEUE = Some(Arc::new(Mutex::new(VecDeque::new())));
        
        // Create placeholder recording session
        let session_id = format!("custom_{}", uuid::Uuid::new_v4());
        
        log_info!("✅ Custom recording session started: {}", session_id);
        log_info!("📍 Mic: {} | System Audio: {}", 
                 mic_device_arc.name,
                 system_device_arc.as_ref().map(|d| d.name.as_str()).unwrap_or("Disabled"));
                 
        Ok(session_id)
    }
}
