//! Speaker-labeled transcript assembly for summary generation.
//!
//! `api_process_transcript` receives a `text` string that the frontend concatenated from
//! transcript segments WITHOUT speaker labels (see
//! `frontend/src/hooks/meeting-details/useSummaryGeneration.ts::buildSummaryTranscriptPayload`).
//! When a meeting's stored transcripts carry speaker information — a diarized `speaker_id` or
//! a diarized speaker profile (or a remote 'system' channel tag) — we rebuild that text
//! server-side, prefixing each line with
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
//!   1. A diarized profile explicitly marked `is_self` resolves to "You".
//!   2. Another `speaker_id` resolving to a non-empty display name uses that name.
//!   3. A remote `system` channel without identity resolves to "Others".
//!   4. A `mic` channel alone is never identity evidence; it stays unlabeled.
//!
//! ## Line format
//! We keep the frontend's exact line shape — `<time-prefix> <text>` — and inject
//! `<label>: ` right before the text when a label resolves: `<time-prefix> <label>: <text>`.
//! The `[MM:SS]` time prefix is intentionally preserved (NOT dropped, and segments are NOT
//! merged): the built-in Standard Meeting template's Action Items section instructs the model
//! to record a "Segment Time stamp" per item, so the per-segment timestamp is load-bearing.

use crate::database::repositories::speaker::{SpeakersRepository, TranscriptSpeakerSegment};
use sqlx::SqlitePool;

fn stable_text_fingerprint(text: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}:{}", hash, text.len())
}

/// Resolve a segment's speaker label, mirroring the UI's `resolveSpeakerLabel`.
///
/// Note: an empty `display_name` is treated as "not found" (JS truthiness of the string),
/// falling through to the channel tag — matching the frontend exactly.
pub fn resolve_segment_label(seg: &TranscriptSpeakerSegment) -> Option<String> {
    if seg.speaker_id.is_some() {
        if seg.is_self {
            return Some("You".to_string());
        }
        if let Some(name) = seg.display_name.as_deref().filter(|n| !n.is_empty()) {
            return Some(name.to_string());
        }
    }
    match seg.speaker.as_deref() {
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

/// Fingerprint only speaker attribution, independently from transcript wording.
///
/// Each ordered segment contributes its stable speaker profile id and the label rendered into
/// the summary prompt. A rename or re-diarization therefore invalidates the snapshot, while a
/// transcript typo correction does not masquerade as a speaker-name change.
pub fn speaker_attribution_fingerprint(segments: &[TranscriptSpeakerSegment]) -> Option<String> {
    if !segments
        .iter()
        .any(|segment| resolve_segment_label(segment).is_some())
    {
        return None;
    }

    let material = segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            format!(
                "{index}|{}|{}",
                segment
                    .speaker_id
                    .map(|speaker_id| speaker_id.to_string())
                    .unwrap_or_else(|| "channel".to_string()),
                resolve_segment_label(segment).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    Some(stable_text_fingerprint(&material))
}

/// Read the current speaker-attribution snapshot for a meeting.
pub async fn current_speaker_attribution_fingerprint(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let segments = SpeakersRepository::meeting_transcript_segments(pool, meeting_id).await?;
    Ok(speaker_attribution_fingerprint(&segments))
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
            is_self: false,
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
            resolve_segment_label(&seg(
                "hi",
                "t",
                Some(0.0),
                Some("system"),
                Some(1),
                Some("Андрей")
            )),
            Some("Андрей".to_string())
        );
    }

    #[test]
    fn resolve_label_uses_confirmed_self_identity_instead_of_channel() {
        let mut own = seg("hi", "t", Some(0.0), Some("system"), Some(1), Some("Миша"));
        own.is_self = true;
        assert_eq!(resolve_segment_label(&own), Some("You".to_string()));

        let mic_guest = seg("hello", "t", Some(1.0), Some("mic"), Some(2), Some("Анна"));
        assert_eq!(resolve_segment_label(&mic_guest), Some("Анна".to_string()));
    }

    #[test]
    fn resolve_label_falls_through_when_display_name_missing_or_empty() {
        // speaker_id set but the speaker row is gone (display_name NULL) -> channel tag.
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("system"), Some(9), None)),
            Some("Others".to_string())
        );
        // A mic channel cannot identify the user when the diarized profile is missing.
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("mic"), Some(9), Some(""))),
            None
        );
    }

    #[test]
    fn resolve_label_channel_tags_and_none() {
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", None, Some("mic"), None, None)),
            None
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
            seg(
                "ну давайте начнем",
                "00:00:01",
                Some(0.0),
                Some("mic"),
                Some(1),
                Some("Андрей"),
            ),
            // Remote channel, no diarized id.
            seg(
                "да, я готов",
                "00:00:03",
                Some(3.0),
                Some("system"),
                None,
                None,
            ),
            // Shared microphone channel without diarized identity: deliberately unlabeled.
            seg("отлично", "00:00:07", Some(7.0), Some("mic"), None, None),
            // No speaker info at all -> unlabeled line, but still contributes (mixed meeting).
            seg("(тишина)", "00:00:09", Some(65.0), None, None, None),
        ];

        let out = assemble_labeled_transcript(&segments).expect("has labels");
        assert_eq!(
            out,
            "[00:00] Андрей: ну давайте начнем\n\
             [00:03] Others: да, я готов\n\
             [00:07] отлично\n\
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
    fn speaker_attribution_fingerprint_changes_on_rename_but_not_transcript_edit() {
        let original = vec![seg(
            "исходный текст",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Андрей"),
        )];
        let renamed = vec![seg(
            "исходный текст",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Максим"),
        )];
        let corrected_text = vec![seg(
            "исправленный текст",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Андрей"),
        )];

        assert_ne!(
            speaker_attribution_fingerprint(&original),
            speaker_attribution_fingerprint(&renamed)
        );
        assert_eq!(
            speaker_attribution_fingerprint(&original),
            speaker_attribution_fingerprint(&corrected_text)
        );
    }

    #[test]
    fn speaker_attribution_fingerprint_changes_when_owner_identity_changes() {
        let guest = seg(
            "реплика",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Миша"),
        );
        let mut owner = guest.clone();
        owner.is_self = true;

        assert_ne!(
            speaker_attribution_fingerprint(&[guest]),
            speaker_attribution_fingerprint(&[owner])
        );
    }

    #[test]
    fn microphone_only_transcript_stays_unlabeled_without_diarized_identity() {
        let segments = vec![seg("hola", "14:30:05", None, Some("mic"), None, None)];
        assert_eq!(assemble_labeled_transcript(&segments), None);
    }
}
