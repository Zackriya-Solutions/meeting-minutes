use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    database::{
        models::Transcript,
        repositories::diarization::{
            diarization_status_to_database_value, DiarizationRepository, MeetingSpeaker,
        },
    },
    diarization::{
        alignment::assign_speaker_to_transcript,
        types::{DiarizationStatus, SpeakerSegment, TranscriptWindow},
    },
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationSettingsDto {
    pub enabled: bool,
    pub mode: String,
    pub show_provisional_labels: bool,
    pub post_call_refinement_enabled: bool,
    pub overlap_handling: String,
    pub speaker_review_enabled: bool,
}

#[tauri::command]
pub async fn get_diarization_settings(
    state: State<'_, AppState>,
) -> Result<DiarizationSettingsDto, String> {
    let settings = DiarizationRepository::get_settings(state.db_manager.pool())
        .await
        .map_err(|error| error.to_string())?;

    Ok(DiarizationSettingsDto {
        enabled: settings.enabled != 0,
        mode: settings.mode,
        show_provisional_labels: settings.show_provisional_labels != 0,
        post_call_refinement_enabled: settings.post_call_refinement_enabled != 0,
        overlap_handling: settings.overlap_handling,
        speaker_review_enabled: settings.speaker_review_enabled != 0,
    })
}

