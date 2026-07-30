//! Automatic post-meeting refinement pass.
//!
//! The live transcription path favors latency: short VAD redemption fragments speech into
//! ~13-word segments, and each fragment is transcribed without surrounding context. On a
//! reference 31-min meeting that cost measurably against the batch path (90.1% vs 92.4%
//! matched words, with numbers and word boundaries mangled at fragment edges). This pass
//! runs once per recorded meeting, right after the frontend persists the live transcript:
//!
//!   1. re-transcribe the saved recording through the batch path (2000 ms redemption +
//!      25 s silence splitting) — replaces the live transcript rows in the DB;
//!   2. diarize the recording so the fresh rows get speaker attribution;
//!   3. rewrite the recording folder's `transcripts.json` with speaker labels and
//!      turn-merged text (the shape reference transcription apps produce).
//!
//! Fire-and-forget: failures degrade to the live transcript, never block saving. The pass
//! is skipped when the `refinement.auto` app setting is "false", when the meeting has no
//! saved audio, or when a manual retranscription is already running.

use std::path::Path;

use log::{info, warn};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

/// `app_settings_kv` key for the auto-refinement toggle. Missing key = enabled.
pub const AUTO_REFINE_SETTING_KEY: &str = "refinement.auto";

/// Whether the automatic pass is enabled (default: yes; only an explicit "false" disables).
pub async fn auto_refine_enabled(pool: &SqlitePool) -> bool {
    match sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key = ?")
        .bind(AUTO_REFINE_SETTING_KEY)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(value)) => value != "false",
        Ok(None) => true,
        Err(e) => {
            warn!("[refinement] could not read {AUTO_REFINE_SETTING_KEY}: {e}; assuming enabled");
            true
        }
    }
}

/// Spawn the refinement pass for a just-saved recorded meeting. Returns immediately.
pub fn spawn_post_meeting_refinement<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    folder_path: String,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_post_meeting_refinement(&app, &meeting_id, &folder_path).await {
            warn!("[refinement] meeting {meeting_id}: pass did not complete: {e}");
            let _ = app.emit(
                "refinement-error",
                serde_json::json!({ "meeting_id": meeting_id, "error": e.to_string() }),
            );
        }
    });
}

