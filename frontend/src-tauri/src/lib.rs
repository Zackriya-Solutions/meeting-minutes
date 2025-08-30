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

use audio::{
    default_input_device, default_output_device, AudioStream,
    encode_single_audio,
};
use ollama::{OllamaModel};
use analytics::{AnalyticsClient, AnalyticsConfig};
use utils::format_timestamp;
use tauri::{Runtime, AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use log::{info as log_info, error as log_error, debug as log_debug};
use reqwest::multipart::{Form, Part};
use tokio::sync::mpsc;

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
const MAX_AUDIO_QUEUE_SIZE: usize = 10; // Maximum number of chunks in queue

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
    system_stream: Arc<AudioStream>,
    is_running: Arc<AtomicBool>,
    sample_rate: u32,
    recording_start_time: std::time::Instant,
    app_handle: AppHandle<R>,
) -> Result<(), String> {
    log_info!("Audio collection task started");
    
    let mut mic_receiver = mic_stream.subscribe().await;
    let mut system_receiver = system_stream.subscribe().await;
    
    let chunk_samples = (WHISPER_SAMPLE_RATE as f32 * (CHUNK_DURATION_MS as f32 / 1000.0)) as usize;
    let min_samples = (WHISPER_SAMPLE_RATE as f32 * (MIN_CHUNK_DURATION_MS as f32 / 1000.0)) as usize;
    let mut current_chunk: Vec<f32> = Vec::with_capacity(chunk_samples);
    let mut last_chunk_time = std::time::Instant::now();
    let chunk_start_time = std::time::Instant::now();
    
    while is_running.load(Ordering::SeqCst) {
        // Collect audio samples
        let mut new_samples = Vec::new();
        let mut mic_samples = Vec::new();
        let mut system_samples = Vec::new();
        
        // Get microphone samples
        while let Ok(chunk) = mic_receiver.try_recv() {
            log_debug!("Received {} mic samples", chunk.len());
            mic_samples.extend(chunk);
        }
        
        // Get system audio samples
        while let Ok(chunk) = system_receiver.try_recv() {
            log_debug!("Received {} system samples", chunk.len());
            system_samples.extend(chunk);
        }
        
        // Mix samples (80% mic, 20% system)
        let max_len = mic_samples.len().max(system_samples.len());
        for i in 0..max_len {
            let mic_sample = if i < mic_samples.len() { mic_samples[i] } else { 0.0 };
            let system_sample = if i < system_samples.len() { system_samples[i] } else { 0.0 };
            new_samples.push((mic_sample * 0.8) + (system_sample * 0.2));
        }
        
        // Add samples to current chunk
        for sample in new_samples {
            current_chunk.push(sample);
        }
        
        // Check if we should create a chunk
        let should_create_chunk = current_chunk.len() >= chunk_samples || 
                                (current_chunk.len() >= min_samples && 
                                 last_chunk_time.elapsed() >= Duration::from_millis(CHUNK_DURATION_MS as u64));
        
        if should_create_chunk && !current_chunk.is_empty() {
            // Process chunk for Whisper API
            let whisper_samples = if sample_rate != WHISPER_SAMPLE_RATE {
                log_debug!("Resampling audio from {} to {}", sample_rate, WHISPER_SAMPLE_RATE);
                resample_audio(&current_chunk, sample_rate, WHISPER_SAMPLE_RATE)
            } else {
                current_chunk.clone()
            };
            
            // Create audio chunk
            let chunk_id = CHUNK_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
            let chunk_timestamp = chunk_start_time.elapsed().as_secs_f64();
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

#[tauri::command]
async fn start_recording<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    log_info!("Attempting to start recording...");
    
    if is_recording() {
        log_error!("Recording already in progress");
        return Err("Recording already in progress".to_string());
    }

    // Reset dropped chunk counter for new recording session
    DROPPED_CHUNK_COUNTER.store(0, Ordering::SeqCst);
    log_info!("Reset dropped chunk counter for new recording session");

    // Stop any existing tasks first
    unsafe {
        if let Some(task) = AUDIO_COLLECTION_TASK.take() {
            log_info!("Stopping existing audio collection task...");
            task.abort();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if let Some(task) = TRANSCRIPTION_TASK.take() {
            log_info!("Stopping existing transcription task...");
            task.abort();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // Initialize recording flag and buffers
    RECORDING_FLAG.store(true, Ordering::SeqCst);
    log_info!("Recording flag set to true");
    
    // Reset error event flag and activity tracking for new recording session
    unsafe {
        ERROR_EVENT_EMITTED = false;
    }
    
    // Reset transcription activity tracking
    LAST_TRANSCRIPTION_ACTIVITY.store(0, Ordering::SeqCst);
    ACTIVE_WORKERS.store(0, Ordering::SeqCst);


    // Store recording start time
    unsafe {
        RECORDING_START_TIME = Some(std::time::Instant::now());
    }

    // Initialize audio buffers and queue
    unsafe {
        MIC_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        SYSTEM_BUFFER = Some(Arc::new(Mutex::new(Vec::new())));
        AUDIO_CHUNK_QUEUE = Some(Arc::new(Mutex::new(VecDeque::new())));
        log_info!("Initialized audio buffers and chunk queue");
    }
    
    // Get default devices
    let mic_device = Arc::new(default_input_device().map_err(|e| {
        log_error!("Failed to get default input device: {}", e);
        e.to_string()
    })?);
    
    let system_device = Arc::new(default_output_device().map_err(|e| {
        log_error!("Failed to get default output device: {}", e);
        e.to_string()
    })?);
    
    // Create audio streams
    let is_running = Arc::new(AtomicBool::new(true));
    
    // Create microphone stream
    let mic_stream = AudioStream::from_device(mic_device.clone(), is_running.clone())
        .await
        .map_err(|e| {
            log_error!("Failed to create microphone stream: {}", e);
            e.to_string()
        })?;
    let mic_stream = Arc::new(mic_stream);
    
    // Create system audio stream
    let system_stream = AudioStream::from_device(system_device.clone(), is_running.clone())
        .await
        .map_err(|e| {
            log_error!("Failed to create system stream: {}", e);
            e.to_string()
        })?;
    let system_stream = Arc::new(system_stream);

    unsafe {
        MIC_STREAM = Some(mic_stream.clone());
        SYSTEM_STREAM = Some(system_stream.clone());
        IS_RUNNING = Some(is_running.clone());
    }
    
    // Create HTTP client for transcription
    let client = reqwest::Client::new();
    
    // Use hardcoded transcript server URL
    let stream_url = format!("{}/stream", TRANSCRIPT_SERVER_URL);
    log_info!("Using hardcoded stream URL: {}", stream_url);

    let device_config = mic_stream.device_config.clone();
    let sample_rate = device_config.sample_rate().0;
    let channels = device_config.channels();
    
    log_info!("Mic config: {} Hz, {} channels", sample_rate, channels);
    
    // Get recording start time for proper elapsed time calculation
    let recording_start_time = unsafe { 
        RECORDING_START_TIME.unwrap_or_else(|| std::time::Instant::now()) 
    };
    
    // Start audio collection task
    let audio_collection_handle = {
        let mic_stream_clone = mic_stream.clone();
        let system_stream_clone = system_stream.clone();
        let is_running_clone = is_running.clone();
        let app_handle_clone = app.clone();
        tokio::spawn(async move {
            if let Err(e) = audio_collection_task(
                mic_stream_clone,
                system_stream_clone,
                is_running_clone,
                sample_rate,
                recording_start_time,
                app_handle_clone,
            ).await {
                log_error!("Audio collection task error: {}", e);
            }
        })
    };
    
    // Start multiple transcription workers
    const NUM_WORKERS: usize = 3;
    let mut worker_handles = Vec::new();
    
    for worker_id in 0..NUM_WORKERS {
        let client_clone = client.clone();
        let stream_url_clone = stream_url.clone();
        let app_handle_clone = app.clone();
        
        let worker_handle = tokio::spawn(async move {
            transcription_worker(
                client_clone,
                stream_url_clone,
                app_handle_clone,
                worker_id,
            ).await;
        });
        
        worker_handles.push(worker_handle);
    }
    
    // Store task handles globally
    unsafe {
        AUDIO_COLLECTION_TASK = Some(audio_collection_handle);
        // Store the first worker as the main transcription task for compatibility
        if let Some(first_worker) = worker_handles.into_iter().next() {
            TRANSCRIPTION_TASK = Some(first_worker);
        }
    }

    let _ = app
    .notification()
    .builder()
    .title("Meetily")
    .body("Recording has started. Please inform others in the meeting.")
    .icon("icon.png")
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
