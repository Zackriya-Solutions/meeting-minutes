use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiarizationStatus {
    None,
    Provisional,
    Final,
    FallbackToLive,
    Failed,
    NeedsReview,
}

impl Default for DiarizationStatus {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerSegment {
    pub meeting_id: String,
    pub source: String,
    pub start_time: f64,
    pub end_time: f64,
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub confidence: Option<f64>,
    pub is_overlap: bool,
    pub diarization_status: DiarizationStatus,
    pub diarization_method: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWindow {
    pub transcript_id: String,
    pub audio_start_time: Option<f64>,
    pub audio_end_time: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerAssignment {
    pub speaker_id: Option<String>,
    pub speaker_label: Option<String>,
    pub speaker_color: Option<String>,
    pub is_overlap: bool,
    pub diarization_status: DiarizationStatus,
    pub diarization_method: Option<String>,
    pub diarization_confidence: Option<f64>,
}

impl SpeakerAssignment {
    pub fn unknown(status: DiarizationStatus, method: impl Into<Option<String>>) -> Self {
        Self {
            speaker_id: None,
            speaker_label: Some("Unknown speaker".to_string()),
            speaker_color: None,
            is_overlap: false,
            diarization_status: status,
            diarization_method: method.into(),
            diarization_confidence: None,
        }
    }

    pub fn overlap(confidence: f64, status: DiarizationStatus, method: impl Into<String>) -> Self {
        Self {
            speaker_id: None,
            speaker_label: Some("Multiple speakers".to_string()),
            speaker_color: Some("#f97316".to_string()),
            is_overlap: true,
            diarization_status: status,
            diarization_method: Some(method.into()),
            diarization_confidence: Some(confidence),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diarization_status_serializes_as_snake_case() {
        let json = serde_json::to_string(&DiarizationStatus::FallbackToLive).unwrap();
        assert_eq!(json, "\"fallback_to_live\"");
    }

    #[test]
    fn overlap_assignment_uses_multiple_speakers_label() {
        let assignment =
            SpeakerAssignment::overlap(0.91, DiarizationStatus::Provisional, "fluid_audio_online");

        assert_eq!(assignment.speaker_id, None);
        assert_eq!(
            assignment.speaker_label.as_deref(),
            Some("Multiple speakers")
        );
        assert!(assignment.is_overlap);
        assert_eq!(
            assignment.diarization_status,
            DiarizationStatus::Provisional
        );
        assert_eq!(
            assignment.diarization_method.as_deref(),
            Some("fluid_audio_online")
        );
    }
}
