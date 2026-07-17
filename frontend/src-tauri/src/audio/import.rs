// Audio file import module - allows importing external audio files as new meetings

use crate::api::TranscriptSegment;
use crate::audio::decoder::{
    decode_audio_file, decode_audio_file_to_whisper, decode_audio_file_with_progress,
};
use crate::audio::vad::{get_speech_chunks_with_progress, ContinuousVadProcessor, SpeechSegment};
use crate::config::{DEFAULT_PARAKEET_MODEL, DEFAULT_WHISPER_MODEL};
use crate::parakeet_engine::ParakeetEngine;
use crate::state::AppState;
use crate::whisper_engine::WhisperEngine;
use anyhow::{anyhow, Result};
use futures_util::FutureExt;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use super::audio_processing::create_meeting_folder;
use super::common::{create_transcript_segments, split_segment_at_silence, write_transcripts_json};
use super::constants::AUDIO_EXTENSIONS;
use super::ffmpeg::find_ffmpeg_path;
use super::recording_preferences::get_default_recordings_folder;

/// Global flag to track if import is in progress
static IMPORT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Global flag to signal cancellation
static IMPORT_CANCELLED: AtomicBool = AtomicBool::new(false);
const MAX_BATCH_AUDIO_FILES: usize = 500;

/// RAII guard for IMPORT_IN_PROGRESS flag
/// Ensures flag is cleared even if import panics or returns early
struct ImportGuard;

impl ImportGuard {
    /// Create guard and set flag atomically
    fn acquire() -> Result<Self, String> {
        if IMPORT_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Import already in progress".to_string());
        }
        Ok(ImportGuard)
    }
}

impl Drop for ImportGuard {
    fn drop(&mut self) {
        IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// Removes a partially-created meeting folder unless its database transaction commits.
struct PendingMeetingFolder {
    path: PathBuf,
    committed: bool,
}

impl PendingMeetingFolder {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for PendingMeetingFolder {
    fn drop(&mut self) {
        if !self.committed && self.path.exists() {
            if let Err(error) = std::fs::remove_dir_all(&self.path) {
                warn!(
                    "Failed to clean up partial import folder {}: {}",
                    self.path.display(),
                    error
                );
            }
        }
    }
}

/// VAD redemption time in milliseconds - bridges natural pauses in speech
/// Batch processing needs longer redemption (2000ms) than live pipeline (400ms)
/// because the entire file is processed at once by VAD, and 400ms fragments
/// speech at every natural sentence/topic pause (500ms-2s)
const VAD_REDEMPTION_TIME_MS: u32 = 2000;

/// Maximum file size: 20GB (prevents OOM and excessive processing time)
const MAX_FILE_SIZE_BYTES: u64 = 20 * 1024 * 1024 * 1024; // 20GB

/// Decode to 16 kHz mono and feed VAD incrementally.
///
/// This is the safe path for large recordings: at no point do we retain the
/// complete decoded PCM stream. The old `Command::output` path could keep
/// several complete copies (bytes, f32 samples, VAD input and cloned speech
/// segments), turning a 1 GB compressed source into tens of GB of RAM.
fn stream_decode_speech_segments<F>(
    path: &Path,
    redemption_time_ms: u32,
    expected_duration_seconds: Option<f64>,
    mut progress: F,
) -> Result<(Vec<SpeechSegment>, f64)>
where
    F: FnMut(u32, usize) -> bool,
{
    let ffmpeg = find_ffmpeg_path().ok_or_else(|| anyhow!("FFmpeg is not available"))?;
    let mut child = Command::new(ffmpeg)
        .args(["-nostdin", "-v", "error"])
        .arg("-i")
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| anyhow!("Failed to start FFmpeg streaming decode: {error}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("FFmpeg stdout is unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("FFmpeg stderr is unavailable"))?;
    let stderr_reader = std::thread::spawn(move || {
        let mut details = String::new();
        let _ = stderr.read_to_string(&mut details);
        details
    });

    let mut processor = ContinuousVadProcessor::new(16_000, redemption_time_ms)?;
    let mut segments = Vec::new();
    // 10 seconds of mono f32 at 16 kHz: small enough for stable memory, large
    // enough to avoid excessive process/VAD overhead.
    let mut bytes = vec![0_u8; 16_000 * 4 * 10];
    let mut carry = Vec::<u8>::with_capacity(3);
    let mut processed_samples = 0_u64;
    let expected_samples = expected_duration_seconds
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| (duration * 16_000.0) as u64);
    let mut last_progress = 0_u32;

    loop {
        let read = stdout
            .read(&mut bytes)
            .map_err(|error| anyhow!("Failed to read decoded audio stream: {error}"))?;
        if read == 0 {
            break;
        }
        carry.extend_from_slice(&bytes[..read]);
        let aligned = carry.len() - (carry.len() % 4);
        let mut samples = Vec::with_capacity(aligned / 4);
        for chunk in carry[..aligned].chunks_exact(4) {
            let sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            samples.push(if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            });
        }
        let remaining = carry.split_off(aligned);
        carry = remaining;
        processed_samples += samples.len() as u64;
        segments.extend(processor.process_audio(&samples)?);

        if let Some(total) = expected_samples {
            let next = ((processed_samples.saturating_mul(100) / total.max(1)).min(99)) as u32;
            if next >= last_progress + 2 {
                if !progress(next, segments.len()) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stderr_reader.join();
                    return Err(anyhow!("Import cancelled"));
                }
                last_progress = next;
            }
        } else if !progress(0, segments.len()) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.join();
            return Err(anyhow!("Import cancelled"));
        }
    }

    if !carry.is_empty() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_reader.join();
        return Err(anyhow!("FFmpeg returned an incomplete f32 sample"));
    }
    segments.extend(processor.flush()?);
    let status = child
        .wait()
        .map_err(|error| anyhow!("Failed to wait for FFmpeg: {error}"))?;
    let details = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        return Err(anyhow!(
            "FFmpeg streaming decode failed: {}",
            details.trim()
        ));
    }
    if processed_samples == 0 {
        return Err(anyhow!("FFmpeg decoded no audio samples"));
    }
    let duration_seconds = processed_samples as f64 / 16_000.0;
    let _ = progress(100, segments.len());
    Ok((segments, duration_seconds))
}

/// Information about a selected audio file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFileInfo {
    pub path: String,
    pub filename: String,
    pub duration_seconds: f64,
    pub size_bytes: u64,
    pub format: String,
}

/// Progress update emitted during import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub stage: String, // "copying", "decoding", "vad", "transcribing", "saving"
    pub progress_percentage: u32,
    pub message: String,
}

