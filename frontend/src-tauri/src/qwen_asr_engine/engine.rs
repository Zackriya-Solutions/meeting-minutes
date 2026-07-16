use super::model::{
    inspect_model, mark_model_verified, model_info, model_spec, model_specs, verify_model_file,
    verify_model_hashes, DownloadProgress, ModelInfo, ModelSpec, ModelStatus,
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
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

const SAMPLE_RATE: i32 = 16_000;
const MIN_AUDIO_SAMPLES: usize = 1_600;
pub struct QwenAsrEngine {
    models_dir: PathBuf,
    recognizer: RwLock<Option<Arc<OfflineRecognizer>>>,
    current_model: RwLock<Option<String>>,
    downloading: AtomicBool,
    downloading_model: Mutex<Option<String>>,
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
            downloading_model: Mutex::new(None),
            cancel_download: AtomicBool::new(false),
        })
    }

    pub fn discover_models(&self) -> Vec<ModelInfo> {
        let downloading_model = self.downloading_model.lock().unwrap().clone();
        model_specs()
            .iter()
            .map(|spec| {
                let mut info = model_info(&self.models_dir, spec);
                if downloading_model.as_deref() == Some(spec.name) {
                    info.status = ModelStatus::Downloading { progress: 0 };
                }
                info
            })
            .collect()
    }

    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        let spec = model_spec(model_name)
            .ok_or_else(|| anyhow!("Unknown Qwen3-ASR model: {model_name}"))?;

        if self.current_model.read().await.as_deref() == Some(model_name)
            && self.recognizer.read().await.is_some()
        {
            return Ok(());
        }

        let model_dir = self.models_dir.join(model_name);
        if inspect_model(spec, &model_dir) != ModelStatus::Available {
            return Err(anyhow!(
                "Qwen3-ASR model is not downloaded or is incomplete: {}",
                model_dir.display()
            ));
        }

        self.recognizer.write().await.take();
        self.current_model.write().await.take();
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

    pub async fn transcribe_audio(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<String> {
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

        let language_hint = qwen_language_name(language.as_deref());
        let audio_duration_seconds = audio.len() as f64 / SAMPLE_RATE as f64;
        let started = Instant::now();
        let text = tokio::task::spawn_blocking(move || {
            let stream = recognizer.create_stream();
            if let Some(language_hint) = language_hint {
                stream.set_option("language", language_hint);
            }
            stream.accept_waveform(SAMPLE_RATE, &audio);
            recognizer.decode(&stream);
            stream
                .get_result()
                .map(|result| result.text.trim().to_string())
                .ok_or_else(|| anyhow!("Qwen3-ASR returned no result"))
        })
        .await
        .map_err(|error| anyhow!("Qwen3-ASR inference task failed: {error}"))??;
        log::info!(
            "Qwen3-ASR decoded {:.2}s of audio in {:.2}s (language: {})",
            audio_duration_seconds,
            started.elapsed().as_secs_f64(),
            language_hint.unwrap_or("auto")
        );
        Ok(text)
    }

    pub async fn download_model<F>(&self, model_name: &str, progress: F) -> Result<()>
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
        let spec = match model_spec(model_name) {
            Some(spec) => spec,
            None => {
                self.downloading.store(false, Ordering::SeqCst);
                return Err(anyhow!("Unknown Qwen3-ASR model: {model_name}"));
            }
        };
        *self.downloading_model.lock().unwrap() = Some(model_name.to_string());
        self.cancel_download.store(false, Ordering::SeqCst);

        let result = self.download_model_inner(spec, &progress).await;
        self.downloading_model.lock().unwrap().take();
        self.downloading.store(false, Ordering::SeqCst);
        result
    }

    async fn download_model_inner<F>(&self, spec: &'static ModelSpec, progress: &F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        let model_dir = self.models_dir.join(spec.name);
        tokio::fs::create_dir_all(&model_dir).await?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(7_200))
            .build()?;

        let started = Instant::now();
        let mut completed_bytes = existing_valid_bytes(spec, &model_dir).await;
        let initial_bytes = completed_bytes;
        let mut last_progress_emit = Instant::now();
        emit_progress(spec, progress, completed_bytes, initial_bytes, started);

        for file in spec.files {
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
                let file_to_verify = *file;
                let path_to_verify = destination.clone();
                let hash_matches = tokio::task::spawn_blocking(move || {
                    verify_model_file(&file_to_verify, &path_to_verify).is_ok()
                })
                .await
                .map_err(|error| anyhow!("Qwen3-ASR file verification task failed: {error}"))?;
                if hash_matches {
                    continue;
                }
                tokio::fs::remove_file(&destination).await?;
                completed_bytes = completed_bytes.saturating_sub(existing_size);
            }
            if existing_size > file.size {
                tokio::fs::remove_file(&destination).await?;
            }
            let resume_from = if existing_size < file.size {
                existing_size
            } else {
                0
            };
            let url = format!("{}/{}/{}", spec.base_url, spec.revision, file.relative_path);
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
                    emit_progress(spec, progress, completed_bytes, initial_bytes, started);
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
            emit_progress(spec, progress, completed_bytes, initial_bytes, started);
        }

        let verification_dir = model_dir.clone();
        tokio::task::spawn_blocking(move || {
            verify_model_hashes(spec, &verification_dir).map_err(anyhow::Error::msg)?;
            mark_model_verified(spec, &verification_dir).map_err(anyhow::Error::msg)
        })
        .await
        .map_err(|error| anyhow!("Qwen3-ASR verification task failed: {error}"))??;
        if inspect_model(spec, &model_dir) != ModelStatus::Available {
            return Err(anyhow!(
                "Downloaded Qwen3-ASR model failed final validation"
            ));
        }
        emit_progress(spec, progress, spec.size_bytes, initial_bytes, started);
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

    pub async fn delete_model(&self, model_name: &str) -> Result<()> {
        model_spec(model_name).ok_or_else(|| anyhow!("Unknown Qwen3-ASR model: {model_name}"))?;
        if self.current_model.read().await.as_deref() == Some(model_name) {
            self.unload_model().await;
        }
        let model_dir = self.models_dir.join(model_name);
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

async fn existing_valid_bytes(spec: &ModelSpec, model_dir: &Path) -> u64 {
    let mut total = 0;
    for file in spec.files {
        if let Ok(metadata) = tokio::fs::metadata(model_dir.join(file.relative_path)).await {
            if metadata.len() <= file.size {
                total += metadata.len();
            }
        }
    }
    total
}

fn emit_progress<F>(
    spec: &ModelSpec,
    callback: &F,
    downloaded_bytes: u64,
    initial_bytes: u64,
    started: Instant,
) where
    F: Fn(DownloadProgress),
{
    let downloaded_bytes = downloaded_bytes.min(spec.size_bytes);
    let elapsed = started.elapsed().as_secs_f64().max(0.001);
    callback(DownloadProgress {
        percent: ((downloaded_bytes as f64 / spec.size_bytes as f64) * 100.0) as u8,
        downloaded_bytes,
        total_bytes: spec.size_bytes,
        downloaded_mb: downloaded_bytes as f64 / 1_048_576.0,
        total_mb: spec.size_bytes as f64 / 1_048_576.0,
        speed_mbps: downloaded_bytes.saturating_sub(initial_bytes) as f64 / 1_048_576.0 / elapsed,
    });
}

#[async_trait]
impl TranscriptionProvider for QwenAsrEngine {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_AUDIO_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_AUDIO_SAMPLES,
            });
        }
        let text = self
            .transcribe_audio(audio, language)
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

