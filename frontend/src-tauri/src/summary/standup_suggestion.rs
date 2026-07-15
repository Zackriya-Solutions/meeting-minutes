//! Local, explainable summary-template suggestions.
//!
//! Suggestions never select a template or call a model. They combine conservative meeting
//! metadata, transcript-language categories, and user-reviewed history from the same series.

use crate::state::AppState;
use serde::Serialize;
use sqlx::SqlitePool;

const STANDUP_TEMPLATE: &str = "daily_standup";
const STANDARD_TEMPLATE: &str = "standard_meeting";

#[derive(Debug, Clone, Default)]
struct SuggestionSignals {
    standup_title: bool,
    other_meeting_title: bool,
    reviewed_series_history: bool,
    transcript_available: bool,
    status_categories: usize,
    status_round_handoff: bool,
    standup_time: bool,
    standup_duration: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSuggestion {
    pub template_id: String,
    pub confidence: String,
    pub score: i32,
    pub reasons: Vec<String>,
}

fn contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

fn hour_minute_is_standup_like(hour: u8, minute: u8) -> bool {
    ((hour, minute) >= (10, 30)) && ((hour, minute) <= (12, 0))
}

fn time_is_standup_like(value: &str) -> bool {
    value.split_once('T').is_some_and(|(_, time)| {
        let hour = time.get(0..2).and_then(|value| value.parse::<u8>().ok());
        let minute = time.get(3..5).and_then(|value| value.parse::<u8>().ok());
        hour.zip(minute)
            .is_some_and(|(hour, minute)| hour_minute_is_standup_like(hour, minute))
    })
}

fn title_time_is_standup_like(title: &str) -> bool {
    title.split('_').any(|part| {
        let hour = part.get(0..2).and_then(|value| value.parse::<u8>().ok());
        let minute = part.get(3..5).and_then(|value| value.parse::<u8>().ok());
        (part.as_bytes().get(2) == Some(&b'-'))
            && hour
                .zip(minute)
                .is_some_and(|(hour, minute)| hour_minute_is_standup_like(hour, minute))
    })
}

fn transcript_status_categories(transcript: &str) -> usize {
    let transcript = transcript.to_lowercase();
    [
        contains_any(
            &transcript,
            &[
                "сделал",
                "сделали",
                "готово",
                "завершил",
                "completed",
                "finished",
            ],
        ),
        contains_any(
            &transcript,
            &["сегодня", "дальше", "планирую", "буду", "next", "today"],
        ),
        contains_any(
            &transcript,
            &["блокер", "заблокирован", "мешает", "blocker", "blocked"],
        ),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

fn transcript_has_status_round_handoff(transcript: &str) -> bool {
    let transcript = transcript.to_lowercase();
    contains_any(
        &transcript,
        &[
            "идём дальше",
            "идем дальше",
            "поехали дальше",
            "давай с тебя",
            "рассказывай дальше",
            "следующий участник",
            "next up",
            "over to you",
        ],
    )
}

fn suggest_from_signals(signals: SuggestionSignals) -> TemplateSuggestion {
    let mut score = 0;
    let mut reasons = Vec::new();
    if signals.standup_title {
        score += 6;
        reasons.push("standup_title".to_string());
    }
    if signals.reviewed_series_history {
        score += 4;
        reasons.push("reviewed_series_history".to_string());
    }
    if signals.status_categories > 0 {
        score += signals.status_categories as i32;
        reasons.push("status_round_language".to_string());
    }
    if signals.status_round_handoff {
        score += 3;
        reasons.push("status_round_handoff".to_string());
    }
    if signals.standup_time {
        score += 1;
        reasons.push("standup_time_window".to_string());
    }
    if signals.standup_duration {
        score += 1;
        reasons.push("standup_duration".to_string());
    }
    if signals.other_meeting_title {
        score -= 8;
        reasons.push("other_meeting_title".to_string());
    }

    let transcript_supports_standup = signals.status_categories > 0 && signals.status_round_handoff;
    let pre_meeting_supports_standup =
        !signals.transcript_available && (signals.standup_title || signals.reviewed_series_history);
    let is_standup = !signals.other_meeting_title
        && score >= 4
        && (transcript_supports_standup || pre_meeting_supports_standup);
    TemplateSuggestion {
        template_id: if is_standup {
            STANDUP_TEMPLATE
        } else {
            STANDARD_TEMPLATE
        }
        .to_string(),
        confidence: if is_standup && score >= 6 || !is_standup && signals.other_meeting_title {
            "high"
        } else if is_standup {
            "medium"
        } else {
            "low"
        }
        .to_string(),
        score,
        reasons,
    }
}

async fn suggestion_for_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<TemplateSuggestion, String> {
    let meeting: Option<(String, String)> = sqlx::query_as(
        "SELECT title, COALESCE(occurred_at, created_at) FROM meetings WHERE id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (title, occurred_at) = meeting.ok_or_else(|| format!("Meeting not found: {meeting_id}"))?;
    let title = title.to_lowercase();

    let transcript: Vec<String> = sqlx::query_scalar(
        "SELECT transcript FROM transcripts WHERE meeting_id = ? AND trim(transcript) != ''",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let duration: Option<f64> =
        sqlx::query_scalar("SELECT MAX(audio_end_time) FROM transcripts WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
    let reviewed_series_history: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
             SELECT 1 FROM meeting_collections current \
             JOIN collections c ON c.id = current.collection_id AND c.kind = 'series' \
             JOIN meeting_collections related ON related.collection_id = c.id \
             JOIN standup_records sr ON sr.meeting_id = related.meeting_id \
              AND sr.review_status = 'accepted' \
             WHERE current.meeting_id = ? AND related.meeting_id != ? \
         )",
    )
    .bind(meeting_id)
    .bind(meeting_id)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;

    let transcript = transcript.join("\n");
    Ok(suggest_from_signals(SuggestionSignals {
        standup_title: contains_any(
            &title,
            &["standup", "стендап", "daily", "дейли", "ministandup"],
        ),
        other_meeting_title: contains_any(
            &title,
            &[
                "one-to-one",
                "1:1",
                "planning",
                "планир",
                "retro",
                "ретро",
                "interview",
                "собесед",
            ],
        ),
        reviewed_series_history,
        transcript_available: !transcript.trim().is_empty(),
        status_categories: transcript_status_categories(&transcript),
        status_round_handoff: transcript_has_status_round_handoff(&transcript),
        standup_time: time_is_standup_like(&occurred_at) || title_time_is_standup_like(&title),
        standup_duration: duration.is_some_and(|seconds| (300.0..=2_700.0).contains(&seconds)),
    }))
}

#[tauri::command]
pub async fn suggest_summary_template(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<TemplateSuggestion, String> {
    let meeting_id = meeting_id.trim();
    if meeting_id.is_empty() {
        return Err("Meeting ID is required".to_string());
    }
    suggestion_for_meeting(state.db_manager.pool(), meeting_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_title_suggests_standup_before_transcript_without_automatic_selection() {
        let suggestion = suggest_from_signals(SuggestionSignals {
            standup_title: true,
            ..Default::default()
        });
        assert_eq!(suggestion.template_id, STANDUP_TEMPLATE);
        assert_eq!(suggestion.confidence, "high");
        assert!(suggestion.reasons.contains(&"standup_title".to_string()));
    }

    #[test]
    fn misleading_title_without_status_round_stays_standard() {
        let suggestion = suggest_from_signals(SuggestionSignals {
            standup_title: true,
            transcript_available: true,
            status_categories: 2,
            ..Default::default()
        });
        assert_eq!(suggestion.template_id, STANDARD_TEMPLATE);
        assert_eq!(suggestion.confidence, "low");
    }

    #[test]
    fn weak_signals_need_more_than_time_and_duration() {
        let suggestion = suggest_from_signals(SuggestionSignals {
            standup_time: true,
            standup_duration: true,
            ..Default::default()
        });
        assert_eq!(suggestion.template_id, STANDARD_TEMPLATE);
        assert_eq!(suggestion.confidence, "low");
    }

    #[test]
    fn explicit_other_meeting_title_suppresses_status_markers() {
        let suggestion = suggest_from_signals(SuggestionSignals {
            other_meeting_title: true,
            reviewed_series_history: true,
            status_categories: 3,
            standup_time: true,
            standup_duration: true,
            ..Default::default()
        });
        assert_eq!(suggestion.template_id, STANDARD_TEMPLATE);
        assert_eq!(suggestion.confidence, "high");
    }

    #[test]
    fn detects_status_categories_and_safe_time_window() {
        assert_eq!(
            transcript_status_categories("Вчера завершил. Сегодня буду дальше. Блокеров нет."),
            3
        );
        assert!(transcript_has_status_round_handoff(
            "Спасибо, идём дальше. Макс, давай с тебя."
        ));
        assert!(!transcript_has_status_round_handoff(
            "Обсудили обратную связь и атмосферу в команде."
        ));
        assert!(time_is_standup_like("2026-07-15T11:05:00"));
        assert!(!time_is_standup_like("2026-07-15T17:35:00"));
        assert!(!time_is_standup_like("2026-07-15T12:30:00"));
        assert!(title_time_is_standup_like(
            "date-unknown_mon_11-04_standup_assistant"
        ));
    }

    #[tokio::test]
    async fn reviewed_series_history_is_read_locally() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, occurred_at TEXT)",
            "CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_end_time REAL)",
            "CREATE TABLE collections(id INTEGER PRIMARY KEY, kind TEXT)",
            "CREATE TABLE meeting_collections(meeting_id TEXT, collection_id INTEGER)",
            "CREATE TABLE standup_records(meeting_id TEXT, review_status TEXT)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO meetings VALUES \
             ('current', 'Team sync', '2026-07-15T11:00:00', NULL), \
             ('previous', 'Earlier', '2026-07-14T11:00:00', NULL); \
             INSERT INTO transcripts VALUES ('current', 'Сегодня закончил задачу. Идём дальше.', 900); \
             INSERT INTO collections VALUES (1, 'series'); \
             INSERT INTO meeting_collections VALUES ('current', 1), ('previous', 1); \
             INSERT INTO standup_records VALUES ('previous', 'accepted')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let suggestion = suggestion_for_meeting(&pool, "current").await.unwrap();
        assert_eq!(suggestion.template_id, STANDUP_TEMPLATE);
        assert!(suggestion
            .reasons
            .contains(&"reviewed_series_history".to_string()));
    }
}
