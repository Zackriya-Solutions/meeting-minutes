use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{
    database::{models::Transcript, repositories::diarization::DiarizationRepository},
    diarization::{
        alignment::assign_speaker_to_transcript,
        types::{SpeakerSegment, TranscriptWindow},
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

    let transcripts = sqlx::query_as::<_, Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(&meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    for transcript in transcripts {
        let window = TranscriptWindow {
            transcript_id: transcript.id.clone(),
            audio_start_time: transcript.audio_start_time,
            audio_end_time: transcript.audio_end_time,
        };

        let assignment = assign_speaker_to_transcript(&window, &segments, 0.1, &method);

        DiarizationRepository::update_transcript_assignment(pool, &transcript.id, &assignment)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn segments_for_meeting(segments: Vec<SpeakerSegment>, meeting_id: &str) -> Vec<SpeakerSegment> {
    segments
        .into_iter()
        .filter(|segment| segment.meeting_id.as_str() == meeting_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diarization::types::DiarizationStatus;

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
}
