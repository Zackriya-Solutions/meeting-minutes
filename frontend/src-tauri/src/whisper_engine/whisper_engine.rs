use std::path::{PathBuf};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};
use reqwest::Client;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading { progress: u8 },
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_mb: u32,
    pub accuracy: String,
    pub speed: String,
    pub status: ModelStatus,
    pub description: String,
}

pub struct WhisperEngine {
    models_dir: PathBuf,
    current_context: Arc<RwLock<Option<WhisperContext>>>,
    current_model: Arc<RwLock<Option<String>>>,
    available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    // State tracking for smart logging
    last_transcription_was_short: Arc<RwLock<bool>>,
    short_audio_warning_logged: Arc<RwLock<bool>>,
}

impl WhisperEngine {
    pub fn new() -> Result<Self> {
        // Use backend/models directory to preserve existing models
        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
        
        // Development: Use frontend/models or backend directories
        // Production: Use system directories like ~/Library/Application Support/Meetily/models
        let models_dir = if cfg!(debug_assertions) {
            // Development mode - try frontend and backend directories
            if current_dir.join("models").exists() {
                current_dir.join("models")
            } else if current_dir.join("../models").exists() {
                current_dir.join("../models")
            } else if current_dir.join("backend/whisper-server-package/models").exists() {
                current_dir.join("backend/whisper-server-package/models")
            } else if current_dir.join("../backend/whisper-server-package/models").exists() {
                current_dir.join("../backend/whisper-server-package/models")
            } else {
                // Create models directory in current directory for development
                current_dir.join("models")
            }
        } else {
            // Production mode - use system directories
            dirs::data_dir()
                .or_else(|| dirs::home_dir())
                .ok_or_else(|| anyhow!("Could not find system data directory"))?
                .join("Meetily")
                .join("models")
        };
        
        log::info!("WhisperEngine using models directory: {}", models_dir.display());
        log::info!("Debug mode: {}", cfg!(debug_assertions));
        
        let engine = Self {
            models_dir,
            current_context: Arc::new(RwLock::new(None)),
            current_model: Arc::new(RwLock::new(None)),
            available_models: Arc::new(RwLock::new(HashMap::new())),
            // Initialize state tracking
            last_transcription_was_short: Arc::new(RwLock::new(false)),
            short_audio_warning_logged: Arc::new(RwLock::new(false)),
        };
        
        Ok(engine)
    }
    
    pub async fn discover_models(&self) -> Result<Vec<ModelInfo>> {
        let models_dir = &self.models_dir;
        let mut models = Vec::new();
                // Using standard ggerganov/whisper.cpp GGML models
        let model_configs = [
            ("tiny", "ggml-tiny.bin", 39, "Decent", "Very Fast", "Fastest processing, good for real-time use"),
            ("base", "ggml-base.bin", 142, "Good", "Fast", "Good balance of speed and accuracy"), 
            ("small", "ggml-small.bin", 466, "Good", "Medium", "Better accuracy, moderate speed"),
            ("medium", "ggml-medium.bin", 1420, "High", "Slow", "High accuracy for professional use"),
            ("large-v3", "ggml-large-v3.bin", 2870, "High", "Slow", "Best accuracy, latest large model"),
            ("large-v3-turbo", "ggml-large-v3-turbo.bin", 809, "High", "Medium", "Best accuracy with improved speed"),
        ];
        
        for (name, filename, size_mb, accuracy, speed, description) in model_configs {
            let model_path = models_dir.join(filename);
            let status = if model_path.exists() {
                ModelStatus::Available
            } else {
                ModelStatus::Missing
            };
            
            let model_info = ModelInfo {
                name: name.to_string(),
                path: model_path,
                size_mb,
                accuracy: accuracy.to_string(),
                speed: speed.to_string(),
                status,
                description: description.to_string(),
            };
            
            models.push(model_info);
        }
        
        // Update internal cache
        let mut available_models = self.available_models.write().await;
        available_models.clear();
        for model in &models {
            available_models.insert(model.name.clone(), model.clone());
        }
        
        Ok(models)
    }
    
    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        let models = self.available_models.read().await;
        let model_info = models.get(model_name)
            .ok_or_else(|| anyhow!("Model {} not found", model_name))?;
        