/// Result of import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub meeting_id: String,
    pub title: String,
    pub segments_count: usize,
    pub duration_seconds: f64,
    pub processable_segments: usize,
    pub transcribed_segments: usize,
    pub empty_segments: usize,
    pub transcription_coverage: Option<f64>,
    pub average_confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportItem {
    pub source_path: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportFailure {
    pub source_path: String,
    pub title: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportProgress {
    pub current_index: usize,
    pub total: usize,
    pub current_title: String,
    pub completed: usize,
    pub skipped: usize,
    pub truncated: usize,
    pub failed: usize,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchImportResult {
    pub total: usize,
    pub imported: Vec<ImportResult>,
    pub skipped: Vec<BatchImportItem>,
    #[serde(default)]
    pub truncated: Vec<BatchImportItem>,
    pub failed: Vec<BatchImportFailure>,
    pub cancelled: bool,
}

/// Error during import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub error: String,
}

/// Warning emitted during import (non-fatal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWarning {
    pub warning: String,
    pub details: Option<String>,
}

/// Response when import is started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportStarted {
    pub message: String,
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)
        .map_err(|error| anyhow!("Failed to open {} for hashing: {}", path.display(), error))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|error| anyhow!("Failed to hash {}: {}", path.display(), error))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_audio_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| anyhow!("Cannot read folder {}: {}", directory.display(), error))?;
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_symlink() {
                match std::fs::metadata(&path) {
                    Ok(metadata) if metadata.is_file() => {
                        debug!("Following audio file symlink: {}", path.display());
                    }
                    Ok(metadata) if metadata.is_dir() => {
                        warn!(
                            "Skipping directory symlink during audio scan to avoid cycles: {}",
                            path.display()
                        );
                        continue;
                    }
                    Ok(_) => {
                        warn!("Skipping unsupported symlink target: {}", path.display());
                        continue;
                    }
                    Err(error) => {
                        warn!("Skipping unreadable symlink {}: {}", path.display(), error);
                        continue;
                    }
                }
            }
            if !file_type.is_symlink() && file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_symlink() && !file_type.is_file() {
                continue;
            }
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_lowercase())
                .unwrap_or_default();
            if AUDIO_EXTENSIONS.contains(&extension.as_str()) {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn batch_items_from_folder(root: &Path) -> Result<Vec<BatchImportItem>> {
    let files = collect_audio_files(root)?;
    if files.is_empty() {
        return Err(anyhow!(
            "No supported audio files found in {}",
            root.display()
        ));
    }

    Ok(files
        .into_iter()
        .map(|path| {
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Imported meeting");
            let title = strip_hash_suffix(stem);
            BatchImportItem {
                source_path: path.to_string_lossy().to_string(),
                title,
            }
        })
        .collect())
}

fn cap_batch_items(
    mut items: Vec<BatchImportItem>,
) -> (Vec<BatchImportItem>, Vec<BatchImportItem>) {
    if items.len() <= MAX_BATCH_AUDIO_FILES {
        return (items, Vec::new());
    }
    let over_limit = items.split_off(MAX_BATCH_AUDIO_FILES);
    (items, over_limit)
}

fn strip_hash_suffix(stem: &str) -> String {
    let Some((title, suffix)) = stem.rsplit_once("__") else {
        return stem.to_string();
    };
    if suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        title.to_string()
    } else {
        stem.to_string()
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    "unknown panic payload".to_string()
}

fn write_batch_report(path: &Path, result: &BatchImportResult) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, serde_json::to_vec_pretty(result)?)?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

fn collect_imported_hashes(recordings_folder: &Path) -> HashSet<String> {
    let mut hashes = HashSet::new();
    let Ok(entries) = std::fs::read_dir(recordings_folder) else {
        return hashes;
    };
    for entry in entries.flatten() {
        let metadata_path = entry.path().join("metadata.json");
        let Ok(contents) = std::fs::read_to_string(metadata_path) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        if let Some(hash) = metadata
            .get("source_sha256")
            .and_then(|value| value.as_str())
        {
            hashes.insert(hash.to_string());
        }
    }
    hashes
}

fn deferred_audio_file_info(path: &Path) -> AudioFileInfo {
    AudioFileInfo {
        path: path.to_string_lossy().to_string(),
        filename: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("audio")
            .to_string(),
        duration_seconds: 0.0,
        size_bytes: std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        format: path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase(),
    }
}

/// Check if import is currently in progress
pub fn is_import_in_progress() -> bool {
    IMPORT_IN_PROGRESS.load(Ordering::SeqCst)
}

/// Cancel ongoing import
pub fn cancel_import() {
    IMPORT_CANCELLED.store(true, Ordering::SeqCst);
}

/// Validate an audio file and return its info using metadata-only approach
/// Falls back to full decode if metadata is unavailable
pub fn validate_audio_file(path: &Path) -> Result<AudioFileInfo> {
    // Check file exists
    if !path.exists() {
        return Err(anyhow!("File does not exist: {}", path.display()));
    }

    // Check extension
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Err(anyhow!(
            "Unsupported format: .{}. Supported: {}",
            extension,
            AUDIO_EXTENSIONS.join(", ")
        ));
    }

    // Get file size
    let metadata = std::fs::metadata(path).map_err(|e| anyhow!("Cannot read file: {}", e))?;
    let size_bytes = metadata.len();

    // Check file size limit
    if size_bytes > MAX_FILE_SIZE_BYTES {
        return Err(anyhow!(
            "File too large: {:.2}GB. Maximum supported size is {}GB",
            size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            MAX_FILE_SIZE_BYTES / (1024 * 1024 * 1024)
        ));
    }

    // Get filename without extension for title
    let filename = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported Audio")
        .to_string();

    // Try fast metadata-only validation first
    let duration_seconds = match extract_duration_from_metadata(path) {
        Ok(duration) => {
            debug!("Got duration from metadata: {:.2}s (fast path)", duration);
            duration
        }
        Err(e) => {
            // Fallback to full decode if metadata unavailable
            warn!(
                "Metadata extraction failed: {}, falling back to full decode",
                e
            );
            let decoded = decode_audio_file(path)?;
            decoded.duration_seconds
        }
    };

    Ok(AudioFileInfo {
        path: path.to_string_lossy().to_string(),
        filename,
        duration_seconds,
        size_bytes,
        format: extension.to_uppercase(),
    })
}

/// Extract duration from audio file metadata without full decode
/// Returns error if metadata is unavailable, triggering fallback to full decode
pub(crate) fn extract_duration_from_metadata(path: &Path) -> Result<f64> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Open the file
    let file =
        std::fs::File::open(path).map_err(|e| anyhow!("Failed to open audio file: {}", e))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Set up format hint based on file extension
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Probe the file format (lightweight operation)
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to probe audio format: {}", e))?;

    let format = probed.format;

    // Find the first audio track
    use symphonia::core::codecs::CODEC_TYPE_NULL;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No audio track found in file"))?;

    // Extract duration from metadata
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("Unknown sample rate"))?;

    let n_frames = track
        .codec_params
        .n_frames
        .ok_or_else(|| anyhow!("Frame count not available in metadata"))?;

    let duration_seconds = n_frames as f64 / sample_rate as f64;

    debug!(
        "Extracted metadata: {}Hz, {} frames, {:.2}s",
        sample_rate, n_frames, duration_seconds
    );

    Ok(duration_seconds)
}

/// Start import of an audio file
pub async fn start_import<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportResult> {
    // Acquire guard - ensures flag is cleared even on panic/early return
    let _guard = ImportGuard::acquire().map_err(|e| anyhow!(e))?;

    // Reset cancellation flag
    IMPORT_CANCELLED.store(false, Ordering::SeqCst);

    let use_parakeet = provider.as_deref() == Some("parakeet");
    let result = run_import(
        app.clone(),
        source_path,
        title,
        language,
        model,
        provider,
        None,
    )
    .await;

    // Unload the engine after the batch job (success, failure, or cancellation)
    super::common::unload_engine_after_batch(use_parakeet).await;

    // Guard will automatically clear flag on drop
    // No need for manual: IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);

    match &result {
        Ok(res) => {
            let _ = app.emit(
                "import-complete",
                serde_json::json!({
                    "meeting_id": res.meeting_id,
                    "title": res.title,
                    "segments_count": res.segments_count,
                    "duration_seconds": res.duration_seconds,
                    "processable_segments": res.processable_segments,
                    "transcribed_segments": res.transcribed_segments,
                    "empty_segments": res.empty_segments,
                    "transcription_coverage": res.transcription_coverage,
                    "average_confidence": res.average_confidence
                }),
            );
        }
        Err(e) => {
            let _ = app.emit(
                "import-error",
                ImportError {
                    error: e.to_string(),
                },
            );
        }
    }

    result
}

