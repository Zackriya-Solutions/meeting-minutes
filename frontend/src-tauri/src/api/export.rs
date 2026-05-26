//! Export transcripts to .txt / .vtt files for issue #441.
//!
//! Pure formatters live alongside the Tauri command so the test surface
//! stays in one place. The formatters take owned data and return owned
//! `String`, so they're trivially unit-testable with no DB.

use crate::database::models::Transcript;

/// Format transcripts as plain text, one segment per line.
///
/// Line shape: `[HH:MM:SS] text` when `audio_start_time` is present,
/// else just `text`. Internal newlines in `transcript` are collapsed
/// to a single space. The output ends with a trailing newline.
pub fn format_txt(segments: &[Transcript]) -> String {
    let mut sorted: Vec<&Transcript> = segments.iter().collect();
    sorted.sort_by(|a, b| {
        a.audio_start_time
            .partial_cmp(&b.audio_start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::new();
    for seg in sorted {
        let line_text = seg.transcript.replace(['\n', '\r'], " ");
        match seg.audio_start_time {
            Some(t) => {
                out.push_str(&format!("[{}] {}\n", format_hms(t), line_text));
            }
            None => {
                out.push_str(&line_text);
                out.push('\n');
            }
        }
    }
    out
}

fn format_hms(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

/// Format transcripts as WebVTT v1. Returns `(content, segments_skipped)`.
/// Segments with `audio_start_time = None` cannot form a valid cue and are
/// reported in the second tuple field so the caller can warn the user.
pub fn format_vtt(segments: &[Transcript]) -> (String, usize) {
    let mut sorted: Vec<&Transcript> = segments.iter().collect();
    sorted.sort_by(|a, b| {
        a.audio_start_time
            .partial_cmp(&b.audio_start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = String::from("WEBVTT\n\n");
    let mut skipped: usize = 0;

    for seg in sorted {
        let start = match seg.audio_start_time {
            Some(t) => t,
            None => {
                skipped += 1;
                continue;
            }
        };
        let end = seg
            .audio_end_time
            .or_else(|| seg.duration.map(|d| start + d))
            .unwrap_or(start + 5.0);

        let cue_text = escape_vtt(&seg.transcript.replace(['\n', '\r'], " "));
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_vtt_timestamp(start),
            format_vtt_timestamp(end),
            cue_text
        ));
    }

    (out, skipped)
}

fn format_vtt_timestamp(seconds: f64) -> String {
    let secs = seconds.max(0.0);
    let hours = (secs / 3600.0).floor() as u64;
    let minutes = ((secs % 3600.0) / 60.0).floor() as u64;
    let whole_seconds = (secs % 60.0).floor() as u64;
    let millis = ((secs - secs.floor()) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        hours, minutes, whole_seconds, millis
    )
}

fn escape_vtt(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(
        text: &str,
        audio_start_time: Option<f64>,
        audio_end_time: Option<f64>,
        duration: Option<f64>,
    ) -> Transcript {
        Transcript {
            id: "t-test".into(),
            meeting_id: "m-test".into(),
            transcript: text.into(),
            timestamp: "2026-05-26T00:00:00Z".into(),
            summary: None,
            action_items: None,
            key_points: None,
            audio_start_time,
            audio_end_time,
            duration,
        }
    }

    #[test]
    fn format_txt_happy_path() {
        let segments = vec![
            seg("We should ship the feature.", Some(83.0), Some(88.0), Some(5.0)),
            seg("Agreed.", Some(90.0), Some(92.0), Some(2.0)),
        ];
        let out = format_txt(&segments);
        assert_eq!(
            out,
            "[00:01:23] We should ship the feature.\n[00:01:30] Agreed.\n"
        );
    }

    #[test]
    fn format_txt_null_timestamp_omits_bracket() {
        let segments = vec![seg("Imported audio.", None, None, None)];
        let out = format_txt(&segments);
        assert_eq!(out, "Imported audio.\n");
    }

    #[test]
    fn format_txt_collapses_internal_newlines() {
        let segments = vec![seg(
            "line one\nline two\rline three",
            Some(0.0),
            Some(1.0),
            Some(1.0),
        )];
        let out = format_txt(&segments);
        assert_eq!(out, "[00:00:00] line one line two line three\n");
    }

    #[test]
    fn format_txt_sorts_by_audio_start_time() {
        let segments = vec![
            seg("third", Some(3.0), Some(4.0), Some(1.0)),
            seg("first", Some(1.0), Some(2.0), Some(1.0)),
            seg("second", Some(2.0), Some(3.0), Some(1.0)),
        ];
        let out = format_txt(&segments);
        assert!(out.starts_with("[00:00:01] first\n"));
        assert!(out.contains("[00:00:02] second\n"));
        assert!(out.ends_with("[00:00:03] third\n"));
    }

    #[test]
    fn format_txt_handles_hour_rollover() {
        let segments = vec![seg("late", Some(3725.0), Some(3730.0), Some(5.0))];
        let out = format_txt(&segments);
        assert_eq!(out, "[01:02:05] late\n");
    }

    #[test]
    fn format_txt_empty_input_returns_empty_string() {
        let out = format_txt(&[]);
        assert_eq!(out, "");
    }

    #[test]
    fn format_txt_ends_with_trailing_newline_when_non_empty() {
        let segments = vec![seg("hi", Some(0.0), Some(1.0), Some(1.0))];
        let out = format_txt(&segments);
        assert!(out.ends_with('\n'));
    }

    // ----- format_vtt -----

    #[test]
    fn format_vtt_happy_path() {
        let segments = vec![
            seg("We should ship the feature.", Some(0.0), Some(8.5), Some(8.5)),
            seg("Agreed.", Some(8.5), Some(12.3), Some(3.8)),
        ];
        let (out, skipped) = format_vtt(&segments);
        assert_eq!(skipped, 0);
        assert_eq!(
            out,
            "WEBVTT\n\n\
             00:00:00.000 --> 00:00:08.500\n\
             We should ship the feature.\n\n\
             00:00:08.500 --> 00:00:12.300\n\
             Agreed.\n\n"
        );
    }

    #[test]
    fn format_vtt_uses_duration_when_end_missing() {
        let segments = vec![seg("only duration", Some(10.0), None, Some(2.5))];
        let (out, _) = format_vtt(&segments);
        assert!(out.contains("00:00:10.000 --> 00:00:12.500"));
    }

    #[test]
    fn format_vtt_uses_5s_fallback_when_end_and_duration_missing() {
        let segments = vec![seg("naked start", Some(20.0), None, None)];
        let (out, _) = format_vtt(&segments);
        assert!(out.contains("00:00:20.000 --> 00:00:25.000"));
    }

    #[test]
    fn format_vtt_skips_segments_without_start_time() {
        let segments = vec![
            seg("good", Some(0.0), Some(1.0), Some(1.0)),
            seg("orphan", None, None, None),
            seg("good 2", Some(2.0), Some(3.0), Some(1.0)),
        ];
        let (out, skipped) = format_vtt(&segments);
        assert_eq!(skipped, 1);
        assert!(out.contains("good"));
        assert!(out.contains("good 2"));
        assert!(!out.contains("orphan"));
    }

    #[test]
    fn format_vtt_escapes_special_characters() {
        let segments = vec![seg("3 < 5 & 7 > 6", Some(0.0), Some(1.0), Some(1.0))];
        let (out, _) = format_vtt(&segments);
        assert!(out.contains("3 &lt; 5 &amp; 7 &gt; 6"));
    }

    #[test]
    fn format_vtt_keeps_overlapping_segments_as_distinct_cues() {
        let segments = vec![
            seg("mic talks", Some(0.0), Some(5.0), Some(5.0)),
            seg("system talks", Some(2.0), Some(6.0), Some(4.0)),
        ];
        let (out, _) = format_vtt(&segments);
        assert!(out.contains("00:00:00.000 --> 00:00:05.000\nmic talks"));
        assert!(out.contains("00:00:02.000 --> 00:00:06.000\nsystem talks"));
    }

    #[test]
    fn format_vtt_timestamp_at_hour_boundary() {
        let segments = vec![seg("late", Some(3725.5), Some(3730.5), Some(5.0))];
        let (out, _) = format_vtt(&segments);
        assert!(out.contains("01:02:05.500 --> 01:02:10.500"));
    }

    #[test]
    fn format_vtt_empty_input_returns_header_only() {
        let (out, skipped) = format_vtt(&[]);
        assert_eq!(out, "WEBVTT\n\n");
        assert_eq!(skipped, 0);
    }
}
