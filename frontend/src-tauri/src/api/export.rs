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
    let _ = segments; // wired in next task
    String::new()
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
    fn format_txt_compiles() {
        // placeholder; real tests land in Task 2.
        let _ = format_txt(&[]);
        let _ = seg("", None, None, None);
    }
}