pub async fn start_batch_import<R: Runtime>(
    app: AppHandle<R>,
    items: Vec<BatchImportItem>,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<BatchImportResult> {
    let _guard = ImportGuard::acquire().map_err(|error| anyhow!(error))?;
    IMPORT_CANCELLED.store(false, Ordering::SeqCst);

    if items.is_empty() {
        return Err(anyhow!("No audio files selected"));
    }
    let total = items.len();
    let (items, over_limit) = cap_batch_items(items);
    if !over_limit.is_empty() {
        warn!(
            "Batch contains {} files; processing the first {} and reporting {} as truncated",
            total,
            MAX_BATCH_AUDIO_FILES,
            over_limit.len()
        );
    }
    if provider.as_deref() == Some("gigaam") && !crate::gigaam_engine::is_loaded() {
        return Err(anyhow!(
            "GigaAM model is not loaded. Download it in Settings → Transcription first."
        ));
    }

    let mut result = BatchImportResult {
        total,
        imported: Vec::new(),
        skipped: Vec::new(),
        truncated: over_limit,
        failed: Vec::new(),
        cancelled: false,
    };
    let recordings_folder = get_default_recordings_folder();
    let mut imported_hashes =
        tokio::task::spawn_blocking(move || collect_imported_hashes(&recordings_folder))
            .await
            .map_err(|error| anyhow!("Hash scan task join error: {}", error))?;

    for (index, item) in items.into_iter().enumerate() {
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            result.cancelled = true;
            break;
        }

        emit_batch_progress(&app, index + 1, &item, &result, "hashing");
        let source_path = PathBuf::from(&item.source_path);
        let hash_result = tokio::task::spawn_blocking(move || sha256_file(&source_path))
            .await
            .map_err(|error| anyhow!("Hash task join error: {}", error))
            .and_then(|value| value);
        let source_sha256 = match hash_result {
            Ok(value) => value,
            Err(error) => {
                result.failed.push(BatchImportFailure {
                    source_path: item.source_path.clone(),
                    title: item.title.clone(),
                    error: error.to_string(),
                });
                emit_batch_progress(&app, index + 1, &item, &result, "failed");
                continue;
            }
        };

        if imported_hashes.contains(&source_sha256) {
            info!("Skipping already imported audio: {}", item.source_path);
            result.skipped.push(item.clone());
            emit_batch_progress(&app, index + 1, &item, &result, "skipped");
            continue;
        }

        emit_batch_progress(&app, index + 1, &item, &result, "importing");
        let import_result = AssertUnwindSafe(run_import(
            app.clone(),
            item.source_path.clone(),
            item.title.clone(),
            language.clone(),
            model.clone(),
            provider.clone(),
            Some(source_sha256.clone()),
        ))
        .catch_unwind()
        .await
        .unwrap_or_else(|payload| {
            Err(anyhow!(
                "Importer panicked while processing '{}': {}",
                item.source_path,
                panic_message(payload)
            ))
        });

        match import_result {
            Ok(imported) => {
                imported_hashes.insert(source_sha256);
                result.imported.push(imported);
                emit_batch_progress(&app, index + 1, &item, &result, "completed");
            }
            Err(_) if IMPORT_CANCELLED.load(Ordering::SeqCst) => {
                result.cancelled = true;
                break;
            }
            Err(error) => {
                error!("Batch import failed for '{}': {}", item.title, error);
                result.failed.push(BatchImportFailure {
                    source_path: item.source_path.clone(),
                    title: item.title.clone(),
                    error: error.to_string(),
                });
                emit_batch_progress(&app, index + 1, &item, &result, "failed");
            }
        }
    }

    super::common::unload_engine_after_batch(provider.as_deref() == Some("parakeet")).await;
    let _ = app.emit("batch-import-complete", &result);
    Ok(result)
}

/// Import every supported audio file under `folder` without opening a file picker.
/// Existing source hashes are skipped, so the operation is safe to resume.
pub async fn start_batch_import_folder<R: Runtime>(
    app: AppHandle<R>,
    folder: PathBuf,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    report_path: Option<PathBuf>,
) -> Result<BatchImportResult> {
    let items = tokio::task::spawn_blocking(move || batch_items_from_folder(&folder))
        .await
        .map_err(|error| anyhow!("Folder scan task join error: {}", error))??;
    let result = start_batch_import(app, items, language, model, provider).await?;
    if let Some(path) = report_path {
        write_batch_report(&path, &result)?;
        info!("Wrote batch import report to {}", path.display());
    }
    Ok(result)
}

fn emit_batch_progress<R: Runtime>(
    app: &AppHandle<R>,
    current_index: usize,
    item: &BatchImportItem,
    result: &BatchImportResult,
    state: &str,
) {
    let _ = app.emit(
        "batch-import-progress",
        BatchImportProgress {
            current_index,
            total: result.total,
            current_title: item.title.clone(),
            completed: result.imported.len(),
            skipped: result.skipped.len(),
            truncated: result.truncated.len(),
            failed: result.failed.len(),
            state: state.to_string(),
        },
    );
}

