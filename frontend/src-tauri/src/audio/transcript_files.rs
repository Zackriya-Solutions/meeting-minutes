//! Plain-text and WebVTT copies of a meeting transcript, written beside the recording.

use crate::api::TranscriptSegment;
use crate::database::models::{DateTimeUtc, Transcript};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::speaker::{speaker_label, SpeakersRepository};
use crate::state::AppState;
use anyhow::{Context, Result};
use log::{info, warn};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::Path;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

pub const TRANSCRIPT_TXT_FILE: &str = "transcript.txt";
pub const TRANSCRIPT_VTT_FILE: &str = "transcript.vtt";

/// One transcript row ready to format. `speaker` is the label shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptLine {
    pub text: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub duration: Option<f64>,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedTranscript {
    pub content: String,
    pub written: usize,
    pub skipped: usize,
}

impl TranscriptLine {
    pub fn from_segment(segment: &TranscriptSegment, labels: &BTreeMap<String, String>) -> Self {
        Self {
            text: segment.text.clone(),
            start: segment.audio_start_time,
            end: segment.audio_end_time,
            duration: segment.duration,
            speaker: resolved_speaker(segment.speaker.as_deref(), labels),
        }
    }
}

/// Display name for a speaker key, or "Speaker N" when the meeting has no name for it.
fn resolved_speaker(key: Option<&str>, labels: &BTreeMap<String, String>) -> Option<String> {
    let key = key?;
    let label = match labels.get(key) {
        Some(label) => label.clone(),
        None => speaker_label(key, None),
    };
    collapse_whitespace(&label).filter(|name| !name.is_empty())
}

fn collapse_whitespace(value: &str) -> Option<String> {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

/// Voice-span annotation. `>` would end the span, so it is removed along with the other markup characters.
fn voice_annotation(label: &str) -> Option<String> {
    let cleaned: String = label
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | '&'))
        .collect();
    collapse_whitespace(&cleaned)
}

fn cue_text(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_string()
}

fn sorted(lines: &[TranscriptLine]) -> Vec<&TranscriptLine> {
    let mut sorted: Vec<&TranscriptLine> = lines.iter().collect();
    sorted.sort_by(|a, b| match (a.start, b.start) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    sorted
}

fn format_hms(seconds: f64) -> String {
    let total = seconds.max(0.0).floor() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

fn timestamp_ms(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1000.0).round() as u64
}

fn format_timestamp_ms(total_ms: u64) -> String {
    let millis = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let secs = total_secs % 60;
    let minutes = (total_secs / 60) % 60;
    let hours = total_secs / 3600;
    format!("{hours:02}:{minutes:02}:{secs:02}.{millis:03}")
}

fn escape_vtt(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn line_body(line: &TranscriptLine, text: &str) -> String {
    match line.speaker.as_deref().and_then(collapse_whitespace) {
        Some(name) => format!("{name}: {text}"),
        None => text.to_string(),
    }
}

/// Plain text, one segment per line. A missing start time omits the clock prefix.
pub fn format_txt(lines: &[TranscriptLine]) -> FormattedTranscript {
    let mut content = String::new();
    let mut written = 0usize;
    for line in sorted(lines) {
        let text = cue_text(&line.text);
        if text.is_empty() {
            continue;
        }
        let body = line_body(line, &text);
        match line.start {
            Some(start) => content.push_str(&format!("[{}] {body}\n", format_hms(start))),
            None => {
                content.push_str(&body);
                content.push('\n');
            }
        }
        written += 1;
    }
    FormattedTranscript {
        content,
        written,
        skipped: 0,
    }
}

/// WebVTT cues. Segments with no start time are skipped so captions stay aligned with the recording.
pub fn format_vtt(lines: &[TranscriptLine]) -> FormattedTranscript {
    let mut content = String::from("WEBVTT\n\n");
    let mut written = 0usize;
    let mut skipped = 0usize;
    for line in sorted(lines) {
        let text = cue_text(&line.text);
        if text.is_empty() {
            continue;
        }
        let Some(start) = line.start else {
            skipped += 1;
            continue;
        };
        let end = line
            .end
            .or_else(|| line.duration.map(|duration| start + duration))
            .unwrap_or(start + 5.0);
        let start_ms = timestamp_ms(start);
        let end_ms = timestamp_ms(end).max(start_ms + 1);
        let escaped = escape_vtt(&text);
        let payload = match line.speaker.as_deref().and_then(voice_annotation) {
            Some(name) => format!("<v {name}>{escaped}"),
            None => escaped,
        };
        content.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_timestamp_ms(start_ms),
            format_timestamp_ms(end_ms),
            payload
        ));
        written += 1;
    }
    FormattedTranscript {
        content,
        written,
        skipped,
    }
}