pub(crate) async fn run_post_meeting_refinement<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    folder_path: &str,
) -> anyhow::Result<()> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow::anyhow!("app state unavailable"))?;
    let pool = state.db_manager.pool();

    let folder = Path::new(folder_path);
    if crate::audio::retranscription::find_audio_file(folder).is_err() {
        info!("[refinement] meeting {meeting_id}: no saved audio; keeping live transcript");
        return Ok(());
    }
    let duration_seconds = std::fs::read_to_string(folder.join("metadata.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|metadata| {
            metadata
                .get("duration_seconds")
                .and_then(serde_json::Value::as_f64)
        });
    if duration_seconds.is_some_and(|duration| duration < 120.0) {
        info!(
            "[refinement] meeting {meeting_id}: recording is shorter than two minutes; \
             skipping hidden-meeting speaker processing"
        );
        return Ok(());
    }

    // Speaker attribution is an independent, always-on post-meeting step. The
    // refinement toggle controls the slower batch re-transcription only; turning it
    // off must not silently disable diarization for newly recorded meetings.
    let transcript_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await?;
    if !auto_refine_enabled(pool).await && transcript_count > 0 {
        info!(
            "[refinement] meeting {meeting_id}: batch re-transcription disabled; \
             running speaker attribution only"
        );
        diarize_and_export(app, pool, meeting_id, folder).await;
        let _ = app.emit(
            "refinement-complete",
            serde_json::json!({ "meeting_id": meeting_id }),
        );
        return Ok(());
    }
    if transcript_count == 0 {
        info!(
            "[refinement] meeting {meeting_id}: saved recording has no transcript; \
             running recovery transcription before speaker attribution"
        );
    }

    // Same engine the live pass used (transcript_settings is the single active config).
    let (provider, model): (String, String) =
        sqlx::query_as("SELECT provider, model FROM transcript_settings WHERE id='1'")
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no transcription model configured"))?;

    info!("[refinement] meeting {meeting_id}: refinement with {provider}/{model}");
    let _ = app.emit(
        "refinement-started",
        serde_json::json!({ "meeting_id": meeting_id }),
    );

    // Preferred: turn-aligned pass — diarize first, transcribe per speaker turn, so
    // every row is one reply (the reference-app shape). Falls back to the silence-cut
    // batch pass when it can't run (cloud provider, diarization models absent, …).
    match turn_aligned_retranscribe(app, pool, meeting_id, folder).await {
        Ok(outcome) => {
            info!(
                "[refinement] meeting {meeting_id}: turn-aligned pass done — {} speaker(s), {}/{} row(s) attributed",
                outcome.speaker_count, outcome.assigned_segments, outcome.total_segments
            );
            if let Err(e) = write_refined_transcript_export(pool, meeting_id, folder).await {
                warn!("[refinement] meeting {meeting_id}: export rewrite failed: {e}");
            }
        }
        Err(reason) => {
            info!(
                "[refinement] meeting {meeting_id}: turn-aligned pass unavailable ({reason}); \
                 using silence-cut batch pass"
            );
            // 1) Batch re-transcription. Replaces the DB rows and writes transcripts.json;
            //    emits its own retranscription-* progress events. The in-progress guard
            //    inside makes a concurrent manual retranscription win — we just skip.
            crate::audio::retranscription::start_retranscription(
                app.clone(),
                meeting_id.to_string(),
                folder_path.to_string(),
                None,
                Some(model),
                Some(provider),
            )
            .await?;

            // 2) + 3) Speaker attribution and labeled export (shared with the import pass).
            diarize_and_export(app, pool, meeting_id, folder).await;
        }
    }

    let _ = app.emit(
        "refinement-complete",
        serde_json::json!({ "meeting_id": meeting_id }),
    );
    info!("[refinement] meeting {meeting_id}: refinement pass complete");
    Ok(())
}

/// Spawn the post-import pass. Prefers the turn-aligned re-transcription (one row per
/// speaker reply); if that can't run, keeps the import's silence-cut rows and only adds
/// speaker attribution + the labeled export. Fire-and-forget; honors the same
/// `refinement.auto` setting as the recording pass.
pub fn spawn_import_refinement<R: Runtime>(app: AppHandle<R>, meeting_id: String) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            warn!("[refinement] import {meeting_id}: app state unavailable");
            return;
        };
        let pool = state.db_manager.pool();
        let folder: Option<Option<String>> =
            sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
                .bind(&meeting_id)
                .fetch_optional(pool)
                .await
                .unwrap_or_default();
        let Some(folder) = folder.flatten().filter(|f| !f.trim().is_empty()) else {
            info!("[refinement] import {meeting_id}: no folder path; skipping speaker pass");
            return;
        };
        let folder = Path::new(&folder);

        if !auto_refine_enabled(pool).await {
            info!(
                "[refinement] import {meeting_id}: batch re-transcription disabled; \
                 running speaker attribution only"
            );
            diarize_and_export(&app, pool, &meeting_id, folder).await;
            let _ = app.emit(
                "refinement-complete",
                serde_json::json!({ "meeting_id": meeting_id }),
            );
            return;
        }

        match turn_aligned_retranscribe(&app, pool, &meeting_id, folder).await {
            Ok(outcome) => {
                info!(
                    "[refinement] import {meeting_id}: turn-aligned pass done — {} speaker(s), {}/{} row(s) attributed",
                    outcome.speaker_count, outcome.assigned_segments, outcome.total_segments
                );
                if let Err(e) = write_refined_transcript_export(pool, &meeting_id, folder).await {
                    warn!("[refinement] import {meeting_id}: export rewrite failed: {e}");
                }
            }
            Err(reason) => {
                info!(
                    "[refinement] import {meeting_id}: turn-aligned pass unavailable ({reason}); \
                     keeping imported rows, attributing speakers only"
                );
                diarize_and_export(&app, pool, &meeting_id, folder).await;
            }
        }
        let _ = app.emit(
            "refinement-complete",
            serde_json::json!({ "meeting_id": meeting_id }),
        );
        info!("[refinement] import {meeting_id}: speaker pass complete");
    });
}