#[tauri::command]
pub async fn save_diarization_settings(
    state: State<'_, AppState>,
    settings: DiarizationSettingsDto,
) -> Result<(), String> {
    DiarizationRepository::save_settings(
        state.db_manager.pool(),
        settings.enabled,
        &settings.mode,
        settings.show_provisional_labels,
        settings.post_call_refinement_enabled,
        &settings.overlap_handling,
        settings.speaker_review_enabled,
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_meeting_speakers(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    if meeting_id.trim().is_empty() {
        return Err("Meeting id cannot be empty".to_string());
    }

    DiarizationRepository::get_meeting_speakers(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSpeakerRequest {
    pub meeting_id: String,
    pub speaker_id: String,
    pub label: String,
}

#[tauri::command]
pub async fn rename_speaker(
    state: State<'_, AppState>,
    request: RenameSpeakerRequest,
) -> Result<(), String> {
    let RenameSpeakerRequest {
        meeting_id,
        speaker_id,
        label,
    } = request;
    let label = normalize_speaker_label(&label)?;

    if meeting_id.trim().is_empty() {
        return Err("Meeting id cannot be empty".to_string());
    }

    if speaker_id.trim().is_empty() {
        return Err("Speaker id cannot be empty".to_string());
    }

    let updated_count = DiarizationRepository::rename_speaker(
        state.db_manager.pool(),
        &meeting_id,
        &speaker_id,
        &label,
    )
    .await
    .map_err(|error| error.to_string())?;

    if updated_count == 0 {
        return Err("Speaker not found for meeting".to_string());
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDiarizationRequest {
    pub meeting_id: String,
    pub method: String,
    pub segments: Vec<SpeakerSegment>,
}

#[tauri::command]
pub async fn apply_diarization_segments(
    state: State<'_, AppState>,
    request: ApplyDiarizationRequest,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    let ApplyDiarizationRequest {
        meeting_id,
        method,
        segments,
    } = request;
    let segments = segments_for_meeting(segments, &meeting_id);

    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;

    replace_speaker_segments(&mut transaction, &meeting_id, &method, &segments)
        .await
        .map_err(|error| error.to_string())?;
    upsert_meeting_diarization_status(&mut transaction, &meeting_id, &method, &segments)
        .await
        .map_err(|error| error.to_string())?;

    let transcripts = sqlx::query_as::<_, Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(&meeting_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;

    for transcript in transcripts {
        let window = TranscriptWindow {
            transcript_id: transcript.id.clone(),
            audio_start_time: transcript.audio_start_time,
            audio_end_time: transcript.audio_end_time,
        };

        let assignment = assign_speaker_to_transcript(&window, &segments, 0.1, &method);

        DiarizationRepository::update_transcript_assignment(
            &mut *transaction,
            &transcript.id,
            &assignment,
        )
        .await
        .map_err(|error| error.to_string())?;
    }

    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn segments_for_meeting(segments: Vec<SpeakerSegment>, meeting_id: &str) -> Vec<SpeakerSegment> {
    segments
        .into_iter()
        .filter(|segment| segment.meeting_id.as_str() == meeting_id)
        .collect()
}

async fn replace_speaker_segments(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    meeting_id: &str,
    method: &str,
    segments: &[SpeakerSegment],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM speaker_segments WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut **transaction)
        .await?;

    for (index, segment) in segments.iter().enumerate() {
        let segment_id = format!("{meeting_id}:{method}:{index}");
        let segment_status = diarization_status_to_database_value(segment.diarization_status);
        let segment_method = segment.diarization_method.as_deref().unwrap_or(method);

        sqlx::query(
            "INSERT INTO speaker_segments (
                id, meeting_id, source, start_time, end_time,
                speaker_id, speaker_label, confidence, is_overlap,
                diarization_status, diarization_method
             )
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(segment_id)
        .bind(meeting_id)
        .bind(&segment.source)
        .bind(segment.start_time)
        .bind(segment.end_time)
        .bind(&segment.speaker_id)
        .bind(&segment.speaker_label)
        .bind(segment.confidence)
        .bind(segment.is_overlap as i64)
        .bind(segment_status)
        .bind(segment_method)
        .execute(&mut **transaction)
        .await?;
    }

    Ok(())
}

async fn upsert_meeting_diarization_status(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    meeting_id: &str,
    method: &str,
    segments: &[SpeakerSegment],
) -> Result<(), sqlx::Error> {
    let current_status = meeting_status_from_segments(segments);
    let method = method.to_ascii_lowercase();
    let live_enabled = current_status == "provisional"
        || current_status == "fallback_to_live"
        || method.contains("live")
        || method.contains("online");
    let post_call_enabled =
        current_status == "final" || method.contains("post") || method.contains("offline");

    sqlx::query(
        "INSERT INTO meeting_diarization_status (
            meeting_id, enabled, live_enabled, post_call_enabled,
            current_status, quality_flags, processed_at, updated_at
         )
         VALUES (?, 1, ?, ?, ?, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(meeting_id) DO UPDATE SET
            enabled = 1,
            live_enabled = excluded.live_enabled,
            post_call_enabled = excluded.post_call_enabled,
            current_status = excluded.current_status,
            quality_flags = NULL,
            processed_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(meeting_id)
    .bind(live_enabled as i64)
    .bind(post_call_enabled as i64)
    .bind(current_status)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

fn meeting_status_from_segments(segments: &[SpeakerSegment]) -> &'static str {
    if segments.is_empty() {
        return "needs_review";
    }

    if segments
        .iter()
        .any(|segment| segment.diarization_status == DiarizationStatus::Failed)
    {
        return "failed";
    }

    if segments
        .iter()
        .any(|segment| segment.diarization_status == DiarizationStatus::NeedsReview)
    {
        return "needs_review";
    }

    if segments
        .iter()
        .any(|segment| segment.diarization_status == DiarizationStatus::FallbackToLive)
    {
        return "fallback_to_live";
    }

    if segments
        .iter()
        .any(|segment| segment.diarization_status == DiarizationStatus::Provisional)
    {
        return "provisional";
    }

    if segments
        .iter()
        .all(|segment| segment.diarization_status == DiarizationStatus::Final)
    {
        return "final";
    }

    "none"
}

fn normalize_speaker_label(label: &str) -> Result<String, String> {
    let label = label.trim();

    if label.is_empty() {
        return Err("Speaker label cannot be empty".to_string());
    }

    Ok(label.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(meeting_id: &str, speaker_label: &str) -> SpeakerSegment {
        SpeakerSegment {
            meeting_id: meeting_id.to_string(),
            source: "unit_test".to_string(),
            start_time: 0.0,
            end_time: 1.0,
            speaker_id: Some(speaker_label.to_lowercase().replace(' ', "-")),
            speaker_label: Some(speaker_label.to_string()),
            confidence: Some(0.8),
            is_overlap: false,
            diarization_status: DiarizationStatus::Provisional,
            diarization_method: Some("unit_test".to_string()),
        }
    }

    #[test]
    fn segments_for_meeting_excludes_segments_from_other_meetings() {
        let segments = segments_for_meeting(
            vec![
                segment("meeting-1", "Speaker 1"),
                segment("meeting-2", "Speaker 2"),
            ],
            "meeting-1",
        );

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].meeting_id, "meeting-1");
        assert_eq!(segments[0].speaker_label.as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn meeting_status_from_segments_returns_needs_review_for_empty_segments() {
        assert_eq!(meeting_status_from_segments(&[]), "needs_review");
    }

    #[test]
    fn meeting_status_from_segments_prefers_failed_over_final_segments() {
        let mut failed = segment("meeting-1", "Speaker 1");
        failed.diarization_status = DiarizationStatus::Failed;
        let mut final_segment = segment("meeting-1", "Speaker 2");
        final_segment.diarization_status = DiarizationStatus::Final;

        assert_eq!(
            meeting_status_from_segments(&[final_segment, failed]),
            "failed"
        );
    }

    #[test]
    fn normalize_speaker_label_trims_non_empty_label() {
        assert_eq!(
            normalize_speaker_label("  Alice Johnson  ").expect("label should be valid"),
            "Alice Johnson"
        );
    }

    #[test]
    fn normalize_speaker_label_rejects_blank_label() {
        assert_eq!(
            normalize_speaker_label(" \n\t ").expect_err("blank label should be rejected"),
            "Speaker label cannot be empty"
        );
    }
}
