use crate::parakeet_engine::model::ParakeetModel;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncWriteExt, BufWriter};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex, RwLock};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// Quantization type for Parakeet models
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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

#[derive(Clone, Copy)]
struct ArtifactSpec {
    filename: &'static str,
    exact_bytes: u64,
}

struct ModelSpec {
    name: &'static str,
    size_mb: u32,
    quantization: QuantizationType,
    speed: &'static str,
    description: &'static str,
    source_base_url: &'static str,
    artifacts: &'static [ArtifactSpec],
}

impl ModelSpec {
    fn exact_bytes(&self) -> u64 {
        self.artifacts.iter().map(|artifact| artifact.exact_bytes).sum()
    }
}

const PARAKEET_V3_ARTIFACTS: &[ArtifactSpec] = &[
    ArtifactSpec { filename: "encoder-model.int8.onnx", exact_bytes: 652_183_999 },
    ArtifactSpec { filename: "decoder_joint-model.int8.onnx", exact_bytes: 18_202_004 },
    ArtifactSpec { filename: "nemo128.onnx", exact_bytes: 139_764 },
    ArtifactSpec { filename: "vocab.txt", exact_bytes: 93_939 },
];

const PARAKEET_V2_ARTIFACTS: &[ArtifactSpec] = &[
    ArtifactSpec { filename: "encoder-model.int8.onnx", exact_bytes: 652_184_014 },
    ArtifactSpec { filename: "decoder_joint-model.int8.onnx", exact_bytes: 8_998_286 },
    ArtifactSpec { filename: "nemo128.onnx", exact_bytes: 139_764 },
    ArtifactSpec { filename: "vocab.txt", exact_bytes: 9_384 },
];

const PARAKEET_MODEL_SPECS: &[ModelSpec] = &[
    ModelSpec {
        name: "parakeet-tdt-0.6b-v3-int8",
        size_mb: 670,
        quantization: QuantizationType::Int8,
        speed: "Ultra Fast (v3)",
        description: "Real time on M4 Max, latest version with int8 quantization",
        source_base_url: "https://meetily.towardsgeneralintelligence.com/models/parakeet-tdt-0.6b-v3-onnx",
        artifacts: PARAKEET_V3_ARTIFACTS,
    },
    ModelSpec {
        name: "parakeet-tdt-0.6b-v2-int8",
        size_mb: 661,
        quantization: QuantizationType::Int8,
        speed: "Fast (v2)",
        description: "Previous version with int8 quantization, good balance of speed and accuracy",
        source_base_url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/0bbb45a3365852604aef28b538a8f066f4ccaa85",
        artifacts: PARAKEET_V2_ARTIFACTS,
    },
];

fn find_model_spec(model_name: &str) -> Option<&'static ModelSpec> {
    PARAKEET_MODEL_SPECS
        .iter()
        .find(|spec| spec.name == model_name)
}

struct ActiveDownload {
    cancellation: CancellationToken,
    completion: watch::Sender<bool>,
}

#[derive(Debug, thiserror::Error)]
#[error("Download cancelled by user")]
pub(crate) struct DownloadCancelled;

pub(crate) fn is_download_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<DownloadCancelled>().is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CancelDownloadOutcome {
    Cancelled,
    Pending,
}

const CANCEL_DOWNLOAD_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
struct DownloadStateTestHook {
    finalization_ready: tokio::sync::Notify,
    continue_finalization: tokio::sync::Notify,
}

enum ContentRange {
    Range { start: u64, end: u64, total: u64 },
    Unsatisfied { total: u64 },
}