/// Diarize a meeting's saved recording and rewrite its folder export with speaker
/// labels. Both stages degrade gracefully — models absent or diarization failing
/// leaves rows unattributed and the export unlabeled.
async fn diarize_and_export<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    folder: &Path,
) {
    match crate::pipeline::diarization_commands::run_diarization_core(app, pool, meeting_id)
        .await
    {
        Ok(outcome) => info!(
            "[refinement] meeting {meeting_id}: diarized — {} speaker(s), {}/{} segment(s) attributed",
            outcome.speaker_count, outcome.assigned_segments, outcome.total_segments
        ),
        Err(e) => {
            use crate::pipeline::diarization_commands::DiarizeError;
            let reason = match e {
                DiarizeError::ModelsUnavailable => "models not downloaded".to_string(),
                DiarizeError::NoRecording => "no saved recording".to_string(),
                DiarizeError::Other(err) => err.to_string(),
            };
            warn!(
                "[refinement] meeting {meeting_id}: diarization skipped/failed ({reason}); export stays unlabeled"
            );
        }
    }

    if let Err(e) = write_refined_transcript_export(pool, meeting_id, folder).await {
        warn!("[refinement] meeting {meeting_id}: export rewrite failed: {e}");
    }
}

/// Plan ASR segment boundaries from diarized speaker turns, so each transcript row is
/// one speaker's reply instead of a silence-bounded block spanning several people.
/// (Measured on the reference meeting: silence-cut 25 s blocks left 27/81 rows
/// unattributed and merged multi-speaker dialogue into single rows — the docx-style
/// reference splits exactly at speaker changes.)
///
/// Rules, in order:
/// - turns are linearized (sorted; a turn overlapping its predecessor is clipped to
///   start where the predecessor ended — overlapped speech can't be duplicated in a
///   linear transcript);
/// - adjacent same-cluster turns merge while the silence between them is at most
///   `merge_gap_ms` and the merged span stays within `max_ms`;
/// - spans shorter than `min_ms` after clipping fold into the previous same-cluster
///   span when contiguous, else are dropped (too little audio to transcribe).
///
/// Spans longer than `max_ms` (a single uninterrupted monologue turn) are returned
/// as-is; the caller splits them at silence like the batch import path does.
pub fn plan_turn_asr_segments(
    turns: &[crate::pipeline::diarization::SpeakerTurn],
    merge_gap_ms: i64,
    min_ms: i64,
    max_ms: i64,
) -> Vec<(i64, i64)> {
    let mut sorted: Vec<_> = turns.to_vec();
    sorted.sort_by_key(|t| (t.start_ms, t.end_ms));

    // (start, end, cluster)
    let mut spans: Vec<(i64, i64, i64)> = Vec::new();
    for t in &sorted {
        let prev_end = spans.last().map(|s| s.1).unwrap_or(i64::MIN);
        let start = t.start_ms.max(prev_end);
        if t.end_ms - start < min_ms {
            // Too short after clipping: extend a contiguous same-cluster predecessor.
            if let Some(last) = spans.last_mut() {
                if last.2 == t.cluster_id
                    && t.end_ms > last.1
                    && start - last.1 <= merge_gap_ms
                    && t.end_ms - last.0 <= max_ms
                {
                    last.1 = t.end_ms;
                }
            }
            continue;
        }
        if let Some(last) = spans.last_mut() {
            if last.2 == t.cluster_id
                && start - last.1 <= merge_gap_ms
                && t.end_ms - last.0 <= max_ms
            {
                last.1 = last.1.max(t.end_ms);
                continue;
            }
        }
        spans.push((start, t.end_ms, t.cluster_id));
    }
    spans.into_iter().map(|(s, e, _)| (s, e)).collect()
}

