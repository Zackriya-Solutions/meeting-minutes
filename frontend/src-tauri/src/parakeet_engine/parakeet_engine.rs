use crate::parakeet_engine::model::ParakeetModel;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

/// Quantization type for Parakeet models
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QuantizationType {
    FP32,   // Full precision
    Int8,   // 8-bit integer quantization (faster)
}

impl Default for QuantizationType {
    fn default() -> Self {
        QuantizationType::Int8 // Default to int8 for best performance
    }
}

/// Model status for Parakeet models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading { progress: u8 },
    Error(String),
    Corrupted { file_size: u64, expected_min_size: u64 },
}

/// Information about a Parakeet model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: PathBuf,
    pub size_mb: u32,
    pub quantization: QuantizationType,
    pub speed: String,     // Performance description
    pub status: ModelStatus,
    pub description: String,
}

#[derive(Debug)]
pub enum ParakeetEngineError {
    ModelNotLoaded,
    ModelNotFound(String),
    TranscriptionFailed(String),
    DownloadFailed(String),
    IoError(std::io::Error),
    Other(String),
}

impl std::fmt::Display for ParakeetEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParakeetEngineError::ModelNotLoaded => write!(f, "No Parakeet model loaded"),
            ParakeetEngineError::ModelNotFound(name) => write!(f, "Model '{}' not found", name),
            ParakeetEngineError::TranscriptionFailed(err) => write!(f, "Transcription failed: {}", err),
            ParakeetEngineError::DownloadFailed(err) => write!(f, "Download failed: {}", err),
            ParakeetEngineError::IoError(err) => write!(f, "IO error: {}", err),
            ParakeetEngineError::Other(err) => write!(f, "Error: {}", err),
        }
    }
}

impl std::error::Error for ParakeetEngineError {}

impl From<std::io::Error> for ParakeetEngineError {
    fn from(err: std::io::Error) -> Self {
        ParakeetEngineError::IoError(err)
    }
}

pub struct ParakeetEngine {
    models_dir: PathBuf,
    current_model: Arc<RwLock<Option<ParakeetModel>>>,
    current_model_name: Arc<RwLock<Option<String>>>,
    available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    cancel_download_flag: Arc<RwLock<Option<String>>>, // Model name being cancelled
    // Active downloads tracking to prevent concurrent downloads
    active_downloads: Arc<RwLock<HashSet<String>>>, // Set of models currently being downloaded
}

