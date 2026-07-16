use super::model::{
    inspect_model, mark_model_verified, model_info, verify_model_hashes, DownloadProgress,
    ModelInfo, ModelStatus, MODEL_FILES, QWEN3_ASR_MODEL, QWEN3_ASR_REVISION, QWEN3_ASR_SIZE_BYTES,
};
use crate::audio::transcription::provider::{
    TranscriptResult, TranscriptionError, TranscriptionProvider,
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::RANGE;
use sherpa_onnx::{OfflineQwen3ASRModelConfig, OfflineRecognizer, OfflineRecognizerConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

const SAMPLE_RATE: i32 = 16_000;
const MIN_AUDIO_SAMPLES: usize = 1_600;
const MODEL_BASE_URL: &str =
    "https://huggingface.co/csukuangfj2/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25/resolve";

pub struct QwenAsrEngine {
    models_dir: PathBuf,
    recognizer: RwLock<Option<Arc<OfflineRecognizer>>>,
    current_model: RwLock<Option<String>>,
    downloading: AtomicBool,
    cancel_download: AtomicBool,
}

impl QwenAsrEngine {
    pub fn new(models_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&models_dir).with_context(|| {
            format!(
                "Failed to create Qwen model directory {}",
                models_dir.display()
            )
        })?;
        Ok(Self {
            models_dir,
            recognizer: RwLock::new(None),
            current_model: RwLock::new(None),
            downloading: AtomicBool::new(false),
            cancel_download: AtomicBool::new(false),
        })
    }

    pub fn discover_model(&self) -> ModelInfo {
        let mut info = model_info(&self.models_dir);
        if self.downloading.load(Ordering::SeqCst) {
            info.status = ModelStatus::Downloading { progress: 0 };
        }
        info
    }

    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        if model_name != QWEN3_ASR_MODEL {
            return Err(anyhow!("Unknown Qwen3-ASR model: {model_name}"));
        }

        if self.current_model.read().await.as_deref() == Some(model_name)
            && self.recognizer.read().await.is_some()
        {
            return Ok(());
        }

        let model_dir = self.models_dir.join(model_name);
        if inspect_model(&model_dir) != ModelStatus::Available {
            return Err(anyhow!(
                "Qwen3-ASR model is not downloaded or is incomplete: {}",
                model_dir.display()
            ));
        }

        let recognizer = tokio::task::spawn_blocking(move || create_recognizer(&model_dir))
            .await
            .map_err(|error| anyhow!("Qwen3-ASR model loading task failed: {error}"))??;

        *self.recognizer.write().await = Some(Arc::new(recognizer));
        *self.current_model.write().await = Some(model_name.to_string());
        log::info!("Qwen3-ASR model '{}' loaded", model_name);
        Ok(())
    }

    pub async fn unload_model(&self) -> bool {
        let unloaded = self.recognizer.write().await.take().is_some();
        self.current_model.write().await.take();
        unloaded
    }

    pub async fn is_model_loaded(&self) -> bool {
        self.recognizer.read().await.is_some()
    }

    pub async fn get_current_model(&self) -> Option<String> {
        self.current_model.read().await.clone()
    }

    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<String> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Err(anyhow!(
                "Audio too short: {} samples (minimum {})",
                audio.len(),
                MIN_AUDIO_SAMPLES
            ));
        }

        let recognizer = self
            .recognizer
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("No Qwen3-ASR model is loaded"))?;

        tokio::task::spawn_blocking(move || {
            let stream = recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE, &audio);
            recognizer.decode(&stream);
            stream
                .get_result()
                .map(|result| result.text.trim().to_string())
                .ok_or_else(|| anyhow!("Qwen3-ASR returned no result"))
        })
        .await
        .map_err(|error| anyhow!("Qwen3-ASR inference task failed: {error}"))?
    }

    pub async fn download_model<F>(&self, progress: F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        if self
            .downloading
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow!("Qwen3-ASR model download is already running"));
        }
        self.cancel_download.store(false, Ordering::SeqCst);

        let result = self.download_model_inner(&progress).await;
        self.downloading.store(false, Ordering::SeqCst);
        result
    }

    async fn download_model_inner<F>(&self, progress: &F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        let model_dir = self.models_dir.join(QWEN3_ASR_MODEL);
        tokio::fs::create_dir_all(&model_dir).await?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(7_200))
            .build()?;

        let started = Instant::now();
        let mut completed_bytes = existing_valid_bytes(&model_dir).await;
        let initial_bytes = completed_bytes;
        let mut last_progress_emit = Instant::now();
        emit_progress(progress, completed_bytes, initial_bytes, started);

        for file in MODEL_FILES {
            if self.cancel_download.load(Ordering::SeqCst) {
                return Err(anyhow!("Qwen3-ASR model download cancelled"));
            }

            let destination = model_dir.join(file.relative_path);
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }

            let existing_size = tokio::fs::metadata(&destination)
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            if existing_size == file.size {
                continue;
            }
            if existing_size > file.size {
                tokio::fs::remove_file(&destination).await?;
            }
            let resume_from = if existing_size < file.size {
                existing_size
            } else {
                0
            };
            let url = format!(
                "{}/{}/{}",
                MODEL_BASE_URL, QWEN3_ASR_REVISION, file.relative_path
            );
            let mut request = client.get(url);
            if resume_from > 0 {
                request = request.header(RANGE, format!("bytes={resume_from}-"));
            }
            let response = request.send().await?;
            let resuming = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            if !response.status().is_success() {
                return Err(anyhow!(
                    "Failed to download {}: HTTP {}",
                    file.relative_path,
                    response.status()
                ));
            }

            let mut options = tokio::fs::OpenOptions::new();
            options.create(true).write(true);
            if resuming {
                options.append(true);
            } else {
                options.truncate(true);
                completed_bytes = completed_bytes.saturating_sub(resume_from);
            }
            let mut output = options.open(&destination).await?;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                if self.cancel_download.load(Ordering::SeqCst) {
                    return Err(anyhow!("Qwen3-ASR model download cancelled"));
                }
                let chunk = chunk?;
                output.write_all(&chunk).await?;
                completed_bytes += chunk.len() as u64;
                if last_progress_emit.elapsed() >= std::time::Duration::from_millis(250) {
                    emit_progress(progress, completed_bytes, initial_bytes, started);
                    last_progress_emit = Instant::now();
                }
            }
            output.flush().await?;

            let actual_size = tokio::fs::metadata(&destination).await?.len();
            if actual_size != file.size {
                return Err(anyhow!(
                    "Downloaded {} has {} bytes, expected {}",
                    file.relative_path,
                    actual_size,
                    file.size
                ));
            }
            emit_progress(progress, completed_bytes, initial_bytes, started);
        }

        let verification_dir = model_dir.clone();
        tokio::task::spawn_blocking(move || {
            verify_model_hashes(&verification_dir).map_err(anyhow::Error::msg)?;
            mark_model_verified(&verification_dir).map_err(anyhow::Error::msg)
        })
        .await
        .map_err(|error| anyhow!("Qwen3-ASR verification task failed: {error}"))??;
        if inspect_model(&model_dir) != ModelStatus::Available {
            return Err(anyhow!(
                "Downloaded Qwen3-ASR model failed final validation"
            ));
        }
        emit_progress(progress, QWEN3_ASR_SIZE_BYTES, initial_bytes, started);
        Ok(())
    }

    pub fn cancel_download(&self) -> bool {
        if self.downloading.load(Ordering::SeqCst) {
            self.cancel_download.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub async fn delete_model(&self) -> Result<()> {
        self.unload_model().await;
        let model_dir = self.models_dir.join(QWEN3_ASR_MODEL);
        if model_dir.exists() {
            tokio::fs::remove_dir_all(model_dir).await?;
        }
        Ok(())
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }
}

fn create_recognizer(model_dir: &Path) -> Result<OfflineRecognizer> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
        conv_frontend: Some(path_string(model_dir.join("conv_frontend.onnx"))?),
        encoder: Some(path_string(model_dir.join("encoder.int8.onnx"))?),
        decoder: Some(path_string(model_dir.join("decoder.int8.onnx"))?),
        tokenizer: Some(path_string(model_dir.join("tokenizer"))?),
        max_new_tokens: 512,
        ..Default::default()
    };
    config.model_config.num_threads = 3;
    config.model_config.provider = Some("cpu".to_string());
    config.decoding_method = Some("greedy_search".to_string());

    OfflineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("sherpa-onnx could not create the Qwen3-ASR recognizer"))
}

