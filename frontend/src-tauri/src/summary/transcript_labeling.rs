//! Speaker-labeled transcript assembly for summary generation.
//!
//! `api_process_transcript` receives a `text` string that the frontend concatenated from
//! transcript segments WITHOUT speaker labels (see
//! `frontend/src/hooks/meeting-details/useSummaryGeneration.ts::buildSummaryTranscriptPayload`).
//! When a meeting's stored transcripts carry speaker information — a diarized `speaker_id` or
//! a 'mic'/'system' channel tag — we rebuild that text server-side, prefixing each line with
//! the speaker's display name so the LLM can attribute statements, decisions, and action items.
//!
//! Rebuilding server-side keyed on `meeting_id` means (a) speaker renames are always fresh at
//! generation time and (b) every frontend call site (Generate + Regenerate) is covered at once.
//! Transcripts are never edited in place, so the DB is the single source of truth for the text.
//!
//! ## Behavior preservation
//! When NO segment carries any speaker label (every pre-diarization meeting, and unsaved/live
//! meetings whose transcripts aren't in the DB yet), we keep the caller's original `text`
//! byte-for-byte — zero behavior change.
//!
//! ## Label resolution (mirrors the UI's `resolveSpeakerLabel`, frontend/src/types/index.ts)
//!   1. `speaker_id` resolving to a non-empty `speakers.display_name` wins.
//!   2. else channel tag: 'mic' -> "You", 'system' -> "Others".
//!   3. else no label for that line.
//!
//! ## Line format
//! We keep the frontend's exact line shape — `<time-prefix> <text>` — and inject
//! `<label>: ` right before the text when a label resolves: `<time-prefix> <label>: <text>`.
//! The `[MM:SS]` time prefix is intentionally preserved (NOT dropped, and segments are NOT
//! merged): the built-in Standard Meeting template's Action Items section instructs the model
//! to record a "Segment Time stamp" per item, so the per-segment timestamp is load-bearing.

use crate::database::repositories::speaker::{SpeakersRepository, TranscriptSpeakerSegment};
use sqlx::SqlitePool;

/// Resolve a segment's speaker label, mirroring the UI's `resolveSpeakerLabel`.
///
/// Note: an empty `display_name` is treated as "not found" (JS truthiness of the string),
/// falling through to the channel tag — matching the frontend exactly.
pub fn resolve_segment_label(seg: &TranscriptSpeakerSegment) -> Option<String> {
    if seg.speaker_id.is_some() {
        if let Some(name) = seg.display_name.as_deref().filter(|n| !n.is_empty()) {
            return Some(name.to_string());
        }
    }
    match seg.speaker.as_deref() {
        Some("mic") => Some("You".to_string()),
        Some("system") => Some("Others".to_string()),
        _ => None,
    }
}

/// Reproduce the frontend `formatTime` that prefixes each transcript line: `[MM:SS]` derived
/// from `audio_start_time` seconds, or the wall-clock `timestamp` fallback when it is unknown.
fn format_time_prefix(audio_start_time: Option<f64>, fallback_timestamp: &str) -> String {
    match audio_start_time {
        Some(seconds) => {
            let total_secs = seconds.floor() as i64;
            format!("[{:02}:{:02}]", total_secs / 60, total_secs % 60)
        }
        None => fallback_timestamp.to_string(),
    }
}

/// Build speaker-labeled summary text from a meeting's ordered segments.
///
/// Returns `None` when no segment carries any speaker label — the signal for the caller to
/// keep the original unlabeled `text` (bit-for-bit behavior preservation for pre-diarization
/// meetings). Returns `Some(labeled_text)` when at least one segment resolves to a label.
pub fn assemble_labeled_transcript(segments: &[TranscriptSpeakerSegment]) -> Option<String> {
    if !segments.iter().any(|s| resolve_segment_label(s).is_some()) {
        return None;
    }

    let mut lines = Vec::with_capacity(segments.len());
    for seg in segments {
        let prefix = format_time_prefix(seg.audio_start_time, &seg.timestamp);
        match resolve_segment_label(seg) {
            Some(label) => lines.push(format!("{} {}: {}", prefix, label, seg.text)),
            None => lines.push(format!("{} {}", prefix, seg.text)),
        }
    }
    Some(lines.join("\n"))
}

