use crate::parakeet_engine::model::ParakeetModel;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;

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

/// Detailed download progress info (MB-based with speed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Bytes downloaded so far
    pub downloaded_bytes: u64,
    /// Total file size in bytes
    pub total_bytes: u64,
    /// Downloaded in MB (for display)
    pub downloaded_mb: f64,
    /// Total size in MB (for display)
    pub total_mb: f64,
    /// Download speed in MB/s
    pub speed_mbps: f64,
    /// Percentage complete (0-100)
    pub percent: u8,
}

impl DownloadProgress {
    pub fn new(downloaded: u64, total: u64, speed_mbps: f64) -> Self {
        let percent = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).min(100.0) as u8
        } else {
            0
        };
        Self {
            downloaded_bytes: downloaded,
            total_bytes: total,
            downloaded_mb: downloaded as f64 / (1024.0 * 1024.0),
            total_mb: total as f64 / (1024.0 * 1024.0),
            speed_mbps,
            percent,
        }
    }
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
    pub(crate) available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    cancelled_downloads: Arc<RwLock<HashSet<String>>>, // Models with pending cancellation requests
    // Active downloads tracking to prevent concurrent downloads
    pub(crate) active_downloads: Arc<RwLock<HashSet<String>>>, // Set of models currently being downloaded
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
            cancelled_downloads: Arc::new(RwLock::new(HashSet::new())),
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

        // Get active downloads to override status
        let active_downloads = self.active_downloads.read().await;

        for (name, size_mb, quantization, speed, description) in model_configs {
            let model_path = models_dir.join(name);

            // Check if model is currently downloading
            let status = if active_downloads.contains(name) {
                // If downloading, preserve that status regardless of file system
                // We don't know the exact progress here without more state, but 0 is safe fallback
                // The progress events will update the UI
                ModelStatus::Downloading { progress: 0 }
            } else if model_path.exists() {
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

    /// Validate model directory by checking if all required files exist AND have valid sizes
    async fn validate_model_directory(&self, model_dir: &PathBuf) -> Result<()> {
        // Check if vocab.txt exists and is readable
        let vocab_path = model_dir.join("vocab.txt");
        if !vocab_path.exists() {
            return Err(anyhow!("vocab.txt not found"));
        }

        // Determine which files to check based on what exists
        let is_int8 = model_dir.join("encoder-model.int8.onnx").exists();
        let is_fp32 = model_dir.join("encoder-model.onnx").exists();

        if !is_int8 && !is_fp32 {
            return Err(anyhow!("No ONNX model files found"));
        }

        // Check preprocessor
        if !model_dir.join("nemo128.onnx").exists() {
            return Err(anyhow!("Preprocessor (nemo128.onnx) not found"));
        }

        // Define minimum file sizes (90% of expected to allow some variance)
        // These are critical to catch partial downloads that would crash on load
        let expected_sizes: Vec<(&str, u64)> = if is_int8 {
            vec![
                ("encoder-model.int8.onnx", 580_000_000),    // ~652 MB, min 580 MB (89%)
                ("decoder_joint-model.int8.onnx", 8_000_000), // ~18 MB, min 8 MB
                ("nemo128.onnx", 100_000),                    // ~140 KB, min 100 KB
                ("vocab.txt", 5_000),                         // ~94 KB, min 5 KB
            ]
        } else {
            vec![
                ("encoder-model.onnx", 2_200_000_000),        // ~2.44 GB, min 2.2 GB
                ("decoder_joint-model.onnx", 65_000_000),     // ~72 MB, min 65 MB
                ("nemo128.onnx", 100_000),                    // ~140 KB, min 100 KB
                ("vocab.txt", 5_000),                         // ~94 KB, min 5 KB
            ]
        };

        // Validate each file exists AND has sufficient size
        for (filename, min_size) in expected_sizes {
            let file_path = model_dir.join(filename);
            if !file_path.exists() {
                return Err(anyhow!("{} not found", filename));
            }

            match std::fs::metadata(&file_path) {
                Ok(metadata) => {
                    let actual_size = metadata.len();
                    if actual_size < min_size {
                        return Err(anyhow!(
                            "{} is incomplete: {} bytes (expected at least {} bytes)",
                            filename,
                            actual_size,
                            min_size
                        ));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Failed to read {} metadata: {}", filename, e));
                }
            }
        }

        Ok(())
    }

    /// Count bytes that can be reused by the per-file skip/resume logic.
    async fn existing_artifact_bytes(
        model_dir: &Path,
        filenames: &[&str],
        expected_sizes: &HashMap<&str, u64>,
    ) -> u64 {
        let mut reusable_bytes = 0;
        for filename in filenames {
            let file_path = model_dir.join(filename);
            if let (Ok(metadata), Some(expected_size)) =
                (fs::metadata(&file_path).await, expected_sizes.get(filename))
            {
                // 这些尺寸是近似值，只能用于进度估算，不能据此删除文件；
                // 缺失、偏小或版本变化后的偏大 artifact 都交给逐文件下载/校验流程处理。
                reusable_bytes += metadata.len().min(*expected_size);
            }
        }

        reusable_bytes
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

    /// Download a Parakeet model from HuggingFace (backward-compatible wrapper)
    pub async fn download_model(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        // Wrap simple callback to use detailed version
        let detailed_callback: Option<Box<dyn Fn(DownloadProgress) + Send>> =
            progress_callback.map(|cb| {
                Box::new(move |p: DownloadProgress| cb(p.percent)) as Box<dyn Fn(DownloadProgress) + Send>
            });
        self.download_model_detailed(model_name, detailed_callback).await
    }

    /// Download a Parakeet model with detailed progress (MB/speed/resume support)
    pub async fn download_model_detailed(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        log::info!("Starting download for Parakeet model: {}", model_name);

        // 检查与注册必须在同一写锁内完成，避免两个重试任务同时通过检查并写入同一个 artifact。
        {
            let mut active = self.active_downloads.write().await;
            if active.contains(model_name) {
                log::warn!(
                    "Download already in progress for Parakeet model: {}",
                    model_name
                );
                return Err(anyhow!(
                    "Download already in progress for model: {}",
                    model_name
                ));
            }

            // cancel_download 采用相同的 active -> cancelled 锁顺序；因此这里清理的只能是
            // 上一次任务留下的请求，新取消请求会在注册完成后写入，不会被启动流程吞掉。
            self.cancelled_downloads.write().await.remove(model_name);
            active.insert(model_name.to_string());
        }

        let result = self
            .download_model_detailed_inner(model_name, progress_callback)
            .await;

        // active 标记由最外层统一释放，确保 TLS、代理、文件 I/O 和取消等所有返回路径
        // 都不会遗留“already in progress”，也不会在 worker 尚未退出时过早允许重试。
        if result.is_err() {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Missing;
            }
        }
        self.active_downloads.write().await.remove(model_name);

        result
    }

    async fn download_model_detailed_inner(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        // Get model info
        let model_info = {
            let models = self.available_models.read().await;
            match models.get(model_name).cloned() {
                Some(info) => info,
                None => return Err(anyhow!("Model {} not found", model_name)),
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
            "https://meetily.towardsgeneralintelligence.com/models/parakeet-tdt-0.6b-v3-onnx"
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
                return Err(anyhow!("Failed to create model directory: {}", e));
            }
        }

        // Optimized HTTP client for large file downloads
        let client = reqwest::Client::builder()
            .tcp_nodelay(true)              // Disable Nagle's algorithm for better streaming
            .pool_max_idle_per_host(1)      // Keep connection alive
            .timeout(Duration::from_secs(3600))  // 1 hour timeout for large files
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;

        let total_files = files_to_download.len();

        // Calculate total download size for weighted progress
        // Note: These are approximate sizes based on HuggingFace repo inspection
        let file_sizes: std::collections::HashMap<&str, u64> = match model_info.quantization {
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

        // 只读取现有 artifact 来计算续传进度；单个文件失败时不再删除已完成的兄弟文件。
        let already_downloaded =
            Self::existing_artifact_bytes(model_dir, &files_to_download, &file_sizes).await;

        let mut total_downloaded: u64 = already_downloaded;

        // Timing for speed calculation
        let download_start_time = Instant::now();
        let mut last_report_time = Instant::now();
        let mut bytes_since_last_report: u64 = 0;
        let mut last_reported_progress: u8 = 0;

        log::info!(
            "Starting weighted download for {} files, total size: {:.2} MB (already downloaded: {:.2} MB)",
            total_files,
            total_size_bytes as f64 / 1_048_576.0,
            already_downloaded as f64 / 1_048_576.0
        );

        for (index, filename) in files_to_download.iter().enumerate() {
            let file_url = format!("{}/{}", base_url, filename);
            let file_path = model_dir.join(filename);

            // Check for existing partial file to resume
            let existing_size: u64 = if file_path.exists() {
                fs::metadata(&file_path).await.map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };

            let expected_size = file_sizes.get(*filename).copied().unwrap_or(0);

            // Skip if file is already complete (with 1% tolerance for size variations)
            let size_tolerance = (expected_size as f64 * 0.99) as u64;
            if existing_size >= size_tolerance && expected_size > 0 {
                log::info!(
                    "Skipping complete file: {} ({:.2} MB, expected: {:.2} MB)",
                    filename,
                    existing_size as f64 / 1_048_576.0,
                    expected_size as f64 / 1_048_576.0
                );
                continue;
            }

            log::info!("Downloading file {}/{}: {} (resuming from {} bytes)", index + 1, total_files, filename, existing_size);

            // Build request with optional Range header for resume
            let mut request = client.get(&file_url);
            if existing_size > 0 {
                request = request.header("Range", format!("bytes={}-", existing_size));
                log::info!("Resuming download from byte {}", existing_size);
            }

            let mut response = request.send().await
                .map_err(|e| {
                    anyhow!("Failed to start download for {}: {}", filename, e)
                })?;

            // Handle response status
            let (file_total_size, resuming) = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                // Server supports resume, get remaining size
                let remaining = response.content_length().unwrap_or(0);
                log::info!("Server supports resume, remaining: {} bytes", remaining);
                (existing_size + remaining, true)
            } else if response.status().is_success() {
                // Fresh download or server doesn't support resume
                if existing_size > 0 {
                    log::warn!("Server doesn't support resume for {}, starting fresh download", filename);
                }
                (response.content_length().unwrap_or(0), false)
            } else if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                // 416: Range not satisfiable - file complete or invalid range
                log::warn!("Server returned 416 Range Not Satisfiable for {}", filename);

                let size_tolerance = (expected_size as f64 * 0.99) as u64;
                if existing_size >= size_tolerance && expected_size > 0 {
                    // File is complete - skip it
                    log::info!("File {} complete ({} bytes). Skipping.", filename, existing_size);
                    continue;
                } else {
                    // File incomplete but server won't accept range - delete and retry
                    log::warn!(
                        "File {} incomplete ({}/{} bytes). Deleting and retrying.",
                        filename, existing_size, expected_size
                    );

                    if let Err(e) = fs::remove_file(&file_path).await {
                        return Err(anyhow!("Failed to delete incomplete file {}: {}", filename, e));
                    }

                    // Retry without Range header
                    log::info!("Retrying {} without resume", filename);
                    response = client.get(&file_url).send().await
                        .map_err(|e| anyhow!("Retry failed for {}: {}", filename, e))?;

                    if !response.status().is_success() {
                        return Err(anyhow!("Retry failed for {} with status: {}", filename, response.status()));
                    }

                    (response.content_length().unwrap_or(0), false)
                }
            } else {
                // Other errors
                return Err(anyhow!("Download failed for {} with status: {}", filename, response.status()));
            };

            // Open file for writing (append if resuming, create new if not)
            let file = if resuming {
                fs::OpenOptions::new()
                    .append(true)
                    .open(&file_path)
                    .await
                    .map_err(|e| anyhow!("Failed to open file for resume {}: {}", filename, e))?
            } else {
                fs::File::create(&file_path)
                    .await
                    .map_err(|e| anyhow!("Failed to create file {}: {}", filename, e))?
            };

            // Use buffered writer for better I/O performance (8MB buffer)
            let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, file);

            // Stream download
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();
            let mut file_downloaded = if resuming { existing_size } else { 0u64 };

            loop {
                // Check for cancellation before processing chunk
                {
                    let cancelled_downloads = self.cancelled_downloads.read().await;
                    if cancelled_downloads.contains(model_name) {
                        log::info!("Download cancelled for {}", model_name);
                        // Flush and keep partial file for resume on next attempt
                        let _ = writer.flush().await;
                        drop(writer);
                        return Err(anyhow!("Download cancelled by user"));
                    }
                }

                // Add per-chunk timeout (30 seconds) to detect stalled connections
                let next_result = timeout(Duration::from_secs(30), stream.next()).await;

                let chunk = match next_result {
                    // Timeout - no data received for 30 seconds
                    Err(_) => {
                        log::warn!("Download timeout for {}: no data received for 30 seconds", model_name);
                        let _ = writer.flush().await;

                        // Update model status to Missing so retry can work
                        {
                            let mut models = self.available_models.write().await;
                            if let Some(model) = models.get_mut(model_name) {
                                model.status = ModelStatus::Missing;
                            }
                        }

                        return Err(anyhow!("Download timeout - No data received for 30 seconds"));
                    },
                    // Stream ended
                    Ok(None) => break,
                    // Got chunk result
                    Ok(Some(chunk_result)) => {
                        match chunk_result {
                            Ok(c) => c,
                            // Detect error type for better user feedback
                            Err(e) => {
                                log::error!("Download error for {}: {:?}", model_name, e);
                                let _ = writer.flush().await;

                                // Update model status to Missing so retry can work
                                {
                                    let mut models = self.available_models.write().await;
                                    if let Some(model) = models.get_mut(model_name) {
                                        model.status = ModelStatus::Missing;
                                    }
                                }

                                let error_msg = if e.is_timeout() {
                                    "Connection timeout - Check your internet"
                                } else if e.is_connect() {
                                    "Connection failed - Check your internet"
                                } else if e.is_body() {
                                    "Stream interrupted - Network unstable"
                                } else {
                                    "Download error"
                                };

                                return Err(anyhow!("{}: {}", error_msg, e));
                            }
                        }
                    }
                };

                if let Err(e) = writer.write_all(&chunk).await {
                    // Update model status to Missing so retry can work
                    {
                        let mut models = self.available_models.write().await;
                        if let Some(model) = models.get_mut(model_name) {
                            model.status = ModelStatus::Missing;
                        }
                    }

                    return Err(anyhow!("Failed to write chunk to file: {}", e));
                }

                let chunk_len = chunk.len() as u64;
                file_downloaded += chunk_len;
                total_downloaded += chunk_len;
                bytes_since_last_report += chunk_len;

                // Calculate weighted overall progress based on total bytes downloaded
                let overall_progress = if total_size_bytes > 0 {
                    ((total_downloaded as f64 / total_size_bytes as f64) * 100.0).min(99.0) as u8
                } else {
                    // Fallback to per-file progress if total size unknown
                    ((index as f64 + (file_downloaded as f64 / file_total_size.max(1) as f64)) / total_files as f64 * 100.0) as u8
                };

                // Report every 1% progress change OR every 500ms for smooth UI updates
                let elapsed_since_report = last_report_time.elapsed();
                let progress_changed = overall_progress > last_reported_progress;
                let time_threshold = elapsed_since_report >= Duration::from_millis(500);
                let is_complete = file_downloaded >= file_total_size;

                let should_report = progress_changed || time_threshold || is_complete;

                if should_report {
                    // Calculate download speed
                    let speed_mbps = if elapsed_since_report.as_secs_f64() >= 0.1 {
                        (bytes_since_last_report as f64 / (1024.0 * 1024.0)) / elapsed_since_report.as_secs_f64()
                    } else {
                        // Fallback to overall average speed
                        let total_elapsed = download_start_time.elapsed().as_secs_f64();
                        if total_elapsed > 0.0 {
                            ((total_downloaded - already_downloaded) as f64 / (1024.0 * 1024.0)) / total_elapsed
                        } else {
                            0.0
                        }
                    };

                    last_reported_progress = overall_progress;
                    last_report_time = Instant::now();
                    bytes_since_last_report = 0;

                    // Create detailed progress and report
                    let progress = DownloadProgress::new(total_downloaded, total_size_bytes, speed_mbps);
                    if let Some(ref callback) = progress_callback {
                        callback(progress);
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

            // Flush the buffered writer
            if let Err(e) = writer.flush().await {
                // Update model status to Missing so retry can work
                {
                    let mut models = self.available_models.write().await;
                    if let Some(model) = models.get_mut(model_name) {
                        model.status = ModelStatus::Missing;
                    }
                }

                return Err(anyhow!("Failed to flush file {}: {}", filename, e));
            }

            log::info!(
                "Completed download: {} ({:.2} MB, overall progress: {:.1}%)",
                filename,
                file_downloaded as f64 / 1_048_576.0,
                (total_downloaded as f64 / total_size_bytes as f64) * 100.0
            );
        }

        // Report 100% progress with final speed
        let total_elapsed = download_start_time.elapsed().as_secs_f64();
        let final_speed = if total_elapsed > 0.0 {
            ((total_downloaded - already_downloaded) as f64 / (1024.0 * 1024.0)) / total_elapsed
        } else {
            0.0
        };
        let final_progress = DownloadProgress::new(total_size_bytes, total_size_bytes, final_speed);
        if let Some(ref callback) = progress_callback {
            callback(final_progress);
        }

        // Update model status to available
        {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Available;
                model.path = model_dir.clone();
            }
        }

        self.cancelled_downloads.write().await.remove(model_name);

        log::info!("Download completed for Parakeet model: {}", model_name);
        Ok(())
    }

    /// Cancel an ongoing model download
    pub async fn cancel_download(&self, model_name: &str) -> Result<()> {
        log::info!("Cancelling download for Parakeet model: {}", model_name);

        // 与下载注册保持 active -> cancelled 的固定锁顺序，关闭“刚注册就误清取消”的窗口。
        // 没有活跃 worker 时无需留下会污染下一次下载的陈旧请求。
        {
            let active = self.active_downloads.read().await;
            if active.contains(model_name) {
                self.cancelled_downloads
                    .write()
                    .await
                    .insert(model_name.to_string());
            }
        }

        // Update model status to Missing (so it can be retried)
        {
            let mut models = self.available_models.write().await;
            if let Some(model) = models.get_mut(model_name) {
                model.status = ModelStatus::Missing;
            }
        }

        // 取消只发出停止信号，不提前释放 active 标记：旧任务 flush 并退出前，不允许重试任务并发写同一文件。
        // 部分文件和已完整的兄弟文件都保留；整个模型目录仅能由显式的 delete_model 操作删除。

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const TEST_MODEL_NAME: &str = "parakeet-tdt-0.6b-v3-int8";

    #[tokio::test]
    async fn missing_artifact_does_not_delete_existing_siblings() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let model_dir = temp_dir.path().join(TEST_MODEL_NAME);
        fs::create_dir_all(&model_dir)
            .await
            .expect("create model directory");

        // 缺失 vocab 时，其他文件可能是完整文件，也可能是可续传的部分文件；两者都不能被连带删除。
        let existing_artifacts: [(&str, &[u8]); 3] = [
            ("encoder-model.int8.onnx", b"encoder-partial"),
            ("decoder_joint-model.int8.onnx", b"decoder-complete"),
            ("nemo128.onnx", b"preprocessor-complete"),
        ];
        for (filename, contents) in existing_artifacts {
            fs::write(model_dir.join(filename), contents)
                .await
                .expect("seed existing artifact");
        }

        let expected_sizes = HashMap::from([
            ("encoder-model.int8.onnx", 64),
            (
                "decoder_joint-model.int8.onnx",
                b"decoder-complete".len() as u64,
            ),
            ("nemo128.onnx", b"preprocessor-complete".len() as u64),
            ("vocab.txt", 32),
        ]);
        let filenames = [
            "encoder-model.int8.onnx",
            "decoder_joint-model.int8.onnx",
            "nemo128.onnx",
            "vocab.txt",
        ];

        let reusable_bytes =
            ParakeetEngine::existing_artifact_bytes(&model_dir, &filenames, &expected_sizes).await;
        let expected_reusable_bytes: u64 = existing_artifacts
            .iter()
            .map(|(_, contents)| contents.len() as u64)
            .sum();
        assert_eq!(reusable_bytes, expected_reusable_bytes);

        for (filename, contents) in existing_artifacts {
            assert_eq!(
                fs::read(model_dir.join(filename))
                    .await
                    .expect("existing artifact must be preserved"),
                contents
            );
        }
    }

    #[tokio::test]
    async fn failed_download_releases_active_registration() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
            .expect("create Parakeet engine");
        let missing_model = "unknown-model";

        let error = engine
            .download_model_detailed(missing_model, None)
            .await
            .expect_err("unknown model must fail before network access");

        assert!(error.to_string().contains("not found"));
        assert!(
            !engine.active_downloads.read().await.contains(missing_model),
            "outer cleanup must release active registration on every error path"
        );
    }

    #[tokio::test]
    async fn concurrent_retry_keeps_existing_active_registration() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
            .expect("create Parakeet engine");
        engine
            .active_downloads
            .write()
            .await
            .insert(TEST_MODEL_NAME.to_string());

        let error = engine
            .download_model_detailed(TEST_MODEL_NAME, None)
            .await
            .expect_err("a second worker must not bypass active registration");

        assert!(error.to_string().contains("already in progress"));
        assert!(
            engine
                .active_downloads
                .read()
                .await
                .contains(TEST_MODEL_NAME),
            "the existing worker remains the owner of the active registration"
        );
    }

    #[tokio::test]
    async fn starting_another_model_preserves_pending_cancellation() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
            .expect("create Parakeet engine");
        engine
            .active_downloads
            .write()
            .await
            .insert(TEST_MODEL_NAME.to_string());
        engine
            .cancel_download(TEST_MODEL_NAME)
            .await
            .expect("record cancellation request");

        let _ = engine.download_model_detailed("unknown-model", None).await;

        assert!(
            engine
                .cancelled_downloads
                .read()
                .await
                .contains(TEST_MODEL_NAME),
            "starting another model must not consume this model's cancellation request"
        );
    }

    #[tokio::test]
    async fn rejected_retry_preserves_active_workers_cancellation() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
            .expect("create Parakeet engine");
        engine
            .active_downloads
            .write()
            .await
            .insert(TEST_MODEL_NAME.to_string());
        engine
            .cancel_download(TEST_MODEL_NAME)
            .await
            .expect("record cancellation request");

        let _ = engine.download_model_detailed(TEST_MODEL_NAME, None).await;

        assert!(
            engine
                .cancelled_downloads
                .read()
                .await
                .contains(TEST_MODEL_NAME),
            "a rejected retry must not clear the active worker's cancellation request"
        );
    }

    #[tokio::test]
    async fn cancellation_preserves_downloaded_artifacts_for_resume() {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
            .expect("create Parakeet engine");
        let model_dir = engine.models_dir.join(TEST_MODEL_NAME);
        fs::create_dir_all(&model_dir)
            .await
            .expect("create model directory");
        let encoder_path = model_dir.join("encoder-model.int8.onnx");
        fs::write(&encoder_path, b"downloaded-encoder")
            .await
            .expect("seed downloaded artifact");
        engine
            .active_downloads
            .write()
            .await
            .insert(TEST_MODEL_NAME.to_string());

        engine
            .cancel_download(TEST_MODEL_NAME)
            .await
            .expect("cancel model download");

        assert_eq!(
            fs::read(encoder_path)
                .await
                .expect("cancellation must preserve resumable artifacts"),
            b"downloaded-encoder"
        );
        assert!(
            engine
                .active_downloads
                .read()
                .await
                .contains(TEST_MODEL_NAME),
            "the download worker owns the active registration until it exits"
        );
    }
}
