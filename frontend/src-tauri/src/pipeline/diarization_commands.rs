//! Diarization model management (PLAN.md Phase 2): download / status of the pyannote-style
//! cascade. Files live in `app_data_dir/models/diarization/`, a sibling of the gigaam /
//! embedding model dirs. Two ONNX files are needed:
//!   * `segmentation.onnx`  — pyannote segmentation-3.0 (~5.9 MB, MIT)
//!   * `embedding.v2.onnx`  — 3D-Speaker CAM++ zh-en advanced speaker embeddings (~28 MB,
//!     Apache-2.0, sherpa-onnx export; replaced the WeSpeaker VoxCeleb v1 model 2026-07-20)
//!
//! This local cascade is the only diarization engine (see [`resolve_diarization_provider`]
//! for the measurements behind that). Both files are required: until they are present,
//! `diarization_status.available` is false and diarization reports
//! [`DiarizeError::ModelsUnavailable`] so the UI can offer the one-time download (existing
//! v1 installs re-enter that consent flow once).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::anyhow;
use futures_util::StreamExt;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::speaker::SpeakersRepository;
use crate::pipeline::diarization::{
    assign_segment, DiarizationParams, Diarizer, DiarizerConfig, SpeakerTurn, EMBEDDING_FILE,
    MIN_OVERLAP_RATIO, SEGMENTATION_FILE,
};
use crate::state::AppState;

/// Verified 200-OK direct-download URLs (checked 2026-07-20: 5,983,836 B / 28,281,164 B).
/// The embedding is the 3D-Speaker CAM++ zh-en "advanced" export from sherpa-onnx —
/// see [`super::diarization::EMBEDDING_FILE`] for why v1 (WeSpeaker VoxCeleb) was
/// retired. Note "recongition" is a genuine typo in the upstream release tag.
const SEGMENTATION_URL: &str =
    "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0/segmentation-3.0.onnx";
const EMBEDDING_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx";

/// v1 embedding file retired 2026-07-20 (non-separable on real meeting audio); removed
/// on the next model download so stale installs don't keep 28 MB of dead weight.
const LEGACY_EMBEDDING_FILE: &str = "embedding.onnx";

/// Combined download size in MB, reported by [`diarization_status`] so the Settings and
/// onboarding cards state the real cost instead of hardcoding a number that drifts when the
/// URLs above change. Derived from the byte counts verified in the comment on those URLs.
const DOWNLOAD_MB: u32 = (5_983_836 + 28_281_164) / 1_000_000;

pub fn diarization_model_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("models")
        .join("diarization");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn models_present(dir: &Path) -> bool {
    dir.join(SEGMENTATION_FILE).exists() && dir.join(EMBEDDING_FILE).exists()
}

/// Set while a download runs. There are now two entry points — Settings and onboarding — and
/// letting both stream into the same `.part` file would corrupt it.
static DOWNLOAD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Claims [`DOWNLOAD_IN_PROGRESS`], clearing it on drop so an early `?` can't wedge the flag.
struct DownloadGuard;

impl DownloadGuard {
    /// `None` when a download is already running — the caller should wait for
    /// `diarization-ready` rather than start a second one.
    fn acquire() -> Option<Self> {
        std::sync::atomic::AtomicBool::compare_exchange(
            &DOWNLOAD_IN_PROGRESS,
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .ok()
        .map(|_| Self)
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOAD_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(serde::Serialize, Clone)]
struct DownloadProgress {
    file: String,
    downloaded: u64,
    total: u64,
    percent: u8,
}

/// Stream a URL to `dest` (atomic via a `.part` temp), emitting `diarization-download-progress`.
/// No-op if `dest` already exists.
async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    let tmp = dest.with_extension("part");
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download {label}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download {label}: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    use std::io::Write;
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download {label}: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = ((downloaded.saturating_mul(100)) / total) as u8;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit(
                    "diarization-download-progress",
                    DownloadProgress {
                        file: label.to_string(),
                        downloaded,
                        total,
                        percent: pct,
                    },
                );
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Report whether both diarization models are present, and where they live.
#[tauri::command]
pub async fn diarization_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value, String> {
    let dir = diarization_model_dir(&app)?;
    Ok(serde_json::json!({
        "available": models_present(&dir),
        "model_dir": dir.to_string_lossy(),
        "segmentation_present": dir.join(SEGMENTATION_FILE).exists(),
        "embedding_present": dir.join(EMBEDDING_FILE).exists(),
        "download_mb": DOWNLOAD_MB,
        "downloading": DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::SeqCst),
    }))
}

/// Download both diarization models (segmentation first, then the larger embedding model).
/// Emits `diarization-download-progress` while downloading and `diarization-ready` on success.
/// Unlike the transcription models there is no global instance to load here — the diarize
/// job constructs a [`crate::pipeline::diarization::Diarizer`] on demand from `model_dir`.
#[tauri::command]
pub async fn download_diarization_models<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    // A second caller (the other of Settings / onboarding) is a no-op rather than an error:
    // both are waiting on `diarization-ready`, which the in-flight download will emit.
    let Some(_guard) = DownloadGuard::acquire() else {
        log::info!("diarization model download already in progress; not starting a second one");
        return Ok(());
    };
    let result = async {
        let dir = diarization_model_dir(&app)?;
        download_file(
            &app,
            SEGMENTATION_URL,
            &dir.join(SEGMENTATION_FILE),
            SEGMENTATION_FILE,
        )
        .await?;
        download_file(
            &app,
            EMBEDDING_URL,
            &dir.join(EMBEDDING_FILE),
            EMBEDDING_FILE,
        )
        .await?;

        // Retire the v1 embedding if this install still carries it.
        let legacy = dir.join(LEGACY_EMBEDDING_FILE);
        if legacy.exists() {
            if let Err(e) = std::fs::remove_file(&legacy) {
                log::warn!(
                    "could not remove legacy embedding model {}: {e}",
                    legacy.display()
                );
            } else {
                log::info!("removed legacy v1 embedding model {}", legacy.display());
            }
        }
        Ok::<PathBuf, String>(dir)
    }
    .await;

