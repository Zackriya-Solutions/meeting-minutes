// transcribe_engine/engine.rs
//
// Model management and transcription on top of transcribe.cpp. Replaces both
// WhisperEngine (whisper-rs) and ParakeetEngine (ONNX Runtime): one GGUF
// runtime covers both families, so there is one engine, one models directory
// layout, and one download path.
//
// Threading contract inherited from transcribe.cpp:
//   - `Model` is Arc-backed, Send + Sync, cheap to clone.
//   - `Session` is Send but NOT Sync; its mutating calls take &mut self.
//   - At most ONE in-flight compute (run OR active stream) per Model. A batch
//     run attempted while a stream is live fails with Error::Busy.
// This engine therefore owns the Model and hands out Sessions; whoever drives a
// stream owns its Session for the stream's lifetime.

use anyhow::{anyhow, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use transcribe_cpp::{Model, RunOptions, Session, Transcript};

use crate::config::{
    transcribe_model, DEFAULT_TRANSCRIBE_MODEL, TRANSCRIBE_MODEL_BASE_URL,
    TRANSCRIBE_MODEL_CATALOG,
};
// Same shape the summary-model download already reports; the UI renders both
// with the same MB/speed widgets, so there is one struct, not two.
pub use crate::summary::summary_engine::model_manager::DownloadProgress;

/// Mirrors the shape the frontend model manager already consumes, so the UI
/// keeps its existing status handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModelStatus {
    Available,
    Missing,
    Downloading { progress: u8 },
    Error(String),
    Corrupted { file_size: u64, expected_min_size: u64 },
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
    /// Whether the picker may offer this model for live recording without the
    /// VAD-segmented fallback. Catalog metadata, so it is known before download.
    pub streaming: bool,
    /// Coverage blurb for the picker; `Capabilities::languages` supersedes it
    /// once the model is loaded.
    pub languages: String,
    /// Display family, so the picker can group ~86 rows into collapsible sections.
    pub family: String,
    /// Whether this row belongs above the "All models" disclosure.
    pub recommended: bool,
}

/// A batch transcription result. `confidence` is the mean per-token probability
/// transcribe.cpp reports; every supported family provides it, so unlike the old
/// Parakeet path it is never absent.
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub text: String,
    pub confidence: f32,
}

pub struct TranscribeEngine {
    models_dir: PathBuf,
    model: Arc<RwLock<Option<Model>>>,
    current_model: Arc<RwLock<Option<String>>>,
    available_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    cancel_download_flag: Arc<RwLock<Option<String>>>,
    active_downloads: Arc<RwLock<HashSet<String>>>,
}

impl TranscribeEngine {
    pub fn new() -> Result<Self> {
        Self::new_with_models_dir(None)
    }