/// Rebuild the summary input with speaker labels when the meeting's stored transcripts carry
/// speaker info; otherwise return `fallback` (the frontend-assembled text) unchanged.
///
/// Never fails the summary: a DB read error logs a warning and falls back to the unlabeled text.
pub async fn build_speaker_labeled_transcript(
    pool: &SqlitePool,
    meeting_id: &str,
    fallback: String,
) -> String {
    match SpeakersRepository::meeting_transcript_segments(pool, meeting_id).await {
        Ok(segments) => assemble_labeled_transcript(&segments).unwrap_or(fallback),
        Err(e) => {
            log::warn!(
                "[summary] failed to load transcript segments for speaker labeling (meeting {meeting_id}): {e}; using unlabeled text"
            );
            fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(
        text: &str,
        timestamp: &str,
        audio_start_time: Option<f64>,
        speaker: Option<&str>,
        speaker_id: Option<i64>,
        display_name: Option<&str>,
    ) -> TranscriptSpeakerSegment {
        TranscriptSpeakerSegment {
            text: text.to_string(),
            timestamp: timestamp.to_string(),
            audio_start_time,
            speaker: speaker.map(str::to_string),
            speaker_id,
            display_name: display_name.map(str::to_string),
        }
    }

    #[test]
    fn format_time_prefix_matches_frontend_formattime() {
        assert_eq!(format_time_prefix(Some(0.0), "ignored"), "[00:00]");
        assert_eq!(format_time_prefix(Some(5.9), "ignored"), "[00:05]"); // floors
        assert_eq!(format_time_prefix(Some(125.3), "ignored"), "[02:05]");
        assert_eq!(format_time_prefix(Some(3661.0), "ignored"), "[61:01]"); // minutes past 99 keep >2 digits
        assert_eq!(format_time_prefix(None, "14:30:05"), "14:30:05"); // wall-clock fallback
    }

    #[test]
    fn resolve_label_speaker_id_display_name_wins() {
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("system"), Some(1), Some("Андрей"))),
            Some("Андрей".to_string())
        );
    }

    #[test]
    fn resolve_label_falls_through_when_display_name_missing_or_empty() {
        // speaker_id set but the speaker row is gone (display_name NULL) -> channel tag.
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("system"), Some(9), None)),
            Some("Others".to_string())
        );
        // empty display_name is treated as not-found (matches JS truthiness).
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("mic"), Some(9), Some(""))),
            Some("You".to_string())
        );
    }

    #[test]
    fn resolve_label_channel_tags_and_none() {
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", None, Some("mic"), None, None)),
            Some("You".to_string())
        );
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", None, Some("system"), None, None)),
            Some("Others".to_string())
        );
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", None, None, None, None)),
            None
        );
        // Unknown channel value -> no label.
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", None, Some("other"), None, None)),
            None
        );
    }

    #[test]
    fn assemble_mixed_speaker_id_channel_and_none() {
        let segments = vec![
            // Diarized, renamed speaker.
            seg("ну давайте начнем", "00:00:01", Some(0.0), Some("mic"), Some(1), Some("Андрей")),
            // Remote channel, no diarized id.
            seg("да, я готов", "00:00:03", Some(3.0), Some("system"), None, None),
            // Local channel.
            seg("отлично", "00:00:07", Some(7.0), Some("mic"), None, None),
            // No speaker info at all -> unlabeled line, but still contributes (mixed meeting).
            seg("(тишина)", "00:00:09", Some(65.0), None, None, None),
        ];

        let out = assemble_labeled_transcript(&segments).expect("has labels");
        assert_eq!(
            out,
            "[00:00] Андрей: ну давайте начнем\n\
             [00:03] Others: да, я готов\n\
             [00:07] You: отлично\n\
             [01:05] (тишина)"
        );
    }

    #[test]
    fn assemble_returns_none_when_no_segment_has_speaker_info() {
        // Every pre-diarization segment: no speaker_id, no channel tag.
        let segments = vec![
            seg("first line", "00:00:01", Some(0.0), None, None, None),
            seg("second line", "00:00:02", Some(2.0), None, None, None),
        ];
        // None signals the caller to keep the original unlabeled text verbatim.
        assert_eq!(assemble_labeled_transcript(&segments), None);
    }

    #[test]
    fn assemble_returns_none_for_empty_meeting() {
        assert_eq!(assemble_labeled_transcript(&[]), None);
    }

    #[test]
    fn assemble_uses_wall_clock_when_audio_time_missing() {
        let segments = vec![seg("hola", "14:30:05", None, Some("mic"), None, None)];
        assert_eq!(
            assemble_labeled_transcript(&segments),
            Some("14:30:05 You: hola".to_string())
        );
    }
}