    match result {
        Ok(dir) => {
            let _ = app.emit("diarization-ready", ());
            log::info!("diarization models downloaded to {}", dir.display());
            Ok(())
        }
        // Emitted as well as returned: the card that started the download gets the Err, but
        // the other one (Settings vs onboarding) is only listening to events.
        Err(error) => {
            let _ = app.emit("diarization-download-error", error.clone());
            Err(error)
        }
    }
}

// ---------------------------------------------------------------------------
// Post-meeting diarization: engine → transcript attribution → speaker identities.
// ---------------------------------------------------------------------------

/// Global app handle, set once at startup ([`set_app_handle`]). The background job runner
/// (`crate::jobs`) has no `AppHandle` of its own, so the diarize job reaches Tauri paths
/// (model-dir resolution + `diarization-complete` emission) through this. The app is always
/// the Wry runtime in production; commands still take a generic `AppHandle<R>`.
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Record the process-wide app handle (call once during setup). Idempotent.
pub fn set_app_handle(app: tauri::AppHandle) {
    let _ = APP_HANDLE.set(app);
}

/// The app handle stored by [`set_app_handle`], if any (None before startup / in tests).
pub fn app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.get().cloned()
}

/// Resolve the diarization model dir. `MEETILY_DIARIZATION_MODEL_DIR` overrides the
/// canonical `app_data_dir/models/diarization` location (parity with the diarize job stub).
fn resolve_model_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    if let Ok(env) = std::env::var("MEETILY_DIARIZATION_MODEL_DIR") {
        if !env.trim().is_empty() {
            return Ok(PathBuf::from(env));
        }
    }
    diarization_model_dir(app)
}

/// Why a diarization run produced no attribution — lets the job degrade silently while the
/// command surfaces a user-facing message.
pub enum DiarizeError {
    /// Diarization ONNX models not present. Job: skip (segments stay unattributed).
    ModelsUnavailable,
    /// The meeting has no saved recording to diarize. Job: skip.
    NoRecording,
    /// A genuine failure (decode/inference/DB). Job: propagate so it retries.
    Other(anyhow::Error),
}

/// Counts from a completed diarization run.
pub struct DiarizeOutcome {
    pub speaker_count: i64,
    pub assigned_segments: i64,
    pub total_segments: i64,
}

/// Convert a seconds timestamp (as stored in `transcripts.audio_*_time`) to milliseconds
/// (as produced by the diarizer's [`SpeakerTurn`]s). Rounds to the nearest ms.
fn secs_to_ms(secs: f64) -> i64 {
    (secs * 1000.0).round() as i64
}

fn cluster_overlap_ratio(
    start_ms: i64,
    end_ms: i64,
    cluster_id: i64,
    turns: &[SpeakerTurn],
) -> f64 {
    let duration = (end_ms - start_ms).max(1) as f64;
    let overlap: i64 = turns
        .iter()
        .filter(|turn| turn.cluster_id == cluster_id)
        .map(|turn| (end_ms.min(turn.end_ms) - start_ms.max(turn.start_ms)).max(0))
        .sum();
    (overlap as f64 / duration).clamp(0.0, 1.0)
}

/// Pure assignment glue (unit-tested): map each meeting segment to a resolved speaker id.
///
/// For every segment with known start+end timing, convert seconds→ms, pick the dominant
/// overlapping turn via [`assign_segment`] (≥ [`MIN_OVERLAP_RATIO`]), then translate that
/// turn's `cluster_id` to a `speaker_id` via `cluster_to_speaker`. Segments with NULL
/// timing, no confident overlap, or an unmapped cluster are omitted (they stay NULL).
fn assign_segments_to_speakers(
    segments: &[(String, Option<f64>, Option<f64>)],
    turns: &[SpeakerTurn],
    cluster_to_speaker: &HashMap<i64, i64>,
) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for (id, start_s, end_s) in segments {
        let (Some(start_s), Some(end_s)) = (start_s, end_s) else {
            continue;
        };
        let start_ms = secs_to_ms(*start_s);
        let end_ms = secs_to_ms(*end_s);
        if let Some(cluster_id) = assign_segment(start_ms, end_ms, turns, MIN_OVERLAP_RATIO) {
            if let Some(speaker_id) = cluster_to_speaker.get(&cluster_id) {
                out.push((id.clone(), *speaker_id));
            }
        }
    }
    out
}

