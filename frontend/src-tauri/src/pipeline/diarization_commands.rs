//! Diarization model management (PLAN.md Phase 2): download / status of the pyannote-style
//! cascade. Files live in `app_data_dir/models/diarization/`, a sibling of the gigaam /
//! embedding model dirs. Two ONNX files are needed:
//!   * `segmentation.onnx` — pyannote segmentation-3.0 (~5.9 MB, MIT)
//!   * `embedding.onnx`     — WeSpeaker CAM++ VoxCeleb speaker embeddings (~29 MB)
//!
//! Both are hosted as direct `.onnx` release assets by `thewh1teagle/pyannote-rs` (the
//! reference implementation this cascade is cribbed from). Everything degrades gracefully:
//! until both files are present, `diarization_status.available` is false and the diarize
//! job leaves segments unattributed.

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
    assign_segment, fold_embedding, match_speaker, DiarizationParams, Diarizer, DiarizerConfig,
    SpeakerTurn, DEFAULT_SPEAKER_TAU, EMBEDDING_FILE, MIN_OVERLAP_RATIO, SEGMENTATION_FILE,
};
use crate::state::AppState;

/// Verified 200-OK direct-download URLs (checked 2026-07: 5,983,836 B / 29,292,684 B).
/// `CAM%2B%2B` is the URL-encoded `CAM++`.
const SEGMENTATION_URL: &str =
    "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0/segmentation-3.0.onnx";
const EMBEDDING_URL: &str =
    "https://github.com/thewh1teagle/pyannote-rs/releases/download/v0.1.0/wespeaker_en_voxceleb_CAM%2B%2B.onnx";

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
                    DownloadProgress { file: label.to_string(), downloaded, total, percent: pct },
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
pub async fn diarization_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = diarization_model_dir(&app)?;
    Ok(serde_json::json!({
        "available": models_present(&dir),
        "model_dir": dir.to_string_lossy(),
        "segmentation_present": dir.join(SEGMENTATION_FILE).exists(),
        "embedding_present": dir.join(EMBEDDING_FILE).exists(),
    }))
}

/// Download both diarization models (segmentation first, then the larger embedding model).
/// Emits `diarization-download-progress` while downloading and `diarization-ready` on success.
/// Unlike the transcription models there is no global instance to load here — the diarize
/// job constructs a [`crate::pipeline::diarization::Diarizer`] on demand from `model_dir`.
#[tauri::command]
pub async fn download_diarization_models<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let dir = diarization_model_dir(&app)?;
    download_file(&app, SEGMENTATION_URL, &dir.join(SEGMENTATION_FILE), SEGMENTATION_FILE).await?;
    download_file(&app, EMBEDDING_URL, &dir.join(EMBEDDING_FILE), EMBEDDING_FILE).await?;

    let _ = app.emit("diarization-ready", ());
    log::info!("diarization models downloaded to {}", dir.display());
    Ok(())
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

/// Resolve each diarized cluster to a persisted speaker profile (cross-meeting identity),
/// returning a `cluster_id -> speakers.id` map for transcript attribution.
///
/// For each cluster mean embedding: cosine-match against known profiles (≥
/// [`DEFAULT_SPEAKER_TAU`]) → fold the observation into the matched profile; else insert a
/// new unconfirmed `Speaker N` (N = speaker count captured at run start + running index).
/// NOTE on folding: `speakers` has no per-profile sample-count column (no migration is
/// permitted for this stage), so [`fold_embedding`] is called with `count = 1` — an
/// equal-weight blend of the stored profile and the new observation.
async fn resolve_cluster_speakers(
    pool: &SqlitePool,
    cluster_embeddings: &[(i64, Vec<f32>)],
) -> anyhow::Result<HashMap<i64, i64>> {
    let mut known = SpeakersRepository::list_with_embeddings(pool).await?;
    let base_count = SpeakersRepository::count(pool).await?;

    // Dimension guard: known profiles whose dim differs from these embeddings can never
    // match (cosine_similarity returns 0 for mismatched lengths), so they yield new
    // speakers. Warn once rather than silently.
    if let Some((_, sample)) = cluster_embeddings.first() {
        let dim = sample.len();
        let mismatched = known.iter().filter(|(_, e)| e.len() != dim).count();
        if mismatched > 0 {
            log::warn!(
                "[diarize] {mismatched} existing speaker profile(s) have an embedding dim != {dim}; \
                 they will not match and new speakers may be created"
            );
        }
    }

    let mut map = HashMap::new();
    let mut new_index: i64 = 0;
    for (cluster_id, emb) in cluster_embeddings {
        match match_speaker(emb, &known, DEFAULT_SPEAKER_TAU) {
            Some(speaker_id) => {
                if let Some(slot) = known.iter_mut().find(|(id, _)| *id == speaker_id) {
                    let folded = fold_embedding(&slot.1, 1, emb);
                    SpeakersRepository::update_embedding(pool, speaker_id, &folded).await?;
                    slot.1 = folded; // keep in-memory profiles current for later clusters
                }
                map.insert(*cluster_id, speaker_id);
            }
            None => {
                new_index += 1;
                let name = format!("Speaker {}", base_count + new_index);
                let id = SpeakersRepository::insert(pool, &name, emb, false).await?;
                known.push((id, emb.clone())); // distinct later clusters from this one
                map.insert(*cluster_id, id);
            }
        }
    }
    Ok(map)
}