fn write_atomic(folder: &Path, file_name: &str, contents: &str) -> Result<()> {
    let dest = folder.join(file_name);
    let temp = folder.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    std::fs::write(&temp, contents)
        .with_context(|| format!("Failed to write {}", temp.display()))?;
    if let Err(error) = std::fs::rename(&temp, &dest) {
        let _ = std::fs::remove_file(&temp);
        return Err(error).with_context(|| format!("Failed to replace {}", dest.display()));
    }
    Ok(())
}

/// Writes transcript.txt and transcript.vtt, replacing any existing copies.
pub fn write_transcript_files(folder: &Path, lines: &[TranscriptLine]) -> Result<()> {
    let txt = format_txt(lines);
    let vtt = format_vtt(lines);
    write_atomic(folder, TRANSCRIPT_TXT_FILE, &txt.content)?;
    write_atomic(folder, TRANSCRIPT_VTT_FILE, &vtt.content)?;
    Ok(())
}

async fn lines_for_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<TranscriptLine>, sqlx::Error> {
    let rows: Vec<Transcript> =
        sqlx::query_as("SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time")
            .bind(meeting_id)
            .fetch_all(pool)
            .await?;
    let labels = SpeakersRepository::labels(pool, meeting_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| TranscriptLine::from_segment(&TranscriptSegment::from(row), &labels))
        .collect())
}

/// Writes transcript.txt and transcript.vtt into the recording folder.
/// A missing folder or a file error is logged and does not change the database save that already succeeded.
pub async fn write_saved_transcript_files(pool: &SqlitePool, meeting_id: &str) {
    let meeting = match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
        Ok(Some(meeting)) => meeting,
        Ok(None) => {
            warn!(
                "Meeting not found while writing transcript files: {}",
                meeting_id
            );
            return;
        }
        Err(e) => {
            warn!(
                "Failed to read meeting folder for transcript files (meeting_id={}): {}",
                meeting_id, e
            );
            return;
        }
    };

    let Some(folder_path) = meeting.folder_path.filter(|path| !path.trim().is_empty()) else {
        info!(
            "No recording folder for meeting {}; skipped transcript.txt and transcript.vtt",
            meeting_id
        );
        return;
    };

    let lines = match lines_for_meeting(pool, meeting_id).await {
        Ok(lines) => lines,
        Err(e) => {
            warn!(
                "Failed to read transcripts for transcript files (meeting_id={}): {}",
                meeting_id, e
            );
            return;
        }
    };

    if let Err(e) = write_transcript_files(Path::new(&folder_path), &lines) {
        warn!(
            "Failed to write transcript.txt and transcript.vtt for meeting_id={}: {:#}",
            meeting_id, e
        );
    }
}

/// Writes the same files into a folder that is already known, such as the one just used for transcripts.json.
pub fn write_transcript_files_for_segments(
    folder: &Path,
    segments: &[TranscriptSegment],
    labels: &BTreeMap<String, String>,
) -> Result<()> {
    let lines: Vec<TranscriptLine> = segments
        .iter()
        .map(|segment| TranscriptLine::from_segment(segment, labels))
        .collect();
    write_transcript_files(folder, &lines)
}