fn should_preserve_existing_assignments(
    had_existing_assignments: bool,
    total_segments: i64,
    assigned_segments: usize,
) -> bool {
    had_existing_assignments && total_segments > 0 && assigned_segments == 0
}

/// Diarization engine: always the local ONNX cascade.
///
/// The local cascade measured better on real meetings — the cloud engine found 4 of 7
/// speakers against local's 7/7, which is why the Local/SaluteSpeech selector was removed
/// from Settings → Transcription — and the reply-splitting refinements (two half-offset
/// segmentation grids, interjection carving, covered-dominance attribution) only exist on
/// the local path, so routing to the cloud silently gives up per-reply rows. There is no
/// cloud fallback: when the models aren't downloaded, diarization reports
/// [`DiarizeError::ModelsUnavailable`] and the UI offers the one-time download, rather than
/// quietly producing worse turns.
///
/// `app_settings_kv.diarization.provider` is deliberately *not* consulted: nothing has
/// written that key since the selector was deleted, so a stored value is a stale artifact
/// of a removed control (its own default was the string `"salutespeech"`) and installs
/// carrying one could never get back to the better engine.
///
/// `MEETILY_DIARIZATION_PROVIDER` remains the one way to reach the cloud engine, for
/// headless runs and the `research_salutespeech_diarize` harness.
fn resolve_diarization_provider() -> String {
    std::env::var("MEETILY_DIARIZATION_PROVIDER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/// Create a fresh unconfirmed `Speaker N` profile per distinct cloud speaker id, returning
/// a `cluster_id -> speakers.id` map. Cloud speaker ids aren't stable across meetings, so
/// (unlike the local path) we don't attempt cross-meeting embedding matching — profiles are
/// stored with an empty embedding (never matches) and orphans are GC'd after the run.
async fn resolve_cloud_speakers(
    pool: &SqlitePool,
    turns: &[SpeakerTurn],
) -> anyhow::Result<HashMap<i64, i64>> {
    let mut distinct: Vec<i64> = turns.iter().map(|t| t.cluster_id).collect();
    distinct.sort_unstable();
    distinct.dedup();

    let base_count = SpeakersRepository::count(pool).await?;
    let mut map = HashMap::new();
    for (i, cluster_id) in distinct.iter().enumerate() {
        let name = format!("Speaker {}", base_count + 1 + i as i64);
        let id = SpeakersRepository::insert(pool, &name, &[], false).await?;
        map.insert(*cluster_id, id);
    }
    Ok(map)
}

/// Keep SaluteSpeech's speaker timeline, but derive local voice embeddings when the local
/// models are present. Cloud speaker numbers reset for every recording; embeddings are the
/// only safe way to offer a confirmed cross-meeting identity without trusting those ids.
async fn resolve_cloud_speakers_with_identity<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    audio_path: &Path,
    turns: &[SpeakerTurn],
) -> anyhow::Result<(HashMap<i64, i64>, HashMap<i64, i64>)> {
    let model_dir = resolve_model_dir(app).map_err(anyhow::Error::msg)?;
    let config = DiarizerConfig { model_dir };
    if !config.is_available() {
        log::info!(
            "[diarize] local voice models are unavailable; cloud speakers remain meeting-local"
        );
        return Ok((resolve_cloud_speakers(pool, turns).await?, HashMap::new()));
    }

    let audio_path = audio_path.to_path_buf();
    let embedding_turns = turns.to_vec();
    let cluster_embeddings = match tokio::task::spawn_blocking(move || {
        let diarizer = Diarizer::load(config)?;
        diarizer.embed_labeled_turns(&audio_path, &embedding_turns)
    })
    .await
    {
        Ok(Ok(embeddings)) if !embeddings.is_empty() => embeddings,
        Ok(Ok(_)) => {
            log::warn!("[diarize] cloud turns contained no voice suitable for identity matching");
            return Ok((resolve_cloud_speakers(pool, turns).await?, HashMap::new()));
        }
        Ok(Err(error)) => {
            log::warn!("[diarize] local identity embedding failed: {error}");
            return Ok((resolve_cloud_speakers(pool, turns).await?, HashMap::new()));
        }
        Err(error) => {
            log::warn!("[diarize] local identity embedding task failed: {error}");
            return Ok((resolve_cloud_speakers(pool, turns).await?, HashMap::new()));
        }
    };

    let run_id = uuid::Uuid::new_v4().to_string();
    let resolved = match crate::learning::identity::resolve_clusters(
        pool,
        meeting_id,
        &run_id,
        turns,
        &cluster_embeddings,
    )
    .await
    {
        Ok(resolved) => resolved,
        Err(error) => {
            log::warn!("[diarize] cloud identity resolution failed: {error}");
            let _ = sqlx::query(
                "DELETE FROM speaker_clusters WHERE meeting_id=? AND diarization_run_id=?",
            )
            .bind(meeting_id)
            .bind(&run_id)
            .execute(pool)
            .await;
            return Ok((resolve_cloud_speakers(pool, turns).await?, HashMap::new()));
        }
    };

    let mut cluster_to_speaker = resolved
        .iter()
        .map(|(local_cluster, (speaker_id, _))| (*local_cluster, *speaker_id))
        .collect::<HashMap<_, _>>();
    let cluster_to_learning = resolved
        .into_iter()
        .map(|(local_cluster, (_, cluster_id))| (local_cluster, cluster_id))
        .collect::<HashMap<_, _>>();

    // Very short speakers may not yield a trustworthy embedding. They still need a local
    // label, but are deliberately excluded from cross-meeting learning.
    let mut missing_clusters = turns
        .iter()
        .map(|turn| turn.cluster_id)
        .filter(|cluster_id| !cluster_to_speaker.contains_key(cluster_id))
        .collect::<Vec<_>>();
    missing_clusters.sort_unstable();
    missing_clusters.dedup();
    let base_count = SpeakersRepository::count(pool).await?;
    for (index, cluster_id) in missing_clusters.into_iter().enumerate() {
        let name = format!("Speaker {}", base_count + 1 + index as i64);
        let speaker_id = SpeakersRepository::insert(pool, &name, &[], false).await?;
        cluster_to_speaker.insert(cluster_id, speaker_id);
    }
    Ok((cluster_to_speaker, cluster_to_learning))
}

/// Decode a recording to 16 kHz mono 16-bit LE PCM (for cloud upload). CPU-bound — call
/// under `spawn_blocking`.
fn decode_to_pcm16_16k(path: &Path) -> anyhow::Result<Vec<u8>> {
    crate::audio::decoder::decode_audio_file_to_pcm16(path).or_else(|direct_error| {
        log::warn!(
            "[diarize] direct FFmpeg PCM decode failed; falling back to the in-process decoder: {direct_error}"
        );
        let decoded = crate::audio::decoder::decode_audio_file(path)?;
        let mono: Vec<f32> = if decoded.channels > 1 {
            let channels = decoded.channels as usize;
            decoded
                .samples
                .chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            decoded.samples
        };
        let samples_16k = if decoded.sample_rate != 16_000 {
            crate::audio::audio_processing::resample_audio(&mono, decoded.sample_rate, 16_000)
        } else {
            mono
        };
        let mut pcm = Vec::with_capacity(samples_16k.len() * std::mem::size_of::<i16>());
        for sample in samples_16k {
            let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            pcm.extend_from_slice(&value.to_le_bytes());
        }
        Ok(pcm)
    })
}

async fn run_local_diarization<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    audio_path: &Path,
    expected_speakers: Option<usize>,
) -> Result<(Vec<SpeakerTurn>, HashMap<i64, i64>, HashMap<i64, i64>), DiarizeError> {
    let model_dir = resolve_model_dir(app).map_err(|e| DiarizeError::Other(anyhow!(e)))?;
    let config = DiarizerConfig { model_dir };
    if !config.is_available() {
        return Err(DiarizeError::ModelsUnavailable);
    }
    let params = DiarizationParams {
        num_speakers: expected_speakers,
        ..Default::default()
    };
    let ap = audio_path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let diarizer = Diarizer::load_with_params(config, params)?;
        diarizer.diarize(&ap)
    })
    .await
    .map_err(|e| DiarizeError::Other(anyhow!("diarize task panicked: {e}")))?
    .map_err(DiarizeError::Other)?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let resolved = crate::learning::identity::resolve_clusters(
        pool,
        meeting_id,
        &run_id,
        &result.turns,
        &result.cluster_embeddings,
    )
    .await
    .map_err(|error| DiarizeError::Other(anyhow!(error)))?;
    let cluster_to_speaker = resolved
        .iter()
        .map(|(local_cluster, (speaker_id, _))| (*local_cluster, *speaker_id))
        .collect();
    let cluster_to_learning = resolved
        .into_iter()
        .map(|(local_cluster, (_, cluster_id))| (local_cluster, cluster_id))
        .collect();
    Ok((result.turns, cluster_to_speaker, cluster_to_learning))
}

