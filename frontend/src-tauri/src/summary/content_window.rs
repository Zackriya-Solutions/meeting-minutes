//! Conservative, local-only detection of sparse transcript fragments after a meeting.
//!
//! Transcript rows are never deleted. A detected primary window is only used for summary
//! generation after explicit user confirmation, and the preference can be reset at any time.

use crate::state::AppState;
use serde::Serialize;
use sqlx::SqlitePool;

const LONG_GAP_MS: i64 = 10 * 60 * 1_000;
const MIN_PRIMARY_SEGMENTS: usize = 5;
const MIN_PRIMARY_CHARS: usize = 400;
const MAX_TRAILING_SEGMENTS: usize = 12;
const MAX_TRAILING_CHARS_ABSOLUTE: usize = 300;

#[derive(Debug, Clone)]
struct TimedSegment {
    start_ms: i64,
    end_ms: i64,
    text_chars: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MeetingContentWindowSuggestion {
    pub suggested: bool,
    pub selected: bool,
    pub primary_start_ms: Option<i64>,
    pub primary_end_ms: Option<i64>,
    pub excluded_segment_count: usize,
    pub gap_ms: Option<i64>,
    pub excluded_text_ratio: Option<f64>,
    pub confidence: Option<String>,
    pub reason: Option<String>,
}

fn analyze_segments(mut segments: Vec<TimedSegment>) -> MeetingContentWindowSuggestion {
    segments.sort_by_key(|segment| segment.start_ms);
    if segments.len() < MIN_PRIMARY_SEGMENTS + 1 {
        return MeetingContentWindowSuggestion::default();
    }

    let split_at = segments
        .windows(2)
        .position(|pair| pair[1].start_ms.saturating_sub(pair[0].end_ms) >= LONG_GAP_MS);
    let Some(split_at) = split_at.map(|index| index + 1) else {
        return MeetingContentWindowSuggestion::default();
    };
    let (primary, trailing) = segments.split_at(split_at);
    let primary_chars = primary
        .iter()
        .map(|segment| segment.text_chars)
        .sum::<usize>();
    let trailing_chars = trailing
        .iter()
        .map(|segment| segment.text_chars)
        .sum::<usize>();
    if primary.len() < MIN_PRIMARY_SEGMENTS
        || primary_chars < MIN_PRIMARY_CHARS
        || trailing.is_empty()
        || trailing.len() > MAX_TRAILING_SEGMENTS
    {
        return MeetingContentWindowSuggestion::default();
    }

    if trailing_chars > MAX_TRAILING_CHARS_ABSOLUTE
        || trailing_chars.saturating_mul(10) > primary_chars
    {
        return MeetingContentWindowSuggestion::default();
    }

    let primary_start_ms = primary.first().map(|segment| segment.start_ms);
    let primary_end_ms = primary.last().map(|segment| segment.end_ms);
    let gap_ms = match (primary_end_ms, trailing.first()) {
        (Some(end), Some(first)) => Some(first.start_ms.saturating_sub(end)),
        _ => None,
    };
    let ratio = trailing_chars as f64 / primary_chars as f64;
    MeetingContentWindowSuggestion {
        suggested: true,
        selected: false,
        primary_start_ms,
        primary_end_ms,
        excluded_segment_count: trailing.len(),
        gap_ms,
        excluded_text_ratio: Some(ratio),
        confidence: Some(if ratio <= 0.05 { "high" } else { "medium" }.to_string()),
        reason: Some("long_trailing_gap_and_sparse_fragments".to_string()),
    }
}

fn preference_key(meeting_id: &str) -> String {
    format!("summary.content_window.{meeting_id}")
}

async fn preference_selected(pool: &SqlitePool, meeting_id: &str) -> Result<bool, String> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings_kv WHERE key = ?")
            .bind(preference_key(meeting_id))
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    Ok(value.as_deref() == Some("primary"))
}

async fn analyze_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<MeetingContentWindowSuggestion, String> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meetings WHERE id = ?)")
        .bind(meeting_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    if !exists {
        return Err("Meeting not found".to_string());
    }
    let rows: Vec<(Option<f64>, Option<f64>, String)> = sqlx::query_as(
        "SELECT audio_start_time, audio_end_time, transcript FROM transcripts \
         WHERE meeting_id = ? AND audio_start_time IS NOT NULL AND trim(transcript) != '' \
         ORDER BY audio_start_time, timestamp, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let segments = rows
        .into_iter()
        .filter_map(|(start, end, text)| {
            let start_ms = (start? * 1_000.0).round() as i64;
            let end_ms = (end.unwrap_or(start_ms as f64 / 1_000.0) * 1_000.0).round() as i64;
            Some(TimedSegment {
                start_ms,
                end_ms: end_ms.max(start_ms),
                text_chars: text.trim().chars().count(),
            })
        })
        .collect();
    let mut result = analyze_segments(segments);
    result.selected = result.suggested && preference_selected(pool, meeting_id).await?;
    Ok(result)
}

#[tauri::command]
pub async fn get_meeting_content_window_suggestion(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingContentWindowSuggestion, String> {
    analyze_meeting(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn set_meeting_content_window_preference(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    use_primary: bool,
) -> Result<MeetingContentWindowSuggestion, String> {
    let pool = state.db_manager.pool();
    let suggestion = analyze_meeting(pool, &meeting_id).await?;
    let key = preference_key(&meeting_id);
    if use_primary {
        if !suggestion.suggested {
            return Err("No safe primary content window is available".to_string());
        }
        sqlx::query(
            "INSERT INTO app_settings_kv(key, value, updated_at) VALUES(?, 'primary', datetime('now')) \
             ON CONFLICT(key) DO UPDATE SET value='primary', updated_at=datetime('now')",
        )
        .bind(&key)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query("DELETE FROM app_settings_kv WHERE key = ?")
            .bind(&key)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    analyze_meeting(pool, &meeting_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start_minutes: i64, end_minutes: i64, text_chars: usize) -> TimedSegment {
        TimedSegment {
            start_ms: start_minutes * 60_000,
            end_ms: end_minutes * 60_000,
            text_chars,
        }
    }

    #[test]
    fn suggests_only_sparse_fragments_after_a_long_gap() {
        let mut segments = (0..10)
            .map(|index| segment(index, index + 1, 100))
            .collect::<Vec<_>>();
        segments.extend([segment(31, 32, 20), segment(36, 37, 15)]);

        let result = analyze_segments(segments);
        assert!(result.suggested);
        assert_eq!(result.primary_end_ms, Some(10 * 60_000));
        assert_eq!(result.excluded_segment_count, 2);
        assert_eq!(result.gap_ms, Some(21 * 60_000));
        assert_eq!(result.confidence.as_deref(), Some("high"));
    }

    #[test]
    fn preserves_a_substantial_second_content_window() {
        let mut segments = (0..10)
            .map(|index| segment(index, index + 1, 100))
            .collect::<Vec<_>>();
        segments.extend((31..38).map(|index| segment(index, index + 1, 100)));
        assert!(!analyze_segments(segments).suggested);
    }

    #[test]
    fn preserves_a_short_but_relatively_substantial_second_window() {
        let mut segments = (0..5)
            .map(|index| segment(index, index + 1, 100))
            .collect::<Vec<_>>();
        segments.extend([segment(31, 32, 30), segment(33, 34, 30)]);
        assert!(!analyze_segments(segments).suggested);
    }

    #[test]
    fn preserves_continuous_transcripts() {
        let segments = (0..12)
            .map(|index| segment(index, index + 1, 100))
            .collect::<Vec<_>>();
        assert!(!analyze_segments(segments).suggested);
    }
}