impl ParakeetEngine {
    /// Create a new Parakeet engine with optional custom models directory
    pub fn new_with_models_dir(models_dir: Option<PathBuf>) -> Result<Self> {
        let models_dir = if let Some(dir) = models_dir {
            dir.join("parakeet") // Parakeet models in subdirectory
        } else {
            // Fallback to default location
            let current_dir = std::env::current_dir()
                .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;

            if cfg!(debug_assertions) {
                // Development mode
                current_dir.join("models").join("parakeet")
            } else {
                // Production mode
                dirs::data_dir()
                    .or_else(|| dirs::home_dir())
                    .ok_or_else(|| anyhow!("Could not find system data directory"))?
                    .join("Meetily")
                    .join("models")
                    .join("parakeet")
            }
        };

        log::info!("ParakeetEngine using models directory: {}", models_dir.display());

        // Create directory if it doesn't exist
        if !models_dir.exists() {
            std::fs::create_dir_all(&models_dir)?;
        }

        Ok(Self {
            models_dir,
            current_model: Arc::new(RwLock::new(None)),
            current_model_name: Arc::new(RwLock::new(None)),
            available_models: Arc::new(RwLock::new(HashMap::new())),
            cancel_download_flag: Arc::new(RwLock::new(None)),
            // Initialize active downloads tracking
            active_downloads: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Discover available Parakeet models
    pub async fn discover_models(&self) -> Result<Vec<ModelInfo>> {
        let models_dir = &self.models_dir;
        let mut models = Vec::new();

        // Parakeet model configurations
        // Model name format: parakeet-tdt-0.6b-v{version}-{quantization}
        // Sizes match actual download sizes (encoder + decoder + preprocessor + vocab)
        let model_configs = [
            ("parakeet-tdt-0.6b-v3-int8", 670, QuantizationType::Int8, "Ultra Fast (v3)", "Real time on M4 Max, latest version with int8 quantization"),
            ("parakeet-tdt-0.6b-v2-int8", 661, QuantizationType::Int8, "Fast (v2)", "Previous version with int8 quantization, good balance of speed and accuracy"),
        ];

        for (name, size_mb, quantization, speed, description) in model_configs {
            let model_path = models_dir.join(name);

            // Check if model directory exists and contains required files
            let status = if model_path.exists() {
                // Check for required ONNX files
                let required_files = match quantization {
                    QuantizationType::Int8 => vec![
                        "encoder-model.int8.onnx",
                        "decoder_joint-model.int8.onnx",
                        "nemo128.onnx",
                        "vocab.txt",
                    ],
                    QuantizationType::FP32 => vec![
                        "encoder-model.onnx",
                        "decoder_joint-model.onnx",
                        "nemo128.onnx",
                        "vocab.txt",
                    ],
                };

                let all_files_exist = required_files.iter().all(|file| {
                    model_path.join(file).exists()
                });

                if all_files_exist {
                    // Validate model by checking file sizes
                    match self.validate_model_directory(&model_path).await {
                        Ok(_) => ModelStatus::Available,
                        Err(_) => {
                            log::warn!("Model directory {} appears corrupted", name);
                            // Calculate total size of existing files
                            let mut total_size = 0u64;
                            for file in required_files {
                                if let Ok(metadata) = std::fs::metadata(model_path.join(file)) {
                                    total_size += metadata.len();
                                }
                            }
                            ModelStatus::Corrupted {
                                file_size: total_size,
                                expected_min_size: (size_mb as u64) * 1024 * 1024,
                            }
                        }
                    }
                } else {
                    ModelStatus::Missing
                }
            } else {
                ModelStatus::Missing
            };

            let model_info = ModelInfo {
                name: name.to_string(),
                path: model_path,
                size_mb: size_mb as u32,
                quantization: quantization.clone(),
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

    /// Validate model directory by checking if all required files exist and are valid
    async fn validate_model_directory(&self, model_dir: &PathBuf) -> Result<()> {
        // Check if vocab.txt exists and is readable
        let vocab_path = model_dir.join("vocab.txt");
        if !vocab_path.exists() {
            return Err(anyhow!("vocab.txt not found"));
        }

        // Check if ONNX files exist (either int8 or fp32)
        let has_int8 = model_dir.join("encoder-model.int8.onnx").exists()
            && model_dir.join("decoder_joint-model.int8.onnx").exists();
        let has_fp32 = model_dir.join("encoder-model.onnx").exists()
            && model_dir.join("decoder_joint-model.onnx").exists();

        if !has_int8 && !has_fp32 {
            return Err(anyhow!("No ONNX model files found"));
        }

        // Check preprocessor
        if !model_dir.join("nemo128.onnx").exists() {
            return Err(anyhow!("Preprocessor (nemo128.onnx) not found"));
        }

        Ok(())
    }

    /// Load a Parakeet model
    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        let models = self.available_models.read().await;
        let model_info = models
            .get(model_name)
            .ok_or_else(|| anyhow!("Model {} not found", model_name))?;

        match model_info.status {
            ModelStatus::Available => {
                // Check if this model is already loaded
                if let Some(current_model) = self.current_model_name.read().await.as_ref() {
                    if current_model == model_name {
                        log::info!("Parakeet model {} is already loaded, skipping reload", model_name);
                        return Ok(());
                    }

                    // Unload current model before loading new one
                    log::info!("Unloading current Parakeet model '{}' before loading '{}'", current_model, model_name);
                    self.unload_model().await;
                }

                log::info!("Loading Parakeet model: {}", model_name);

                // Load model based on quantization type
                let quantized = model_info.quantization == QuantizationType::Int8;
                let model = ParakeetModel::new(&model_info.path, quantized)
                    .map_err(|e| anyhow!("Failed to load Parakeet model {}: {}", model_name, e))?;

                // Update current model and model name
                *self.current_model.write().await = Some(model);
                *self.current_model_name.write().await = Some(model_name.to_string());

                log::info!(
                    "Successfully loaded Parakeet model: {} ({})",
                    model_name,
                    if quantized { "Int8 quantized" } else { "FP32" }
                );
                Ok(())
            }
            ModelStatus::Missing => {
                Err(anyhow!("Parakeet model {} is not downloaded", model_name))
            }
            ModelStatus::Downloading { .. } => {
                Err(anyhow!("Parakeet model {} is currently downloading", model_name))
            }
            ModelStatus::Error(ref err) => {
                Err(anyhow!("Parakeet model {} has error: {}", model_name, err))
            }
            ModelStatus::Corrupted { .. } => {
                Err(anyhow!("Parakeet model {} is corrupted and cannot be loaded", model_name))
            }
        }
    }

    /// Unload the current model
    pub async fn unload_model(&self) -> bool {
        let mut model_guard = self.current_model.write().await;
        let unloaded = model_guard.take().is_some();
        if unloaded {
            log::info!("Parakeet model unloaded");
        }

        let mut model_name_guard = self.current_model_name.write().await;
        model_name_guard.take();

        unloaded
    }

    /// Get the currently loaded model name
    pub async fn get_current_model(&self) -> Option<String> {
        self.current_model_name.read().await.clone()
    }

    /// Check if a model is loaded
    pub async fn is_model_loaded(&self) -> bool {
        self.current_model.read().await.is_some()
    }

    /// Transcribe audio samples using the loaded Parakeet model
    pub async fn transcribe_audio(&self, audio_data: Vec<f32>) -> Result<String> {
        let mut model_guard = self.current_model.write().await;
        let model = model_guard
            .as_mut()
            .ok_or_else(|| anyhow!("No Parakeet model loaded. Please load a model first."))?;

        let duration_seconds = audio_data.len() as f64 / 16000.0; // Assuming 16kHz
        log::debug!(
            "Parakeet transcribing {} samples ({:.1}s duration)",
            audio_data.len(),
            duration_seconds
        );

        // Transcribe using Parakeet model
        let result = model
            .transcribe_samples(audio_data)
            .map_err(|e| anyhow!("Parakeet transcription failed: {}", e))?;

        log::debug!("Parakeet transcription result: '{}'", result.text);

        Ok(result.text)
    }

    /// Get the models directory path
    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_dir.clone()
    }

    /// Delete a corrupted model
    pub async fn delete_model(&self, model_name: &str) -> Result<String> {
        log::info!("Attempting to delete Parakeet model: {}", model_name);

        // Get model info to find the directory path
        let model_info = {
            let models = self.available_models.read().await;
            models.get(model_name).cloned()
        };

        let model_info = model_info.ok_or_else(|| anyhow!("Parakeet model '{}' not found", model_name))?;

        log::info!("Parakeet model '{}' has status: {:?}", model_name, model_info.status);

        // Allow deletion of corrupted or available models
        match &model_info.status {
            ModelStatus::Corrupted { .. } | ModelStatus::Available => {
                // Delete the entire model directory
                if model_info.path.exists() {
                    fs::remove_dir_all(&model_info.path).await
                        .map_err(|e| anyhow!("Failed to delete directory '{}': {}", model_info.path.display(), e))?;
                    log::info!("Successfully deleted Parakeet model directory: {}", model_info.path.display());
                } else {
                    log::warn!("Directory '{}' does not exist, nothing to delete", model_info.path.display());
                }

                // Update model status to Missing
                {
                    let mut models = self.available_models.write().await;
                    if let Some(model) = models.get_mut(model_name) {
                        model.status = ModelStatus::Missing;
                    }
                }

                Ok(format!("Successfully deleted Parakeet model '{}'", model_name))
            }
            _ => {
                Err(anyhow!(
                    "Can only delete corrupted or available Parakeet models. Model '{}' has status: {:?}",
                    model_name,
                    model_info.status
                ))
            }
        }
    }

    /// Download a Parakeet model from HuggingFace
    pub async fn download_model(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        log::info!("Starting download for Parakeet model: {}", model_name);

        // Check if download is already in progress for this model
        {
            let active = self.active_downloads.read().await;
            if active.contains(model_name) {
                log::warn!("Download already in progress for Parakeet model: {}", model_name);
                return Err(anyhow!("Download already in progress for model: {}", model_name));
            }
        }

        // Add to active downloads
        {
            let mut active = self.active_downloads.write().await;
            active.insert(model_name.to_string());
        }

        // Clear any previous cancellation flag for this model
        {
            let mut cancel_flag = self.cancel_download_flag.write().await;
            *cancel_flag = None;
        }

        // Get model info
        let model_info = {
            let models = self.available_models.read().await;
            match models.get(model_name).cloned() {
                Some(info) => info,
                None => {
                    // Remove from active downloads on error
                    let mut active = self.active_downloads.write().await;
                    active.remove(model_name);
                    return Err(anyhow!("Model {} not found", model_name));
                }
            }
        };

        // Update model status to downloading
        {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Downloading { progress: 0 };
            }
        }

        // HuggingFace base URL for Parakeet models (version-specific)
        let base_url = if model_name.contains("-v2-") {
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/main"
        } else {
            // Default to v3 for v3 models
            "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main"
        };

        // Determine which files to download based on quantization
        let files_to_download = match model_info.quantization {
            QuantizationType::Int8 => vec![
                "encoder-model.int8.onnx",
                "decoder_joint-model.int8.onnx",
                "nemo128.onnx",
                "vocab.txt",
            ],
            QuantizationType::FP32 => vec![
                "encoder-model.onnx",
                "decoder_joint-model.onnx",
                "nemo128.onnx",
                "vocab.txt",
            ],
        };

        // Create model directory
        let model_dir = &model_info.path;
        if !model_dir.exists() {
            if let Err(e) = fs::create_dir_all(model_dir).await {
                // Remove from active downloads on error
                let mut active = self.active_downloads.write().await;
                active.remove(model_name);
                return Err(anyhow!("Failed to create model directory: {}", e));
            }
        }

        // Download each file
        let client = reqwest::Client::new();
        let total_files = files_to_download.len();

        // Calculate total download size for weighted progress
        // Note: These are approximate sizes based on HuggingFace repo inspection
        use std::collections::HashMap;
        let file_sizes: HashMap<&str, u64> = match model_info.quantization {
            QuantizationType::Int8 => {
                if model_name.contains("-v2-") {
                    // V2 model sizes
                    [
                        ("encoder-model.int8.onnx", 652_000_000u64),       // 652 MB
                        ("decoder_joint-model.int8.onnx", 9_000_000u64),   // 9 MB
                        ("nemo128.onnx", 140_000u64),                      // 140 KB
                        ("vocab.txt", 9_380u64),                           // 9.38 KB
                    ].iter().cloned().collect()
                } else {
                    // V3 model sizes (default)
                    [
                        ("encoder-model.int8.onnx", 652_000_000u64),       // 652 MB
                        ("decoder_joint-model.int8.onnx", 18_200_000u64),  // 18.2 MB
                        ("nemo128.onnx", 140_000u64),                      // 140 KB
                        ("vocab.txt", 93_900u64),                          // 93.9 KB
                    ].iter().cloned().collect()
                }
            }
            QuantizationType::FP32 => {
                // FP32 model sizes (encoder has .onnx + .onnx.data)
                [
                    ("encoder-model.onnx", 41_800_000u64 + 2_440_000_000u64), // 41.8 MB + 2.44 GB
                    ("decoder_joint-model.onnx", 72_500_000u64),               // 72.5 MB
                    ("nemo128.onnx", 140_000u64),                              // 140 KB
                    ("vocab.txt", 93_900u64),                                  // 93.9 KB
                ].iter().cloned().collect()
            }
        };

        // Calculate total expected download size
        let total_size_bytes: u64 = files_to_download.iter()
            .filter_map(|f| file_sizes.get(*f))
            .copied()
            .sum();

        let mut total_downloaded: u64 = 0;

        log::info!(
            "Starting weighted download for {} files, total size: {:.2} MB",
            total_files,
            total_size_bytes as f64 / 1_048_576.0
        );

        for (index, filename) in files_to_download.iter().enumerate() {
            let file_url = format!("{}/{}", base_url, filename);
            let file_path = model_dir.join(filename);

            log::info!("Downloading file {}/{}: {}", index + 1, total_files, filename);

            // Download file
            let response = client.get(&file_url).send().await
                .map_err(|e| {
                    // Note: cleanup will happen at function end via drop or explicit cleanup
                    anyhow!("Failed to start download for {}: {}", filename, e)
                })?;

            if !response.status().is_success() {
                // Remove from active downloads on error
                let mut active = self.active_downloads.write().await;
                active.remove(model_name);
                return Err(anyhow!("Download failed for {} with status: {}", filename, response.status()));
            }

            let total_size = response.content_length().unwrap_or(0);
            let mut file = match fs::File::create(&file_path).await {
                Ok(f) => f,
                Err(e) => {
                    // Remove from active downloads on error
                    let mut active = self.active_downloads.write().await;
                    active.remove(model_name);
                    return Err(anyhow!("Failed to create file {}: {}", filename, e));
                }
            };

            // Stream download
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();
            let mut downloaded = 0u64;
            let mut last_reported_mb = 0u64;
            let mut last_reported_progress = 0u8;

            while let Some(chunk_result) = stream.next().await {
                // Check for cancellation before processing chunk
                {
                    let cancel_flag = self.cancel_download_flag.read().await;
                    if cancel_flag.as_ref() == Some(&model_name.to_string()) {
                        log::info!("Download cancelled for {}", model_name);
                        // Remove from active downloads on cancellation
                        let mut active = self.active_downloads.write().await;
                        active.remove(model_name);
                        return Err(anyhow!("Download cancelled by user"));
                    }
                }

                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        // Remove from active downloads on error
                        let mut active = self.active_downloads.write().await;
                        active.remove(model_name);
                        return Err(anyhow!("Failed to read chunk: {}", e));
                    }
                };

                if let Err(e) = file.write_all(&chunk).await {
                    // Remove from active downloads on error
                    let mut active = self.active_downloads.write().await;
                    active.remove(model_name);
                    return Err(anyhow!("Failed to write chunk to file: {}", e));
                }

                downloaded += chunk.len() as u64;
                total_downloaded += chunk.len() as u64;

                // Calculate weighted overall progress based on total bytes downloaded
                let overall_progress = if total_size_bytes > 0 {
                    ((total_downloaded as f64 / total_size_bytes as f64) * 100.0).min(99.0) as u8
                } else {
                    // Fallback to per-file progress if total size unknown
                    ((index as f64 + (downloaded as f64 / total_size.max(1) as f64)) / total_files as f64 * 100.0) as u8
                };

                // Improved throttling: Report every 5MB OR every 5% progress change OR on completion
                // This significantly reduces event spam while keeping progress smooth
                let current_mb = total_downloaded / 1_048_576;
                let mb_threshold_crossed = current_mb / 5 > last_reported_mb / 5; // Every 5MB
                let progress_threshold_crossed = overall_progress >= last_reported_progress + 5; // Every 5%
                let is_complete = downloaded == total_size;

                let should_report = mb_threshold_crossed || progress_threshold_crossed || is_complete;

                if should_report {
                    last_reported_mb = current_mb;
                    last_reported_progress = overall_progress;

                    // Report progress
                    if let Some(ref callback) = progress_callback {
                        callback(overall_progress);
                    }

                    // Update model status
                    {
                        let mut models = self.available_models.write().await;
                        if let Some(model) = models.get_mut(model_name) {
                            model.status = ModelStatus::Downloading { progress: overall_progress };
                        }
                    }
                }
            }

            if let Err(e) = file.flush().await {
                // Remove from active downloads on error
                let mut active = self.active_downloads.write().await;
                active.remove(model_name);
                return Err(anyhow!("Failed to flush file {}: {}", filename, e));
            }

            log::info!(
                "Completed download: {} ({:.2} MB, overall progress: {:.1}%)",
                filename,
                downloaded as f64 / 1_048_576.0,
                (total_downloaded as f64 / total_size_bytes as f64) * 100.0
            );
        }

        // Report 100% progress
        if let Some(ref callback) = progress_callback {
            callback(100);
        }

        // Update model status to available
        {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Available;
                model.path = model_dir.clone();
            }
        }

        // Remove from active downloads on completion
        {
            let mut active = self.active_downloads.write().await;
            active.remove(model_name);
        }

        // Clear cancellation flag on successful completion
        {
            let mut cancel_flag = self.cancel_download_flag.write().await;
            if cancel_flag.as_ref() == Some(&model_name.to_string()) {
                *cancel_flag = None;
            }
        }

        log::info!("Download completed for Parakeet model: {}", model_name);
        Ok(())
    }

    /// Cancel an ongoing model download
    pub async fn cancel_download(&self, model_name: &str) -> Result<()> {
        log::info!("Cancelling download for Parakeet model: {}", model_name);

        // Set cancellation flag to interrupt the download loop
        {
            let mut cancel_flag = self.cancel_download_flag.write().await;
            *cancel_flag = Some(model_name.to_string());
        }

        // Remove from active downloads
        {
            let mut active = self.active_downloads.write().await;
            active.remove(model_name);
        }

        // Update model status to Missing (so it can be retried)
        {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Missing;
            }
        }

        // Clean up partially downloaded files
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await; // Brief delay to let download loop exit

        let model_path = self.models_dir.join(model_name);
        if model_path.exists() {
            if let Err(e) = fs::remove_dir_all(&model_path).await {
                log::warn!("Failed to clean up cancelled download directory: {}", e);
            } else {
                log::info!("Cleaned up cancelled download directory: {}", model_path.display());
            }
        }

        Ok(())
    }
}