/// Everything the diarization engine learned about a meeting's voices, before any
/// transcript rows are touched: raw speaker turns plus the resolved cluster→speaker
/// maps. Produced by [`compute_speaker_turns`]; consumed by [`attribute_transcripts`]
/// (row attribution) and by the refinement pass (turn-aligned re-transcription — the
/// turns become the ASR segment boundaries so replies split per speaker).
pub struct SpeakerTurnsPlan {
    pub turns: Vec<SpeakerTurn>,
    pub cluster_to_speaker: HashMap<i64, i64>,
    pub cluster_to_learning: HashMap<i64, i64>,
}

/// Shared diarization core, called by both the `diarize_meeting` command and the `diarize`
/// job handler. Runs the engine on the meeting's recording, attributes transcript segments
/// to resolved speakers, persists cross-meeting speaker identities, and emits
/// `diarization-complete` `{ meeting_id, speaker_count, assigned_segments }` on success.
pub async fn run_diarization_core<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<DiarizeOutcome, DiarizeError> {
    let plan = compute_speaker_turns(app, pool, meeting_id).await?;
    attribute_transcripts(app, pool, meeting_id, &plan).await
}

/// Phase 1 of diarization: run the configured engine (cloud with local fallback, or
/// local) on the meeting's recording and resolve speaker identities. Persists identity
/// bookkeeping (speaker clusters, inference runs) but does not touch transcript rows.
pub async fn compute_speaker_turns<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<SpeakerTurnsPlan, DiarizeError> {
    // 1) Locate the meeting's saved recording (both engines need it).
    let meeting = MeetingsRepository::get_meeting_metadata(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?
        .ok_or_else(|| DiarizeError::Other(anyhow!("meeting not found: {meeting_id}")))?;
    let Some(folder) = meeting.folder_path.filter(|p| !p.trim().is_empty()) else {
        return Err(DiarizeError::NoRecording);
    };
    let audio_path = crate::audio::retranscription::find_audio_file(Path::new(&folder))
        .map_err(|_| DiarizeError::NoRecording)?;

    // Per-meeting expected-speaker-count hint (in-meeting control pill). Constrains
    // clustering (local) / count_of_speaker (cloud); None = automatic estimation.
    let expected_speakers: Option<usize> =
        MeetingsRepository::get_diarization_prefs(pool, meeting_id)
            .await
            .map_err(|e| DiarizeError::Other(e.into()))?
            .and_then(|(_, expected)| expected)
            .filter(|n| *n >= 1)
            .map(|n| n as usize);
    if let Some(n) = expected_speakers {
        log::info!("[diarize] meeting {meeting_id}: using expected speaker count hint = {n}");
    }

    let provider = resolve_diarization_provider();
    let cloud_processing_allowed =
        sqlx::query_scalar::<_, i64>("SELECT cloud_processing_allowed FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| DiarizeError::Other(error.into()))?
            .unwrap_or(0)
            != 0;
    let use_salutespeech = provider == "salutespeech" && cloud_processing_allowed;
    if provider == "salutespeech" && !cloud_processing_allowed {
        log::info!(
            "[diarize] meeting {meeting_id}: cloud processing is disabled; using local engine"
        );
    }

    // 2) Produce speaker turns + a cluster->speaker map from the configured engine.
    let (turns, cluster_to_speaker, cluster_to_learning): (
        Vec<SpeakerTurn>,
        HashMap<i64, i64>,
        HashMap<i64, i64>,
    ) = if use_salutespeech {
        // Cloud: SaluteSpeech async recognition with speaker separation.
        log::info!("[diarize] using SaluteSpeech cloud engine");
        let cloud_result = async {
            let cfg = crate::salutespeech::resolve_config(pool)
                .await
                .ok_or_else(|| anyhow!("SaluteSpeech Authorization Key not configured"))?;
            let ap = audio_path.clone();
            let pcm16 = tokio::task::spawn_blocking(move || decode_to_pcm16_16k(&ap))
                .await
                .map_err(|e| anyhow!("audio decode task panicked: {e}"))??;
            let cloud_turns = crate::salutespeech::diarize::diarize_pcm16(
                &cfg,
                pcm16,
                expected_speakers.map(|n| n as u32),
            )
            .await
            .map_err(|e| anyhow!(e))?;
            let turns: Vec<SpeakerTurn> = cloud_turns
                .into_iter()
                .map(|t| SpeakerTurn {
                    start_ms: t.start_ms,
                    end_ms: t.end_ms,
                    cluster_id: t.speaker_id,
                })
                .collect();
            if turns.is_empty() {
                return Err(anyhow!("SaluteSpeech returned no speaker turns"));
            }
            let (cluster_to_speaker, cluster_to_learning) =
                resolve_cloud_speakers_with_identity(app, pool, meeting_id, &audio_path, &turns)
                    .await?;
            Ok::<_, anyhow::Error>((turns, cluster_to_speaker, cluster_to_learning))
        }
        .await;

        match cloud_result {
            Ok(result) => result,
            Err(cloud_error) => {
                log::warn!(
                    "[diarize] SaluteSpeech unavailable; trying local diarization: {cloud_error}"
                );
                let _ = app.emit(
                    "diarization-fallback",
                    serde_json::json!({ "meeting_id": meeting_id }),
                );
                match run_local_diarization(
                        app,
                        pool,
                        meeting_id,
                        &audio_path,
                        expected_speakers,
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(DiarizeError::ModelsUnavailable) => return Err(DiarizeError::Other(anyhow!(
                            "SaluteSpeech is unavailable ({cloud_error}); local diarization models are not downloaded"
                        ))),
                        Err(DiarizeError::Other(local_error)) => return Err(DiarizeError::Other(anyhow!(
                            "SaluteSpeech is unavailable ({cloud_error}); local fallback failed: {local_error}"
                        ))),
                        Err(other) => return Err(other),
                    }
            }
        }
    } else {
        // Local: pyannote-style ONNX cascade (segmentation + CAM++ + clustering).
        run_local_diarization(app, pool, meeting_id, &audio_path, expected_speakers).await?
    };

    Ok(SpeakerTurnsPlan {
        turns,
        cluster_to_speaker,
        cluster_to_learning,
    })
}

/// Phase 2 of diarization: attribute the meeting's transcript rows to the plan's
/// speakers and finish the run (learning provenance, orphan GC, `diarization-complete`).
pub async fn attribute_transcripts<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    plan: &SpeakerTurnsPlan,
) -> Result<DiarizeOutcome, DiarizeError> {
    let SpeakerTurnsPlan {
        turns,
        cluster_to_speaker,
        cluster_to_learning,
    } = plan;

    // 5) Attribute transcript segments. Reset first so re-runs are idempotent (only
    //    speaker_id is touched — the 'mic'/'system' channel tag is preserved).
    let segments = SpeakersRepository::list_meeting_segments(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?;
    let total_segments = segments.len() as i64;
    let assignments = assign_segments_to_speakers(&segments, turns, cluster_to_speaker);
    let had_existing_assignments = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM transcripts WHERE meeting_id = ? AND speaker_id IS NOT NULL)",
    )
    .bind(meeting_id)
    .fetch_one(pool)
    .await
    .map_err(|error| DiarizeError::Other(error.into()))?;

    // An empty/low-confidence rerun must not erase previously useful labels.
    if should_preserve_existing_assignments(
        had_existing_assignments,
        total_segments,
        assignments.len(),
    ) {
        // Cloud diarization creates provisional profiles before transcript attribution.
        // They are still unreferenced here, so remove them before preserving the old labels.
        if let Err(error) = SpeakersRepository::delete_orphaned_unconfirmed(pool).await {
            log::warn!(
                "[diarize] provisional-speaker cleanup failed while preserving labels: {error}"
            );
        }
        return Err(DiarizeError::Other(anyhow!(
            "Speaker detection produced no confident transcript assignments; existing labels were preserved"
        )));
    }

    SpeakersRepository::clear_meeting_speaker_ids(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?;
    for (segment_id, speaker_id) in &assignments {
        SpeakersRepository::set_segment_speaker(pool, segment_id, *speaker_id)
            .await
            .map_err(|e| DiarizeError::Other(e.into()))?;
    }
    for (segment_id, start_s, end_s) in &segments {
        let (Some(start_s), Some(end_s)) = (start_s, end_s) else {
            continue;
        };
        let Some(local_cluster) = assign_segment(
            secs_to_ms(*start_s),
            secs_to_ms(*end_s),
            turns,
            MIN_OVERLAP_RATIO,
        ) else {
            continue;
        };
        let Some(cluster_id) = cluster_to_learning.get(&local_cluster) else {
            continue;
        };
        if let Err(error) = crate::learning::identity::link_cluster_segment(
            pool,
            *cluster_id,
            segment_id,
            cluster_overlap_ratio(
                secs_to_ms(*start_s),
                secs_to_ms(*end_s),
                local_cluster,
                turns,
            ),
        )
        .await
        {
            // Speaker attribution has already succeeded. Provenance is valuable for
            // learning, but a bookkeeping failure must not make the whole run look failed.
            log::warn!("Could not persist learning provenance for segment {segment_id}: {error}");
        }
    }

    let assigned_segments = assignments.len() as i64;
    let speaker_count = assignments
        .iter()
        .map(|(_, s)| *s)
        .collect::<HashSet<i64>>()
        .len() as i64;

    // 5b) Garbage-collect orphaned auto-created profiles. Re-runs clear this meeting's
    //     speaker_ids and may resolve clusters to new profiles, stranding previous
    //     "Speaker N" rows; without GC they accumulate across re-runs and pollute future
    //     profile matching. Never touches confirmed (user-renamed) speakers. Best-effort:
    //     a GC failure must not fail the diarization run.
    match SpeakersRepository::delete_orphaned_unconfirmed(pool).await {
        Ok(0) => {}
        Ok(n) => log::info!("[diarize] GC removed {n} orphaned unconfirmed speaker profile(s)"),
        Err(e) => log::warn!("[diarize] orphaned-speaker GC failed (non-fatal): {e}"),
    }

    // Names are a second, text-based pass over the diarized transcript. Apply only
    // unambiguous provisional mappings; a failure must not invalidate voice separation.
    match crate::pipeline::speaker_names::infer_and_apply_names(pool, meeting_id).await {
        Ok(0) => {}
        Ok(count) => {
            log::info!("[diarize] meeting {meeting_id}: provisionally named {count} speaker(s)")
        }
        Err(error) => {
            log::warn!("[diarize] meeting {meeting_id}: automatic speaker naming failed: {error}")
        }
    }

    // Whatever the local pass could not name, the model reads the conversation for. Queued
    // rather than awaited: voice separation is done and the user should see it now, and a
    // model call has its own failure modes that must not reflect on this run.
    if let Err(error) = crate::jobs::enqueue_speaker_naming(pool, meeting_id).await {
        log::warn!("[diarize] meeting {meeting_id}: could not queue speaker naming: {error}");
    }

    // 6) Notify the UI. snake_case field names, exactly as the frontend expects.
    let _ = app.emit(
        "diarization-complete",
        serde_json::json!({
            "meeting_id": meeting_id,
            "speaker_count": speaker_count,
            "assigned_segments": assigned_segments,
        }),
    );

    log::info!(
        "[diarize] meeting {meeting_id}: {speaker_count} speaker(s), \
         {assigned_segments}/{total_segments} segment(s) attributed"
    );
    Ok(DiarizeOutcome {
        speaker_count,
        assigned_segments,
        total_segments,
    })
}

/// Result of the `diarize_meeting` command (superset of the `diarization-complete` payload).
#[derive(serde::Serialize)]
pub struct DiarizeMeetingResult {
    pub meeting_id: String,
    pub speaker_count: i64,
    pub assigned_segments: i64,
    pub total_segments: i64,
}

/// Run diarization for a single meeting on demand (also emits `diarization-complete`).
#[tauri::command]
pub async fn diarize_meeting<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<DiarizeMeetingResult, String> {
    let pool = state.db_manager.pool();
    match run_diarization_core(&app, pool, &meeting_id).await {
        Ok(o) => Ok(DiarizeMeetingResult {
            meeting_id,
            speaker_count: o.speaker_count,
            assigned_segments: o.assigned_segments,
            total_segments: o.total_segments,
        }),
        Err(DiarizeError::ModelsUnavailable) => {
            Err("Diarization models not downloaded".to_string())
        }
        Err(DiarizeError::NoRecording) => Err("No recording found for this meeting".to_string()),
        Err(DiarizeError::Other(e)) => Err(format!("Diarization failed: {e}")),
    }
}

/// Speakers referenced by a meeting's transcripts, with per-meeting segment counts.
#[tauri::command]
pub async fn get_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<crate::database::repositories::speaker::MeetingSpeaker>, String> {
    SpeakersRepository::meeting_speakers(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| format!("Failed to load meeting speakers: {e}"))
}

/// Sanity ceiling for the expected-speaker hint; matches the pill's stepper range.
const MAX_EXPECTED_SPEAKERS: i64 = 32;

/// Store the in-meeting control pill's choices on the saved meeting row. `enabled = false`
/// skips the automatic post-meeting diarize job (the manual "Detect speakers" button still
/// runs); `expected_speakers` constrains clustering in both engines. Nulls reset to defaults.
#[tauri::command]
pub async fn set_meeting_diarization_prefs(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    enabled: Option<bool>,
    expected_speakers: Option<i64>,
) -> Result<(), String> {
    if let Some(n) = expected_speakers {
        if !(1..=MAX_EXPECTED_SPEAKERS).contains(&n) {
            return Err(format!(
                "expected_speakers must be between 1 and {MAX_EXPECTED_SPEAKERS}"
            ));
        }
    }
    let updated = MeetingsRepository::set_diarization_prefs(
        state.db_manager.pool(),
        &meeting_id,
        enabled,
        expected_speakers,
    )
    .await
    .map_err(|e| format!("Failed to save diarization preferences: {e}"))?;
    if !updated {
        return Err(format!("Meeting {meeting_id} not found"));
    }
    Ok(())
}

/// Rename a speaker profile and mark it confirmed. Rejects empty/whitespace names.
///
/// This command means "a person typed this name", and voice learning depends on that:
/// its sole caller is the rename control in the transcript UI. Code that applies a name
/// the app worked out for itself must go through the provisional path instead (see
/// [`crate::pipeline::speaker_naming`]), which leaves `is_confirmed` at 0 and teaches
/// nothing. A future batch-rename or import flow must do the same.
#[tauri::command]
pub async fn rename_speaker(
    state: tauri::State<'_, AppState>,
    speaker_id: i64,
    display_name: String,
) -> Result<(), String> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err("Speaker name cannot be empty".to_string());
    }
    let pool = state.db_manager.pool();
    let affected = SpeakersRepository::rename(pool, speaker_id, name)
        .await
        .map_err(|e| format!("Failed to rename speaker: {e}"))?;
    if affected == 0 {
        return Err(format!("Speaker {speaker_id} not found"));
    }
    // Putting a name to a voice is the user's own assertion, so it is also the moment the
    // app may remember that voice. A failure here must not undo the rename the user asked
    // for — the label is the point, the voiceprint is the bonus.
    match crate::learning::identity::learn_named_speaker(pool, speaker_id, name).await {
        Ok(0) => {}
        Ok(clusters) => log::info!(
            "[speakers] learned speaker {speaker_id} from {clusters} confirmed cluster(s)"
        ),
        Err(error) => log::warn!("[speakers] could not learn the named voice: {error}"),
    }
    Ok(())
}