/// Internal function to run import
async fn run_import<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    source_sha256: Option<String>,
) -> Result<ImportResult> {
    let source = PathBuf::from(&source_path);

    // Validate source file
    if !source.exists() {
        return Err(anyhow!("Source file not found: {}", source.display()));
    }
    let source_sha256 = match source_sha256 {
        Some(value) => value,
        None => {
            let hash_path = source.clone();
            tokio::task::spawn_blocking(move || sha256_file(&hash_path))
                .await
                .map_err(|error| anyhow!("Hash task join error: {}", error))??
        }
    };

    info!(
        "Starting import for '{}' from {} with language {:?}, model {:?}, provider {:?}",
        title, source_path, language, model, provider
    );

    // Determine which provider to use (default to whisper)
    let use_parakeet = provider.as_deref() == Some("parakeet");
    let use_gigaam = provider.as_deref() == Some("gigaam");
    let use_salutespeech = provider.as_deref() == Some("salutespeech");

    emit_progress(&app, "copying", 5, "Creating meeting folder...");

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Import cancelled"));
    }

    // Create meeting folder
    let base_folder = get_default_recordings_folder();
    let meeting_folder = create_meeting_folder(&base_folder, &title, false)?;
    let mut pending_folder = PendingMeetingFolder::new(meeting_folder.clone());

    // Copy audio file to meeting folder
    emit_progress(&app, "copying", 10, "Copying audio file...");

    let dest_filename = format!(
        "audio.{}",
        source.extension().and_then(|e| e.to_str()).unwrap_or("mp4")
    );
    let dest_path = meeting_folder.join(&dest_filename);

    let src = source.clone();
    let dst = dest_path.clone();
    tokio::task::spawn_blocking(move || std::fs::copy(&src, &dst))
        .await
        .map_err(|e| anyhow!("Copy task join error: {}", e))?
        .map_err(|e| anyhow!("Failed to copy audio file: {}", e))?;

    info!("Copied audio to: {}", dest_path.display());

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "decoding", 15, "Decoding audio file...");
    emit_progress(
        &app,
        "vad",
        20,
        "Streaming audio through speech detection...",
    );

    let expected_duration = extract_duration_from_metadata(&dest_path).ok();
    let path_for_stream = dest_path.clone();
    let app_for_stream = app.clone();
    let streamed = tokio::task::spawn_blocking(move || {
        stream_decode_speech_segments(
            &path_for_stream,
            VAD_REDEMPTION_TIME_MS,
            expected_duration,
            |stream_progress, segments_found| {
                let overall = 15 + ((stream_progress as f32 * 0.15) as u32);
                emit_progress(
                    &app_for_stream,
                    "vad",
                    overall,
                    &format!(
                        "Streaming audio... {}% ({} speech segments)",
                        stream_progress, segments_found
                    ),
                );
                !IMPORT_CANCELLED.load(Ordering::SeqCst)
            },
        )
    })
    .await
    .map_err(|error| anyhow!("Streaming decode task panicked: {error}"))?;

    let (speech_segments, duration_seconds) = match streamed {
        Ok(result) => result,
        Err(stream_error) => {
            let source_size = std::fs::metadata(&dest_path)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            const SAFE_FULL_DECODE_LIMIT: u64 = 512 * 1024 * 1024;
            if source_size > SAFE_FULL_DECODE_LIMIT {
                return Err(anyhow!(
                    "Large recording could not be processed in memory-safe streaming mode: {stream_error}"
                ));
            }

            warn!(
                "Streaming decode failed ({}); using compatibility decoder for small file",
                stream_error
            );
            let path_for_decode = dest_path.clone();
            let decoded = tokio::task::spawn_blocking(move || {
                decode_audio_file_to_whisper(&path_for_decode).or_else(|_| {
                    let decoded = decode_audio_file_with_progress(&path_for_decode, None)?;
                    let duration_seconds = decoded.duration_seconds;
                    Ok::<crate::audio::decoder::WhisperAudio, anyhow::Error>(
                        crate::audio::decoder::WhisperAudio {
                            samples: decoded.to_whisper_format(),
                            duration_seconds,
                        },
                    )
                })
            })
            .await
            .map_err(|error| anyhow!("Compatibility decode task panicked: {error}"))??;
            let duration = decoded.duration_seconds;
            let app_for_vad = app.clone();
            let segments = tokio::task::spawn_blocking(move || {
                get_speech_chunks_with_progress(
                    &decoded.samples,
                    VAD_REDEMPTION_TIME_MS,
                    |vad_progress, segments_found| {
                        emit_progress(
                            &app_for_vad,
                            "vad",
                            20 + ((vad_progress as f32 * 0.10) as u32),
                            &format!(
                                "Detecting speech segments... {}% ({} found)",
                                vad_progress, segments_found
                            ),
                        );
                        !IMPORT_CANCELLED.load(Ordering::SeqCst)
                    },
                )
            })
            .await
            .map_err(|error| anyhow!("VAD task panicked: {error}"))??;
            (segments, duration)
        }
    };

    let total_segments = speech_segments.len();
    info!(
        "VAD detected {} speech segments (redemption_time={}ms)",
        total_segments, VAD_REDEMPTION_TIME_MS
    );

    // Diagnostic: log segment duration distribution
    if !speech_segments.is_empty() {
        let durations_ms: Vec<f64> = speech_segments
            .iter()
            .map(|s| s.end_timestamp_ms - s.start_timestamp_ms)
            .collect();
        let total_speech_ms: f64 = durations_ms.iter().sum();
        let avg_duration = total_speech_ms / durations_ms.len() as f64;
        let min_duration = durations_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_duration = durations_ms
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        info!(
            "VAD segment stats: avg={:.0}ms, min={:.0}ms, max={:.0}ms, total_speech={:.1}s/{:.1}s ({:.0}%)",
            avg_duration, min_duration, max_duration,
            total_speech_ms / 1000.0, duration_seconds,
            (total_speech_ms / 1000.0 / duration_seconds) * 100.0
        );
        // Log first 10 segments for detailed inspection
        for (i, seg) in speech_segments.iter().take(10).enumerate() {
            let dur = seg.end_timestamp_ms - seg.start_timestamp_ms;
            debug!(
                "  Segment {}: {:.0}ms-{:.0}ms ({:.0}ms, {} samples)",
                i,
                seg.start_timestamp_ms,
                seg.end_timestamp_ms,
                dur,
                seg.samples.len()
            );
        }
        if total_segments > 10 {
            debug!("  ... and {} more segments", total_segments - 10);
        }
    }

    if total_segments == 0 {
        warn!("No speech detected in audio");

        // Emit warning to frontend
        let _ = app.emit(
            "import-warning",
            ImportWarning {
                warning: "No speech detected in audio file".to_string(),
                details: Some(
                    "The file was imported successfully, but VAD did not detect any speech. \
                     The meeting was created but contains no transcripts."
                        .to_string(),
                ),
            },
        );
        // Still create the meeting, just with no transcripts
    }

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "transcribing", 30, "Loading transcription engine...");

    // Initialize the appropriate engine
    let whisper_engine = if !use_parakeet && !use_gigaam && !use_salutespeech && total_segments > 0
    {
        Some(get_or_init_whisper(&app, model.as_deref()).await?)
    } else {
        None
    };
    let parakeet_engine = if use_parakeet && total_segments > 0 {
        Some(get_or_init_parakeet(&app, model.as_deref()).await?)
    } else {
        None
    };
    // GigaAM uses a process-global model (no per-batch engine object) — just ensure it's
    // loaded (downloaded via Settings → Transcription and loaded at startup).
    if use_gigaam && total_segments > 0 && !crate::gigaam_engine::is_loaded() {
        return Err(anyhow!(
            "GigaAM model is not loaded. Download it in Settings → Transcription first."
        ));
    }
    // SaluteSpeech (cloud): build the provider up front so we fail fast if the auth key
    // is missing, then reuse it (and its cached token) for every segment.
    let salute_provider = if use_salutespeech && total_segments > 0 {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| anyhow!("App state not available for SaluteSpeech"))?;
        let cfg = crate::salutespeech::resolve_config(state.db_manager.pool())
            .await
            .ok_or_else(|| {
                anyhow!("SaluteSpeech Authorization Key is not configured. Set it in Settings → Transcription.")
            })?;
        Some(crate::salutespeech::SaluteSpeechProvider::new(cfg))
    } else {
        None
    };

    // Split very long segments at silence boundaries for better transcription quality.
    // Hard cuts at arbitrary sample positions lose words at boundaries. Instead, scan
    // for the lowest-energy window near the target split point and cut there.
    const MAX_SEGMENT_SAMPLES: usize = 25 * 16000; // 25 seconds at 16kHz

    let mut processable_segments: Vec<crate::audio::vad::SpeechSegment> = Vec::new();
    for segment in speech_segments {
        if segment.samples.len() > MAX_SEGMENT_SAMPLES {
            debug!(
                "Splitting large segment ({:.0}ms, {} samples) at silence boundaries",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len()
            );

            let sub_segments = split_segment_at_silence(&segment, MAX_SEGMENT_SAMPLES);
            debug!("Split into {} sub-segments", sub_segments.len());
            processable_segments.extend(sub_segments);
        } else {
            processable_segments.push(segment);
        }
    }

    let processable_count = processable_segments.len();
    info!(
        "Processing {} segments (after splitting)",
        processable_count
    );

    // Process each speech segment
    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new();
    let mut total_confidence = 0.0f32;
    let mut measured_confidence_count = 0usize;

    for (i, segment) in processable_segments.iter().enumerate() {
        if IMPORT_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Import cancelled"));
        }

        let progress = 30 + ((i as f32 / processable_count.max(1) as f32) * 50.0) as u32;
        let segment_duration_sec = (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0;
        emit_progress(
            &app,
            "transcribing",
            progress,
            &format!(
                "Transcribing segment {} of {} ({:.1}s)...",
                i + 1,
                processable_count,
                segment_duration_sec
            ),
        );

        // Skip very short segments
        if segment.samples.len() < 1600 {
            debug!(
                "Skipping short segment {} with {} samples",
                i,
                segment.samples.len()
            );
            continue;
        }

        // Transcribe
        let (text, conf) = if use_salutespeech {
            use crate::audio::transcription::TranscriptionProvider;
            let provider = salute_provider
                .as_ref()
                .expect("SaluteSpeech provider built above when use_salutespeech");
            match provider
                .transcribe(segment.samples.clone(), language.clone())
                .await
            {
                Ok(r) => (r.text, None),
                Err(e) => {
                    return Err(anyhow!(
                        "SaluteSpeech transcription failed on segment {}: {}",
                        i,
                        e
                    ))
                }
            }
        } else if use_gigaam {
            match crate::gigaam_engine::transcribe(segment.samples.clone()).await {
                Some(Ok(text)) => (text, None),
                Some(Err(e)) => {
                    return Err(anyhow!(
                        "GigaAM transcription failed on segment {}: {}",
                        i,
                        e
                    ))
                }
                None => return Err(anyhow!("GigaAM model not loaded")),
            }
        } else if use_parakeet {
            let engine = parakeet_engine.as_ref().unwrap();
            let text = engine
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("Parakeet transcription failed on segment {}: {}", i, e))?;
            (text, None)
        } else {
            let engine = whisper_engine.as_ref().unwrap();
            let (text, conf, _) = engine
                .transcribe_audio_with_confidence(segment.samples.clone(), language.clone())
                .await
                .map_err(|e| anyhow!("Whisper transcription failed on segment {}: {}", i, e))?;
            (text, Some(conf))
        };

        let trimmed = text.trim();
        if !trimmed.is_empty() {
            debug!(
                "Segment {}/{}: {:.1}s, conf={}, text='{}'",
                i + 1,
                processable_count,
                segment_duration_sec,
                conf.map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "unavailable".to_string()),
                if trimmed.len() > 80 {
                    let mut end = 80;
                    while !trimmed.is_char_boundary(end) {
                        end -= 1;
                    }
                    &trimmed[..end]
                } else {
                    trimmed
                }
            );
            all_transcripts.push((text, segment.start_timestamp_ms, segment.end_timestamp_ms));
            if let Some(confidence) = conf {
                total_confidence += confidence;
                measured_confidence_count += 1;
            }
        } else {
            debug!(
                "Segment {}/{}: {:.1}s — empty transcription",
                i + 1,
                processable_count,
                segment_duration_sec
            );
        }
    }

    let transcribed_count = all_transcripts.len();
    let avg_confidence = if measured_confidence_count > 0 {
        Some(total_confidence / measured_confidence_count as f32)
    } else {
        None
    };
    let empty_segments = processable_count.saturating_sub(transcribed_count);
    let transcription_coverage = if processable_count > 0 {
        Some(transcribed_count as f64 / processable_count as f64)
    } else {
        None
    };

    match avg_confidence {
        Some(confidence) => info!(
            "Transcription complete: {} segments transcribed out of {}, measured avg confidence: {:.2}",
            transcribed_count, processable_count, confidence
        ),
        None => info!(
            "Transcription complete: {} segments transcribed out of {}, provider confidence unavailable",
            transcribed_count, processable_count
        ),
    }

    // Check for cancellation
    if IMPORT_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Import cancelled"));
    }

    emit_progress(&app, "saving", 85, "Creating meeting...");

    // Create transcript segments
    let segments = create_transcript_segments(&all_transcripts);

    // Save to database
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    // Write transcripts.json and metadata.json to the meeting folder
    emit_progress(&app, "saving", 90, "Writing transcript files...");

    if let Err(e) = write_transcripts_json(&meeting_folder, &segments) {
        warn!("Failed to write transcripts.json: {}", e);
    }

    let meeting_id = format!("meeting-{}", Uuid::new_v4());
    let pool = app_state.db_manager.pool();
    create_meeting_with_transcripts(
        pool,
        &meeting_id,
        &title,
        &segments,
        meeting_folder.to_string_lossy().to_string(),
    )
    .await?;

    if let Err(error) = write_import_metadata(
        &meeting_folder,
        &meeting_id,
        &title,
        duration_seconds,
        &dest_filename,
        "import",
        source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio"),
        &source_sha256,
        processable_count,
        transcribed_count,
        avg_confidence,
    ) {
        if let Err(cleanup_error) = delete_newly_imported_meeting(pool, &meeting_id).await {
            // The DB row still points at this folder. Keep it intact rather than
            // letting the pending-folder guard create a broken database reference.
            pending_folder.commit();
            return Err(anyhow!(
                "Failed to write resumable import metadata: {error}; \
                 failed to roll back imported meeting: {cleanup_error}"
            ));
        }
        return Err(anyhow!(
            "Failed to write resumable import metadata: {error}"
        ));
    }
    pending_folder.commit();

    emit_progress(&app, "complete", 100, "Import complete");

    Ok(ImportResult {
        meeting_id,
        title,
        segments_count: segments.len(),
        duration_seconds,
        processable_segments: processable_count,
        transcribed_segments: transcribed_count,
        empty_segments,
        transcription_coverage,
        average_confidence: avg_confidence,
    })
}