fn parse_content_range(value: &reqwest::header::HeaderValue) -> Result<ContentRange> {
    let value = value
        .to_str()
        .map_err(|e| anyhow!("Invalid Content-Range header encoding: {}", e))?;
    let value = value
        .strip_prefix("bytes ")
        .ok_or_else(|| anyhow!("Content-Range must use bytes: {}", value))?;

    if let Some(total) = value.strip_prefix("*/") {
        return total
            .parse()
            .map(|total| ContentRange::Unsatisfied { total })
            .map_err(|e| anyhow!("Invalid unsatisfied Content-Range total: {}", e));
    }

    let (range, total) = value
        .split_once('/')
        .ok_or_else(|| anyhow!("Malformed Content-Range: {}", value))?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| anyhow!("Malformed Content-Range range: {}", value))?;
    let start = start
        .parse()
        .map_err(|e| anyhow!("Invalid Content-Range start: {}", e))?;
    let end = end
        .parse()
        .map_err(|e| anyhow!("Invalid Content-Range end: {}", e))?;
    let total = total
        .parse()
        .map_err(|e| anyhow!("Invalid Content-Range total: {}", e))?;
    if start > end {
        return Err(anyhow!("Content-Range start exceeds end: {}", value));
    }

    Ok(ContentRange::Range { start, end, total })
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
    active_downloads: Arc<Mutex<HashMap<String, Arc<ActiveDownload>>>>,
    #[cfg(test)]
    download_state_test_hook: Mutex<Option<Arc<DownloadStateTestHook>>>,
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
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            download_state_test_hook: Mutex::new(None),
        })
    }

    #[cfg(test)]
    async fn test_hook(&self) -> Option<Arc<DownloadStateTestHook>> {
        self.download_state_test_hook.lock().await.clone()
    }

    /// Discover available Parakeet models.
    pub async fn discover_models(&self) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::with_capacity(PARAKEET_MODEL_SPECS.len());

        for spec in PARAKEET_MODEL_SPECS {
            let model_path = self.models_dir.join(spec.name);
            let is_downloading = self.active_downloads.lock().await.contains_key(spec.name);
            let status = if is_downloading {
                ModelStatus::Downloading { progress: 0 }
            } else if model_path.exists() {
                match Self::validate_model_directory(&model_path, spec.artifacts) {
                    Ok(()) => ModelStatus::Available,
                    Err(error) => {
                        let file_size = spec
                            .artifacts
                            .iter()
                            .filter_map(|artifact| std::fs::metadata(model_path.join(artifact.filename)).ok())
                            .map(|metadata| metadata.len())
                            .sum();
                        log::warn!("Model directory {} appears corrupted: {}", spec.name, error);
                        ModelStatus::Corrupted {
                            file_size,
                            expected_min_size: spec.exact_bytes(),
                        }
                    }
                }
            } else {
                ModelStatus::Missing
            };

            models.push(ModelInfo {
                name: spec.name.to_string(),
                path: model_path,
                size_mb: spec.size_mb,
                quantization: spec.quantization,
                speed: spec.speed.to_string(),
                status,
                description: spec.description.to_string(),
            });
        }

        let mut available_models = self.available_models.write().await;
        available_models.clear();
        for model in &models {
            available_models.insert(model.name.clone(), model.clone());
        }

        Ok(models)
    }

    fn validate_model_directory(model_dir: &Path, artifacts: &[ArtifactSpec]) -> Result<()> {
        for artifact in artifacts {
            let path = model_dir.join(artifact.filename);
            let metadata = std::fs::metadata(&path)
                .map_err(|error| anyhow!("Failed to read {} metadata: {}", artifact.filename, error))?;
            if metadata.len() != artifact.exact_bytes {
                return Err(anyhow!(
                    "{} has {} bytes, expected exactly {} bytes",
                    artifact.filename,
                    metadata.len(),
                    artifact.exact_bytes
                ));
            }
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

    /// Download a Parakeet model from HuggingFace (backward-compatible wrapper).
    pub async fn download_model(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(u8) + Send>>,
    ) -> Result<()> {
        let detailed_callback = progress_callback.map(|callback| {
            Box::new(move |progress: DownloadProgress| callback(progress.percent))
                as Box<dyn Fn(DownloadProgress) + Send>
        });
        self.download_model_detailed(model_name, detailed_callback).await
    }

    /// Download a catalogued Parakeet model with detailed progress.
    pub async fn download_model_detailed(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        let model_info = self
            .available_models
            .read()
            .await
            .get(model_name)
            .cloned()
            .ok_or_else(|| anyhow!("Model {} not found", model_name))?;
        let spec = find_model_spec(model_name)
            .ok_or_else(|| anyhow!("Unsupported Parakeet model: {}", model_name))?;

        self.download_model_detailed_from_source(
            model_name,
            &model_info.path,
            spec.source_base_url,
            spec.artifacts,
            progress_callback,
        )
        .await
    }

    async fn reserve_active_download(&self, model_name: &str) -> Result<Arc<ActiveDownload>> {
        let mut active_downloads = self.active_downloads.lock().await;
        if active_downloads.contains_key(model_name) {
            return Err(anyhow!("Download already in progress for model: {}", model_name));
        }

        let (completion, _) = watch::channel(false);
        let active_download = Arc::new(ActiveDownload {
            cancellation: CancellationToken::new(),
            completion,
        });
        active_downloads.insert(model_name.to_string(), Arc::clone(&active_download));
        Ok(active_download)
    }

    async fn set_downloading_status(&self, model_name: &str, progress: u8) {
        let mut models = self.available_models.write().await;
        if let Some(model) = models.get_mut(model_name) {
            model.status = ModelStatus::Downloading { progress };
        }
    }

    fn in_flight_progress(
        downloaded_bytes: u64,
        total_bytes: u64,
        speed_mbps: f64,
    ) -> DownloadProgress {
        let mut progress = DownloadProgress::new(downloaded_bytes, total_bytes, speed_mbps);
        progress.percent = progress.percent.min(99);
        progress
    }


    async fn send_download_request(
        &self,
        client: &reqwest::Client,
        file_url: &str,
        range_start: Option<u64>,
        active_download: &ActiveDownload,
    ) -> Result<reqwest::Response> {
        let mut request = client.get(file_url);
        if let Some(range_start) = range_start {
            request = request.header(reqwest::header::RANGE, format!("bytes={range_start}-"));
        }

        tokio::select! {
            biased;
            _ = active_download.cancellation.cancelled() => Err(DownloadCancelled.into()),
            response = request.send() => response
                .map_err(|error| anyhow!("Failed to start download for {}: {}", file_url, error)),
        }
    }

    fn declared_content_length(response: &reqwest::Response) -> Result<Option<u64>> {
        response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .map(|value| {
                value
                    .to_str()
                    .map_err(|error| anyhow!("Invalid Content-Length header encoding: {}", error))?
                    .parse()
                    .map_err(|error| anyhow!("Invalid Content-Length header: {}", error))
            })
            .transpose()
    }

    fn validate_full_response(response: &reqwest::Response, exact_bytes: u64) -> Result<()> {
        if response.status() != reqwest::StatusCode::OK {
            return Err(anyhow!(
                "Expected full 200 response, received {}",
                response.status()
            ));
        }
        if let Some(content_length) = Self::declared_content_length(response)? {
            if content_length != exact_bytes {
                return Err(anyhow!(
                    "Full response declared {} bytes, expected {}",
                    content_length,
                    exact_bytes
                ));
            }
        }
        Ok(())
    }

    fn validate_partial_response(
        response: &reqwest::Response,
        expected_start: u64,
        exact_bytes: u64,
    ) -> Result<()> {
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(anyhow!(
                "Expected partial 206 response, received {}",
                response.status()
            ));
        }
        let content_range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .ok_or_else(|| anyhow!("Partial response is missing Content-Range"))?;
        let ContentRange::Range { start, end, total } = parse_content_range(content_range)? else {
            return Err(anyhow!("Partial response has an unsatisfied Content-Range"));
        };
        if start != expected_start || end != exact_bytes - 1 || total != exact_bytes {
            return Err(anyhow!(
                "Partial response range {}-{} / {} does not match {}-{} / {}",
                start,
                end,
                total,
                expected_start,
                exact_bytes - 1,
                exact_bytes
            ));
        }
        let expected_length = end
            .checked_sub(start)
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| anyhow!("Partial response range length overflow"))?;
        if let Some(content_length) = Self::declared_content_length(response)? {
            if content_length != expected_length {
                return Err(anyhow!(
                    "Partial response declared {} bytes, expected {}",
                    content_length,
                    expected_length
                ));
            }
        }
        Ok(())
    }

    fn validate_unsatisfied_response(response: &reqwest::Response, exact_bytes: u64) -> Result<()> {
        if response.status() != reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Err(anyhow!(
                "Expected range-not-satisfiable 416 response, received {}",
                response.status()
            ));
        }
        let content_range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .ok_or_else(|| anyhow!("416 response is missing Content-Range"))?;
        match parse_content_range(content_range)? {
            ContentRange::Unsatisfied { total } if total == exact_bytes => Ok(()),
            ContentRange::Unsatisfied { total } => Err(anyhow!(
                "416 response reports {} total bytes, expected {}",
                total,
                exact_bytes
            )),
            ContentRange::Range { .. } => Err(anyhow!("416 response has a satisfied Content-Range")),
        }
    }

    async fn download_model_detailed_from_source(
        &self,
        model_name: &str,
        model_dir: &Path,
        base_url: &str,
        artifacts: &[ArtifactSpec],
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        let active_download = self.reserve_active_download(model_name).await?;
        self.set_downloading_status(model_name, 0).await;

        let result = self
            .download_artifacts(
                model_name,
                model_dir,
                base_url,
                artifacts,
                &active_download,
                progress_callback,
            )
            .await;
        let (result, progress_callback) = match result {
            Ok((progress, progress_callback)) => (Ok(progress), progress_callback),
            Err(error) => (Err(error), None),
        };

        self.finish_download(
            model_name,
            model_dir,
            artifacts,
            &active_download,
            result,
            progress_callback,
        )
        .await
    }

    async fn download_artifacts(
        &self,
        model_name: &str,
        model_dir: &Path,
        base_url: &str,
        artifacts: &[ArtifactSpec],
        active_download: &ActiveDownload,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<(DownloadProgress, Option<Box<dyn Fn(DownloadProgress) + Send>>)> {
        if active_download.cancellation.is_cancelled() {
            return Err(DownloadCancelled.into());
        }
        fs::create_dir_all(model_dir)
            .await
            .map_err(|error| anyhow!("Failed to create model directory: {}", error))?;

        let client = reqwest::Client::builder()
            .tcp_nodelay(true)
            .pool_max_idle_per_host(1)
            .timeout(Duration::from_secs(3600))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| anyhow!("Failed to create HTTP client: {}", error))?;
        let total_bytes: u64 = artifacts.iter().map(|artifact| artifact.exact_bytes).sum();
        let download_started = Instant::now();
        let mut confirmed_bytes = 0u64;
        let mut streamed_bytes = 0u64;
        let mut bytes_since_report = 0u64;
        let mut last_report = Instant::now();
        let mut last_percent = 0u8;

        for artifact in artifacts {
            if active_download.cancellation.is_cancelled() {
                return Err(DownloadCancelled.into());
            }

            let file_path = model_dir.join(artifact.filename);
            let local_bytes = match fs::metadata(&file_path).await {
                Ok(metadata) => metadata.len(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
                Err(error) => {
                    return Err(anyhow!(
                        "Failed to read {} metadata: {}",
                        artifact.filename,
                        error
                    ));
                }
            };

            if local_bytes == artifact.exact_bytes {
                confirmed_bytes = confirmed_bytes
                    .checked_add(artifact.exact_bytes)
                    .ok_or_else(|| anyhow!("Progress overflow while skipping {}", artifact.filename))?;
                let progress = Self::in_flight_progress(confirmed_bytes, total_bytes, 0.0);
                if let Some(callback) = &progress_callback {
                    callback(progress.clone());
                }
                self.set_downloading_status(model_name, progress.percent).await;
                last_percent = progress.percent;
                last_report = Instant::now();
                continue;
            }

            let file_url = format!("{}/{}", base_url.trim_end_matches('/'), artifact.filename);
            let range_start = (local_bytes > 0 && local_bytes < artifact.exact_bytes)
                .then_some(local_bytes);
            let response = self
                .send_download_request(&client, &file_url, range_start, active_download)
                .await?;

            let (response, mut artifact_bytes, append) = match range_start {
                Some(range_start) => match response.status() {
                    reqwest::StatusCode::PARTIAL_CONTENT => {
                        Self::validate_partial_response(&response, range_start, artifact.exact_bytes)?;
                        confirmed_bytes = confirmed_bytes
                            .checked_add(range_start)
                            .ok_or_else(|| anyhow!("Progress overflow while resuming {}", artifact.filename))?;
                        let progress =
                            Self::in_flight_progress(confirmed_bytes, total_bytes, 0.0);
                        if let Some(callback) = &progress_callback {
                            callback(progress.clone());
                        }
                        self.set_downloading_status(model_name, progress.percent).await;
                        last_percent = progress.percent;
                        last_report = Instant::now();
                        (response, range_start, true)
                    }
                    reqwest::StatusCode::OK => {
                        Self::validate_full_response(&response, artifact.exact_bytes)?;
                        (response, 0, false)
                    }
                    reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                        Self::validate_unsatisfied_response(&response, artifact.exact_bytes)?;
                        let retry = self
                            .send_download_request(&client, &file_url, None, active_download)
                            .await?;
                        Self::validate_full_response(&retry, artifact.exact_bytes)?;
                        (retry, 0, false)
                    }
                    status => {
                        return Err(anyhow!(
                            "Download failed for {} with status {}",
                            artifact.filename,
                            status
                        ));
                    }
                },
                None => match response.status() {
                    reqwest::StatusCode::OK => {
                        Self::validate_full_response(&response, artifact.exact_bytes)?;
                        (response, 0, false)
                    }
                    reqwest::StatusCode::PARTIAL_CONTENT => {
                        Self::validate_partial_response(&response, 0, artifact.exact_bytes)?;
                        (response, 0, false)
                    }
                    status => {
                        return Err(anyhow!(
                            "Download failed for {} with status {}",
                            artifact.filename,
                            status
                        ));
                    }
                },
            };

            if active_download.cancellation.is_cancelled() {
                return Err(DownloadCancelled.into());
            }
            let file = if append {
                fs::OpenOptions::new()
                    .append(true)
                    .open(&file_path)
                    .await
                    .map_err(|error| anyhow!("Failed to open {} for resume: {}", artifact.filename, error))?
            } else {
                fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&file_path)
                    .await
                    .map_err(|error| anyhow!("Failed to replace {}: {}", artifact.filename, error))?
            };
            let mut writer = BufWriter::with_capacity(8 * 1024 * 1024, file);
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();

            loop {
                let next_chunk = tokio::select! {
                    biased;
                    _ = active_download.cancellation.cancelled() => {
                        writer.flush().await.map_err(|error| {
                            anyhow!("Failed to preserve {} during cancellation: {}", artifact.filename, error)
                        })?;
                        return Err(DownloadCancelled.into());
                    }
                    chunk = timeout(Duration::from_secs(30), stream.next()) => chunk,
                };
                let chunk = match next_chunk {
                    Err(_) => {
                        writer.flush().await.map_err(|error| {
                            anyhow!("Failed to preserve {} after timeout: {}", artifact.filename, error)
                        })?;
                        return Err(anyhow!(
                            "Download timeout for {}: no data received for 30 seconds",
                            artifact.filename
                        ));
                    }
                    Ok(None) => break,
                    Ok(Some(Err(error))) => {
                        writer.flush().await.map_err(|flush_error| {
                            anyhow!(
                                "Failed to preserve {} after stream error: {}",
                                artifact.filename,
                                flush_error
                            )
                        })?;
                        return Err(anyhow!("Download stream failed for {}: {}", artifact.filename, error));
                    }
                    Ok(Some(Ok(chunk))) => chunk,
                };

                let chunk_bytes = chunk.len() as u64;
                let next_artifact_bytes = artifact_bytes
                    .checked_add(chunk_bytes)
                    .ok_or_else(|| anyhow!("{} size overflow", artifact.filename))?;
                if next_artifact_bytes > artifact.exact_bytes {
                    writer.flush().await.map_err(|error| {
                        anyhow!("Failed to preserve {} after overlong response: {}", artifact.filename, error)
                    })?;
                    return Err(anyhow!(
                        "{} response exceeds its exact {} byte size",
                        artifact.filename,
                        artifact.exact_bytes
                    ));
                }
                let next_confirmed_bytes = confirmed_bytes
                    .checked_add(chunk_bytes)
                    .ok_or_else(|| anyhow!("Progress overflow while downloading {}", artifact.filename))?;
                if next_confirmed_bytes > total_bytes {
                    return Err(anyhow!("Download progress exceeds the catalog total"));
                }

                writer
                    .write_all(&chunk)
                    .await
                    .map_err(|error| anyhow!("Failed to write {}: {}", artifact.filename, error))?;
                artifact_bytes = next_artifact_bytes;
                confirmed_bytes = next_confirmed_bytes;
                streamed_bytes = streamed_bytes
                    .checked_add(chunk_bytes)
                    .ok_or_else(|| anyhow!("Streamed byte count overflow"))?;
                bytes_since_report = bytes_since_report
                    .checked_add(chunk_bytes)
                    .ok_or_else(|| anyhow!("Progress byte count overflow"))?;

                let progress = Self::in_flight_progress(confirmed_bytes, total_bytes, 0.0);
                let elapsed = last_report.elapsed();
                if progress.percent > last_percent
                    || elapsed >= Duration::from_millis(500)
                    || artifact_bytes == artifact.exact_bytes
                {
                    let speed_mbps = if elapsed.as_secs_f64() > 0.0 {
                        bytes_since_report as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64()
                    } else {
                        0.0
                    };
                    if let Some(callback) = &progress_callback {
                        callback(Self::in_flight_progress(
                            confirmed_bytes,
                            total_bytes,
                            speed_mbps,
                        ));
                    }
                    self.set_downloading_status(model_name, progress.percent).await;
                    last_percent = progress.percent;
                    last_report = Instant::now();
                    bytes_since_report = 0;
                }
            }

            writer
                .flush()
                .await
                .map_err(|error| anyhow!("Failed to flush {}: {}", artifact.filename, error))?;
            drop(writer);

            if active_download.cancellation.is_cancelled() {
                return Err(DownloadCancelled.into());
            }
            let stored_bytes = fs::metadata(&file_path)
                .await
                .map_err(|error| anyhow!("Failed to read {} after download: {}", artifact.filename, error))?
                .len();
            if stored_bytes != artifact.exact_bytes {
                return Err(anyhow!(
                    "{} stored {} bytes, expected exactly {} bytes",
                    artifact.filename,
                    stored_bytes,
                    artifact.exact_bytes
                ));
            }
        }

        if confirmed_bytes != total_bytes {
            return Err(anyhow!(
                "Download confirmed {} bytes, expected {} bytes",
                confirmed_bytes,
                total_bytes
            ));
        }
        let elapsed = download_started.elapsed().as_secs_f64();
        let speed_mbps = if elapsed > 0.0 {
            streamed_bytes as f64 / (1024.0 * 1024.0) / elapsed
        } else {
            0.0
        };
        Ok((
            DownloadProgress::new(total_bytes, total_bytes, speed_mbps),
            progress_callback,
        ))
    }

    async fn finish_download(
        &self,
        model_name: &str,
        model_dir: &Path,
        artifacts: &[ArtifactSpec],
        active_download: &Arc<ActiveDownload>,
        mut result: Result<DownloadProgress>,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        if result.is_ok() && !active_download.cancellation.is_cancelled() {
            let final_progress = result.expect("successful transfer must carry final progress");
            result = Self::validate_model_directory(model_dir, artifacts).map(|()| final_progress);
        }

        #[cfg(test)]
        if let Some(hook) = self.test_hook().await {
            hook.finalization_ready.notify_one();
            hook.continue_finalization.notified().await;
        }

        let mut active_downloads = self.active_downloads.lock().await;
        let owner_matches = matches!(
            active_downloads.get(model_name),
            Some(owner) if Arc::ptr_eq(owner, active_download)
        );
        if !owner_matches {
            drop(active_downloads);
            active_download.completion.send_replace(true);
            return result.map(|_| ());
        }

        let cancellation_won = active_download.cancellation.is_cancelled()
            || result
                .as_ref()
                .err()
                .is_some_and(is_download_cancelled);
        let mut models = self.available_models.write().await;
        active_downloads.remove(model_name);
        if let Some(model) = models.get_mut(model_name) {
            if cancellation_won || result.is_err() {
                model.status = ModelStatus::Missing;
            } else {
                model.status = ModelStatus::Available;
                model.path = model_dir.to_path_buf();
            }
        }
        drop(models);
        drop(active_downloads);
        active_download.completion.send_replace(true);

        if cancellation_won {
            return Err(DownloadCancelled.into());
        }
        let final_progress = result?;
        if let Some(callback) = progress_callback {
            callback(final_progress);
        }
        log::info!("Download completed for Parakeet model: {}", model_name);
        Ok(())
    }

    /// Cancel an ongoing model download.
    pub async fn cancel_download(&self, model_name: &str) -> Result<CancelDownloadOutcome> {
        self.cancel_download_with_timeout(model_name, CANCEL_DOWNLOAD_CLEANUP_TIMEOUT)
            .await
    }

    async fn cancel_download_with_timeout(
        &self,
        model_name: &str,
        cleanup_timeout: Duration,
    ) -> Result<CancelDownloadOutcome> {
        let active_download = {
            let active_downloads = self.active_downloads.lock().await;
            let Some(active_download) = active_downloads.get(model_name).cloned() else {
                return Ok(CancelDownloadOutcome::Cancelled);
            };
            active_download.cancellation.cancel();
            active_download
        };

        let mut completion = active_download.completion.subscribe();
        if !*completion.borrow() {
            match timeout(cleanup_timeout, completion.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => {
                    return Err(anyhow!(
                        "Download worker ended before completing cancellation cleanup"
                    ));
                }
                Err(_) => return Ok(CancelDownloadOutcome::Pending),
            }
        }

        Ok(CancelDownloadOutcome::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::queue::SegQueue;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const TEST_MODEL_NAME: &str = "parakeet-test";
    const SMALL_ARTIFACTS: &[ArtifactSpec] = &[
        ArtifactSpec { filename: "encoder.bin", exact_bytes: 4 },
        ArtifactSpec { filename: "decoder.bin", exact_bytes: 3 },
        ArtifactSpec { filename: "nemo.bin", exact_bytes: 2 },
        ArtifactSpec { filename: "vocab.txt", exact_bytes: 1 },
    ];

    struct ExpectedResponse {
        filename: &'static str,
        range: Option<&'static str>,
        status: &'static str,
        content_length: Option<u64>,
        content_range: Option<&'static str>,
        body: &'static [u8],
    }

    fn response(
        filename: &'static str,
        range: Option<&'static str>,
        status: &'static str,
        body: &'static [u8],
        content_range: Option<&'static str>,
    ) -> ExpectedResponse {
        ExpectedResponse {
            filename,
            range,
            status,
            content_length: Some(body.len() as u64),
            content_range,
            body,
        }
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let read = socket.read(&mut buffer).await.expect("read request");
            assert_ne!(read, 0, "request ended before its headers");
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                return String::from_utf8(request).expect("request is valid UTF-8");
            }
        }
    }

    async fn serve_requests(
        expected_responses: Vec<ExpectedResponse>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback test server");
        let address = listener.local_addr().expect("get loopback address");
        let server = tokio::spawn(async move {
            for expected in expected_responses {
                let (mut socket, _) = listener.accept().await.expect("accept test request");
                let request = read_request(&mut socket).await;
                assert!(
                    request.starts_with(&format!("GET /{} HTTP/", expected.filename)),
                    "unexpected request path: {request}"
                );
                let requested_range = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("range").then_some(value.trim())
                });
                assert_eq!(requested_range, expected.range);

                let mut headers = format!("HTTP/1.1 {}\r\nConnection: close\r\n", expected.status);
                if let Some(content_length) = expected.content_length {
                    headers.push_str(&format!("Content-Length: {content_length}\r\n"));
                }
                if let Some(content_range) = expected.content_range {
                    headers.push_str(&format!("Content-Range: {content_range}\r\n"));
                }
                headers.push_str("\r\n");
                socket
                    .write_all(headers.as_bytes())
                    .await
                    .expect("write response headers");
                socket
                    .write_all(expected.body)
                    .await
                    .expect("write response body");
            }
        });

        (format!("http://{address}"), server)
    }

    async fn test_engine() -> (tempfile::TempDir, Arc<ParakeetEngine>, PathBuf) {
        let temp_dir = tempdir().expect("create temporary models directory");
        let engine = Arc::new(
            ParakeetEngine::new_with_models_dir(Some(temp_dir.path().to_path_buf()))
                .expect("create Parakeet engine"),
        );
        let model_dir = engine.models_dir.join(TEST_MODEL_NAME);
        engine.available_models.write().await.insert(
            TEST_MODEL_NAME.to_string(),
            ModelInfo {
                name: TEST_MODEL_NAME.to_string(),
                path: model_dir.clone(),
                size_mb: 1,
                quantization: QuantizationType::Int8,
                speed: "test".to_string(),
                status: ModelStatus::Missing,
                description: "test model".to_string(),
            },
        );
        (temp_dir, engine, model_dir)
    }

    async fn test_model_status(engine: &ParakeetEngine) -> ModelStatus {
        engine
            .available_models
            .read()
            .await
            .get(TEST_MODEL_NAME)
            .expect("test model remains registered")
            .status
            .clone()
    }

    #[tokio::test]
    async fn directory_validation_requires_exact_artifact_sizes() {
        let temp_dir = tempdir().expect("create temporary model directory");
        for artifact in SMALL_ARTIFACTS {
            fs::write(
                temp_dir.path().join(artifact.filename),
                vec![0; artifact.exact_bytes as usize],
            )
            .await
            .expect("seed exact artifact");
        }
        assert!(ParakeetEngine::validate_model_directory(temp_dir.path(), SMALL_ARTIFACTS).is_ok());

        fs::remove_file(temp_dir.path().join("vocab.txt"))
            .await
            .expect("remove required artifact");
        assert!(ParakeetEngine::validate_model_directory(temp_dir.path(), SMALL_ARTIFACTS).is_err());

        fs::write(temp_dir.path().join("vocab.txt"), [])
            .await
            .expect("restore undersized artifact");
        fs::write(temp_dir.path().join("encoder.bin"), [0; 3])
            .await
            .expect("seed one-byte-short artifact");
        assert!(ParakeetEngine::validate_model_directory(temp_dir.path(), SMALL_ARTIFACTS).is_err());

        fs::write(temp_dir.path().join("encoder.bin"), [0; 5])
            .await
            .expect("seed one-byte-oversized artifact");
        assert!(ParakeetEngine::validate_model_directory(temp_dir.path(), SMALL_ARTIFACTS).is_err());
    }

    #[tokio::test]
    async fn completed_sibling_survives_403_then_retry_resumes_partial() {
        let (_temp_dir, engine, model_dir) = test_engine().await;
        fs::create_dir_all(&model_dir).await.expect("create model directory");
        fs::write(model_dir.join("encoder.bin"), b"ABCD")
            .await
            .expect("seed completed sibling");
        fs::write(model_dir.join("decoder.bin"), b"X")
            .await
            .expect("seed resumable artifact");

        let (base_url, server) = serve_requests(vec![response(
            "decoder.bin",
            Some("bytes=1-"),
            "403 Forbidden",
            b"",
            None,
        )])
        .await;
        let error = engine
            .download_model_detailed_from_source(
                TEST_MODEL_NAME,
                &model_dir,
                &base_url,
                SMALL_ARTIFACTS,
                None,
            )
            .await
            .expect_err("403 must fail without destructive cleanup");
        server.await.expect("join 403 server");
        assert!(error.to_string().contains("403"));
        assert_eq!(fs::read(model_dir.join("encoder.bin")).await.unwrap(), b"ABCD");
        assert_eq!(fs::read(model_dir.join("decoder.bin")).await.unwrap(), b"X");
        assert!(!engine.active_downloads.lock().await.contains_key(TEST_MODEL_NAME));

        let (base_url, server) = serve_requests(vec![
            response(
                "decoder.bin",
                Some("bytes=1-"),
                "206 Partial Content",
                b"YZ",
                Some("bytes 1-2/3"),
            ),
            response("nemo.bin", None, "200 OK", b"NO", None),
            response("vocab.txt", None, "200 OK", b"V", None),
        ])
        .await;
        engine
            .download_model_detailed_from_source(
                TEST_MODEL_NAME,
                &model_dir,
                &base_url,
                SMALL_ARTIFACTS,
                None,
            )
            .await
            .expect("retry resumes only the partial artifact");
        server.await.expect("join retry server");

        assert_eq!(fs::read(model_dir.join("encoder.bin")).await.unwrap(), b"ABCD");
        assert_eq!(fs::read(model_dir.join("decoder.bin")).await.unwrap(), b"XYZ");
        assert!(matches!(test_model_status(&engine).await, ModelStatus::Available));
        assert!(!engine.active_downloads.lock().await.contains_key(TEST_MODEL_NAME));
    }

    #[tokio::test]
    async fn near_complete_artifact_is_resumed_not_skipped() {
        const ARTIFACTS: &[ArtifactSpec] = &[ArtifactSpec {
            filename: "near.bin",
            exact_bytes: 100,
        }];
        let (_temp_dir, engine, model_dir) = test_engine().await;
        fs::create_dir_all(&model_dir).await.expect("create model directory");
        fs::write(model_dir.join("near.bin"), vec![b'A'; 99])
            .await
            .expect("seed 99 percent artifact");

        let (base_url, server) = serve_requests(vec![response(
            "near.bin",
            Some("bytes=99-"),
            "206 Partial Content",
            b"B",
            Some("bytes 99-99/100"),
        )])
        .await;
        engine
            .download_model_detailed_from_source(
                TEST_MODEL_NAME,
                &model_dir,
                &base_url,
                ARTIFACTS,
                None,
            )
            .await
            .expect("99 percent artifact must resume");
        server.await.expect("join resume server");
        assert_eq!(
            fs::metadata(model_dir.join("near.bin")).await.unwrap().len(),
            100
        );
    }

    #[tokio::test]
    async fn range_ignored_replaces_partial_with_honest_progress() {
        const ARTIFACTS: &[ArtifactSpec] = &[ArtifactSpec {
            filename: "model.bin",
            exact_bytes: 4,
        }];
        let (_temp_dir, engine, model_dir) = test_engine().await;
        fs::create_dir_all(&model_dir).await.expect("create model directory");
        fs::write(model_dir.join("model.bin"), b"zz")
            .await
            .expect("seed partial artifact");
        let events = Arc::new(SegQueue::new());
        let callback_events = Arc::clone(&events);

        let (base_url, server) = serve_requests(vec![response(
            "model.bin",
            Some("bytes=2-"),
            "200 OK",
            b"ABCD",
            None,
        )])
        .await;
        engine
            .download_model_detailed_from_source(
                TEST_MODEL_NAME,
                &model_dir,
                &base_url,
                ARTIFACTS,
                Some(Box::new(move |progress| {
                    callback_events.push(progress);
                })),
            )
            .await
            .expect("range-ignored response replaces the partial artifact");
        server.await.expect("join range-ignored server");

        let events: Vec<_> = std::iter::from_fn(|| events.pop()).collect();
        assert_eq!(fs::read(model_dir.join("model.bin")).await.unwrap(), b"ABCD");
        assert!(events.iter().all(|progress| progress.downloaded_bytes <= progress.total_bytes));
        assert_eq!(events.last().expect("final event").percent, 100);
        assert!(events[..events.len() - 1].iter().all(|progress| progress.percent < 100));
    }

    #[tokio::test]
    async fn range_416_retries_fresh_with_honest_progress() {
        const ARTIFACTS: &[ArtifactSpec] = &[ArtifactSpec {
            filename: "model.bin",
            exact_bytes: 4,
        }];
        let (_temp_dir, engine, model_dir) = test_engine().await;
        fs::create_dir_all(&model_dir).await.expect("create model directory");
        fs::write(model_dir.join("model.bin"), b"zz")
            .await
            .expect("seed partial artifact");
        let events = Arc::new(SegQueue::new());
        let callback_events = Arc::clone(&events);

        let (base_url, server) = serve_requests(vec![
            response(
                "model.bin",
                Some("bytes=2-"),
                "416 Range Not Satisfiable",
                b"",
                Some("bytes */4"),
            ),
            response("model.bin", None, "200 OK", b"ABCD", None),
        ])
        .await;
        engine
            .download_model_detailed_from_source(
                TEST_MODEL_NAME,
                &model_dir,
                &base_url,
                ARTIFACTS,
                Some(Box::new(move |progress| {
                    callback_events.push(progress);
                })),
            )
            .await
            .expect("416 should retry without Range");
        server.await.expect("join 416 server");

        assert_eq!(fs::read(model_dir.join("model.bin")).await.unwrap(), b"ABCD");
        assert!(std::iter::from_fn(|| events.pop())
            .all(|progress| progress.downloaded_bytes <= progress.total_bytes));
    }

    #[tokio::test]
    async fn invalid_or_short_response_never_publishes_available() {
        const ARTIFACTS: &[ArtifactSpec] = &[
            ArtifactSpec {
                filename: "complete.bin",
                exact_bytes: 1,
            },
            ArtifactSpec {
                filename: "target.bin",
                exact_bytes: 4,
            },
        ];
        let cases = vec![
            (
                "malformed",
                ExpectedResponse {
                    filename: "target.bin",
                    range: Some("bytes=1-"),
                    status: "206 Partial Content",
                    content_length: Some(3),
                    content_range: Some("bytes invalid"),
                    body: b"XYZ",
                },
            ),
            (
                "wrong-start",
                ExpectedResponse {
                    filename: "target.bin",
                    range: Some("bytes=1-"),
                    status: "206 Partial Content",
                    content_length: Some(4),
                    content_range: Some("bytes 0-3/4"),
                    body: b"ABCD",
                },
            ),
            (
                "wrong-total",
                ExpectedResponse {
                    filename: "target.bin",
                    range: Some("bytes=1-"),
                    status: "206 Partial Content",
                    content_length: Some(3),
                    content_range: Some("bytes 1-3/5"),
                    body: b"XYZ",
                },
            ),
            (
                "short",
                ExpectedResponse {
                    filename: "target.bin",
                    range: Some("bytes=1-"),
                    status: "200 OK",
                    content_length: Some(4),
                    content_range: None,
                    body: b"ABC",
                },
            ),
            (
                "overlong",
                ExpectedResponse {
                    filename: "target.bin",
                    range: Some("bytes=1-"),
                    status: "200 OK",
                    content_length: Some(5),
                    content_range: None,
                    body: b"ABCDE",
                },
            ),
        ];

        for (case_name, invalid_response) in cases {
            let (_temp_dir, engine, model_dir) = test_engine().await;
            fs::create_dir_all(&model_dir).await.expect("create model directory");
            fs::write(model_dir.join("complete.bin"), b"C")
                .await
                .expect("seed completed sibling");
            fs::write(model_dir.join("target.bin"), b"Z")
                .await
                .expect("seed retained prefix");

            let (base_url, server) = serve_requests(vec![invalid_response]).await;
            assert!(
                engine
                    .download_model_detailed_from_source(
                        TEST_MODEL_NAME,
                        &model_dir,
                        &base_url,
                        ARTIFACTS,
                        None,
                    )
                    .await
                    .is_err(),
                "{case_name} response must fail"
            );
            server.await.expect("join invalid-response server");
            assert_eq!(fs::read(model_dir.join("complete.bin")).await.unwrap(), b"C");
            assert!(!matches!(test_model_status(&engine).await, ModelStatus::Available));
            assert!(!engine.active_downloads.lock().await.contains_key(TEST_MODEL_NAME));
        }
    }

    #[tokio::test]
    async fn pending_cancellation_keeps_owner_and_blocks_retry() {
        let (_temp_dir, engine, model_dir) = test_engine().await;
        fs::create_dir_all(&model_dir).await.expect("create model directory");
        fs::write(model_dir.join("encoder.bin"), b"AB")
            .await
            .expect("seed resumable prefix");
        let owner = engine
            .reserve_active_download(TEST_MODEL_NAME)
            .await
            .expect("reserve initial owner");

        assert_eq!(
            engine
                .cancel_download_with_timeout(TEST_MODEL_NAME, Duration::from_millis(1))
                .await
                .expect("request cancellation"),
            CancelDownloadOutcome::Pending
        );
        assert!(engine.reserve_active_download(TEST_MODEL_NAME).await.is_err());

        let error = engine
            .finish_download(
                TEST_MODEL_NAME,
                &model_dir,
                SMALL_ARTIFACTS,
                &owner,
                Err(DownloadCancelled.into()),
                None,
            )
            .await
            .expect_err("cancelled owner must finish as cancellation");
        assert!(is_download_cancelled(&error));
        assert_eq!(fs::read(model_dir.join("encoder.bin")).await.unwrap(), b"AB");
        assert!(!engine.active_downloads.lock().await.contains_key(TEST_MODEL_NAME));
        assert!(engine.reserve_active_download(TEST_MODEL_NAME).await.is_ok());
    }

    #[tokio::test]
    async fn cancellation_wins_before_terminal_commit() {
        const ARTIFACTS: &[ArtifactSpec] = &[ArtifactSpec {
            filename: "model.bin",
            exact_bytes: 4,
        }];
        let (_temp_dir, engine, model_dir) = test_engine().await;
        let hook = Arc::new(DownloadStateTestHook {
            finalization_ready: tokio::sync::Notify::new(),
            continue_finalization: tokio::sync::Notify::new(),
        });
        *engine.download_state_test_hook.lock().await = Some(Arc::clone(&hook));
        let events = Arc::new(SegQueue::new());
        let callback_events = Arc::clone(&events);

        let (base_url, server) =
            serve_requests(vec![response("model.bin", None, "200 OK", b"ABCD", None)]).await;
        let download_engine = Arc::clone(&engine);
        let download_dir = model_dir.clone();
        let download = tokio::spawn(async move {
            download_engine
                .download_model_detailed_from_source(
                    TEST_MODEL_NAME,
                    &download_dir,
                    &base_url,
                    ARTIFACTS,
                    Some(Box::new(move |progress| {
                        callback_events.push(progress);
                    })),
                )
                .await
        });

        hook.finalization_ready.notified().await;
        assert_eq!(
            engine
                .cancel_download_with_timeout(TEST_MODEL_NAME, Duration::from_millis(1))
                .await
                .expect("request cancellation while finalization is paused"),
            CancelDownloadOutcome::Pending
        );
        hook.continue_finalization.notify_one();

        let error = download
            .await
            .expect("join download task")
            .expect_err("cancellation must win before terminal commit");
        server.await.expect("join cancellation server");
        assert!(is_download_cancelled(&error));
        assert!(std::iter::from_fn(|| events.pop()).all(|progress| progress.percent < 100));
        assert!(matches!(test_model_status(&engine).await, ModelStatus::Missing));
        assert!(!engine.active_downloads.lock().await.contains_key(TEST_MODEL_NAME));
    }
}
