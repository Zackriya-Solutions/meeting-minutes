//! Reviewable meeting-type and collection classification.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct MeetingTypeSuggestion {
    pub id: i64,
    pub meeting_id: String,
    pub suggested_type: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionSuggestion {
    pub id: i64,
    pub meeting_id: String,
    pub collection_id: Option<i64>,
    pub suggested_name: String,
    pub suggestion_kind: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct LearningInboxItem {
    pub meeting_id: String,
    pub title: String,
    pub occurred_at: Option<String>,
    pub created_at: String,
    pub meeting_type: String,
    pub meeting_type_review_status: String,
    pub suggestion: Option<MeetingTypeSuggestion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMeetingTypeInput {
    pub suggestion_id: i64,
    pub status: String,
    #[serde(default)]
    pub corrected_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCollectionSuggestionInput {
    pub suggestion_id: i64,
    pub status: String,
}

fn valid_meeting_type(value: &str) -> bool {
    matches!(
        value,
        "uncertain"
            | "general"
            | "standup"
            | "planning"
            | "project_sync"
            | "one_on_one"
            | "interview"
            | "client_sync"
            | "technical_deep_dive"
    )
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

pub fn classify_text(
    title: &str,
    transcript: &str,
    memory_type: &str,
) -> (String, f64, Vec<String>) {
    let title = title.to_lowercase();
    let transcript = transcript.to_lowercase();
    let mut reasons = Vec::new();

    if memory_type == "interview" || contains_any(&title, &["интервью", "собесед", "interview"])
    {
        reasons.push("explicit_interview_context".to_string());
        return ("interview".to_string(), 0.98, reasons);
    }
    if contains_any(
        &title,
        &[
            "1:1",
            "1-to-1",
            "one-on-one",
            "one on one",
            "один на один",
            "личная встреча",
        ],
    ) {
        reasons.push("explicit_one_on_one_title".to_string());
        return ("one_on_one".to_string(), 0.96, reasons);
    }
    if contains_any(
        &title,
        &["planning", "планирован", "sprint plan", "план спринта"],
    ) || contains_any(
        &transcript,
        &["план спринта", "оценим задачи", "sprint planning"],
    ) {
        reasons.push("planning_language".to_string());
        return ("planning".to_string(), 0.90, reasons);
    }
    let standup_title = contains_any(&title, &["standup", "stand-up", "стендап", "daily"]);
    let status_round = contains_any(
        &transcript,
        &[
            "что делал вчера",
            "что сделаю сегодня",
            "какие блокеры",
            "вчера сделал",
            "сегодня буду",
            "yesterday i",
            "today i",
            "my blocker",
        ],
    );
    if standup_title && status_round {
        reasons.push("standup_title".to_string());
        reasons.push("status_round_language".to_string());
        return ("standup".to_string(), 0.94, reasons);
    }
    if status_round {
        reasons.push("status_round_language".to_string());
        return ("standup".to_string(), 0.82, reasons);
    }
    if contains_any(&title, &["client", "customer", "клиент", "заказчик"])
        || contains_any(
            &transcript,
            &["клиент попросил", "заказчик", "customer asked"],
        )
    {
        reasons.push("client_context".to_string());
        return ("client_sync".to_string(), 0.82, reasons);
    }
    if contains_any(
        &title,
        &[
            "deep dive",
            "технический разбор",
            "архитектурный разбор",
            "tech talk",
        ],
    ) {
        reasons.push("technical_deep_dive_title".to_string());
        return ("technical_deep_dive".to_string(), 0.88, reasons);
    }
    if contains_any(&title, &["sync", "синк", "status", "статус", "project"])
        || contains_any(
            &transcript,
            &["статус проекта", "по проекту", "project status"],
        )
    {
        reasons.push("project_sync_language".to_string());
        return ("project_sync".to_string(), 0.76, reasons);
    }
    if transcript.chars().count() >= 300 {
        reasons.push("substantial_meeting_transcript".to_string());
        return ("general".to_string(), 0.62, reasons);
    }
    reasons.push("insufficient_evidence".to_string());
    ("uncertain".to_string(), 0.25, reasons)
}

pub async fn ensure_inbox(pool: &SqlitePool) -> Result<i64, String> {
    if let Some(id) =
        sqlx::query_scalar::<_, i64>("SELECT id FROM collections WHERE system_key='inbox' LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Failed to locate Inbox: {error}"))?
    {
        return Ok(id);
    }
    for suffix in 0..100 {
        let name = if suffix == 0 {
            "Memento Inbox".to_string()
        } else {
            format!("Memento Inbox ({suffix})")
        };
        let inserted = sqlx::query_scalar::<_, i64>(
            "INSERT INTO collections(name, kind, is_system, system_key) \
             VALUES(?, 'manual', 1, 'inbox') \
             ON CONFLICT(name) DO NOTHING RETURNING id",
        )
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("Failed to create Inbox: {error}"))?;
        if let Some(id) = inserted {
            return Ok(id);
        }
    }
    Err("Could not allocate a system Inbox collection".to_string())
}

pub async fn add_to_inbox(pool: &SqlitePool, meeting_id: &str) -> Result<i64, String> {
    let inbox_id = ensure_inbox(pool).await?;
    sqlx::query(
        "INSERT INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?) \
         ON CONFLICT(meeting_id, collection_id) DO NOTHING",
    )
    .bind(meeting_id)
    .bind(inbox_id)
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to add meeting to Inbox: {error}"))?;
    Ok(inbox_id)
}

pub async fn suggest_meeting_type(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<MeetingTypeSuggestion, String> {
    let row = sqlx::query("SELECT title, memory_type FROM meetings WHERE id=?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("Failed to load meeting for classification: {error}"))?
        .ok_or_else(|| format!("Meeting {meeting_id} not found"))?;
    let title: String = row.get("title");
    let memory_type: String = row.get("memory_type");
    let transcript: String = sqlx::query_scalar(
        "SELECT COALESCE(group_concat(transcript, ' '), '') FROM transcripts WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to load transcript for classification: {error}"))?;
    let (suggested_type, confidence, reasons) = classify_text(&title, &transcript, &memory_type);
    sqlx::query(
        "UPDATE meeting_type_suggestions SET status='superseded' \
         WHERE meeting_id=? AND status='pending'",
    )
    .bind(meeting_id)
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to supersede old classification: {error}"))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO meeting_type_suggestions( \
            meeting_id, suggested_type, confidence, explanation_json, model_version \
         ) VALUES(?, ?, ?, ?, 'deterministic_meeting_type_v1') RETURNING id",
    )
    .bind(meeting_id)
    .bind(&suggested_type)
    .bind(confidence)
    .bind(serde_json::to_string(&reasons).unwrap_or_else(|_| "[]".to_string()))
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to store classification suggestion: {error}"))?;
    Ok(MeetingTypeSuggestion {
        id,
        meeting_id: meeting_id.to_string(),
        suggested_type,
        confidence,
        reasons,
        status: "pending".to_string(),
    })
}

pub async fn current_meeting_type_suggestion(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Option<MeetingTypeSuggestion>, String> {
    let review_status: String =
        sqlx::query_scalar("SELECT meeting_type_review_status FROM meetings WHERE id=?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Failed to load meeting review status: {error}"))?
            .ok_or_else(|| format!("Meeting {meeting_id} not found"))?;
    let row = sqlx::query(
        "SELECT id, suggested_type, confidence, explanation_json, status \
         FROM meeting_type_suggestions WHERE meeting_id=? AND status='pending' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load current classification: {error}"))?;
    if let Some(row) = row {
        return Ok(Some(MeetingTypeSuggestion {
            id: row.get("id"),
            meeting_id: meeting_id.to_string(),
            suggested_type: row.get("suggested_type"),
            confidence: row.get("confidence"),
            reasons: serde_json::from_str(&row.get::<String, _>("explanation_json"))
                .unwrap_or_default(),
            status: row.get("status"),
        }));
    }
    if review_status != "pending" {
        return Ok(None);
    }
    suggest_meeting_type(pool, meeting_id).await.map(Some)
}

async fn suggest_series(pool: &SqlitePool, meeting_id: &str) -> Result<(), String> {
    let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id=?")
        .bind(meeting_id)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("Failed to load meeting title: {error}"))?;
    let series: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, match_rule FROM collections \
         WHERE kind='series' AND auto_add=1 AND match_rule IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load recurring series: {error}"))?;
    sqlx::query(
        "UPDATE collection_suggestions SET status='superseded' \
         WHERE meeting_id=? AND status='pending'",
    )
    .bind(meeting_id)
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to supersede collection suggestions: {error}"))?;
    for (collection_id, name, rule) in series {
        if !crate::collections::series_title_matches(&rule, &title) {
            continue;
        }
        sqlx::query(
            "INSERT INTO collection_suggestions( \
                meeting_id, collection_id, suggested_name, suggestion_kind, confidence, \
                explanation_json \
             ) VALUES(?, ?, ?, 'series', 0.90, ?)",
        )
        .bind(meeting_id)
        .bind(collection_id)
        .bind(name)
        .bind(json!(["confirmed_series_rule", "normalized_title_match"]).to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to store series suggestion: {error}"))?;
    }
    Ok(())
}

pub async fn list_collection_suggestions(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<CollectionSuggestion>, String> {
    let rows = sqlx::query(
        "SELECT id, meeting_id, collection_id, suggested_name, suggestion_kind, \
                confidence, explanation_json, status \
         FROM collection_suggestions WHERE meeting_id=? AND status='pending' \
         ORDER BY confidence DESC, id DESC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load collection suggestions: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| CollectionSuggestion {
            id: row.get("id"),
            meeting_id: row.get("meeting_id"),
            collection_id: row.get("collection_id"),
            suggested_name: row.get("suggested_name"),
            suggestion_kind: row.get("suggestion_kind"),
            confidence: row.get("confidence"),
            reasons: serde_json::from_str(&row.get::<String, _>("explanation_json"))
                .unwrap_or_default(),
            status: row.get("status"),
        })
        .collect())
}