/// Emit progress event
fn emit_progress<R: Runtime>(app: &AppHandle<R>, stage: &str, progress: u32, message: &str) {
    let _ = app.emit(
        "import-progress",
        ImportProgress {
            stage: stage.to_string(),
            progress_percentage: progress,
            message: message.to_string(),
        },
    );
}

/// Create a new meeting with transcripts in the database
async fn create_meeting_with_transcripts(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    title: &str,
    segments: &[TranscriptSegment],
    folder_path: String,
) -> Result<()> {
    let now = chrono::Utc::now();

    // Start transaction
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| anyhow!("DB error: {}", e))?;
    let mut tx = sqlx::Connection::begin(&mut *conn)
        .await
        .map_err(|e| anyhow!("Failed to start transaction: {}", e))?;

    // Insert meeting
    sqlx::query(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(meeting_id)
    .bind(title)
    .bind(now)
    .bind(now)
    .bind(&folder_path)
    .execute(&mut *tx)
    .await
    .map_err(|e| anyhow!("Failed to create meeting: {}", e))?;

    // Insert transcripts
    for segment in segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&segment.id)
        .bind(meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("Failed to insert transcript: {}", e))?;
    }

    tx.commit()
        .await
        .map_err(|e| anyhow!("Failed to commit transaction: {}", e))?;
    if let Err(error) = crate::collections::auto_assign_meeting(pool, meeting_id, title).await {
        warn!(
            "Could not apply automatic series rules to imported meeting {}: {}",
            meeting_id, error
        );
    }

    info!(
        "Created meeting '{}' with {} transcripts",
        meeting_id,
        segments.len()
    );

    Ok(())
}

async fn delete_newly_imported_meeting(pool: &sqlx::SqlitePool, meeting_id: &str) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| anyhow!("Failed to start import cleanup transaction: {error}"))?;
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| anyhow!("Failed to remove imported transcripts: {error}"))?;
    sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| anyhow!("Failed to remove imported meeting: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| anyhow!("Failed to commit import cleanup: {error}"))?;
    Ok(())
}

/// Get or initialize the Whisper engine
async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<WhisperEngine>> {
    use crate::whisper_engine::commands::WHISPER_ENGINE;

    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_model(app, "whisper").await?,
            };

            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Whisper model '{}' (current: {:?})",
                    target_model, current_model
                );

                if let Err(e) = e.discover_models().await {
                    warn!("Model discovery error (continuing): {}", e);
                }

                e.load_model(&target_model)
                    .await
                    .map_err(|e| anyhow!("Failed to load model '{}': {}", target_model, e))?;
            }

            Ok(e)
        }
        None => Err(anyhow!("Whisper engine not initialized")),
    }
}

/// Get or initialize the Parakeet engine
async fn get_or_init_parakeet<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<ParakeetEngine>> {
    use crate::parakeet_engine::commands::PARAKEET_ENGINE;

    let engine = {
        let guard = PARAKEET_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_model(app, "parakeet").await?,
            };

            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Parakeet model '{}' (current: {:?})",
                    target_model, current_model
                );

                if let Err(e) = e.discover_models().await {
                    warn!("Model discovery error (continuing): {}", e);
                }

                e.load_model(&target_model)
                    .await
                    .map_err(|e| anyhow!("Failed to load model '{}': {}", target_model, e))?;
            }

            Ok(e)
        }
        None => Err(anyhow!("Parakeet engine not initialized")),
    }
}

/// Get the configured model from database
async fn get_configured_model<R: Runtime>(
    app: &AppHandle<R>,
    provider_type: &str,
) -> Result<String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let result: Option<(String, String)> =
        sqlx::query_as("SELECT provider, model FROM transcript_settings WHERE id = '1'")
            .fetch_optional(app_state.db_manager.pool())
            .await
            .map_err(|e| anyhow!("Failed to query config: {}", e))?;

    match result {
        Some((provider, model)) => {
            if (provider_type == "whisper" && (provider == "localWhisper" || provider == "whisper"))
                || (provider_type == "parakeet" && provider == "parakeet")
            {
                Ok(model)
            } else {
                // Return default model for the requested type
                Ok(if provider_type == "parakeet" {
                    DEFAULT_PARAKEET_MODEL.to_string()
                } else {
                    DEFAULT_WHISPER_MODEL.to_string()
                })
            }
        }
        None => Ok(if provider_type == "parakeet" {
            DEFAULT_PARAKEET_MODEL.to_string()
        } else {
            DEFAULT_WHISPER_MODEL.to_string()
        }),
    }
}

