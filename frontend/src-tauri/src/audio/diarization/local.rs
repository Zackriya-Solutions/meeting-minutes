use crate::api::TranscriptSegment;

const SPEAKER_BUCKET_SECONDS: f64 = 120.0;

pub fn apply_local_diarization(segments: &mut [TranscriptSegment]) {
    for segment in segments.iter_mut() {
        let start = segment.audio_start_time.unwrap_or(0.0);
        let bucket = (start / SPEAKER_BUCKET_SECONDS).floor() as i64;
        let label = format!("Speaker {}", (bucket.rem_euclid(2) + 1));
        segment.speaker = Some(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(start: f64) -> TranscriptSegment {
        TranscriptSegment {
            id: "1".to_string(),
            text: "hello".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            speaker: None,
            audio_start_time: Some(start),
            audio_end_time: Some(start + 1.0),
            duration: Some(1.0),
        }
    }

    #[test]
    fn assigns_stable_speaker_labels() {
        let mut segments = vec![make_segment(0.0), make_segment(121.0), make_segment(240.0)];
        apply_local_diarization(&mut segments);
        assert_eq!(segments[0].speaker.as_deref(), Some("Speaker 1"));
        assert_eq!(segments[1].speaker.as_deref(), Some("Speaker 2"));
        assert_eq!(segments[2].speaker.as_deref(), Some("Speaker 1"));
    }
}