/// Turn-aligned re-transcription: diarize first, then transcribe each speaker turn as
/// its own segment, then attribute rows (trivially, since rows == turns). Produces the
/// reference-app shape: one row per reply. Errors mean "fall back to the silence-cut
/// path" — nothing has been written unless the whole pass got far enough to replace
/// rows atomically.
async fn turn_aligned_retranscribe<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
    folder: &Path,
) -> anyhow::Result<crate::pipeline::diarization_commands::DiarizeOutcome> {
    use crate::pipeline::diarization_commands::{compute_speaker_turns, DiarizeError};

    // Serialize with manual retranscription — both replace this meeting's rows.
    let _guard = crate::audio::retranscription::RetranscriptionGuard::acquire()
        .map_err(|e| anyhow::anyhow!(e))?;

    let (provider, _model): (String, String) =
        sqlx::query_as("SELECT provider, model FROM transcript_settings WHERE id='1'")
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no transcription model configured"))?;
    // Cloud SaluteSpeech per-turn requests would mean ~100 sequential round-trips; the
    // silence-cut fallback handles that provider until a batched path exists.
    if provider != "gigaam" {
        anyhow::bail!("turn-aligned pass supports the gigaam provider (configured: {provider})");
    }
    if !crate::gigaam_engine::is_loaded() {
        anyhow::bail!("GigaAM model is not loaded");
    }

    // 1) Diarize (identity resolution included; transcript rows untouched).
    let plan = match compute_speaker_turns(app, pool, meeting_id).await {
        Ok(plan) => plan,
        Err(DiarizeError::ModelsUnavailable) => {
            anyhow::bail!("diarization models not downloaded")
        }
        Err(DiarizeError::NoRecording) => anyhow::bail!("no saved recording"),
        Err(DiarizeError::Other(e)) => return Err(e),
    };
    if plan.turns.is_empty() {
        anyhow::bail!("diarization produced no speaker turns");
    }

    // 2) Decode the recording once.
    let audio_path = crate::audio::retranscription::find_audio_file(folder)?;
    let decode_path = audio_path.clone();
    let decoded =
        tokio::task::spawn_blocking(move || crate::audio::decoder::decode_audio_file(&decode_path))
            .await??;
    let samples = tokio::task::spawn_blocking(move || decoded.to_whisper_format()).await?;
    let total = samples.len();

    // 3) Cut the audio at speaker-turn boundaries and transcribe each span.
    const SAMPLE_RATE: f64 = 16_000.0;
    const MERGE_GAP_MS: i64 = 1_000;
    const MIN_SPAN_MS: i64 = 350;
    const MAX_SPAN_MS: i64 = 25_000;
    const MAX_SEGMENT_SAMPLES: usize = 25 * 16_000;

    let spans = plan_turn_asr_segments(&plan.turns, MERGE_GAP_MS, MIN_SPAN_MS, MAX_SPAN_MS);
    info!(
        "[refinement] meeting {meeting_id}: transcribing {} speaker-turn segments",
        spans.len()
    );

    let mut transcripts: Vec<(String, f64, f64)> = Vec::new(); // (text, start_ms, end_ms)
    for (start_ms, end_ms) in spans {
        let s = ((start_ms as f64 / 1000.0) * SAMPLE_RATE) as usize;
        let e = (((end_ms as f64 / 1000.0) * SAMPLE_RATE) as usize).min(total);
        if e <= s || e - s < 1_600 {
            continue; // under 100 ms of audio
        }
        let piece = crate::audio::vad::SpeechSegment {
            samples: samples[s..e].to_vec(),
            start_timestamp_ms: start_ms as f64,
            end_timestamp_ms: end_ms as f64,
            confidence: 0.9,
        };
        // Monologue turns longer than the engine's comfortable window split at silence,
        // same as the import path (the RNN-T encoder fails outright on very long input).
        for sub in crate::audio::common::split_segment_at_silence(&piece, MAX_SEGMENT_SAMPLES) {
            if sub.samples.len() < 1_600 {
                continue;
            }
            match crate::gigaam_engine::transcribe(sub.samples.clone()).await {
                Some(Ok(text)) if !text.trim().is_empty() => {
                    transcripts.push((text, sub.start_timestamp_ms, sub.end_timestamp_ms));
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => warn!(
                    "[refinement] meeting {meeting_id}: turn at {:.1}s failed to transcribe: {e}",
                    sub.start_timestamp_ms / 1000.0
                ),
                None => anyhow::bail!("GigaAM model unloaded mid-pass"),
            }
        }
    }
    if transcripts.is_empty() {
        anyhow::bail!("turn-aligned transcription produced no text");
    }

    // 4) Replace the meeting's rows atomically.
    let segments = crate::audio::common::create_transcript_segments(&transcripts);
    let mut conn = pool.acquire().await?;
    let mut tx = sqlx::Connection::begin(&mut *conn).await?;
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    for segment in &segments {
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
        .await?;
    }
    tx.commit().await?;
    info!(
        "[refinement] meeting {meeting_id}: replaced transcript with {} turn-aligned rows",
        segments.len()
    );

    // 5) Attribute rows to speakers — rows were cut from the very turns being assigned,
    //    so overlap is essentially total and every row gets its speaker.
    crate::pipeline::diarization_commands::attribute_transcripts(app, pool, meeting_id, &plan)
        .await
        .map_err(|e| match e {
            crate::pipeline::diarization_commands::DiarizeError::Other(err) => err,
            crate::pipeline::diarization_commands::DiarizeError::ModelsUnavailable => {
                anyhow::anyhow!("attribution failed: models unavailable")
            }
            crate::pipeline::diarization_commands::DiarizeError::NoRecording => {
                anyhow::anyhow!("attribution failed: no recording")
            }
        })
}

/// One transcript row as needed for turn assembly.
#[derive(Debug, Clone)]
pub struct ExportRow {
    pub text: String,
    pub start_s: Option<f64>,
    pub end_s: Option<f64>,
    pub speaker_id: Option<i64>,
    pub speaker_name: Option<String>,
}

/// A merged speaker turn for the export: consecutive same-speaker rows joined.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportTurn {
    pub speaker: Option<String>,
    pub start_s: Option<f64>,
    pub end_s: Option<f64>,
    pub text: String,
}