fn path_string(path: PathBuf) -> Result<String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| anyhow!("Qwen3-ASR model path is not valid UTF-8"))
}

async fn existing_valid_bytes(model_dir: &Path) -> u64 {
    let mut total = 0;
    for file in MODEL_FILES {
        if let Ok(metadata) = tokio::fs::metadata(model_dir.join(file.relative_path)).await {
            if metadata.len() <= file.size {
                total += metadata.len();
            }
        }
    }
    total
}

fn emit_progress<F>(callback: &F, downloaded_bytes: u64, initial_bytes: u64, started: Instant)
where
    F: Fn(DownloadProgress),
{
    let downloaded_bytes = downloaded_bytes.min(QWEN3_ASR_SIZE_BYTES);
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    callback(DownloadProgress {
        percent: ((downloaded_bytes as f64 / QWEN3_ASR_SIZE_BYTES as f64) * 100.0) as u8,
        downloaded_bytes,
        total_bytes: QWEN3_ASR_SIZE_BYTES,
        downloaded_mb: downloaded_bytes as f64 / 1_048_576.0,
        total_mb: QWEN3_ASR_SIZE_BYTES as f64 / 1_048_576.0,
        speed_mbps: downloaded_bytes.saturating_sub(initial_bytes) as f64 / 1_048_576.0 / elapsed,
    });
}

#[async_trait]
impl TranscriptionProvider for QwenAsrEngine {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        _language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_AUDIO_SAMPLES,
            });
        }
        let text = self
            .transcribe_audio(audio)
            .await
            .map_err(|error| TranscriptionError::EngineFailed(error.to_string()))?;
        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        QwenAsrEngine::is_model_loaded(self).await
    }

    async fn get_current_model(&self) -> Option<String> {
        QwenAsrEngine::get_current_model(self).await
    }

    fn provider_name(&self) -> &'static str {
        "Qwen3-ASR"
    }
}