/// Diarization engine selection: `"salutespeech"` (Sber cloud, default) or `"local"`.
/// Reads `app_settings_kv.diarization.provider`; `MEETILY_DIARIZATION_PROVIDER` overrides
/// it (headless runs / tests).
async fn resolve_diarization_provider(pool: &SqlitePool) -> String {
    if let Ok(v) = std::env::var("MEETILY_DIARIZATION_PROVIDER") {
        if !v.trim().is_empty() {
            return v.trim().to_string();
        }
    }
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key = 'diarization.provider'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "salutespeech".to_string())
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

/// Decode a recording to 16 kHz mono 16-bit LE PCM (for cloud upload). CPU-bound — call
/// under `spawn_blocking`.
fn decode_to_pcm16_16k(path: &Path) -> anyhow::Result<Vec<u8>> {
    let decoded = crate::audio::decoder::decode_audio_file(path)?;
    let mono: Vec<f32> = if decoded.channels > 1 {
        let ch = decoded.channels as usize;
        decoded.samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()
    } else {
        decoded.samples
    };
    let s16k = if decoded.sample_rate != 16_000 {
        crate::audio::audio_processing::resample_audio(&mono, decoded.sample_rate, 16_000)
    } else {
        mono
    };
    let mut pcm = Vec::with_capacity(s16k.len() * 2);
    for s in s16k {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    Ok(pcm)
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
    let expected_speakers: Option<usize> = MeetingsRepository::get_diarization_prefs(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?
        .and_then(|(_, expected)| expected)
        .filter(|n| *n >= 1)
        .map(|n| n as usize);
    if let Some(n) = expected_speakers {
        log::info!("[diarize] meeting {meeting_id}: using expected speaker count hint = {n}");
    }

    // 2) Produce speaker turns + a cluster->speaker map from the configured engine.
    let (turns, cluster_to_speaker): (Vec<SpeakerTurn>, HashMap<i64, i64>) =
        if resolve_diarization_provider(pool).await == "salutespeech" {
            // Cloud: SaluteSpeech async recognition with speaker separation.
            log::info!("[diarize] using SaluteSpeech cloud engine");
            let cfg = crate::salutespeech::resolve_config(pool).await.ok_or_else(|| {
                DiarizeError::Other(anyhow!(
                    "SaluteSpeech Authorization Key not configured (Settings → Transcription)"
                ))
            })?;
            let ap = audio_path.clone();
            let pcm16 = tokio::task::spawn_blocking(move || decode_to_pcm16_16k(&ap))
                .await
                .map_err(|e| DiarizeError::Other(anyhow!("audio decode task panicked: {e}")))?
                .map_err(DiarizeError::Other)?;
            let cloud_turns = crate::salutespeech::diarize::diarize_pcm16(
                &cfg,
                pcm16,
                expected_speakers.map(|n| n as u32),
            )
            .await
            .map_err(|e| DiarizeError::Other(anyhow!(e)))?;
            let turns: Vec<SpeakerTurn> = cloud_turns
                .into_iter()
                .map(|t| SpeakerTurn {
                    start_ms: t.start_ms,
                    end_ms: t.end_ms,
                    cluster_id: t.speaker_id,
                })
                .collect();
            let cts = resolve_cloud_speakers(pool, &turns).await.map_err(DiarizeError::Other)?;
            (turns, cts)
        } else {
            // Local: pyannote-style ONNX cascade (segmentation + CAM++ + clustering).
            let model_dir = resolve_model_dir(app).map_err(|e| DiarizeError::Other(anyhow!(e)))?;
            let config = DiarizerConfig { model_dir };
            if !config.is_available() {
                return Err(DiarizeError::ModelsUnavailable);
            }
            let params = DiarizationParams { num_speakers: expected_speakers, ..Default::default() };
            let ap = audio_path.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                let diarizer = Diarizer::load_with_params(config, params)?;
                diarizer.diarize(&ap)
            })
            .await
            .map_err(|e| DiarizeError::Other(anyhow!("diarize task panicked: {e}")))?
            .map_err(DiarizeError::Other)?;
            let cts = resolve_cluster_speakers(pool, &result.cluster_embeddings)
                .await
                .map_err(DiarizeError::Other)?;
            (result.turns, cts)
        };

    // 5) Attribute transcript segments. Reset first so re-runs are idempotent (only
    //    speaker_id is touched — the 'mic'/'system' channel tag is preserved).
    let segments = SpeakersRepository::list_meeting_segments(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?;
    let total_segments = segments.len() as i64;
    let assignments = assign_segments_to_speakers(&segments, &turns, &cluster_to_speaker);

    SpeakersRepository::clear_meeting_speaker_ids(pool, meeting_id)
        .await
        .map_err(|e| DiarizeError::Other(e.into()))?;
    for (segment_id, speaker_id) in &assignments {
        SpeakersRepository::set_segment_speaker(pool, segment_id, *speaker_id)
            .await
            .map_err(|e| DiarizeError::Other(e.into()))?;
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
    Ok(DiarizeOutcome { speaker_count, assigned_segments, total_segments })
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
        Err(DiarizeError::ModelsUnavailable) => Err("Diarization models not downloaded".to_string()),
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
    let affected = SpeakersRepository::rename(state.db_manager.pool(), speaker_id, name)
        .await
        .map_err(|e| format!("Failed to rename speaker: {e}"))?;
    if affected == 0 {
        return Err(format!("Speaker {speaker_id} not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start_ms: i64, end_ms: i64, cluster_id: i64) -> SpeakerTurn {
        SpeakerTurn { start_ms, end_ms, cluster_id }
    }

    #[test]
    fn secs_to_ms_rounds() {
        assert_eq!(secs_to_ms(0.0), 0);
        assert_eq!(secs_to_ms(1.5), 1500);
        assert_eq!(secs_to_ms(2.3006), 2301); // rounds to nearest ms
        assert_eq!(secs_to_ms(0.0004), 0);
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
}