    /// `models_dir` is provided by the caller in production (app data dir); the
    /// dev fallback mirrors what WhisperEngine did so local runs keep working.
    pub fn new_with_models_dir(models_dir: Option<PathBuf>) -> Result<Self> {
        // transcribe.cpp logs ggml/Metal kernel compilation to stderr at load.
        // It exposes a real logging switch, so the stderr-redirect hack the old
        // whisper path needed (_stderr_suppressor.rs) is not required here.
        transcribe_cpp::disable_logging();

        let models_dir = match models_dir {
            Some(dir) => dir,
            None => {
                let current_dir = std::env::current_dir()
                    .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
                if cfg!(debug_assertions) {
                    if current_dir.join("models").exists() {
                        current_dir.join("models")
                    } else if current_dir.join("../models").exists() {
                        current_dir.join("../models")
                    } else {
                        current_dir.join("models")
                    }
                } else {
                    warn!("TranscribeEngine: No models directory provided, using fallback path");
                    dirs::data_dir()
                        .or_else(dirs::home_dir)
                        .ok_or_else(|| anyhow!("Could not find system data directory"))?
                        .join("Conversationaly")
                        .join("models")
                }
            }
        };

        info!("TranscribeEngine using models directory: {}", models_dir.display());
        if !models_dir.exists() {
            std::fs::create_dir_all(&models_dir)?;
        }

        Ok(Self {
            models_dir,
            model: Arc::new(RwLock::new(None)),
            current_model: Arc::new(RwLock::new(None)),
            available_models: Arc::new(RwLock::new(HashMap::new())),
            cancel_download_flag: Arc::new(RwLock::new(None)),
            active_downloads: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    pub async fn get_models_directory(&self) -> PathBuf {
        self.models_dir.clone()
    }

    pub async fn discover_models(&self) -> Result<Vec<ModelInfo>> {
        let mut models = Vec::new();

        for entry in TRANSCRIBE_MODEL_CATALOG {
            let (name, filename, size_mb) = (entry.name, entry.filename, entry.size_mb);
            let path = self.models_dir.join(filename);
            let status = if path.exists() {
                let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                // 90% of the catalog size, same tolerance the whisper path used:
                // catches truncated downloads without tripping on quantization
                // rounding.
                let expected_min = (size_mb as u64 * 1024 * 1024 * 9) / 10;
                if file_size >= expected_min {
                    ModelStatus::Available
                } else if let ModelStatus::Downloading { progress } = self
                    .available_models
                    .read()
                    .await
                    .get(name)
                    .map(|m| m.status.clone())
                    .unwrap_or(ModelStatus::Missing)
                {
                    ModelStatus::Downloading { progress }
                } else {
                    warn!(
                        "Model {} is {} bytes, expected at least {}",
                        filename, file_size, expected_min
                    );
                    ModelStatus::Corrupted { file_size, expected_min_size: expected_min }
                }
            } else {
                ModelStatus::Missing
            };

            models.push(ModelInfo {
                name: name.to_string(),
                path,
                size_mb,
                accuracy: entry.accuracy.to_string(),
                speed: entry.speed.to_string(),
                status,
                description: entry.description.to_string(),
                streaming: entry.streaming,
                languages: entry.languages.to_string(),
                family: entry.family.to_string(),
                recommended: crate::config::is_recommended_model(name),
            });
        }

        let mut cache = self.available_models.write().await;
        cache.clear();
        for m in &models {
            cache.insert(m.name.clone(), m.clone());
        }
        Ok(models)
    }

    pub async fn load_model(&self, model_name: &str) -> Result<()> {
        let entry = transcribe_model(model_name)
            .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;
        let path = self.models_dir.join(entry.filename);
        if !path.exists() {
            return Err(anyhow!(
                "Model '{}' is not downloaded. Download it from settings first.",
                model_name
            ));
        }

        info!("Loading model {} from {}", model_name, path.display());
        let started = std::time::Instant::now();
        // Model::load does GPU init and is blocking; keep it off the async
        // reactor so a slow first Metal kernel compile can't stall other tasks.
        let owned_path = path.clone();
        let model = tokio::task::spawn_blocking(move || Model::load(&owned_path))
            .await
            .map_err(|e| anyhow!("Model load task panicked: {}", e))?
            .map_err(|e| anyhow!("Failed to load model '{}': {}", model_name, e))?;

        // No streaming requirement here: most of the catalog is batch-only, and
        // the live path decodes those with VAD + `Session::run()` per segment
        // (see audio/transcription/stream_worker.rs).
        let caps = model.capabilities();
        info!(
            "Loaded {} in {:?} (backend {}, {} languages, streaming {})",
            model_name,
            started.elapsed(),
            model.backend(),
            caps.languages.len(),
            caps.supports_streaming
        );

        *self.model.write().await = Some(model);
        *self.current_model.write().await = Some(model_name.to_string());
        Ok(())
    }

    pub async fn unload_model(&self) -> bool {
        let had = self.model.write().await.take().is_some();
        *self.current_model.write().await = None;
        had
    }

    pub async fn is_model_loaded(&self) -> bool {
        self.model.read().await.is_some()
    }

    pub async fn get_current_model(&self) -> Option<String> {
        self.current_model.read().await.clone()
    }

    /// Hand out a Session for the caller to drive. The live recording path uses
    /// this and holds the Session for the meeting; batch callers should prefer
    /// [`transcribe_batch`].
    pub async fn open_session(&self) -> Result<Session> {
        let guard = self.model.read().await;
        let model = guard.as_ref().ok_or_else(|| anyhow!("No model loaded"))?;
        model.session().map_err(|e| anyhow!("Failed to open session: {}", e))
    }

    /// Language codes the loaded model advertises, read from GGUF metadata.
    ///
    /// Empty means the model is language-agnostic (it advertises no list), which
    /// callers should treat as "any language", not "no languages".
    pub async fn model_languages(&self) -> Option<Vec<String>> {
        let guard = self.model.read().await;
        Some(guard.as_ref()?.capabilities().languages)
    }

    /// Resolve a stored language preference against the loaded model.
    ///
    /// Every path that turns a user preference into `RunOptions::language` goes
    /// through here or through [`transcribe_batch`], because transcribe.cpp
    /// rejects an unrecognized code outright rather than ignoring it — and on
    /// the live path that kills the stream before a single sample decodes.
    pub async fn resolve_language(&self, preference: Option<String>) -> Option<String> {
        let preference = preference?;
        let advertised = self.model_languages().await.unwrap_or_default();
        let resolved = match_advertised_language(&advertised, &preference);
        if resolved.is_none() && !is_auto_language(&preference) {
            warn!(
                "Loaded model does not support language '{}' (it advertises {:?}); \
                 falling back to the model's own detection",
                preference, advertised
            );
        }
        resolved
    }

    /// One-shot transcription for import / retranscription.
    ///
    /// ponytail: shares the live path's Model, so this returns Error::Busy if a
    /// recording stream is active. Give the batch path its own `Model::load`
    /// (~300ms) if importing during a recording ever needs to work.
    pub async fn transcribe_batch(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<BatchResult> {
        let language = self.resolve_language(language).await;
        let mut session = self.open_session().await?;
        let options = RunOptions { language, ..Default::default() };

        let transcript = tokio::task::spawn_blocking(move || session.run(&audio, &options))
            .await
            .map_err(|e| anyhow!("Transcription task panicked: {}", e))?
            .map_err(|e| anyhow!("Transcription failed: {}", e))?;

        Ok(BatchResult {
            confidence: mean_token_confidence(&transcript),
            text: transcript.text.trim().to_string(),
        })
    }

    pub async fn download_model(
        &self,
        model_name: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        let entry = transcribe_model(model_name)
            .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;
        let (repo, filename) = (entry.hf_repo, entry.filename);

        {
            let mut active = self.active_downloads.write().await;
            if !active.insert(model_name.to_string()) {
                return Err(anyhow!("Model '{}' is already downloading", model_name));
            }
        }
        // Clear any cancel request left over from a previous attempt.
        {
            let mut flag = self.cancel_download_flag.write().await;
            if flag.as_deref() == Some(model_name) {
                *flag = None;
            }
        }

        let result = self
            .download_inner(model_name, repo, filename, progress_callback)
            .await;

        self.active_downloads.write().await.remove(model_name);
        if result.is_err() {
            // Never leave a partial file behind: discover_models would report it
            // as Corrupted and the user would have to delete it by hand.
            let _ = fs::remove_file(self.models_dir.join(filename)).await;
        }
        result
    }

    async fn download_inner(
        &self,
        model_name: &str,
        repo: &str,
        filename: &str,
        progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send>>,
    ) -> Result<()> {
        let url = format!(
            "{}/{}/resolve/main/{}",
            TRANSCRIBE_MODEL_BASE_URL, repo, filename
        );
        let file_path = self.models_dir.join(filename);
        info!("Downloading {} -> {}", url, file_path.display());

        if !self.models_dir.exists() {
            fs::create_dir_all(&self.models_dir).await?;
        }
        self.set_status(model_name, ModelStatus::Downloading { progress: 0 })
            .await;

        let response = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to start download: {}", e))?;
        if !response.status().is_success() {
            return Err(anyhow!("Download failed with status: {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut file = fs::File::create(&file_path).await?;

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let mut last_reported = 0u8;
        let mut last_report_time = std::time::Instant::now();
        let mut bytes_since_report = 0u64;

        if let Some(ref cb) = progress_callback {
            cb(DownloadProgress::new(0, total_size, 0.0));
        }

        while let Some(chunk) = stream.next().await {
            if self.cancel_download_flag.read().await.as_deref() == Some(model_name) {
                *self.cancel_download_flag.write().await = None;
                return Err(anyhow!("Download cancelled by user"));
            }

            let chunk = chunk.map_err(|e| anyhow!("Failed to read chunk: {}", e))?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            bytes_since_report += chunk.len() as u64;

            if total_size > 0 {
                let progress = ((downloaded as f64 / total_size as f64) * 100.0) as u8;
                let elapsed = last_report_time.elapsed();
                // Percent alone ticks once per ~7 MB on a 716 MB model, which
                // leaves the speed readout stale; the 500 ms floor keeps it live.
                if progress > last_reported || elapsed.as_millis() >= 500 {
                    let speed_mbps = if elapsed.as_secs_f64() > 0.0 {
                        (bytes_since_report as f64 / 1048576.0) / elapsed.as_secs_f64()
                    } else {
                        0.0
                    };
                    last_reported = progress;
                    last_report_time = std::time::Instant::now();
                    bytes_since_report = 0;

                    self.set_status(model_name, ModelStatus::Downloading { progress })
                        .await;
                    if let Some(ref cb) = progress_callback {
                        cb(DownloadProgress::new(downloaded, total_size, speed_mbps));
                    }
                }
            }
        }

        file.flush().await?;
        drop(file);

        self.set_status(model_name, ModelStatus::Available).await;
        info!(
            "Downloaded {} ({:.1} MB)",
            model_name,
            downloaded as f64 / 1048576.0
        );
        Ok(())
    }

    pub async fn cancel_download(&self, model_name: &str) -> Result<()> {
        if !self.active_downloads.read().await.contains(model_name) {
            return Err(anyhow!("No active download for '{}'", model_name));
        }
        *self.cancel_download_flag.write().await = Some(model_name.to_string());
        Ok(())
    }

    pub async fn delete_model(&self, model_name: &str) -> Result<()> {
        let entry = transcribe_model(model_name)
            .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;
        let path = self.models_dir.join(entry.filename);

        if self.get_current_model().await.as_deref() == Some(model_name) {
            self.unload_model().await;
        }
        if path.exists() {
            fs::remove_file(&path).await?;
            info!("Deleted model file {}", path.display());
        }
        self.set_status(model_name, ModelStatus::Missing).await;
        Ok(())
    }

    async fn set_status(&self, model_name: &str, status: ModelStatus) {
        if let Some(info) = self.available_models.write().await.get_mut(model_name) {
            info.status = status;
        }
    }

    /// Delete model files left over from the whisper-rs / ONNX engines.
    ///
    /// transcribe.cpp reads GGUF only, so legacy `ggml-*.bin` files and the
    /// Parakeet ONNX directory are dead weight — multiple GB of it for anyone
    /// who had large-v3. Runs once at startup; failures are logged, never fatal.
    pub fn purge_legacy_models(models_dir: &std::path::Path) -> u64 {
        let mut freed = 0u64;

        let entries = match std::fs::read_dir(models_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Could not scan models directory for legacy files: {}", e);
                return 0;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Legacy whisper: ggml-<model>.bin at the top level.
            let is_legacy_whisper = name.starts_with("ggml-") && name.ends_with(".bin");
            // Legacy parakeet: a `parakeet` directory of .onnx files + vocab.txt.
            let is_legacy_parakeet = path.is_dir() && name == "parakeet";

            if !is_legacy_whisper && !is_legacy_parakeet {
                continue;
            }

            let size = dir_or_file_size(&path);
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };

            match removed {
                Ok(()) => {
                    info!("Removed legacy model {} ({:.1} MB)", name, size as f64 / 1048576.0);
                    freed += size;
                }
                Err(e) => warn!("Could not remove legacy model {}: {}", name, e),
            }
        }

        if freed > 0 {
            info!(
                "Reclaimed {:.1} MB from models that the previous engines used",
                freed as f64 / 1048576.0
            );
        }
        freed
    }
}

fn dir_or_file_size(path: &std::path::Path) -> u64 {
    if path.is_file() {
        return std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| dir_or_file_size(&e.path())).sum())
        .unwrap_or(0)
}

/// "Transcribe in whatever language you hear" sentinels the picker stores.
/// Neither is a language code, so neither may reach `RunOptions::language`.
fn is_auto_language(preference: &str) -> bool {
    matches!(preference.trim(), "" | "auto" | "auto-translate")
}

/// Match a stored language preference against the codes a model advertises.
///
/// transcribe.cpp compares `RunOptions::language` to the GGUF language list with
/// an exact `strcmp` and rejects a miss with `TRANSCRIBE_ERR_UNSUPPORTED_LANGUAGE`
/// — on the live path that aborts `stream()` before any audio decodes. Two things
/// make a miss the normal case rather than a rare one:
///   - the picker stores bare ISO-639-1 ("de") while models such as Nemotron
///     advertise locales ("de-DE"), and
///   - "auto"/"auto-translate" are UI sentinels, not codes.
///
/// `None` means "let the model detect it", which is also what a genuinely
/// unsupported language falls back to: a transcript in the wrong language beats
/// no transcript at all.
fn match_advertised_language(advertised: &[String], preference: &str) -> Option<String> {
    let pref = preference.trim();
    if is_auto_language(pref) {
        return None;
    }
    // An empty list means the model advertises nothing, i.e. language-agnostic —
    // not that it supports no languages. Pass the code through untouched.
    if advertised.is_empty() || advertised.iter().any(|l| l == pref) {
        return Some(pref.to_string());
    }
    // "de" -> "de-DE". First match wins: where a model lists several locales for
    // one language (en-US/en-GB, pt-BR/pt-PT) the order is the model's own, and
    // any of them transcribes that language.
    let primary = primary_subtag(pref);
    advertised
        .iter()
        .find(|l| primary_subtag(l).eq_ignore_ascii_case(primary))
        .cloned()
}

fn primary_subtag(code: &str) -> &str {
    code.split('-').next().unwrap_or(code)
}

/// Mean per-token probability. transcribe.cpp reports `p` on every token, so
/// this replaces the old provider-specific confidence handling (Whisper had a
/// score, Parakeet had none).
pub fn mean_token_confidence(transcript: &Transcript) -> f32 {
    if transcript.tokens.is_empty() {
        // No tokens means no text; callers treat empty output as "nothing said"
        // rather than "low confidence", so don't report a misleading 0.0.
        return 1.0;
    }
    transcript.tokens.iter().map(|t| t.p).sum::<f32>() / transcript.tokens.len() as f32
}

/// The model the app falls back to when nothing is configured.
pub fn default_model_name() -> &'static str {
    DEFAULT_TRANSCRIBE_MODEL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RECOMMENDED_IMPORT_MODELS, RECOMMENDED_LIVE_MODELS};

    #[test]
    fn catalog_rows_are_downloadable_and_defaults_resolve() {
        // The catalog is no longer streaming-only: batch-only families are
        // selectable because the live path falls back to VAD segmentation. What
        // still has to hold is that every row can actually be fetched and that
        // the names the app hardcodes exist.
        assert!(transcribe_model(DEFAULT_TRANSCRIBE_MODEL).is_some());
        for e in TRANSCRIBE_MODEL_CATALOG {
            assert!(e.filename.ends_with(".gguf"), "{} is not a GGUF file", e.name);
            assert!(e.size_mb > 0, "{} has no size", e.name);
            assert!(!e.hf_repo.is_empty(), "{} has no HF repo", e.name);
            assert!(!e.languages.is_empty(), "{} has no language blurb", e.name);
            // The generator derives the name from the filename, so a mismatch
            // means a hand-edit crept into the generated block.
            let stem = e.filename.trim_end_matches(".gguf").to_lowercase();
            let (variant, quant) = e.name.rsplit_once('-').unwrap();
            assert!(
                stem.starts_with(variant) && stem.ends_with(&quant_suffix(quant)),
                "{} does not match its filename {}",
                e.name,
                e.filename
            );
        }
        assert!(
            TRANSCRIBE_MODEL_CATALOG.iter().any(|e| e.streaming),
            "no live-capable model left in the catalog"
        );
        for name in RECOMMENDED_LIVE_MODELS {
            let m = transcribe_model(name).expect("recommended live model not in catalog");
            assert!(m.streaming, "{} is recommended for live but cannot stream", name);
        }
        for name in RECOMMENDED_IMPORT_MODELS {
            assert!(
                transcribe_model(name).is_some(),
                "recommended import model {} not in catalog",
                name
            );
        }
    }

    fn quant_suffix(short: &str) -> String {
        match short {
            "q8" => "-q8_0".to_string(),
            "q4" => "-q4_k_m".to_string(),
            other => panic!("unexpected quantization suffix {other}"),
        }
    }

    #[tokio::test]
    async fn discovery_reports_missing_available_and_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let engine = TranscribeEngine::new_with_models_dir(Some(dir.path().to_path_buf())).unwrap();

        // Nothing on disk yet.
        let models = engine.discover_models().await.unwrap();
        assert_eq!(models.len(), TRANSCRIBE_MODEL_CATALOG.len());
        assert!(models.iter().all(|m| matches!(m.status, ModelStatus::Missing)));

        let entry = &TRANSCRIBE_MODEL_CATALOG[0];
        let (name, filename, size_mb) = (entry.name, entry.filename, entry.size_mb);

        // A truncated download must not be reported as usable.
        std::fs::write(dir.path().join(filename), vec![0u8; 1024]).unwrap();
        let models = engine.discover_models().await.unwrap();
        let m = models.iter().find(|m| m.name == name).unwrap();
        assert!(
            matches!(m.status, ModelStatus::Corrupted { .. }),
            "a 1KB file for a {}MB model should be Corrupted, got {:?}",
            size_mb,
            m.status
        );

        // Full size (the catalog value) is Available.
        std::fs::write(
            dir.path().join(filename),
            vec![0u8; size_mb as usize * 1024 * 1024],
        )
        .unwrap();
        let models = engine.discover_models().await.unwrap();
        let m = models.iter().find(|m| m.name == name).unwrap();
        assert!(matches!(m.status, ModelStatus::Available), "got {:?}", m.status);
    }

    #[test]
    fn purge_removes_only_legacy_engine_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();

        // Legacy: must go.
        std::fs::write(p.join("ggml-large-v3-turbo.bin"), b"x".repeat(1000)).unwrap();
        std::fs::write(p.join("ggml-tiny.bin"), b"x".repeat(500)).unwrap();
        std::fs::create_dir(p.join("parakeet")).unwrap();
        std::fs::write(p.join("parakeet/encoder-model.int8.onnx"), b"x".repeat(200)).unwrap();

        // Must survive: the new models, and the summary LLMs which live in the
        // same directory and have nothing to do with transcription.
        std::fs::write(p.join("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf"), b"keep").unwrap();
        std::fs::create_dir(p.join("summary")).unwrap();
        std::fs::write(p.join("summary/model.gguf"), b"keep").unwrap();

        let freed = TranscribeEngine::purge_legacy_models(p);

        assert_eq!(freed, 1700, "should report bytes actually reclaimed");
        assert!(!p.join("ggml-large-v3-turbo.bin").exists());
        assert!(!p.join("ggml-tiny.bin").exists());
        assert!(!p.join("parakeet").exists());
        assert!(p.join("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf").exists());
        assert!(p.join("summary/model.gguf").exists(), "summary LLMs must not be touched");

        // Idempotent: a second launch finds nothing to do.
        assert_eq!(TranscribeEngine::purge_legacy_models(p), 0);
    }

    /// Exactly what Nemotron 3.5 streaming advertises in `general.languages`.
    fn nemotron_languages() -> Vec<String> {
        ["en-US", "en-GB", "es-US", "de-DE", "fr-FR", "fr-CA", "pt-BR", "pt-PT"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn bare_iso_code_resolves_to_the_advertised_locale() {
        // The bug: the picker stores "de", the model advertises "de-DE", and
        // transcribe.cpp strcmp's them and aborts the stream with status 10.
        let langs = nemotron_languages();
        assert_eq!(match_advertised_language(&langs, "de").as_deref(), Some("de-DE"));
        // Several locales for one language: the model's own order decides.
        assert_eq!(match_advertised_language(&langs, "en").as_deref(), Some("en-US"));
        assert_eq!(match_advertised_language(&langs, "pt").as_deref(), Some("pt-BR"));
    }

    #[test]
    fn exact_advertised_code_passes_through() {
        let langs = nemotron_languages();
        assert_eq!(match_advertised_language(&langs, "fr-CA").as_deref(), Some("fr-CA"));
    }

    #[test]
    fn picker_sentinels_are_not_language_codes() {
        let langs = nemotron_languages();
        // "auto-translate" is the shipped default, so this is the fresh-install path.
        for sentinel in ["auto", "auto-translate", "", "  "] {
            assert_eq!(
                match_advertised_language(&langs, sentinel),
                None,
                "'{sentinel}' must not reach RunOptions::language"
            );
        }
    }

    #[test]
    fn unsupported_language_falls_back_to_detection() {
        // Japanese against a model that does not list it: auto-detect rather
        // than a hard failure that leaves the meeting with no transcript.
        assert_eq!(match_advertised_language(&nemotron_languages(), "ja"), None);
    }

    #[test]
    fn model_that_advertises_nothing_is_language_agnostic() {
        assert_eq!(match_advertised_language(&[], "de").as_deref(), Some("de"));
    }

    #[test]
    fn catalog_names_are_unique() {
        let mut seen = HashSet::new();
        for e in TRANSCRIBE_MODEL_CATALOG {
            assert!(seen.insert(e.name), "duplicate catalog entry {}", e.name);
        }
    }
}
