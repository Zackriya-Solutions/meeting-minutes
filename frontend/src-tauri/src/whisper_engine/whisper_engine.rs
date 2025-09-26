use std::path::{PathBuf};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use whisper_rs::{WhisperContext, WhisperContextParameters, WhisperState, FullParams, SamplingStrategy};
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
                
                // Create context parameters with hardware acceleration (following owhisper pattern)
                let context_param = WhisperContextParameters {
                    use_gpu: true,
                    gpu_device: 0,
                    flash_attn: false, // Disabled on macOS due to potential crashes
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
    
    pub async fn transcribe_audio(&self, audio_data: Vec<f32>) -> Result<String> {
        let ctx_lock = self.current_context.read().await;
        let ctx = ctx_lock.as_ref()
            .ok_or_else(|| anyhow!("No model loaded. Please load a model first."))?;
        
        // Create transcription parameters
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        
        // Configure for high quality transcription
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        
        // Run transcription
        log::info!("Starting transcription of {} samples", audio_data.len());
        let mut state = ctx.create_state()?;
        state.full(params, &audio_data)?;
        
        // Extract text
        let num_segments = state.full_n_segments()?;
        log::info!("Transcription completed with {} segments", num_segments);
        let mut result = String::new();
        
        for i in 0..num_segments {
            let segment_text = state.full_get_segment_text(i)?;
            log::info!("Segment {}: '{}'", i, segment_text);
            result.push_str(&segment_text);
            if i < num_segments - 1 {
                result.push(' ');
            }
        }
        
        if result.trim().is_empty() {
            log::info!("Transcription result is empty - no speech detected");
        } else {
            log::info!("Final transcription result: '{}'", result);
        }
        
        Ok(result.trim().to_string())
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
