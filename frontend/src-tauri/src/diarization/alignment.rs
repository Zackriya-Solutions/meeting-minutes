use super::types::{DiarizationStatus, SpeakerAssignment, SpeakerSegment, TranscriptWindow};

pub fn assign_speaker_to_transcript(
    transcript: &TranscriptWindow,
    segments: &[SpeakerSegment],
    min_overlap_ratio: f64,
    method: &str,
) -> SpeakerAssignment {
    let Some(transcript_start) = transcript.audio_start_time else {
        return SpeakerAssignment::unknown(
            DiarizationStatus::NeedsReview,
            Some(method.to_string()),
        );
    };
    let Some(transcript_end) = transcript.audio_end_time else {
        return SpeakerAssignment::unknown(
            DiarizationStatus::NeedsReview,
            Some(method.to_string()),
        );
    };

    let transcript_duration = (transcript_end - transcript_start).max(0.0);
    if transcript_duration == 0.0 {
        return SpeakerAssignment::unknown(
            DiarizationStatus::NeedsReview,
            Some(method.to_string()),
        );
    }

    let mut best_segment: Option<(&SpeakerSegment, f64)> = None;

    for segment in segments {
        let overlap_start = transcript_start.max(segment.start_time);
        let overlap_end = transcript_end.min(segment.end_time);
        let overlap = (overlap_end - overlap_start).max(0.0);
        if overlap <= 0.0 {
            continue;
        }

        let ratio = overlap / transcript_duration;
        if ratio < min_overlap_ratio {
            continue;
        }

        match best_segment {
            Some((_, best_overlap)) if best_overlap >= overlap => {}
            _ => best_segment = Some((segment, overlap)),
        }
    }

    let Some((segment, _)) = best_segment else {
        return SpeakerAssignment::unknown(
            DiarizationStatus::NeedsReview,
            Some(method.to_string()),
        );
    };

    if segment.is_overlap {
        return SpeakerAssignment::overlap(
            segment.confidence.unwrap_or(0.0),
            segment.diarization_status,
            method.to_string(),
        );
    }

    SpeakerAssignment {
        speaker_id: segment.speaker_id.clone(),
        speaker_label: segment
            .speaker_label
            .clone()
            .or_else(|| Some("Unknown speaker".to_string())),
        speaker_color: None,
        is_overlap: false,
        diarization_status: segment.diarization_status,
        diarization_method: Some(method.to_string()),
        diarization_confidence: segment.confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(label: &str, start: f64, end: f64) -> SpeakerSegment {
        SpeakerSegment {
            meeting_id: "meeting-1".to_string(),
            source: "system".to_string(),
            start_time: start,
            end_time: end,
            speaker_id: Some(label.to_lowercase().replace(' ', "-")),
            speaker_label: Some(label.to_string()),
            confidence: Some(0.8),
            is_overlap: false,
            diarization_status: DiarizationStatus::Provisional,
            diarization_method: Some("unit_test".to_string()),
        }
    }

    #[test]
    fn assigns_speaker_with_largest_temporal_overlap() {
        let transcript = TranscriptWindow {
            transcript_id: "t1".to_string(),
            audio_start_time: Some(10.0),
            audio_end_time: Some(16.0),
        };
        let segments = vec![
            segment("Speaker 1", 10.0, 12.0),
            segment("Speaker 2", 12.0, 16.0),
        ];

        let assignment = assign_speaker_to_transcript(&transcript, &segments, 0.1, "unit_test");

        assert_eq!(assignment.speaker_label.as_deref(), Some("Speaker 2"));
        assert_eq!(assignment.speaker_id.as_deref(), Some("speaker-2"));
        assert!(!assignment.is_overlap);
    }

    #[test]
    fn marks_overlap_when_matching_segment_is_overlap() {
        let transcript = TranscriptWindow {
            transcript_id: "t2".to_string(),
            audio_start_time: Some(20.0),
            audio_end_time: Some(24.0),
        };
        let mut overlap = segment("Speaker 1", 20.0, 24.0);
        overlap.is_overlap = true;
        overlap.confidence = Some(0.93);

        let assignment = assign_speaker_to_transcript(&transcript, &[overlap], 0.1, "unit_test");

        assert_eq!(
            assignment.speaker_label.as_deref(),
            Some("Multiple speakers")
        );
        assert!(assignment.is_overlap);
        assert_eq!(assignment.diarization_confidence, Some(0.93));
    }

    #[test]
    fn returns_unknown_when_transcript_has_no_audio_window() {
        let transcript = TranscriptWindow {
            transcript_id: "t3".to_string(),
            audio_start_time: None,
            audio_end_time: None,
        };

        let assignment = assign_speaker_to_transcript(
            &transcript,
            &[segment("Speaker 1", 0.0, 1.0)],
            0.1,
            "unit_test",
        );

        assert_eq!(assignment.speaker_label.as_deref(), Some("Unknown speaker"));
        assert_eq!(
            assignment.diarization_status,
            DiarizationStatus::NeedsReview
        );
    }
}
