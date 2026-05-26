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
}