fn qwen_language_name(language: Option<&str>) -> Option<&'static str> {
    match language {
        None | Some("") | Some("auto") | Some("auto-translate") => None,
        Some("en") => Some("English"),
        Some("zh") => Some("Chinese"),
        Some("yue") => Some("Cantonese"),
        Some("ar") => Some("Arabic"),
        Some("de") => Some("German"),
        Some("fr") => Some("French"),
        Some("es") => Some("Spanish"),
        Some("pt") => Some("Portuguese"),
        Some("id") => Some("Indonesian"),
        Some("it") => Some("Italian"),
        Some("ko") => Some("Korean"),
        Some("ru") => Some("Russian"),
        Some("th") => Some("Thai"),
        Some("vi") => Some("Vietnamese"),
        Some("ja") => Some("Japanese"),
        Some("tr") => Some("Turkish"),
        Some("hi") => Some("Hindi"),
        Some("ms") => Some("Malay"),
        Some("nl") => Some("Dutch"),
        Some("sv") => Some("Swedish"),
        Some("da") => Some("Danish"),
        Some("fi") => Some("Finnish"),
        Some("pl") => Some("Polish"),
        Some("cs") => Some("Czech"),
        Some("fil") => Some("Filipino"),
        Some("fa") => Some("Persian"),
        Some("el") => Some("Greek"),
        Some("hu") => Some("Hungarian"),
        Some("mk") => Some("Macedonian"),
        Some("ro") => Some("Romanian"),
        Some(other) => {
            log::warn!(
                "Qwen3-ASR does not support language hint '{}'; using automatic detection",
                other
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{qwen_language_name, QwenAsrEngine};
    use crate::qwen_asr_engine::{QWEN3_ASR_1_7B_MODEL, QWEN3_ASR_MODEL};

    #[test]
    fn maps_supported_language_codes_to_qwen_prompts() {
        assert_eq!(qwen_language_name(Some("zh")), Some("Chinese"));
        assert_eq!(qwen_language_name(Some("en")), Some("English"));
        assert_eq!(qwen_language_name(Some("yue")), Some("Cantonese"));
        assert_eq!(qwen_language_name(Some("auto")), None);
        assert_eq!(qwen_language_name(Some("auto-translate")), None);
        assert_eq!(qwen_language_name(Some("unsupported")), None);
    }

    #[test]
    fn discovers_both_qwen_model_options() {
        let temp = tempfile::tempdir().unwrap();
        let engine = QwenAsrEngine::new(temp.path().to_path_buf()).unwrap();
        let models = engine.discover_models();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, QWEN3_ASR_MODEL);
        assert_eq!(models[1].name, QWEN3_ASR_1_7B_MODEL);
    }

    #[tokio::test]
    async fn deletes_only_the_requested_model_directory() {
        let temp = tempfile::tempdir().unwrap();
        let compact = temp.path().join(QWEN3_ASR_MODEL);
        let large = temp.path().join(QWEN3_ASR_1_7B_MODEL);
        std::fs::create_dir_all(&compact).unwrap();
        std::fs::create_dir_all(&large).unwrap();
        std::fs::write(compact.join("keep"), b"compact").unwrap();
        std::fs::write(large.join("delete"), b"large").unwrap();

        let engine = QwenAsrEngine::new(temp.path().to_path_buf()).unwrap();
        engine.delete_model(QWEN3_ASR_1_7B_MODEL).await.unwrap();

        assert!(compact.exists());
        assert!(!large.exists());
    }
}
