//! Speaker-labeled transcript assembly for summary generation.
//!
//! `api_process_transcript` receives a `text` string that the frontend concatenated from
//! transcript segments WITHOUT speaker labels (see
//! `frontend/src/hooks/meeting-details/useSummaryGeneration.ts::buildSummaryTranscriptPayload`).
//! When a meeting's stored transcripts carry speaker information — a diarized `speaker_id`, or
//! an audio-channel tag we can still read as identity — we rebuild that text server-side,
//! prefixing each line with the speaker's display name so the LLM can attribute statements,
//! decisions, and action items.
//!
//! Rebuilding server-side keyed on `meeting_id` means (a) speaker renames are always fresh at
//! generation time and (b) every frontend call site (Generate + Regenerate) is covered at once.
//! Transcripts are never edited in place, so the DB is the single source of truth for the text.
//!
//! ## Behavior preservation
//! When NO segment resolves to a label (a recording with neither diarization nor channel tags,
//! and unsaved/live meetings whose transcripts aren't in the DB yet), we keep the caller's
//! original `text` byte-for-byte — zero behavior change.
//!
//! ## Label resolution (mirrors the UI's `resolveSpeakerLabel`, frontend/src/types/index.ts)
//!   1. A diarized profile explicitly marked `is_self` resolves to its own display name
//!      once it has one, and to "You" while it is still an automatic placeholder.
//!   2. Another `speaker_id` resolving to a non-empty display name uses that name.
//!   3. A remote `system` channel without identity resolves to "Others".
//!   4. A `mic` channel is identity evidence only as a last resort — see below.
//!
//! ## The microphone channel as a last resort
//! Once a meeting has an owner voice (some segment carries a diarized profile marked
//! `is_self`), diarization is authoritative and the `mic` channel is never read as identity:
//! an unattributed mic line may well be a colleague sharing the room, and mislabeling them
//! "You" corrupts the attribution the owner voice was established to get right.
//!
//! Until then there is no diarized identity to defer to, and the mic/system split is the only
//! signal the recording carries. Dropping it would silently strip the "You" labels from every
//! meeting recorded before the owner-voice flow existed — the LLM would see one named party
//! ("Others") talking to an anonymous one. So an unattributed mic line still resolves to "You"
//! while the meeting has no owner voice, exactly as it did before `is_self` existed.
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

/// Resolve a segment's speaker label from diarized identity, mirroring the UI's
/// `resolveSpeakerLabel`.
///
/// Note: an empty `display_name` is treated as "not found" (JS truthiness of the string),
/// falling through to the channel tag — matching the frontend exactly.
pub fn resolve_segment_label(seg: &TranscriptSpeakerSegment) -> Option<String> {
    if seg.speaker_id.is_some() {
        let name = seg
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        // A named owner voice is labeled by name, not by "You". The summary is prose in
        // the user's language: an English pronoun leaks into it verbatim ("You сделает
        // презентацию"), and the owner then reads as somebody other than the person the
        // participant list shows. "You" stays the fallback while the owner voice still
        // carries an automatic placeholder.
        if seg.is_self {
            return Some(
                name.filter(|name| {
                    !crate::database::repositories::speaker::is_automatic_speaker_name(name)
                })
                .unwrap_or("You")
                .to_string(),
            );
        }
        if let Some(name) = name {
            return Some(name.to_string());
        }
    }
    match seg.speaker.as_deref() {
        Some("system") => Some("Others".to_string()),
        _ => None,
    }
}

/// Does this meeting have an owner voice — a diarized profile the user marked as their own?
///
/// This is what decides whether the `mic` channel may still stand in for identity; see the
/// module docs.
fn has_owner_voice(segments: &[TranscriptSpeakerSegment]) -> bool {
    segments
        .iter()
        .any(|seg| seg.speaker_id.is_some() && seg.is_self)
}

