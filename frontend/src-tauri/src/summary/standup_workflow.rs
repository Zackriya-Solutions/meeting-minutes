//! Review, action lifecycle, and recurring-series context for Standup V2.

use crate::state::AppState;
use crate::summary::standup::{parse_timestamp_seconds, StandupReport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

const MAX_EDIT_CHARS: usize = 4_000;
const MAX_OWNER_CHARS: usize = 200;
const MAX_DUE_DATE_CHARS: usize = 100;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StandupRecordRow {
    pub id: i64,
    pub meeting_id: String,
    pub kind: String,
    #[sqlx(json)]
    pub payload: Value,
    #[sqlx(json(nullable))]
    pub reviewed_payload: Option<Value>,
    pub review_status: String,
    pub source_schema_version: String,
    pub action_item_id: Option<i64>,
    pub action_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStandupRecordInput {
    pub record_id: i64,
    pub status: String,
    #[serde(default)]
    pub edited_text: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrebriefAction {
    pub id: i64,
    pub text: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub status: String,
    pub source_meeting_id: String,
    pub source_meeting_title: String,
    pub source_start_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrebriefFact {
    pub record_id: i64,
    pub kind: String,
    pub text: String,
    pub source_meeting_id: String,
    pub source_meeting_title: String,
    pub source_start_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StandupPrebrief {
    pub series: Vec<String>,
    pub open_actions: Vec<PrebriefAction>,
    pub recent_risks: Vec<PrebriefFact>,
    pub recent_decisions: Vec<PrebriefFact>,
}

fn normalized_identity(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn evidence_timestamps(payload: &Value) -> String {
    payload
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("timestamp").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(",")
}

fn record_key(kind: &str, payload: &Value) -> String {
    let identity = match kind {
        "overview" | "participant_update" | "unattributed_fact" => payload
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "decision" => payload
            .get("decision")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "action" => payload
            .get("task")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "risk" => payload
            .get("blocker_or_risk")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "deep_dive" => payload
            .get("topic")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        _ => "",
    };
    let participant = payload
        .get("participant")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let category = payload
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let owner = payload
        .get("owner")
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!(
        "{kind}|{}|{}|{}|{}|{}",
        normalized_identity(participant),
        category,
        normalized_identity(owner),
        evidence_timestamps(payload),
        normalized_identity(identity)
    )
}

fn flatten_report(report: &StandupReport) -> Result<Vec<(String, Value)>, serde_json::Error> {
    let mut records = Vec::new();
    for item in &report.overview {
        records.push(("overview".to_string(), serde_json::to_value(item)?));
    }
    for update in &report.participant_updates {
        for (category, items) in [
            ("completed_or_recent", &update.completed_or_recent),
            ("next", &update.next),
            ("blockers", &update.blockers),
        ] {
            for item in items {
                records.push((
                    "participant_update".to_string(),
                    json!({
                        "participant": update.participant,
                        "category": category,
                        "text": item.text,
                        "evidence": item.evidence,
                    }),
                ));
            }
        }
    }
    for item in &report.decisions {
        records.push(("decision".to_string(), serde_json::to_value(item)?));
    }
    for item in &report.action_items {
        records.push(("action".to_string(), serde_json::to_value(item)?));
    }
    for item in &report.risks_and_blockers {
        records.push(("risk".to_string(), serde_json::to_value(item)?));
    }
    for item in &report.deep_dives {
        records.push(("deep_dive".to_string(), serde_json::to_value(item)?));
    }
    for item in &report.unattributed_facts {
        records.push(("unattributed_fact".to_string(), serde_json::to_value(item)?));
    }
    Ok(records)
}

/// Replace only unreviewed generated records. Accepted/rejected records are an audit trail and
/// survive regeneration even if the model phrases or omits the fact differently next time.
pub async fn sync_standup_records(
    pool: &SqlitePool,
    meeting_id: &str,
    report: &StandupReport,
) -> anyhow::Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM standup_records WHERE meeting_id = ? AND review_status = 'pending'")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;

    let mut seen = HashSet::new();
    let mut inserted = 0;
    for (kind, payload) in flatten_report(report)? {
        let key = record_key(&kind, &payload);
        if !seen.insert(key.clone()) {
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO standup_records \
             (meeting_id, record_key, kind, payload, source_schema_version) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(meeting_id, record_key) DO NOTHING",
        )
        .bind(meeting_id)
        .bind(key)
        .bind(kind)
        .bind(serde_json::to_string(&payload)?)
        .bind(&report.schema_version)
        .execute(&mut *tx)
        .await?;
        inserted += result.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(inserted)
}

async fn list_records(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<StandupRecordRow>, sqlx::Error> {
    sqlx::query_as::<_, StandupRecordRow>(
        "SELECT sr.id, sr.meeting_id, sr.kind, sr.payload, sr.reviewed_payload, \
                sr.review_status, sr.source_schema_version, ai.id AS action_item_id, \
                ai.status AS action_status, sr.created_at, sr.updated_at \
         FROM standup_records sr \
         LEFT JOIN action_items ai ON ai.standup_record_id = sr.id \
         WHERE sr.meeting_id = ? \
         ORDER BY CASE sr.review_status WHEN 'pending' THEN 0 WHEN 'accepted' THEN 1 ELSE 2 END, \
                  sr.id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
}

#[tauri::command]
pub async fn list_standup_records(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<StandupRecordRow>, String> {
    list_records(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|error| error.to_string())
}

fn edited_text_field(kind: &str) -> Option<&'static str> {
    match kind {
        "overview" | "participant_update" | "unattributed_fact" => Some("text"),
        "decision" => Some("decision"),
        "action" => Some("task"),
        "risk" => Some("blocker_or_risk"),
        "deep_dive" => Some("topic"),
        _ => None,
    }
}

fn normalize_optional_edit(value: Option<String>, limit: usize) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim().to_string();
    if value.chars().count() > limit {
        return Err(format!("Edited value cannot exceed {limit} characters"));
    }
    Ok(Some(value))
}

fn apply_edits(
    kind: &str,
    original: &Value,
    input: &ReviewStandupRecordInput,
) -> Result<Value, String> {
    let mut edited = original.clone();
    let object = edited
        .as_object_mut()
        .ok_or_else(|| "Stored standup record is not an object".to_string())?;

    if let Some(text) = input.edited_text.as_deref() {
        let text = text.trim();
        if text.is_empty() {
            return Err("Edited text cannot be empty".to_string());
        }
        if text.chars().count() > MAX_EDIT_CHARS {
            return Err(format!(
                "Edited text cannot exceed {MAX_EDIT_CHARS} characters"
            ));
        }
        let field = edited_text_field(kind)
            .ok_or_else(|| format!("Unsupported standup record kind '{kind}'"))?;
        object.insert(field.to_string(), Value::String(text.to_string()));
    }

    if kind == "action" || kind == "risk" {
        if let Some(owner) = normalize_optional_edit(input.owner.clone(), MAX_OWNER_CHARS)? {
            object.insert(
                "owner".to_string(),
                if owner.is_empty() {
                    Value::Null
                } else {
                    Value::String(owner)
                },
            );
        }
    }
    if kind == "action" {
        if let Some(due_date) = normalize_optional_edit(input.due_date.clone(), MAX_DUE_DATE_CHARS)?
        {
            object.insert(
                "due_date".to_string(),
                if due_date.is_empty() {
                    Value::Null
                } else {
                    Value::String(due_date)
                },
            );
        }
    }
    Ok(edited)
}

fn first_evidence(payload: &Value) -> (Option<String>, Option<i64>) {
    let evidence = payload
        .get("evidence")
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    let quote = evidence
        .and_then(|item| item.get("quote"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let start_ms = evidence
        .and_then(|item| item.get("timestamp"))
        .and_then(Value::as_str)
        .and_then(parse_timestamp_seconds)
        .and_then(|seconds| i64::try_from(seconds.saturating_mul(1_000)).ok());
    (quote, start_ms)
}

fn split_due_date(value: Option<&str>) -> (Option<&str>, Option<&str>) {
    let raw = value.map(str::trim).filter(|value| !value.is_empty());
    let iso = raw.filter(|value| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok());
    (iso, raw)
}

async fn sync_accepted_action(
    tx: &mut Transaction<'_, Sqlite>,
    record_id: i64,
    meeting_id: &str,
    payload: &Value,
) -> Result<(), String> {
    let text = payload
        .get("task")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Accepted action must have a task".to_string())?;
    let owner = payload.get("owner").and_then(Value::as_str).map(str::trim);
    let due_date = payload
        .get("due_date")
        .and_then(Value::as_str)
        .map(str::trim);
    let (due_date_iso, due_date_raw) = split_due_date(due_date);
    let owner_speaker_id: Option<i64> = match owner.filter(|value| !value.is_empty()) {
        Some(owner) => sqlx::query_scalar(
            "SELECT id FROM speakers WHERE lower(trim(display_name)) = lower(trim(?)) LIMIT 1",
        )
        .bind(owner)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| error.to_string())?,
        None => None,
    };
    let (quote, source_start_ms) = first_evidence(payload);

    let updated = sqlx::query(
        "UPDATE action_items SET text = ?, owner_speaker_id = ?, owner_name_raw = ?, \
         due_date = ?, due_date_raw = ?, source_quote = ?, source_start_ms = ? \
         WHERE standup_record_id = ?",
    )
    .bind(text)
    .bind(owner_speaker_id)
    .bind(owner.filter(|value| !value.is_empty()))
    .bind(due_date_iso)
    .bind(due_date_raw)
    .bind(quote.as_deref())
    .bind(source_start_ms)
    .bind(record_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO action_items \
             (meeting_id, text, owner_speaker_id, owner_name_raw, due_date, due_date_raw, \
              source_quote, source_start_ms, standup_record_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind(text)
        .bind(owner_speaker_id)
        .bind(owner.filter(|value| !value.is_empty()))
        .bind(due_date_iso)
        .bind(due_date_raw)
        .bind(quote.as_deref())
        .bind(source_start_ms)
        .bind(record_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub async fn review_record(
    pool: &SqlitePool,
    input: ReviewStandupRecordInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "pending" | "accepted" | "rejected") {
        return Err("Review status must be pending, accepted, or rejected".to_string());
    }
    if input.status != "accepted"
        && (input.edited_text.is_some() || input.owner.is_some() || input.due_date.is_some())
    {
        return Err("Edits can only be saved with an accepted record".to_string());
    }

    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT meeting_id, kind, payload, reviewed_payload FROM standup_records WHERE id = ?",
    )
    .bind(input.record_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let (meeting_id, kind, payload_raw, reviewed_payload_raw) =
        row.ok_or_else(|| "Standup record not found".to_string())?;
    let original: Value = serde_json::from_str(&payload_raw)
        .map_err(|error| format!("Stored standup record is invalid: {error}"))?;
    let current: Value = match reviewed_payload_raw {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|error| format!("Stored reviewed standup record is invalid: {error}"))?,
        None => original.clone(),
    };
    let effective = apply_edits(&kind, &current, &input)?;
    let reviewed_payload = if effective == original {
        None
    } else {
        Some(serde_json::to_string(&effective).map_err(|error| error.to_string())?)
    };

    if kind == "action" && input.status != "accepted" {
        let action_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM action_items WHERE standup_record_id = ?")
                .bind(input.record_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        if action_status.is_some_and(|status| matches!(status.as_str(), "done" | "cancelled")) {
            return Err(
                "Completed or cancelled actions must be reopened before rejecting their source"
                    .to_string(),
            );
        }
    }

    sqlx::query(
        "UPDATE standup_records SET review_status = ?, reviewed_payload = ?, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&input.status)
    .bind(reviewed_payload)
    .bind(input.record_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    if kind == "action" && input.status == "accepted" {
        sync_accepted_action(&mut tx, input.record_id, &meeting_id, &effective).await?;
    } else if kind == "action" {
        sqlx::query("DELETE FROM action_items WHERE standup_record_id = ?")
            .bind(input.record_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn review_standup_record(
    state: tauri::State<'_, AppState>,
    input: ReviewStandupRecordInput,
) -> Result<(), String> {
    review_record(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn set_standup_action_status(
    state: tauri::State<'_, AppState>,
    action_item_id: i64,
    status: String,
) -> Result<(), String> {
    if !matches!(status.as_str(), "open" | "done" | "cancelled") {
        return Err("Action status must be open, done, or cancelled".to_string());
    }
    let result = sqlx::query(
        "UPDATE action_items SET status = ? WHERE id = ? AND standup_record_id IS NOT NULL",
    )
    .bind(status)
    .bind(action_item_id)
    .execute(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err("Reviewed standup action not found".to_string());
    }
    Ok(())
}

fn primary_text(kind: &str, payload: &Value) -> Option<String> {
    edited_text_field(kind)
        .and_then(|field| payload.get(field))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn payload_start_ms(payload: &Value) -> Option<i64> {
    first_evidence(payload).1
}

pub async fn get_prebrief(pool: &SqlitePool, meeting_id: &str) -> Result<StandupPrebrief, String> {
    let series: Vec<String> = sqlx::query_scalar(
        "SELECT c.name FROM collections c \
         JOIN meeting_collections mc ON mc.collection_id = c.id \
         WHERE mc.meeting_id = ? AND c.kind = 'series' ORDER BY lower(c.name)",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    if series.is_empty() {
        return Ok(StandupPrebrief::default());
    }

    let action_rows: Vec<(i64, String, Option<String>, Option<String>, String, String, String, Option<i64>)> =
        sqlx::query_as(
            "SELECT DISTINCT ai.id, ai.text, ai.owner_name_raw, \
                    COALESCE(ai.due_date, ai.due_date_raw), ai.status, \
                    source.id, source.title, ai.source_start_ms \
             FROM action_items ai \
             JOIN meetings source ON source.id = ai.meeting_id \
             JOIN meeting_collections source_mc ON source_mc.meeting_id = source.id \
             JOIN meeting_collections current_mc ON current_mc.collection_id = source_mc.collection_id \
             JOIN collections c ON c.id = source_mc.collection_id AND c.kind = 'series' \
             JOIN meetings current ON current.id = current_mc.meeting_id \
             WHERE current.id = ? AND source.id != current.id \
               AND CASE WHEN source.occurred_at IS NOT NULL \
                        THEN julianday(source.occurred_at) \
                        ELSE julianday(source.created_at, 'localtime') END < \
                   CASE WHEN current.occurred_at IS NOT NULL \
                        THEN julianday(current.occurred_at) \
                        ELSE julianday(current.created_at, 'localtime') END \
               AND ai.status = 'open' AND ai.standup_record_id IS NOT NULL \
             ORDER BY CASE WHEN source.occurred_at IS NOT NULL \
                           THEN julianday(source.occurred_at) \
                           ELSE julianday(source.created_at, 'localtime') END DESC, ai.id DESC LIMIT 50",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let open_actions = action_rows
        .into_iter()
        .map(
            |(
                id,
                text,
                owner,
                due_date,
                status,
                source_meeting_id,
                source_meeting_title,
                source_start_ms,
            )| PrebriefAction {
                id,
                text,
                owner,
                due_date,
                status,
                source_meeting_id,
                source_meeting_title,
                source_start_ms,
            },
        )
        .collect();

    let fact_rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "WITH eligible AS ( \
             SELECT DISTINCT sr.id, sr.kind, COALESCE(sr.reviewed_payload, sr.payload) AS payload, \
                    source.id AS source_id, source.title AS source_title, \
                    CASE WHEN source.occurred_at IS NOT NULL \
                         THEN julianday(source.occurred_at) \
                         ELSE julianday(source.created_at, 'localtime') END AS source_time \
             FROM standup_records sr \
             JOIN meetings source ON source.id = sr.meeting_id \
             JOIN meeting_collections source_mc ON source_mc.meeting_id = source.id \
             JOIN meeting_collections current_mc ON current_mc.collection_id = source_mc.collection_id \
             JOIN collections c ON c.id = source_mc.collection_id AND c.kind = 'series' \
             JOIN meetings current ON current.id = current_mc.meeting_id \
             WHERE current.id = ? AND source.id != current.id \
               AND CASE WHEN source.occurred_at IS NOT NULL \
                        THEN julianday(source.occurred_at) \
                        ELSE julianday(source.created_at, 'localtime') END < \
                   CASE WHEN current.occurred_at IS NOT NULL \
                        THEN julianday(current.occurred_at) \
                        ELSE julianday(current.created_at, 'localtime') END \
               AND sr.review_status = 'accepted' AND sr.kind IN ('risk', 'decision') \
         ), ranked AS ( \
             SELECT *, ROW_NUMBER() OVER ( \
                 PARTITION BY kind ORDER BY source_time DESC, id DESC \
             ) AS kind_rank \
             FROM eligible \
         ) \
         SELECT id, kind, payload, source_id, source_title FROM ranked \
         WHERE kind_rank <= 5 ORDER BY source_time DESC, id DESC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut recent_risks = Vec::new();
    let mut recent_decisions = Vec::new();
    for (record_id, kind, payload_raw, source_meeting_id, source_meeting_title) in fact_rows {
        let payload: Value = serde_json::from_str(&payload_raw)
            .map_err(|error| format!("Stored standup record is invalid: {error}"))?;
        let Some(text) = primary_text(&kind, &payload) else {
            continue;
        };
        let fact = PrebriefFact {
            record_id,
            kind: kind.clone(),
            text,
            source_meeting_id,
            source_meeting_title,
            source_start_ms: payload_start_ms(&payload),
        };
        if kind == "risk" {
            recent_risks.push(fact);
        } else {
            recent_decisions.push(fact);
        }
    }
    Ok(StandupPrebrief {
        series,
        open_actions,
        recent_risks,
        recent_decisions,
    })
}

#[tauri::command]
pub async fn get_standup_prebrief(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<StandupPrebrief, String> {
    get_prebrief(state.db_manager.pool(), &meeting_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_record_keys_include_owner_identity() {
        let anna = json!({
            "task": "Обновить документацию",
            "owner": "Анна",
            "evidence": [{"timestamp": "[10:00]"}]
        });
        let boris = json!({
            "task": "Обновить документацию",
            "owner": "Борис",
            "evidence": [{"timestamp": "[10:00]"}]
        });
        assert_ne!(record_key("action", &anna), record_key("action", &boris));
    }
    use crate::summary::standup::{
        EvidenceRef, StandupAction, StandupDecision, StandupReport, StandupRisk,
    };

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, occurred_at TEXT)",
            "CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT NOT NULL)",
            "CREATE TABLE collections(id INTEGER PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL)",
            "CREATE TABLE meeting_collections(meeting_id TEXT NOT NULL, collection_id INTEGER NOT NULL, PRIMARY KEY(meeting_id, collection_id))",
            "CREATE TABLE standup_records(id INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL, record_key TEXT NOT NULL, kind TEXT NOT NULL, payload TEXT NOT NULL, reviewed_payload TEXT, review_status TEXT NOT NULL DEFAULT 'pending', source_schema_version TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')), UNIQUE(meeting_id, record_key))",
            "CREATE TABLE action_items(id INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL, text TEXT NOT NULL, owner_speaker_id INTEGER, owner_name_raw TEXT, due_date TEXT, due_date_raw TEXT, status TEXT NOT NULL DEFAULT 'open', superseded_by INTEGER, source_quote TEXT, source_start_ms INTEGER, created_at TEXT NOT NULL DEFAULT (datetime('now')), standup_record_id INTEGER UNIQUE)",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        pool
    }

    fn evidence(timestamp: &str, quote: &str) -> Vec<EvidenceRef> {
        vec![EvidenceRef {
            timestamp: timestamp.to_string(),
            quote: Some(quote.to_string()),
        }]
    }

    fn report() -> StandupReport {
        let mut report = StandupReport::default();
        report.action_items.push(StandupAction {
            task: "Проверить сборку".into(),
            owner: Some("Анна".into()),
            due_date: None,
            evidence: evidence("[01:02]", "проверить сборку"),
        });
        report.risks_and_blockers.push(StandupRisk {
            blocker_or_risk: "Нет доступа к стенду".into(),
            impact: None,
            owner: None,
            evidence: evidence("[02:03]", "нет доступа"),
        });
        report.decisions.push(StandupDecision {
            decision: "Релиз переносим на пятницу".into(),
            rationale: None,
            evidence: evidence("[03:04]", "переносим на пятницу"),
        });
        report
    }

    #[test]
    fn relative_due_dates_stay_raw_instead_of_becoming_fake_iso_dates() {
        assert_eq!(
            split_due_date(Some("2026-07-17")),
            (Some("2026-07-17"), Some("2026-07-17"))
        );
        assert_eq!(split_due_date(Some("в пятницу")), (None, Some("в пятницу")));
        assert_eq!(split_due_date(Some("  ")), (None, None));
    }

    #[tokio::test]
    async fn review_is_idempotent_preserves_edits_and_controls_action_lifecycle() {
        let pool = test_pool().await;
        sqlx::query("INSERT INTO meetings(id, title, created_at) VALUES('m1', 'Standup', '2026-07-14T10:00:00Z')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO speakers VALUES(7, 'Анна')")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            sync_standup_records(&pool, "m1", &report()).await.unwrap(),
            3
        );
        let records = list_records(&pool, "m1").await.unwrap();
        let action_id = records.iter().find(|row| row.kind == "action").unwrap().id;
        review_record(
            &pool,
            ReviewStandupRecordInput {
                record_id: action_id,
                status: "accepted".into(),
                edited_text: Some("Проверить release-сборку".into()),
                owner: Some("Анна".into()),
                due_date: Some("2026-07-17".into()),
            },
        )
        .await
        .unwrap();

        let action: (String, Option<i64>, Option<String>, i64) = sqlx::query_as(
            "SELECT text, owner_speaker_id, due_date, source_start_ms FROM action_items",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(action.0, "Проверить release-сборку");
        assert_eq!(action.1, Some(7));
        assert_eq!(action.2.as_deref(), Some("2026-07-17"));
        assert_eq!(action.3, 62_000);

        review_record(
            &pool,
            ReviewStandupRecordInput {
                record_id: action_id,
                status: "accepted".into(),
                edited_text: None,
                owner: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        let text: String = sqlx::query_scalar("SELECT text FROM action_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(text, "Проверить release-сборку");

        sqlx::query("UPDATE action_items SET status = 'done'")
            .execute(&pool)
            .await
            .unwrap();
        let error = review_record(
            &pool,
            ReviewStandupRecordInput {
                record_id: action_id,
                status: "rejected".into(),
                edited_text: None,
                owner: None,
                due_date: None,
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("must be reopened"));
        let review_status: String =
            sqlx::query_scalar("SELECT review_status FROM standup_records WHERE id = ?")
                .bind(action_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(review_status, "accepted");
        sqlx::query("UPDATE action_items SET status = 'open'")
            .execute(&pool)
            .await
            .unwrap();

        // Regeneration replaces pending facts but preserves the reviewed action.
        assert_eq!(
            sync_standup_records(&pool, "m1", &report()).await.unwrap(),
            2
        );
        assert_eq!(list_records(&pool, "m1").await.unwrap().len(), 3);

        review_record(
            &pool,
            ReviewStandupRecordInput {
                record_id: action_id,
                status: "rejected".into(),
                edited_text: None,
                owner: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM action_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn prebrief_uses_only_accepted_prior_records_in_the_same_series() {
        let pool = test_pool().await;
        for (id, title, created_at) in [
            ("previous", "Standup 1", "2026-07-14T10:00:00Z"),
            ("current", "Standup 2", "2026-07-15T10:00:00Z"),
            ("outside", "Other", "2026-07-13T10:00:00Z"),
        ] {
            sqlx::query("INSERT INTO meetings(id, title, created_at) VALUES(?, ?, ?)")
                .bind(id)
                .bind(title)
                .bind(created_at)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO collections VALUES(1, 'Команда', 'series')")
            .execute(&pool)
            .await
            .unwrap();
        for meeting_id in ["previous", "current"] {
            sqlx::query("INSERT INTO meeting_collections VALUES(?, 1)")
                .bind(meeting_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sync_standup_records(&pool, "previous", &report())
            .await
            .unwrap();
        for record in list_records(&pool, "previous").await.unwrap() {
            review_record(
                &pool,
                ReviewStandupRecordInput {
                    record_id: record.id,
                    status: "accepted".into(),
                    edited_text: None,
                    owner: None,
                    due_date: None,
                },
            )
            .await
            .unwrap();
        }
        // Pending records from an unrelated meeting never enter the pre-brief.
        sync_standup_records(&pool, "outside", &report())
            .await
            .unwrap();

        let prebrief = get_prebrief(&pool, "current").await.unwrap();
        assert_eq!(prebrief.series, vec!["Команда"]);
        assert_eq!(prebrief.open_actions.len(), 1);
        assert_eq!(prebrief.recent_risks.len(), 1);
        assert_eq!(prebrief.recent_decisions.len(), 1);
        assert_eq!(prebrief.open_actions[0].source_start_ms, Some(62_000));
    }

    #[tokio::test]
    async fn prebrief_prefers_timezone_unknown_occurrence_order_over_import_order() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO meetings(id, title, created_at, occurred_at) VALUES \
             ('previous', 'Standup previous', '2026-07-16T12:00:00Z', '2026-07-14T10:00:00'), \
             ('current', 'Standup current', '2026-07-15T09:00:00Z', '2026-07-15T10:00:00')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO collections VALUES(1, 'Команда', 'series')")
            .execute(&pool)
            .await
            .unwrap();
        for meeting_id in ["previous", "current"] {
            sqlx::query("INSERT INTO meeting_collections VALUES(?, 1)")
                .bind(meeting_id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sync_standup_records(&pool, "previous", &report())
            .await
            .unwrap();
        for record in list_records(&pool, "previous").await.unwrap() {
            review_record(
                &pool,
                ReviewStandupRecordInput {
                    record_id: record.id,
                    status: "accepted".into(),
                    edited_text: None,
                    owner: None,
                    due_date: None,
                },
            )
            .await
            .unwrap();
        }

        let prebrief = get_prebrief(&pool, "current").await.unwrap();
        assert_eq!(prebrief.open_actions.len(), 1);
        assert_eq!(prebrief.recent_risks.len(), 1);
        assert_eq!(prebrief.recent_decisions.len(), 1);
    }
}