/// Write metadata.json to a meeting folder (atomic write with temp file)
fn write_import_metadata(
    folder: &Path,
    meeting_id: &str,
    title: &str,
    duration_seconds: f64,
    audio_filename: &str,
    source: &str,
    source_filename: &str,
    source_sha256: &str,
    processable_segments: usize,
    transcribed_segments: usize,
    average_confidence: Option<f32>,
) -> Result<()> {
    let metadata_path = folder.join("metadata.json");
    let temp_path = folder.join(".metadata.json.tmp");
    let now = chrono::Utc::now().to_rfc3339();

    let json = serde_json::json!({
        "version": "1.1",
        "meeting_id": meeting_id,
        "meeting_name": title,
        "created_at": now,
        "completed_at": now,
        "duration_seconds": duration_seconds,
        "audio_file": audio_filename,
        "transcript_file": "transcripts.json",
        "status": "completed",
        "source": source,
        "source_filename": source_filename,
        "source_sha256": source_sha256,
        "transcription_quality": {
            "processable_segments": processable_segments,
            "transcribed_segments": transcribed_segments,
            "empty_segments": processable_segments.saturating_sub(transcribed_segments),
            "coverage_ratio": if processable_segments > 0 {
                Some(transcribed_segments as f64 / processable_segments as f64)
            } else {
                None
            },
            "average_confidence": average_confidence,
            "confidence_source": if average_confidence.is_some() {
                "model"
            } else {
                "unavailable"
            }
        }
    });

    let json_string = serde_json::to_string_pretty(&json)?;
    std::fs::write(&temp_path, &json_string)?;
    std::fs::rename(&temp_path, &metadata_path)?;

    info!("Wrote metadata.json to {}", metadata_path.display());
    Ok(())
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Select an audio file and validate it
#[tauri::command]
pub async fn select_and_validate_audio_command<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<AudioFileInfo>, String> {
    info!("Opening file dialog for audio import");

    // Use spawn_blocking to avoid blocking async runtime
    let app_clone = app.clone();
    let file_path = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .add_filter(
                "Audio Files",
                &AUDIO_EXTENSIONS.iter().map(|s| *s).collect::<Vec<_>>(),
            )
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("File dialog task failed: {}", e))?;

    match file_path {
        Some(path) => {
            let path_str = path.to_string();
            info!("User selected: {}", path_str);

            match validate_audio_file(Path::new(&path_str)) {
                Ok(info) => Ok(Some(info)),
                Err(e) => {
                    error!("Validation failed: {}", e);
                    Err(e.to_string())
                }
            }
        }
        None => {
            info!("User cancelled file selection");
            Ok(None)
        }
    }
}

/// Select a folder and validate supported audio files in it recursively.
#[tauri::command]
pub async fn select_and_validate_audio_folder_command<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<Vec<AudioFileInfo>>, String> {
    info!("Opening folder dialog for batch audio import");
    let app_clone = app.clone();
    let folder_path =
        tokio::task::spawn_blocking(move || app_clone.dialog().file().blocking_pick_folder())
            .await
            .map_err(|error| format!("Folder dialog task failed: {}", error))?;

    let Some(folder_path) = folder_path else {
        info!("User cancelled folder selection");
        return Ok(None);
    };
    let folder = PathBuf::from(folder_path.to_string());
    let files = tokio::task::spawn_blocking(move || -> Result<Vec<AudioFileInfo>> {
        Ok(collect_audio_files(&folder)?
            .iter()
            .map(|path| match validate_audio_file(path) {
                Ok(info) => info,
                Err(error) => {
                    // Keep the item in the batch. The importer will report this file
                    // as an individual failure instead of aborting folder selection
                    // and hiding all otherwise-valid recordings.
                    warn!(
                        "Deferring failed folder validation for {}: {}",
                        path.display(),
                        error
                    );
                    deferred_audio_file_info(path)
                }
            })
            .collect())
    })
    .await
    .map_err(|error| format!("Folder validation task failed: {}", error))?
    .map_err(|error| error.to_string())?;

    if files.is_empty() {
        return Err("No supported audio files found in the selected folder".to_string());
    }
    Ok(Some(files))
}

/// Validate an audio file from a given path (for drag-drop)
#[tauri::command]
pub async fn validate_audio_file_command(path: String) -> Result<AudioFileInfo, String> {
    info!("Validating audio file: {}", path);
    validate_audio_file(Path::new(&path)).map_err(|e| e.to_string())
}

/// Start importing an audio file (Beta gated using configContext.betaFeatures)
#[tauri::command]
pub async fn start_import_audio_command<R: Runtime>(
    app: AppHandle<R>,
    source_path: String,
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportStarted, String> {
    // Check if import is already in progress (guard will be acquired in start_import)
    if IMPORT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Import already in progress".to_string());
    }

    // Spawn import in background
    tauri::async_runtime::spawn(async move {
        let result = start_import(app, source_path, title, language, model, provider).await;

        if let Err(e) = result {
            error!("Import failed: {}", e);
        }
    });

    Ok(ImportStarted {
        message: "Import started".to_string(),
    })
}

/// Start a resilient, sequential batch import.
#[tauri::command]
pub async fn start_batch_import_audio_command<R: Runtime>(
    app: AppHandle<R>,
    items: Vec<BatchImportItem>,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportStarted, String> {
    if IMPORT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Import already in progress".to_string());
    }
    if items.is_empty() {
        return Err("No audio files selected".to_string());
    }

    tauri::async_runtime::spawn(async move {
        if let Err(error) = start_batch_import(app.clone(), items, language, model, provider).await
        {
            error!("Batch import failed to start: {}", error);
            let _ = app.emit(
                "batch-import-error",
                ImportError {
                    error: error.to_string(),
                },
            );
        }
    });

    Ok(ImportStarted {
        message: "Batch import started".to_string(),
    })
}

/// Start a resumable batch import from a folder path supplied by automation or a
/// trusted local caller. This is the picker-free counterpart to the folder UI.
#[tauri::command]
pub async fn start_batch_import_folder_command<R: Runtime>(
    app: AppHandle<R>,
    folder_path: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    report_path: Option<String>,
) -> Result<ImportStarted, String> {
    if IMPORT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err("Import already in progress".to_string());
    }
    let folder = PathBuf::from(&folder_path);
    if !folder.is_dir() {
        return Err(format!(
            "Import folder does not exist: {}",
            folder.display()
        ));
    }
    let report_path = report_path.map(PathBuf::from);

    tauri::async_runtime::spawn(async move {
        match start_batch_import_folder(app.clone(), folder, language, model, provider, report_path)
            .await
        {
            Ok(result) => {
                info!(
                    "Folder batch import complete: {} imported, {} skipped, {} truncated, {} failed",
                    result.imported.len(),
                    result.skipped.len(),
                    result.truncated.len(),
                    result.failed.len()
                );
            }
            Err(error) => {
                error!("Folder batch import failed: {}", error);
                let _ = app.emit(
                    "batch-import-error",
                    ImportError {
                        error: error.to_string(),
                    },
                );
            }
        }
    });

    Ok(ImportStarted {
        message: "Folder batch import started".to_string(),
    })
}

/// Cancel ongoing import
#[tauri::command]
pub async fn cancel_import_command() -> Result<(), String> {
    if !is_import_in_progress() {
        return Err("No import in progress".to_string());
    }
    cancel_import();
    Ok(())
}