pub async fn prepare_saved_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<(), String> {
    add_to_inbox(pool, meeting_id).await?;
    suggest_meeting_type(pool, meeting_id).await?;
    suggest_series(pool, meeting_id).await?;
    Ok(())
}

pub async fn list_inbox(pool: &SqlitePool) -> Result<Vec<LearningInboxItem>, String> {
    let inbox_id = ensure_inbox(pool).await?;
    let rows = sqlx::query(
        "SELECT m.id, m.title, m.occurred_at, m.created_at, m.meeting_type, \
                m.meeting_type_review_status, mts.id AS suggestion_id, \
                mts.suggested_type, mts.confidence, mts.explanation_json, mts.status \
         FROM meeting_collections mc JOIN meetings m ON m.id=mc.meeting_id \
         LEFT JOIN meeting_type_suggestions mts ON mts.id=( \
             SELECT id FROM meeting_type_suggestions WHERE meeting_id=m.id \
             ORDER BY id DESC LIMIT 1 \
         ) WHERE mc.collection_id=? ORDER BY COALESCE(m.occurred_at, m.created_at) DESC",
    )
    .bind(inbox_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load Inbox: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let suggestion_id: Option<i64> = row.get("suggestion_id");
            LearningInboxItem {
                meeting_id: row.get("id"),
                title: row.get("title"),
                occurred_at: row.get("occurred_at"),
                created_at: row.get("created_at"),
                meeting_type: row.get("meeting_type"),
                meeting_type_review_status: row.get("meeting_type_review_status"),
                suggestion: suggestion_id.map(|id| MeetingTypeSuggestion {
                    id,
                    meeting_id: row.get("id"),
                    suggested_type: row.get("suggested_type"),
                    confidence: row.get("confidence"),
                    reasons: serde_json::from_str(&row.get::<String, _>("explanation_json"))
                        .unwrap_or_default(),
                    status: row.get("status"),
                }),
            }
        })
        .collect())
}