/// Resolve a segment's label with the meeting's identity context.
///
/// `owner_voice_known` comes from [`has_owner_voice`] over the meeting's whole segment list:
/// with an owner voice established, an unattributed `mic` line stays unlabeled; without one it
/// falls back to "You".
pub fn resolve_segment_label_in_meeting(
    seg: &TranscriptSpeakerSegment,
    owner_voice_known: bool,
) -> Option<String> {
    if let Some(label) = resolve_segment_label(seg) {
        return Some(label);
    }
    if !owner_voice_known && seg.speaker_id.is_none() && seg.speaker.as_deref() == Some("mic") {
        return Some("You".to_string());
    }
    None
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
    let owner_voice_known = has_owner_voice(segments);
    if !segments
        .iter()
        .any(|s| resolve_segment_label_in_meeting(s, owner_voice_known).is_some())
    {
        return None;
    }

    let mut lines = Vec::with_capacity(segments.len());
    for seg in segments {
        let prefix = format_time_prefix(seg.audio_start_time, &seg.timestamp);
        match resolve_segment_label_in_meeting(seg, owner_voice_known) {
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
    // The fingerprint tracks the labels actually rendered into the prompt, so it must apply the
    // same meeting-level identity context: marking an owner voice retires the mic fallback and
    // must therefore show up as an attribution change.
    let owner_voice_known = has_owner_voice(segments);
    if !segments
        .iter()
        .any(|segment| resolve_segment_label_in_meeting(segment, owner_voice_known).is_some())
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
                resolve_segment_label_in_meeting(segment, owner_voice_known).unwrap_or_default()
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
        // A named owner voice is a person with a name; "You" is only the stand-in while
        // that voice is still an automatic placeholder. The summary is written in the
        // user's language, and an English pronoun ends up in it verbatim.
        let mut own = seg("hi", "t", Some(0.0), Some("system"), Some(1), Some("Миша"));
        own.is_self = true;
        assert_eq!(resolve_segment_label(&own), Some("Миша".to_string()));

        let mut unnamed_own = seg("hi", "t", Some(0.0), Some("system"), Some(1), Some("Speaker 2"));
        unnamed_own.is_self = true;
        assert_eq!(resolve_segment_label(&unnamed_own), Some("You".to_string()));

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
        // A diarized mic segment whose profile carries no usable name stays unlabeled: the
        // segment IS attributed, just to a nameless profile, so the channel adds nothing.
        assert_eq!(
            resolve_segment_label(&seg("hi", "t", Some(0.0), Some("mic"), Some(9), Some(""))),
            None
        );
        assert_eq!(
            resolve_segment_label_in_meeting(
                &seg("hi", "t", Some(0.0), Some("mic"), Some(9), Some("")),
                false
            ),
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
            // Microphone channel without diarized identity, and no owner voice anywhere in the
            // meeting -> the channel is the only signal left, so it still resolves to "You".
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
    fn mic_fallback_applies_only_until_an_owner_voice_exists() {
        let unattributed_mic = seg("отлично", "00:00:07", Some(7.0), Some("mic"), None, None);

        // No owner voice yet: the mic channel stands in for the user, as it did before
        // `is_self` existed.
        assert_eq!(
            resolve_segment_label_in_meeting(&unattributed_mic, false),
            Some("You".to_string())
        );
        // Owner voice established: an unattributed mic line may be a colleague in the room.
        assert_eq!(
            resolve_segment_label_in_meeting(&unattributed_mic, true),
            None
        );
    }

    #[test]
    fn assemble_keeps_mic_labels_for_meetings_recorded_before_owner_voice() {
        // The realistic pre-migration meeting: is_self is 0 everywhere, nothing is diarized.
        let segments = vec![
            seg("привет", "00:00:01", Some(0.0), Some("mic"), None, None),
            seg(
                "да, слышу",
                "00:00:03",
                Some(3.0),
                Some("system"),
                None,
                None,
            ),
        ];

        assert_eq!(
            assemble_labeled_transcript(&segments).expect("channel tags still label"),
            "[00:00] You: привет\n[00:03] Others: да, слышу"
        );
    }

    #[test]
    fn assemble_drops_mic_guess_once_the_owner_voice_is_known() {
        let mut own = seg("привет", "00:00:01", Some(0.0), Some("mic"), Some(1), None);
        own.is_self = true;
        let segments = vec![
            own,
            // Same channel, but diarization did not attribute this line to anyone.
            seg(
                "а я добавлю",
                "00:00:05",
                Some(5.0),
                Some("mic"),
                None,
                None,
            ),
        ];

        assert_eq!(
            assemble_labeled_transcript(&segments).expect("has labels"),
            "[00:00] You: привет\n[00:05] а я добавлю"
        );
    }

    #[test]
    fn marking_an_owner_voice_changes_the_attribution_fingerprint() {
        let before = vec![
            seg("привет", "00:00:01", Some(0.0), Some("mic"), Some(1), None),
            seg(
                "а я добавлю",
                "00:00:05",
                Some(5.0),
                Some("mic"),
                None,
                None,
            ),
        ];
        let mut after = before.clone();
        after[0].is_self = true;

        let before_fp = speaker_attribution_fingerprint(&before).expect("labeled");
        let after_fp = speaker_attribution_fingerprint(&after).expect("labeled");
        // Retiring the mic fallback is an attribution change: the saved summary is now stale.
        assert_ne!(before_fp, after_fp);
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
        // A voice that is still a placeholder is labeled "You" once it becomes the owner,
        // so the model reads a different transcript and the saved summary is stale.
        let unnamed = seg(
            "реплика",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Speaker 1"),
        );
        let mut unnamed_owner = unnamed.clone();
        unnamed_owner.is_self = true;
        assert_ne!(
            speaker_attribution_fingerprint(&[unnamed]),
            speaker_attribution_fingerprint(&[unnamed_owner])
        );

        // A voice that already carries a name keeps that label either way: nothing the
        // model sees changed, so there is no summary to invalidate.
        let named = seg(
            "реплика",
            "00:00:01",
            Some(0.0),
            Some("mic"),
            Some(7),
            Some("Миша"),
        );
        let mut named_owner = named.clone();
        named_owner.is_self = true;
        assert_eq!(
            speaker_attribution_fingerprint(&[named]),
            speaker_attribution_fingerprint(&[named_owner])
        );
    }

    #[test]
    fn assemble_uses_wall_clock_when_audio_time_missing() {
        let segments = vec![seg("hola", "14:30:05", None, Some("mic"), None, None)];
        assert_eq!(
            assemble_labeled_transcript(&segments),
            Some("14:30:05 You: hola".to_string())
        );
    }

    #[test]
    fn microphone_only_transcript_stays_unlabeled_once_an_owner_voice_exists() {
        let mut owner = seg("привет", "14:30:05", Some(0.0), Some("mic"), Some(3), None);
        owner.is_self = true;
        let segments = vec![
            owner,
            seg("hola", "14:30:09", Some(4.0), Some("mic"), None, None),
        ];

        assert_eq!(
            assemble_labeled_transcript(&segments).expect("owner voice labels its own line"),
            "[00:00] You: привет\n[00:04] hola"
        );
    }
}