/// Persist whether a diarized voice profile belongs to the local user.
/// The repository clears any previous owner assignment in the same transaction.
#[tauri::command]
pub async fn set_self_speaker(
    state: tauri::State<'_, AppState>,
    speaker_id: i64,
    is_self: bool,
) -> Result<(), String> {
    let affected = SpeakersRepository::set_self(state.db_manager.pool(), speaker_id, is_self)
        .await
        .map_err(|e| format!("Failed to update speaker identity: {e}"))?;
    if affected == 0 {
        return Err(format!("Speaker {speaker_id} not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start_ms: i64, end_ms: i64, cluster_id: i64) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            cluster_id,
        }
    }

    #[test]
    fn secs_to_ms_rounds() {
        assert_eq!(secs_to_ms(0.0), 0);
        assert_eq!(secs_to_ms(1.5), 1500);
        assert_eq!(secs_to_ms(2.3006), 2301); // rounds to nearest ms
        assert_eq!(secs_to_ms(0.0004), 0);
    }

    #[test]
    fn cluster_provenance_records_actual_overlap() {
        let turns = vec![turn(0, 600, 0), turn(600, 1000, 1)];
        assert!((cluster_overlap_ratio(0, 1000, 0, &turns) - 0.6).abs() < f64::EPSILON);
        assert!((cluster_overlap_ratio(200, 700, 0, &turns) - 0.8).abs() < f64::EPSILON);
        assert_eq!(cluster_overlap_ratio(0, 1000, 2, &turns), 0.0);
    }

    #[test]
    fn assigns_segments_via_seconds_to_ms_and_cluster_map() {
        // Two turns in ms: cluster 0 owns [0,2000], cluster 1 owns [2000,4000].
        let turns = vec![turn(0, 2000, 0), turn(2000, 4000, 1)];
        // cluster 0 -> speaker 10, cluster 1 -> speaker 20.
        let mut map = HashMap::new();
        map.insert(0, 10);
        map.insert(1, 20);

        let segments = vec![
            // 0.0–1.8s fully inside cluster 0 -> speaker 10.
            ("a".to_string(), Some(0.0), Some(1.8)),
            // 2.1–3.9s fully inside cluster 1 -> speaker 20.
            ("b".to_string(), Some(2.1), Some(3.9)),
            // NULL timing -> unassigned.
            ("c".to_string(), None, Some(1.0)),
        ];

        let out = assign_segments_to_speakers(&segments, &turns, &map);
        assert_eq!(out, vec![("a".to_string(), 10), ("b".to_string(), 20)]);
    }

    #[test]
    fn ambiguous_and_unmapped_segments_stay_unassigned() {
        // Segment spans both clusters ~50/50 -> below MIN_OVERLAP_RATIO -> dropped.
        let turns = vec![turn(0, 1000, 0), turn(1000, 2000, 1)];
        let mut map = HashMap::new();
        map.insert(0, 10);
        map.insert(1, 20);
        let split = vec![("x".to_string(), Some(0.0), Some(2.0))];
        assert!(assign_segments_to_speakers(&split, &turns, &map).is_empty());

        // Confident overlap but the cluster has no speaker mapping -> dropped.
        let empty_map = HashMap::new();
        let clear = vec![("y".to_string(), Some(0.0), Some(0.9))];
        assert!(assign_segments_to_speakers(&clear, &turns, &empty_map).is_empty());
    }

    #[test]
    fn empty_first_run_succeeds_but_empty_rerun_preserves_existing_labels() {
        assert!(!should_preserve_existing_assignments(false, 3, 0));
        assert!(should_preserve_existing_assignments(true, 3, 0));
        assert!(!should_preserve_existing_assignments(true, 3, 1));
        assert!(!should_preserve_existing_assignments(true, 0, 0));
    }

    // Research harness: run SaluteSpeech CLOUD diarization on a real meeting WAV using
    // the config resolved from a real app database (managed gateway or stored key), and
    // write the returned turns as CSV for offline scoring against a ground-truth
    // timeline — comparable 1:1 with `research_diarize_wav` (the local engine). Env:
    //   MEETILY_DB_PATH=<meeting_minutes.sqlite>  MEETILY_DIARIZATION_TEST_WAV=<wav>
    //   RESEARCH_OUT=<turns.csv>  RESEARCH_NUM_SPEAKERS=<optional hint>
    //   cargo test -p meetily --lib pipeline::diarization_commands::tests::research_salutespeech_diarize -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn research_salutespeech_diarize() {
        let db = match std::env::var("MEETILY_DB_PATH") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skip: MEETILY_DB_PATH not set");
                return;
            }
        };
        let wav = match std::env::var("MEETILY_DIARIZATION_TEST_WAV") {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_TEST_WAV not set");
                return;
            }
        };
        let out = std::env::var("RESEARCH_OUT").unwrap_or_else(|_| "salute_turns.csv".into());
        let hint = std::env::var("RESEARCH_NUM_SPEAKERS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{db}?mode=ro"))
            .await
            .expect("open app db read-only");
        let Some(cfg) = crate::salutespeech::resolve_config(&pool).await else {
            eprintln!("skip: SaluteSpeech config not resolvable (privacy/local_only or no key)");
            return;
        };

        let wav_path = std::path::PathBuf::from(&wav);
        let pcm16 = tokio::task::spawn_blocking(move || decode_to_pcm16_16k(&wav_path))
            .await
            .unwrap()
            .expect("decode to pcm16");
        eprintln!(
            "uploading {:.1} MB pcm16 ({:.1}s audio), hint={hint:?}",
            pcm16.len() as f64 / 1e6,
            pcm16.len() as f64 / 32_000.0
        );

        let started = std::time::Instant::now();
        let turns = crate::salutespeech::diarize::diarize_pcm16(&cfg, pcm16, hint)
            .await
            .expect("cloud diarization");
        eprintln!(
            "cloud diarize: {} turns, {} distinct speakers, {:.0}s wall",
            turns.len(),
            turns
                .iter()
                .map(|t| t.speaker_id)
                .collect::<HashSet<_>>()
                .len(),
            started.elapsed().as_secs_f64()
        );

        let mut csv = String::from("start_ms,end_ms,cluster_id\n");
        for t in &turns {
            csv.push_str(&format!("{},{},{}\n", t.start_ms, t.end_ms, t.speaker_id));
        }
        std::fs::write(&out, csv).unwrap();
        eprintln!("wrote {out}");
    }
}