pub async fn review_meeting_type(
    pool: &SqlitePool,
    input: ReviewMeetingTypeInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "accepted" | "rejected") {
        return Err("status must be accepted or rejected".to_string());
    }
    let row = sqlx::query(
        "SELECT meeting_id, suggested_type, confidence FROM meeting_type_suggestions WHERE id=?",
    )
    .bind(input.suggestion_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load meeting-type suggestion: {error}"))?
    .ok_or_else(|| format!("Suggestion {} not found", input.suggestion_id))?;
    let meeting_id: String = row.get("meeting_id");
    let suggested: String = row.get("suggested_type");
    let confidence: f64 = row.get("confidence");
    let selected = input.corrected_type.as_deref().unwrap_or(&suggested);
    if !valid_meeting_type(selected) {
        return Err(format!("Unsupported meeting type: {selected}"));
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin classification review: {error}"))?;
    sqlx::query(
        "UPDATE meeting_type_suggestions SET status=?, reviewed_at=datetime('now') WHERE id=?",
    )
    .bind(&input.status)
    .bind(input.suggestion_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to review classification: {error}"))?;
    if input.status == "accepted" {
        let memory_type = match selected {
            "standup" => "standup",
            "interview" => "interview",
            _ => "general",
        };
        let applied_confidence = (selected == suggested).then_some(confidence);
        sqlx::query(
            "UPDATE meetings SET meeting_type=?, meeting_type_confidence=?, \
                    meeting_type_review_status='accepted', memory_type=? WHERE id=?",
        )
        .bind(selected)
        .bind(applied_confidence)
        .bind(memory_type)
        .bind(&meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to apply meeting classification: {error}"))?;
        let inbox_id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM collections WHERE system_key='inbox'")
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| format!("Failed to locate Inbox: {error}"))?;
        if let Some(inbox_id) = inbox_id {
            sqlx::query("DELETE FROM meeting_collections WHERE meeting_id=? AND collection_id=?")
                .bind(&meeting_id)
                .bind(inbox_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    format!("Failed to remove classified meeting from Inbox: {error}")
                })?;
        }
    } else {
        sqlx::query(
            "UPDATE meetings SET meeting_type='uncertain', meeting_type_confidence=NULL, \
                    meeting_type_review_status='rejected' WHERE id=?",
        )
        .bind(&meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to reject meeting classification: {error}"))?;
    }
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, \
            trust_tier, scope, payload_json \
         ) VALUES(?, ?, 'meeting_type_reviewed', 'meeting', ?, 'user', 'trusted', \
                  'meeting', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&meeting_id)
    .bind(&meeting_id)
    .bind(json!({"status": input.status, "selected_type": selected}).to_string())
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to append classification event: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit classification review: {error}"))?;
    Ok(())
}

pub async fn review_collection_suggestion(
    pool: &SqlitePool,
    input: ReviewCollectionSuggestionInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "accepted" | "rejected") {
        return Err("status must be accepted or rejected".to_string());
    }
    let row =
        sqlx::query("SELECT meeting_id, collection_id FROM collection_suggestions WHERE id=?")
            .bind(input.suggestion_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Failed to load collection suggestion: {error}"))?
            .ok_or_else(|| format!("Collection suggestion {} not found", input.suggestion_id))?;
    let meeting_id: String = row.get("meeting_id");
    let collection_id: Option<i64> = row.get("collection_id");
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin collection review: {error}"))?;
    sqlx::query(
        "UPDATE collection_suggestions SET status=?, reviewed_at=datetime('now') WHERE id=?",
    )
    .bind(&input.status)
    .bind(input.suggestion_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to review collection suggestion: {error}"))?;
    if input.status == "accepted" {
        let collection_id = collection_id.ok_or("Suggested collection no longer exists")?;
        sqlx::query(
            "INSERT INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?) \
             ON CONFLICT(meeting_id, collection_id) DO NOTHING",
        )
        .bind(&meeting_id)
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to add meeting to collection: {error}"))?;
    }
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, \
            trust_tier, scope, payload_json \
         ) VALUES(?, ?, 'collection_reviewed', 'collection_suggestion', ?, 'user', \
                  'trusted', 'meeting', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&meeting_id)
    .bind(input.suggestion_id.to_string())
    .bind(json!({"status": input.status, "collection_id": collection_id}).to_string())
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to append collection review event: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit collection review: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn classify_meeting(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<MeetingTypeSuggestion, String> {
    suggest_meeting_type(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn get_meeting_classification_review(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<Option<MeetingTypeSuggestion>, String> {
    current_meeting_type_suggestion(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn list_learning_inbox(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<LearningInboxItem>, String> {
    list_inbox(state.db_manager.pool()).await
}

#[tauri::command]
pub async fn get_collection_classification_review(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<Vec<CollectionSuggestion>, String> {
    list_collection_suggestions(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn review_meeting_classification(
    state: tauri::State<'_, crate::state::AppState>,
    input: ReviewMeetingTypeInput,
) -> Result<(), String> {
    review_meeting_type(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn review_collection_classification(
    state: tauri::State<'_, crate::state::AppState>,
    input: ReviewCollectionSuggestionInput,
) -> Result<(), String> {
    review_collection_suggestion(state.db_manager.pool(), input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standup_requires_status_round_not_title_alone() {
        let (kind, confidence, _) =
            classify_text("Daily standup", "обсудили релиз и бюджет", "general");
        assert_ne!(kind, "standup");
        assert!(confidence < 0.9);
        let (kind, confidence, _) = classify_text(
            "Daily standup",
            "что делал вчера что сделаю сегодня какие блокеры",
            "general",
        );
        assert_eq!(kind, "standup");
        assert!(confidence >= 0.9);
    }

    #[test]
    fn explicit_specialized_types_win() {
        assert_eq!(classify_text("1:1 Anna", "", "general").0, "one_on_one");
        assert_eq!(
            classify_text("Architecture interview", "", "interview").0,
            "interview"
        );
    }
}