/// Merge consecutive rows by the same speaker into turns, the shape a human transcript
/// reads in (reference apps emit ~30-word speaker turns, not 13-word fragments).
///
/// Merge continues while the speaker profile id matches (two unattributed rows also
/// merge), the silence between rows stays under `max_gap_s`, and the merged turn stays
/// under `max_turn_s` — an unbounded merge would collapse a monologue-heavy meeting into
/// one giant block.
pub fn merge_rows_into_turns(
    rows: &[ExportRow],
    max_gap_s: f64,
    max_turn_s: f64,
) -> Vec<ExportTurn> {
    let mut turns: Vec<ExportTurn> = Vec::new();
    for row in rows {
        let can_merge = turns.last().is_some_and(|turn: &ExportTurn| {
            let same_speaker = turn.speaker == row.speaker_name
                && (turn.speaker.is_some() || row.speaker_id.is_none() && turn.speaker.is_none());
            let gap_ok = match (turn.end_s, row.start_s) {
                (Some(end), Some(start)) => start - end <= max_gap_s,
                _ => true,
            };
            let span_ok = match (turn.start_s, row.end_s) {
                (Some(start), Some(end)) => end - start <= max_turn_s,
                _ => true,
            };
            same_speaker && gap_ok && span_ok
        });

        if can_merge {
            let turn = turns.last_mut().expect("checked by can_merge");
            if !turn.text.is_empty() && !row.text.is_empty() {
                turn.text.push(' ');
            }
            turn.text.push_str(row.text.trim());
            turn.end_s = row.end_s.or(turn.end_s);
        } else {
            turns.push(ExportTurn {
                speaker: row.speaker_name.clone(),
                start_s: row.start_s,
                end_s: row.end_s,
                text: row.text.trim().to_string(),
            });
        }
    }
    turns
}