pub fn sanitize_filename_stem(title: &str) -> String {
    const BANNED: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let cleaned: String = title
        .chars()
        .map(|c| {
            if BANNED.contains(&c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = collapse_whitespace(&cleaned).unwrap_or_default();
    let trimmed = collapsed.trim_matches(|c: char| c.is_whitespace() || c == '.');
    let truncated: String = trimmed.chars().take(80).collect();
    if truncated.is_empty() {
        "transcript".to_string()
    } else {
        truncated
    }
}

fn export_file_name(title: &str, created_at: &DateTimeUtc, extension: &str) -> String {
    format!(
        "{} - {}.{}",
        sanitize_filename_stem(title),
        created_at.0.format("%Y-%m-%d"),
        extension
    )
}

#[derive(Debug, Serialize)]
pub struct TranscriptExportResult {
    pub path: String,
    pub segments_written: usize,
    pub segments_skipped: usize,
}

/// Saves a copy of the meeting transcript to a path the user chooses.
#[tauri::command]
pub async fn export_meeting_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    format: String,
) -> Result<Option<TranscriptExportResult>, String> {
    let (extension, filter_name): (&'static str, &'static str) = match format.as_str() {
        "txt" => ("txt", "Plain text"),
        "vtt" => ("vtt", "WebVTT"),
        other => return Err(format!("Unsupported format: {other}")),
    };

    let pool = state.db_manager.pool();
    let meeting = MeetingsRepository::get_meeting_metadata(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to read meeting: {e}"))?
        .ok_or_else(|| format!("Meeting not found: {meeting_id}"))?;
    let lines = lines_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to read transcripts: {e}"))?;
    if lines.iter().all(|line| cue_text(&line.text).is_empty()) {
        return Err("No transcript to export".to_string());
    }

    let formatted = if extension == "txt" {
        format_txt(&lines)
    } else {
        format_vtt(&lines)
    };
    let file_name = export_file_name(&meeting.title, &meeting.created_at, extension);
    let directory = meeting.folder_path.filter(|path| Path::new(path).is_dir());

    let saved = tokio::task::spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .add_filter(filter_name, &[extension])
            .set_file_name(file_name);
        if let Some(dir) = directory {
            dialog = dialog.set_directory(dir);
        }
        dialog.blocking_save_file()
    })
    .await
    .map_err(|e| format!("File dialog task failed: {e}"))?;

    let Some(path) = saved else {
        return Ok(None);
    };
    let path = path.to_string();
    std::fs::write(&path, &formatted.content)
        .map_err(|e| format!("Failed to write {path}: {e}"))?;
    Ok(Some(TranscriptExportResult {
        path,
        segments_written: formatted.written,
        segments_skipped: formatted.skipped,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(
        text: &str,
        start: Option<f64>,
        end: Option<f64>,
        duration: Option<f64>,
        speaker: Option<&str>,
    ) -> TranscriptLine {
        TranscriptLine {
            text: text.to_string(),
            start,
            end,
            duration,
            speaker: speaker.map(str::to_string),
        }
    }

    fn no_tmp_files(dir: &Path) -> bool {
        std::fs::read_dir(dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
    }

    #[test]
    fn format_txt_includes_the_speaker_when_one_is_set() {
        let lines = vec![
            line(
                "We should ship the feature.",
                Some(83.0),
                Some(88.0),
                Some(5.0),
                Some("Noah"),
            ),
            line("Agreed.", Some(90.0), Some(92.0), Some(2.0), None),
        ];
        let formatted = format_txt(&lines);
        assert_eq!(formatted.written, 2);
        assert_eq!(
            formatted.content,
            "[00:01:23] Noah: We should ship the feature.\n[00:01:30] Agreed.\n"
        );
    }

    #[test]
    fn format_txt_omits_the_clock_when_the_start_time_is_missing() {
        let formatted = format_txt(&[line("Imported audio.", None, None, None, Some("Speaker 1"))]);
        assert_eq!(formatted.content, "Speaker 1: Imported audio.\n");
    }

    #[test]
    fn format_txt_collapses_internal_newlines_and_sorts_by_start() {
        let lines = vec![
            line("third", Some(3.0), Some(4.0), Some(1.0), None),
            line("line one\nline two", Some(1.0), Some(2.0), Some(1.0), None),
        ];
        let formatted = format_txt(&lines);
        assert_eq!(
            formatted.content,
            "[00:00:01] line one line two\n[00:00:03] third\n"
        );
    }

    #[test]
    fn format_txt_rolls_over_hours_and_skips_blank_text() {
        let lines = vec![
            line("   ", Some(1.0), Some(2.0), Some(1.0), None),
            line("late", Some(3725.0), Some(3730.0), Some(5.0), None),
        ];
        let formatted = format_txt(&lines);
        assert_eq!(formatted.content, "[01:02:05] late\n");
        assert_eq!(formatted.written, 1);
        assert_eq!(format_txt(&[]).content, "");
    }

    #[test]
    fn format_vtt_uses_a_voice_span_when_a_speaker_is_set() {
        let lines = vec![
            line(
                "We should ship the feature.",
                Some(83.0),
                Some(88.0),
                Some(5.0),
                Some("Noah"),
            ),
            line("Agreed.", Some(90.0), Some(92.4), Some(2.4), None),
        ];
        let formatted = format_vtt(&lines);
        assert_eq!(formatted.skipped, 0);
        assert_eq!(
            formatted.content,
            "WEBVTT\n\n\
             00:01:23.000 --> 00:01:28.000\n\
             <v Noah>We should ship the feature.\n\n\
             00:01:30.000 --> 00:01:32.400\n\
             Agreed.\n\n"
        );
    }

    #[test]
    fn format_vtt_falls_back_when_the_end_time_is_missing() {
        let duration = format_vtt(&[line("only duration", Some(10.0), None, Some(2.5), None)]);
        assert!(duration.content.contains("00:00:10.000 --> 00:00:12.500"));
        let fallback = format_vtt(&[line("naked start", Some(20.0), None, None, None)]);
        assert!(fallback.content.contains("00:00:20.000 --> 00:00:25.000"));
    }

    #[test]
    fn format_vtt_skips_untimed_text_and_escapes_markup() {
        let lines = vec![
            line(
                "3 < 5 & 7 > 6",
                Some(0.0),
                Some(1.0),
                Some(1.0),
                Some("A > B"),
            ),
            line("orphan", None, None, None, None),
        ];
        let formatted = format_vtt(&lines);
        assert_eq!(formatted.written, 1);
        assert_eq!(formatted.skipped, 1);
        assert!(formatted.content.contains("<v A B>3 &lt; 5 &amp; 7 &gt; 6"));
        assert!(!formatted.content.contains("orphan"));
    }

    #[test]
    fn format_vtt_keeps_overlapping_cues_and_rounds_milliseconds() {
        let lines = vec![
            line("mic talks", Some(0.0), Some(5.0), Some(5.0), None),
            line("system talks", Some(2.0), Some(6.0), Some(4.0), None),
            line("late", Some(1.9996), Some(2.0), Some(0.0004), None),
        ];
        let formatted = format_vtt(&lines);
        assert!(formatted
            .content
            .contains("00:00:00.000 --> 00:00:05.000\nmic talks"));
        assert!(formatted
            .content
            .contains("00:00:02.000 --> 00:00:06.000\nsystem talks"));
        assert!(formatted
            .content
            .contains("00:00:02.000 --> 00:00:02.001\nlate"));
    }

    #[test]
    fn format_vtt_bumps_a_cue_whose_end_is_not_after_its_start() {
        let formatted = format_vtt(&[line("same", Some(1.0), Some(1.0), Some(0.0), None)]);
        assert!(formatted.content.contains("00:00:01.000 --> 00:00:01.001"));
    }

    #[test]
    fn a_speaker_key_uses_the_saved_label() {
        let mut labels = BTreeMap::new();
        labels.insert("spk_0".to_string(), "Noah".to_string());
        let segment = TranscriptSegment {
            id: "t".into(),
            text: "hello".into(),
            timestamp: "ts".into(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(1.0),
            duration: Some(1.0),
            speaker: Some("spk_0".into()),
        };
        assert_eq!(
            TranscriptLine::from_segment(&segment, &labels)
                .speaker
                .as_deref(),
            Some("Noah")
        );

        let unlabeled = TranscriptSegment {
            speaker: Some("spk_1".into()),
            ..segment
        };
        assert_eq!(
            TranscriptLine::from_segment(&unlabeled, &BTreeMap::new())
                .speaker
                .as_deref(),
            Some("Speaker 2")
        );
    }

    #[test]
    fn transcript_files_are_replaced_and_leave_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let first = vec![line("first", Some(0.0), Some(1.0), Some(1.0), None)];
        write_transcript_files(dir.path(), &first).unwrap();
        write_transcript_files(
            dir.path(),
            &[line(
                "second",
                Some(1.0),
                Some(2.0),
                Some(1.0),
                Some("Noah"),
            )],
        )
        .unwrap();

        let txt = std::fs::read_to_string(dir.path().join(TRANSCRIPT_TXT_FILE)).unwrap();
        let vtt = std::fs::read_to_string(dir.path().join(TRANSCRIPT_VTT_FILE)).unwrap();
        assert_eq!(txt, "[00:00:01] Noah: second\n");
        assert!(vtt.contains("<v Noah>second"));
        assert!(txt.ends_with('\n'));
        assert!(vtt.ends_with('\n'));
        assert!(no_tmp_files(dir.path()));
    }

    #[test]
    fn sanitize_filename_stem_strips_path_characters_and_truncates() {
        assert_eq!(
            sanitize_filename_stem("Q3/Strategy: plan?"),
            "Q3 Strategy plan"
        );
        assert_eq!(sanitize_filename_stem("   "), "transcript");
        assert_eq!(sanitize_filename_stem(&"a".repeat(200)).len(), 80);
    }

    #[test]
    fn export_file_name_uses_the_title_and_meeting_date() {
        let created = DateTimeUtc(
            chrono::DateTime::parse_from_rfc3339("2026-10-15T11:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        assert_eq!(
            export_file_name("Team Standup", &created, "vtt"),
            "Team Standup - 2026-10-15.vtt"
        );
    }
}