        match model_info.status {
            ModelStatus::Available => {
                log::info!("Loading model: {}", model_name);
                
                // Create context parameters optimized for streaming accuracy
                let context_param = WhisperContextParameters {
                    use_gpu: true,
                    gpu_device: 0,
                    flash_attn: false, // Disabled due to potential crashes in streaming
                    ..Default::default()
                };
                
                // Load whisper context with parameters
                let ctx = WhisperContext::new_with_params(&model_info.path.to_string_lossy(), context_param)
                    .map_err(|e| anyhow!("Failed to load model {}: {}", model_name, e))?;
                
                // Update current context and model
                *self.current_context.write().await = Some(ctx);
                *self.current_model.write().await = Some(model_name.to_string());
                
                log::info!("Successfully loaded model: {} with GPU acceleration", model_name);
                Ok(())
            },
            ModelStatus::Missing => {
                Err(anyhow!("Model {} is not downloaded", model_name))
            },
            ModelStatus::Downloading { .. } => {
                Err(anyhow!("Model {} is currently downloading", model_name))
            },
            ModelStatus::Error(ref err) => {
                Err(anyhow!("Model {} has error: {}", model_name, err))
            }
        }
    }

    pub async fn unload_model(&self) -> bool  {
        let mut ctx_guard = self.current_context.write().await;
        let unloaded = ctx_guard.take().is_some();
        if unloaded {
            log::info!("📉Whisper model unloaded");
        }

        let mut model_name_guard = self.current_model.write().await;
        model_name_guard.take();

        unloaded
    }

    pub async fn get_current_model(&self) -> Option<String> {
        self.current_model.read().await.clone()
    }
    
    pub async fn is_model_loaded(&self) -> bool {
        self.current_context.read().await.is_some()
    }
    
    // Enhanced function to clean repetitive text patterns and meaningless outputs
    fn clean_repetitive_text(text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        // Check for obviously meaningless patterns first
        if Self::is_meaningless_output(text) {
            log::info!("Detected meaningless output, returning empty: '{}'", text);
            return String::new();
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 3 {
            return text.to_string();
        }

        // Enhanced repetition detection with sliding window
        let cleaned_words = Self::remove_word_repetitions(&words);

        // Remove phrase repetitions with more sophisticated detection
        let cleaned_words = Self::remove_phrase_repetitions(&cleaned_words);

        // Check for overall repetition ratio
        let final_text = cleaned_words.join(" ");
        if Self::calculate_repetition_ratio(&final_text) > 0.7 {
            log::info!("High repetition ratio detected, filtering out: '{}'", final_text);
            return String::new();
        }

        final_text
    }

    // Check for obviously meaningless patterns
    fn is_meaningless_output(text: &str) -> bool {
        let text_lower = text.to_lowercase();

        // Check for common meaningless patterns
        let meaningless_patterns = [
            "thank you for watching",
            "thanks for watching",
            "like and subscribe",
            "music playing",
            "applause",
            "laughter",
            "um um um",
            "uh uh uh",
            "ah ah ah",
        ];

        for pattern in &meaningless_patterns {
            if text_lower.contains(pattern) {
                return true;
            }
        }

        // Check if text is mostly the same character or very short repetitive patterns
        let unique_chars: HashSet<char> = text.chars().collect();
        if unique_chars.len() <= 3 && text.len() > 10 {
            return true;
        }

        false
    }

    // Enhanced word repetition removal
    fn remove_word_repetitions<'a>(words: &'a [&'a str]) -> Vec<&'a str> {
        let mut cleaned_words = Vec::new();
        let mut i = 0;

        while i < words.len() {
            let current_word = words[i];
            let mut repeat_count = 1;

            // Count consecutive repetitions of the same word
            while i + repeat_count < words.len() && words[i + repeat_count] == current_word {
                repeat_count += 1;
            }

            // Be more aggressive: if word is repeated 2+ times, only keep one instance
            if repeat_count >= 2 {
                cleaned_words.push(current_word);
                i += repeat_count;
            } else {
                cleaned_words.push(current_word);
                i += 1;
            }
        }

        cleaned_words
    }

    // Enhanced phrase repetition removal with variable length detection
    fn remove_phrase_repetitions<'a>(words: &'a [&'a str]) -> Vec<&'a str> {
        if words.len() < 4 {
            return words.to_vec();
        }

        let mut final_words = Vec::new();
        let mut i = 0;

        while i < words.len() {
            let mut phrase_found = false;

            // Check for 2-word to 5-word phrase repetitions
            for phrase_len in 2..=std::cmp::min(5, (words.len() - i) / 2) {
                if i + phrase_len * 2 <= words.len() {
                    let phrase1 = &words[i..i + phrase_len];
                    let phrase2 = &words[i + phrase_len..i + phrase_len * 2];

                    if phrase1 == phrase2 {
                        // Add the phrase once and skip the repetition
                        final_words.extend_from_slice(phrase1);
                        i += phrase_len * 2;
                        phrase_found = true;
                        break;
                    }
                }
            }

            if !phrase_found {
                final_words.push(words[i]);
                i += 1;
            }
        }

        final_words
    }

    // Calculate repetition ratio in text
    fn calculate_repetition_ratio(text: &str) -> f32 {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 4 {
            return 0.0;
        }

        let mut word_counts = HashMap::new();
        for word in &words {
            *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
        }

        let total_words = words.len() as f32;
        let repeated_words: usize = word_counts.values().map(|&count| if count > 1 { count - 1 } else { 0 }).sum();

        repeated_words as f32 / total_words
    }
    
    pub async fn transcribe_audio(&self, audio_data: Vec<f32>) -> Result<String> {
        let ctx_lock = self.current_context.read().await;
        let ctx = ctx_lock.as_ref()
            .ok_or_else(|| anyhow!("No model loaded. Please load a model first."))?;

        // Optimize parameters for streaming accuracy based on research
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,      // Increased beam size for better accuracy
            patience: 1.0      // Balance between speed and accuracy
        });

        // Configure for maximum streaming accuracy
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Enhanced settings for better accuracy and anti-repetition
        params.set_suppress_blank(true);         // Suppress blank outputs
        params.set_suppress_nst(true);           // Suppress non-speech tokens
        params.set_temperature(0.4);             // Increased from 0.2 to 0.4 for better diversity and less repetition
        params.set_max_initial_ts(1.0);          // Allow for better initial timestamp detection
        params.set_entropy_thold(2.4);           // Entropy threshold for segment detection
        params.set_logprob_thold(-1.0);          // Stricter threshold (was -2.0) to filter low-confidence outputs
        params.set_no_speech_thold(0.6);         // Stricter threshold (was 0.2) to better detect silence

        // Anti-repetition settings to prevent infinite loops
        params.set_max_len(224);                 // Shorter maximum sequence length to prevent repetition
        params.set_single_segment(false);        // Allow multiple segments but controlled

        // Note: compression_ratio_threshold would be ideal but not available in current whisper-rs
        // This would help detect repetitive outputs: params.set_compression_ratio_threshold(2.4);

        // Duration-based optimization is handled by beam search parameters
        let duration_seconds = audio_data.len() as f64 / 16000.0; // Assuming 16kHz
        let is_short_audio = duration_seconds < 1.0;

        // Smart logging based on audio duration and previous states
        let mut should_log_transcription = true;
        let mut should_log_short_warning = false;

        if is_short_audio {
            let last_was_short = *self.last_transcription_was_short.read().await;
            let warning_logged = *self.short_audio_warning_logged.read().await;

            if !warning_logged {
                should_log_short_warning = true;
                *self.short_audio_warning_logged.write().await = true;
            }

            // Only log transcription start if it's the first short audio or previous wasn't short
            should_log_transcription = !last_was_short;

            *self.last_transcription_was_short.write().await = true;
        } else {
            let last_was_short = *self.last_transcription_was_short.read().await;

            // Always log when transitioning from short to normal audio
            if last_was_short {
                log::info!("Audio duration normalized, resuming transcription");
                *self.short_audio_warning_logged.write().await = false;
            }

            *self.last_transcription_was_short.write().await = false;
        }

        if should_log_short_warning {
            log::warn!("Audio duration is short ({:.1}s < 1.0s). Consider padding the input audio with silence. Further short audio warnings will be suppressed.", duration_seconds);
        }

        if should_log_transcription {
            log::info!("Starting optimized transcription of {} samples ({:.1}s duration)",
                      audio_data.len(), duration_seconds);
        }
        let mut state = ctx.create_state()?;
        state.full(params, &audio_data)?;

        // Extract text with improved segment handling
        let num_segments = state.full_n_segments()?;

        // Only log segment completion for longer audio or when something meaningful was detected
        if should_log_transcription || num_segments > 0 {
            log::debug!("Transcription completed with {} segments", num_segments);
        }
        let mut result = String::new();

        for i in 0..num_segments {
            let segment_text = state.full_get_segment_text(i)?;

            // Get segment timestamps for debugging
            if let (Ok(start_time), Ok(end_time)) = (
                state.full_get_segment_t0(i),
                state.full_get_segment_t1(i)
            ) {
                log::debug!("Segment {} ({:.2}s-{:.2}s): '{}'",
                          i, start_time as f64 / 100.0, end_time as f64 / 100.0, segment_text);
            } else {
                log::debug!("Segment {}: '{}'", i, segment_text);
            }

            // Clean and append segment text
            let cleaned_text = segment_text.trim();
            if !cleaned_text.is_empty() {
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(cleaned_text);
            }
        }

        let final_result = result.trim().to_string();

        // Check for repetition loops and clean them up
        let cleaned_result = Self::clean_repetitive_text(&final_result);

        // Smart logging for transcription results
        if cleaned_result.is_empty() {
            if should_log_transcription {
                log::debug!("Transcription result is empty - no speech detected");
            }
        } else {
            if cleaned_result != final_result {
                log::info!("Cleaned repetitive transcription: '{}' -> '{}'", final_result, cleaned_result);
            }
            // Always log successful transcriptions with meaningful results
            log::info!("Final transcription result: '{}'", cleaned_result);
        }

        Ok(cleaned_result)
    }
    
    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_dir.clone()
    }
    
    pub async fn download_model(&self, model_name: &str, progress_callback: Option<Box<dyn Fn(u8) + Send>>) -> Result<()> {
        log::info!("Starting download for model: {}", model_name);
        
        // Official ggerganov/whisper.cpp model URLs from Hugging Face
        let model_url = match model_name {
            "tiny" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
            "base" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            "small" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
            "medium" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
            "large-v3" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
            "large-v3-turbo" => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
            _ => return Err(anyhow!("Unsupported model: {}", model_name))
        };
        
        log::info!("Model URL for {}: {}", model_name, model_url);
        
        let filename = format!("ggml-{}.bin", model_name);
        let file_path = self.models_dir.join(&filename);
        
        log::info!("Downloading to file path: {}", file_path.display());
        
        // Create models directory if it doesn't exist
        if !self.models_dir.exists() {
            fs::create_dir_all(&self.models_dir).await
                .map_err(|e| anyhow!("Failed to create models directory: {}", e))?;
        }
        
        // Update model status to downloading
        {
            let mut models = self.available_models.write().await;
            if let Some(model_info) = models.get_mut(model_name) {
                model_info.status = ModelStatus::Downloading { progress: 0 };
            }
        }
        
        log::info!("Creating HTTP client and starting request...");
        let client = Client::new();
        
        log::info!("Sending GET request to: {}", model_url);
        let response = client.get(model_url).send().await
            .map_err(|e| anyhow!("Failed to start download: {}", e))?;
        
        log::info!("Received response with status: {}", response.status());
        if !response.status().is_success() {
            return Err(anyhow!("Download failed with status: {}", response.status()));
        }
        
        let total_size = response.content_length().unwrap_or(0);
        log::info!("Response successful, content length: {} bytes ({:.1} MB)", total_size, total_size as f64 / (1024.0 * 1024.0));
        
        if total_size == 0 {
            log::warn!("Content length is 0 or unknown - download may not show accurate progress");
        }
        
        let mut file = fs::File::create(&file_path).await
            .map_err(|e| anyhow!("Failed to create file: {}", e))?;
        
        log::info!("File created successfully at: {}", file_path.display());
        
        // Use bytes().await with timeout for better progress reporting
        log::info!("Starting to read response body (this may take a while for large models)...");
        log::info!("Expected size: {:.1} MB", total_size as f64 / (1024.0 * 1024.0));
        
        let bytes = tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 minute timeout
            response.bytes()
        ).await
        .map_err(|_| {
            log::error!("Download timed out after 5 minutes!");
            anyhow!("Download timed out after 5 minutes")
        })?
        .map_err(|e| {
            log::error!("Failed to download bytes: {}", e);
            anyhow!("Failed to download bytes: {}", e)
        })?;
        
        let total_len = bytes.len();
        log::info!("Downloaded {} bytes into memory, writing to file with progress...", total_len);
        
        // Write with progress reporting
        let chunk_size = 1024 * 1024; // 1MB chunks
        let mut written = 0;
        
        for chunk_start in (0..total_len).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size).min(total_len);
            let chunk = &bytes[chunk_start..chunk_end];
            
            file.write_all(chunk).await
                .map_err(|e| anyhow!("Failed to write chunk to file: {}", e))?;
            
            written += chunk.len();
            
            // Calculate progress
            let progress = ((written as f64 / total_len as f64) * 100.0) as u8;
            
            // Always report progress for debugging
            log::info!("Write progress: {}% ({} MB / {} MB)", 
                     progress, 
                     written / (1024 * 1024),
                     total_len / (1024 * 1024));
            
            // Update progress
            {
                let mut models = self.available_models.write().await;
                if let Some(model_info) = models.get_mut(model_name) {
                    model_info.status = ModelStatus::Downloading { progress };
                }
            }
            
            if let Some(ref callback) = progress_callback {
                callback(progress);
            }
            
            // Small delay to make progress visible
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        // Ensure 100% progress is always reported
        {
            let mut models = self.available_models.write().await;
            if let Some(model_info) = models.get_mut(model_name) {
                model_info.status = ModelStatus::Downloading { progress: 100 };
            }
        }
        
        if let Some(ref callback) = progress_callback {
            callback(100);
        }
        
        file.flush().await
            .map_err(|e| anyhow!("Failed to flush file: {}", e))?;
        
        log::info!("Download completed for model: {}", model_name);
        
        // Update model status to available
        {
            let mut models = self.available_models.write().await;
            if let Some(model_info) = models.get_mut(model_name) {
                model_info.status = ModelStatus::Available;
                model_info.path = file_path.clone();
            }
        }
        
        Ok(())
    }
    
    pub async fn cancel_download(&self, model_name: &str) -> Result<()> {
        // Update the status to cancelled
        let mut models = self.available_models.write().await;
        if let Some(model_info) = models.get_mut(model_name) {
            model_info.status = ModelStatus::Error("Download cancelled".to_string());
        }
        Ok(())
    }
}