/// Rewrite the recording folder's `transcripts.json` with per-segment speaker labels plus
/// a turn-merged view. Additive on the v1 shape: `segments` keeps every field the live
/// writer produces (with a new optional `speaker`), `turns` is new.
pub async fn write_refined_transcript_export(
    pool: &SqlitePool,
    meeting_id: &str,
    folder: &Path,
) -> anyhow::Result<()> {
    let rows: Vec<(
        String,
        Option<f64>,
        Option<f64>,
        Option<i64>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT t.transcript, t.audio_start_time, t.audio_end_time, t.speaker_id, \
                    s.display_name \
             FROM transcripts t \
             LEFT JOIN speakers s ON s.id = t.speaker_id \
             WHERE t.meeting_id = ? \
             ORDER BY t.audio_start_time ASC, t.rowid ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        anyhow::bail!("no transcript rows for meeting {meeting_id}");
    }

    let export_rows: Vec<ExportRow> = rows
        .into_iter()
        .map(
            |(text, start_s, end_s, speaker_id, speaker_name)| ExportRow {
                text,
                start_s,
                end_s,
                speaker_id,
                speaker_name: speaker_name.filter(|n| !n.is_empty()),
            },
        )
        .collect();

    const MAX_TURN_GAP_S: f64 = 10.0;
    const MAX_TURN_SPAN_S: f64 = 120.0;
    let turns = merge_rows_into_turns(&export_rows, MAX_TURN_GAP_S, MAX_TURN_SPAN_S);

    let segments: Vec<serde_json::Value> = export_rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            serde_json::json!({
                "id": format!("seg_{i}"),
                "sequence_id": i,
                "text": r.text,
                "audio_start_time": r.start_s,
                "audio_end_time": r.end_s,
                "duration": match (r.start_s, r.end_s) {
                    (Some(s), Some(e)) => Some(e - s),
                    _ => None,
                },
                "speaker": r.speaker_name,
            })
        })
        .collect();

    let turn_count = turns.len();
    let json = serde_json::json!({
        "version": "2.0",
        "last_updated": chrono::Utc::now().to_rfc3339(),
        "total_segments": segments.len(),
        "segments": segments,
        "turns": turns,
    });

    let transcript_path = folder.join("transcripts.json");
    let temp_path = folder.join(".transcripts.json.tmp");
    std::fs::write(&temp_path, serde_json::to_string_pretty(&json)?)?;
    std::fs::rename(&temp_path, &transcript_path)?;
    info!(
        "[refinement] wrote speaker-labeled transcripts.json ({} segments, {} turns) to {}",
        export_rows.len(),
        turn_count,
        transcript_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::diarization::SpeakerTurn;

    fn turn(start_ms: i64, end_ms: i64, cluster_id: i64) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            cluster_id,
        }
    }

    #[test]
    fn planner_keeps_speaker_changes_as_separate_segments() {
        // A dialogue: three turns, alternating speakers, small gaps. Each reply stays
        // its own ASR segment — this is the whole point of turn-aligned segmentation.
        let turns = vec![
            turn(0, 8_000, 0),
            turn(8_200, 15_000, 1),
            turn(15_300, 20_000, 0),
        ];
        let spans = plan_turn_asr_segments(&turns, 1_000, 350, 25_000);
        assert_eq!(spans, vec![(0, 8_000), (8_200, 15_000), (15_300, 20_000)]);
    }

    #[test]
    fn planner_merges_same_speaker_turns_across_short_pauses() {
        let turns = vec![
            turn(0, 5_000, 0),
            turn(5_500, 9_000, 0),
            turn(9_100, 12_000, 0),
        ];
        let spans = plan_turn_asr_segments(&turns, 1_000, 350, 25_000);
        assert_eq!(spans, vec![(0, 12_000)]);
    }

    #[test]
    fn planner_respects_max_span_when_merging() {
        // Same speaker, but merging would exceed the cap → second span starts fresh.
        let turns = vec![turn(0, 20_000, 0), turn(20_500, 40_000, 0)];
        let spans = plan_turn_asr_segments(&turns, 1_000, 350, 25_000);
        assert_eq!(spans, vec![(0, 20_000), (20_500, 40_000)]);
    }

    #[test]
    fn planner_linearizes_overlapping_turns() {
        // Overlapped speech: the second turn starts before the first ends. The overlap
        // region belongs to the first span; the second is clipped, never duplicated.
        let turns = vec![turn(0, 10_000, 0), turn(8_000, 14_000, 1)];
        let spans = plan_turn_asr_segments(&turns, 1_000, 350, 25_000);
        assert_eq!(spans, vec![(0, 10_000), (10_000, 14_000)]);
    }

    #[test]
    fn planner_drops_backchannel_and_bridges_the_interrupted_turn() {
        // A short overlapped interjection ("угу") is dropped as sub-minimum, and the
        // interrupted speaker's two halves merge back into one continuous ASR span —
        // exactly how a human transcript treats a backchannel.
        let turns = vec![
            turn(0, 10_000, 0),
            turn(9_800, 10_100, 1),
            turn(10_200, 15_000, 0),
        ];
        let spans = plan_turn_asr_segments(&turns, 1_000, 350, 25_000);
        assert_eq!(spans, vec![(0, 15_000)]);
    }

    fn row(text: &str, start: f64, end: f64, speaker: Option<(i64, &str)>) -> ExportRow {
        ExportRow {
            text: text.to_string(),
            start_s: Some(start),
            end_s: Some(end),
            speaker_id: speaker.map(|(id, _)| id),
            speaker_name: speaker.map(|(_, name)| name.to_string()),
        }
    }

    #[test]
    fn merges_consecutive_same_speaker_rows() {
        let rows = vec![
            row("Первая мысль.", 0.0, 5.0, Some((1, "Анна"))),
            row("Продолжение мысли.", 6.0, 10.0, Some((1, "Анна"))),
            row("Ответ.", 11.0, 12.0, Some((2, "Борис"))),
        ];
        let turns = merge_rows_into_turns(&rows, 10.0, 120.0);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text, "Первая мысль. Продолжение мысли.");
        assert_eq!(turns[0].speaker.as_deref(), Some("Анна"));
        assert_eq!(turns[0].end_s, Some(10.0));
        assert_eq!(turns[1].speaker.as_deref(), Some("Борис"));
    }

    #[test]
    fn long_gap_starts_a_new_turn_for_the_same_speaker() {
        let rows = vec![
            row("До паузы.", 0.0, 5.0, Some((1, "Анна"))),
            row("После долгой паузы.", 30.0, 35.0, Some((1, "Анна"))),
        ];
        let turns = merge_rows_into_turns(&rows, 10.0, 120.0);
        assert_eq!(turns.len(), 2);
    }

    #[test]
    fn span_cap_prevents_monologue_collapse() {
        let rows = vec![
            row("Часть один.", 0.0, 100.0, Some((1, "Анна"))),
            row("Часть два.", 101.0, 130.0, Some((1, "Анна"))),
        ];
        let turns = merge_rows_into_turns(&rows, 10.0, 120.0);
        assert_eq!(turns.len(), 2, "merged span would exceed the cap");
    }

    #[test]
    fn unattributed_rows_merge_together_but_not_with_named_rows() {
        let rows = vec![
            row("Без спикера раз.", 0.0, 3.0, None),
            row("Без спикера два.", 4.0, 6.0, None),
            row("Именованный.", 7.0, 9.0, Some((1, "Анна"))),
        ];
        let turns = merge_rows_into_turns(&rows, 10.0, 120.0);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].speaker, None);
        assert_eq!(turns[0].text, "Без спикера раз. Без спикера два.");
    }
}