/// Check if import is in progress
#[tauri::command]
pub async fn is_import_in_progress_command() -> bool {
    is_import_in_progress()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_extensions() {
        assert!(AUDIO_EXTENSIONS.contains(&"mp4"));
        assert!(AUDIO_EXTENSIONS.contains(&"wav"));
        assert!(AUDIO_EXTENSIONS.contains(&"mp3"));
        assert!(!AUDIO_EXTENSIONS.contains(&"txt"));
    }

    #[test]
    fn streaming_decode_keeps_pcm_out_of_the_result() {
        if find_ffmpeg_path().is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("two-seconds.wav");
        let pcm_bytes = 32_000_u32 * 2;
        let mut contents = Vec::with_capacity(44 + pcm_bytes as usize);
        contents.extend_from_slice(b"RIFF");
        contents.extend_from_slice(&(36 + pcm_bytes).to_le_bytes());
        contents.extend_from_slice(b"WAVEfmt ");
        contents.extend_from_slice(&16_u32.to_le_bytes());
        contents.extend_from_slice(&1_u16.to_le_bytes());
        contents.extend_from_slice(&1_u16.to_le_bytes());
        contents.extend_from_slice(&16_000_u32.to_le_bytes());
        contents.extend_from_slice(&32_000_u32.to_le_bytes());
        contents.extend_from_slice(&2_u16.to_le_bytes());
        contents.extend_from_slice(&16_u16.to_le_bytes());
        contents.extend_from_slice(b"data");
        contents.extend_from_slice(&pcm_bytes.to_le_bytes());
        contents.resize(44 + pcm_bytes as usize, 0);
        std::fs::write(&wav, contents).unwrap();

        let (segments, duration) =
            stream_decode_speech_segments(&wav, VAD_REDEMPTION_TIME_MS, Some(2.0), |_, _| true)
                .unwrap();
        assert!(segments.is_empty());
        assert!((duration - 2.0).abs() < 0.02);
    }

    #[test]
    fn test_strip_hash_suffix_only_removes_corpus_hash() {
        assert_eq!(
            strip_hash_suffix("2026-07-15_11-00_standup__deadbeef"),
            "2026-07-15_11-00_standup"
        );
        assert_eq!(
            strip_hash_suffix("standup__not-a-hash"),
            "standup__not-a-hash"
        );
        assert_eq!(strip_hash_suffix("standup"), "standup");
    }

    #[test]
    fn test_batch_items_from_folder_is_recursive_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.path().join("b__deadbeef.mp3"), b"audio").unwrap();
        std::fs::write(nested.join("a.wav"), b"audio").unwrap();
        std::fs::write(dir.path().join("ignored.txt"), b"text").unwrap();

        let items = batch_items_from_folder(dir.path()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "b");
        assert_eq!(items[1].title, "a");
    }

    #[test]
    fn test_batch_limit_reports_overflow_as_skipped_items() {
        let items = (0..(MAX_BATCH_AUDIO_FILES + 3))
            .map(|index| BatchImportItem {
                source_path: format!("/audio/{index}.wav"),
                title: format!("Meeting {index}"),
            })
            .collect();
        let (processable, over_limit) = cap_batch_items(items);
        assert_eq!(processable.len(), MAX_BATCH_AUDIO_FILES);
        assert_eq!(over_limit.len(), 3);
        assert_eq!(over_limit[0].title, "Meeting 500");
    }

    #[cfg(unix)]
    #[test]
    fn test_folder_scan_follows_file_symlinks_but_not_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("original.wav");
        let linked = dir.path().join("linked.wav");
        let nested = dir.path().join("nested");
        std::fs::write(&target, b"audio").unwrap();
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("hidden.wav"), b"audio").unwrap();
        symlink(&target, &linked).unwrap();
        symlink(&nested, dir.path().join("nested-link")).unwrap();

        let files = collect_audio_files(dir.path()).unwrap();
        assert!(files.contains(&target));
        assert!(files.contains(&linked));
        assert!(files.contains(&nested.join("hidden.wav")));
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_create_transcript_segments_empty() {
        let transcripts: Vec<(String, f64, f64)> = vec![];
        let segments = create_transcript_segments(&transcripts);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_create_transcript_segments_single() {
        let transcripts = vec![("Hello world".to_string(), 0.0, 1500.0)];
        let segments = create_transcript_segments(&transcripts);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[0].audio_end_time, Some(1.5));
    }

    #[test]
    fn test_cancellation_flag() {
        IMPORT_CANCELLED.store(false, Ordering::SeqCst);
        IMPORT_IN_PROGRESS.store(false, Ordering::SeqCst);

        assert!(!is_import_in_progress());

        cancel_import();
        assert!(IMPORT_CANCELLED.load(Ordering::SeqCst));

        // Reset
        IMPORT_CANCELLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_extract_duration_from_metadata_wav() {
        // Test with sample WAV file if available
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.wav");
        if test_path.exists() {
            let result = extract_duration_from_metadata(test_path);
            // Should succeed and return a reasonable duration
            assert!(result.is_ok());
            let duration = result.unwrap();
            assert!(
                duration > 0.0 && duration < 60.0,
                "Duration {} seems unreasonable",
                duration
            );
        }
    }

    #[test]
    fn test_extract_duration_from_metadata_mp3() {
        // Test with sample MP3 file if available
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.mp3");
        if test_path.exists() {
            let result = extract_duration_from_metadata(test_path);
            // MP3 files may not have n_frames metadata, so fallback is expected
            // We just verify it doesn't panic
            let _ = result;
        }
    }

    #[test]
    fn test_validate_audio_file_with_metadata() {
        // Test validation with actual audio file
        let test_path = Path::new("../../backend/whisper.cpp/samples/jfk.wav");
        if test_path.exists() {
            let result = validate_audio_file(test_path);
            assert!(result.is_ok());
            let info = result.unwrap();
            assert_eq!(info.format, "WAV");
            assert!(info.duration_seconds > 0.0);
            assert!(info.size_bytes > 0);
        }
    }

    #[test]
    fn deferred_audio_info_keeps_invalid_file_in_batch_queue() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.mp3");
        std::fs::write(&path, b"bad").unwrap();

        let info = deferred_audio_file_info(&path);

        assert_eq!(info.path, path.to_string_lossy());
        assert_eq!(info.filename, "broken");
        assert_eq!(info.duration_seconds, 0.0);
        assert_eq!(info.size_bytes, 3);
        assert_eq!(info.format, "mp3");
    }

    #[test]
    fn test_validate_audio_file_nonexistent() {
        let result = validate_audio_file(Path::new("/nonexistent/file.mp4"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_validate_audio_file_wrong_extension() {
        // Create a temporary file with wrong extension
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_audio.txt");
        let _ = std::fs::write(&temp_file, b"dummy content");

        let result = validate_audio_file(&temp_file);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported format"));

        // Cleanup
        let _ = std::fs::remove_file(temp_file);
    }

    #[test]
    fn test_split_segment_at_silence_short_segment() {
        // Segment shorter than max — returned as-is
        let segment = crate::audio::vad::SpeechSegment {
            samples: vec![0.1; 16000], // 1 second
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 1000.0,
            confidence: 0.9,
        };
        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].samples.len(), 16000);
    }

    #[test]
    fn test_split_segment_at_silence_splits_long_segment() {
        // 60-second segment of low-level noise with a silent gap at ~25s
        let mut samples = vec![0.01f32; 60 * 16000];
        // Insert silence at 25 seconds (sample 400000)
        for i in (25 * 16000)..(25 * 16000 + 3200) {
            samples[i] = 0.0;
        }
        let segment = crate::audio::vad::SpeechSegment {
            samples,
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 60_000.0,
            confidence: 0.9,
        };

        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert!(
            result.len() >= 2,
            "Should split into at least 2 segments, got {}",
            result.len()
        );

        // All sub-segments should have samples
        for (i, seg) in result.iter().enumerate() {
            assert!(!seg.samples.is_empty(), "Segment {} is empty", i);
            assert!(
                seg.start_timestamp_ms < seg.end_timestamp_ms,
                "Segment {} has invalid timestamps: {} >= {}",
                i,
                seg.start_timestamp_ms,
                seg.end_timestamp_ms
            );
        }
    }

    #[test]
    fn test_split_segment_at_silence_no_silence_uses_overlap() {
        // Continuous speech (constant energy) — should still split with overlap
        let segment = crate::audio::vad::SpeechSegment {
            samples: vec![0.5f32; 60 * 16000], // 60 seconds of "speech"
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 60_000.0,
            confidence: 0.9,
        };

        let result = split_segment_at_silence(&segment, 25 * 16000);
        assert!(result.len() >= 2);

        // Total samples should exceed input due to overlap
        let total_samples: usize = result.iter().map(|s| s.samples.len()).sum();
        assert!(
            total_samples >= 60 * 16000,
            "Overlap should not lose samples"
        );
    }

    #[test]
    fn test_write_transcripts_json() {
        let dir = tempfile::tempdir().unwrap();
        let segments = vec![
            TranscriptSegment {
                id: "t-1".to_string(),
                text: "Hello world".to_string(),
                timestamp: "2024-01-01T00:00:00Z".to_string(),
                audio_start_time: Some(0.0),
                audio_end_time: Some(1.5),
                duration: Some(1.5),
                speaker: None,
            },
            TranscriptSegment {
                id: "t-2".to_string(),
                text: "Second segment".to_string(),
                timestamp: "2024-01-01T00:00:01Z".to_string(),
                audio_start_time: Some(2.0),
                audio_end_time: Some(3.5),
                duration: Some(1.5),
                speaker: None,
            },
        ];

        let result = write_transcripts_json(dir.path(), &segments);
        assert!(
            result.is_ok(),
            "write_transcripts_json failed: {:?}",
            result
        );

        // Verify file exists and is valid JSON
        let path = dir.path().join("transcripts.json");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["total_segments"], 2);
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["segments"][0]["text"], "Hello world");
        assert_eq!(parsed["segments"][1]["text"], "Second segment");
        assert_eq!(parsed["segments"][0]["sequence_id"], 0);
        assert_eq!(parsed["segments"][1]["sequence_id"], 1);

        // Verify temp file was cleaned up
        assert!(!dir.path().join(".transcripts.json.tmp").exists());
    }

    #[test]
    fn test_write_import_metadata() {
        let dir = tempfile::tempdir().unwrap();

        let result = write_import_metadata(
            dir.path(),
            "meeting-123",
            "Test Meeting",
            1800.0,
            "audio.mp4",
            "import",
            "source.mp4",
            "abc123",
            10,
            8,
            Some(0.75),
        );
        assert!(result.is_ok(), "write_import_metadata failed: {:?}", result);

        let path = dir.path().join("metadata.json");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], "1.1");
        assert_eq!(parsed["meeting_id"], "meeting-123");
        assert_eq!(parsed["meeting_name"], "Test Meeting");
        assert_eq!(parsed["duration_seconds"], 1800.0);
        assert_eq!(parsed["audio_file"], "audio.mp4");
        assert_eq!(parsed["status"], "completed");
        assert_eq!(parsed["source"], "import");
        assert_eq!(parsed["source_filename"], "source.mp4");
        assert_eq!(parsed["source_sha256"], "abc123");
        assert_eq!(parsed["transcription_quality"]["processable_segments"], 10);
        assert_eq!(parsed["transcription_quality"]["transcribed_segments"], 8);
        assert_eq!(parsed["transcription_quality"]["empty_segments"], 2);
        assert_eq!(parsed["transcription_quality"]["coverage_ratio"], 0.8);
        assert_eq!(parsed["transcription_quality"]["average_confidence"], 0.75);
        assert_eq!(
            parsed["transcription_quality"]["confidence_source"],
            "model"
        );
    }

    #[test]
    fn test_write_import_metadata_does_not_invent_quality() {
        let dir = tempfile::tempdir().unwrap();

        write_import_metadata(
            dir.path(),
            "meeting-empty",
            "No speech",
            12.0,
            "audio.mp3",
            "import",
            "source.mp3",
            "def456",
            0,
            0,
            None,
        )
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("metadata.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed["transcription_quality"]["coverage_ratio"].is_null());
        assert!(parsed["transcription_quality"]["average_confidence"].is_null());
        assert_eq!(
            parsed["transcription_quality"]["confidence_source"],
            "unavailable"
        );
    }

    #[tokio::test]
    async fn test_import_cleanup_removes_committed_meeting() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE meetings(
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT,
                updated_at TEXT,
                folder_path TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(
                id TEXT PRIMARY KEY,
                meeting_id TEXT,
                transcript TEXT,
                timestamp TEXT,
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        create_meeting_with_transcripts(&pool, "meeting-test", "Test", &[], "/tmp/test".into())
            .await
            .unwrap();
        delete_newly_imported_meeting(&pool, "meeting-test")
            .await
            .unwrap();

        let meetings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings")
            .fetch_one(&pool)
            .await
            .unwrap();
        let transcripts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transcripts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(meetings, 0);
        assert_eq!(transcripts, 0);
    }

    #[test]
    fn test_sha256_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audio.mp3");
        std::fs::write(&path, b"memento").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "fe432cb211dac676fcd7d2f05033f82be9fd8325923e6e3a322758fee60e94cf"
        );
    }

    /// Integration test that decodes a real audio file and runs VAD.
    /// Run with: TEST_AUDIO_PATH=/path/to/audio.mp4 cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn test_import_pipeline_decode_vad() {
        let audio_path = std::env::var("TEST_AUDIO_PATH")
            .expect("Set TEST_AUDIO_PATH to run this integration test");

        let path = Path::new(&audio_path);
        assert!(path.exists(), "Audio file not found: {}", audio_path);

        // Step 1: Decode
        println!("Decoding {}...", audio_path);
        let decoded =
            crate::audio::decoder::decode_audio_file(path).expect("Failed to decode audio file");
        println!(
            "Decoded: {:.2}s, {}Hz, {} channels, {} samples",
            decoded.duration_seconds,
            decoded.sample_rate,
            decoded.channels,
            decoded.samples.len()
        );

        // Step 2: Resample to 16kHz mono
        println!("Resampling to 16kHz mono...");
        let samples = decoded.to_whisper_format();
        println!(
            "Resampled: {} samples ({:.2}s at 16kHz)",
            samples.len(),
            samples.len() as f64 / 16000.0
        );

        // Step 3: Run VAD with both redemption times and compare
        for redemption_ms in [400u32, 2000] {
            println!("\n--- VAD with redemption_time={}ms ---", redemption_ms);
            let segments = crate::audio::vad::get_speech_chunks_with_progress(
                &samples,
                redemption_ms,
                |progress, count| {
                    if progress % 20 == 0 {
                        println!("  VAD progress: {}% ({} segments)", progress, count);
                    }
                    true
                },
            )
            .expect("VAD failed");

            let total_segments = segments.len();
            println!("Found {} segments", total_segments);

            if !segments.is_empty() {
                let durations: Vec<f64> = segments
                    .iter()
                    .map(|s| s.end_timestamp_ms - s.start_timestamp_ms)
                    .collect();
                let total_speech: f64 = durations.iter().sum();
                let avg = total_speech / durations.len() as f64;
                let min = durations.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = durations.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

                println!(
                    "Stats: avg={:.0}ms, min={:.0}ms, max={:.0}ms, total_speech={:.1}s/{:.1}s ({:.0}%)",
                    avg, min, max,
                    total_speech / 1000.0,
                    decoded.duration_seconds,
                    (total_speech / 1000.0 / decoded.duration_seconds) * 100.0
                );

                // Segments over 25s that would be split
                let oversized = durations.iter().filter(|d| **d > 25_000.0).count();
                println!("Segments >25s (would be split): {}", oversized);

                // Basic sanity checks
                assert!(total_speech > 0.0, "No speech detected");
                for (i, seg) in segments.iter().enumerate() {
                    assert!(!seg.samples.is_empty(), "Segment {} has no samples", i);
                    assert!(
                        seg.end_timestamp_ms > seg.start_timestamp_ms,
                        "Segment {} has invalid timestamps",
                        i
                    );
                }
            }
        }
    }
}
