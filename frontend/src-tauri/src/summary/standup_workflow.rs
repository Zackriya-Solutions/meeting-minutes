//! Review, action lifecycle, and recurring-series context for Standup V2.

use crate::state::AppState;
use crate::summary::standup::{parse_timestamp_seconds, StandupReport};
use crate::utils::format_timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, Serialize)]
pub struct SeriesDigestItem {
    pub record_id: i64,
    pub kind: String,
    pub text: String,
    pub participant: Option<String>,
    pub category: Option<String>,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub action_status: Option<String>,
    pub parking_lot: bool,
    pub source_meeting_id: String,
    pub source_meeting_title: String,
    pub source_occurred_at: String,
    pub source_start_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct StandupSeriesDigest {
    pub collection_id: i64,
    pub series_name: String,
    pub window_days: Option<u32>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub meeting_count: usize,
    pub meetings_with_accepted_records: usize,
    pub pending_review_count: usize,
    pub highlights: Vec<SeriesDigestItem>,
    pub updates: Vec<SeriesDigestItem>,
    pub decisions: Vec<SeriesDigestItem>,
    pub risks: Vec<SeriesDigestItem>,
    pub deep_dives: Vec<SeriesDigestItem>,
    pub parking_lot: Vec<SeriesDigestItem>,
    pub open_actions: Vec<SeriesDigestItem>,
    pub done_actions: Vec<SeriesDigestItem>,
    pub cancelled_actions: Vec<SeriesDigestItem>,
    pub markdown: String,
}

#[derive(Debug, FromRow)]
struct DigestRecordRow {
    id: i64,
    meeting_id: String,
    meeting_title: String,
    occurred_at: String,
    kind: String,
    payload: String,
    review_status: String,
    action_status: Option<String>,
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

fn parse_digest_datetime(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(value.with_timezone(&chrono::Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(value) = chrono::NaiveDateTime::parse_from_str(value, format) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(
                value,
                chrono::Utc,
            ));
        }
    }
    None
}

fn digest_source_start_ms(payload: &Value) -> Option<i64> {
    payload_start_ms(payload)
}

fn digest_item(row: &DigestRecordRow, payload: &Value) -> Option<SeriesDigestItem> {
    let text = primary_text(&row.kind, payload)?;
    Some(SeriesDigestItem {
        record_id: row.id,
        kind: row.kind.clone(),
        text,
        participant: payload
            .get("participant")
            .and_then(Value::as_str)
            .map(str::to_string),
        category: payload
            .get("category")
            .and_then(Value::as_str)
            .map(str::to_string),
        owner: payload
            .get("owner")
            .and_then(Value::as_str)
            .map(str::to_string),
        due_date: payload
            .get("due_date")
            .and_then(Value::as_str)
            .map(str::to_string),
        action_status: row.action_status.clone(),
        parking_lot: payload
            .get("parking_lot")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source_meeting_id: row.meeting_id.clone(),
        source_meeting_title: row.meeting_title.clone(),
        source_occurred_at: row.occurred_at.clone(),
        source_start_ms: digest_source_start_ms(payload),
    })
}

fn markdown_evidence(item: &SeriesDigestItem) -> String {
    let meeting_title = escape_digest_markdown(&item.source_meeting_title);
    let Some(start_ms) = item.source_start_ms else {
        return format!("— {meeting_title}");
    };
    let seconds = start_ms.max(0) / 1_000;
    let label = format_timestamp(seconds as f64);
    let meeting_id =
        url::form_urlencoded::byte_serialize(item.source_meeting_id.as_bytes()).collect::<String>();
    format!("— [{label}](/meeting-details?id={meeting_id}&t={seconds}) · {meeting_title}")
}

fn escape_digest_markdown(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut escaped = String::with_capacity(collapsed.len());
    for character in collapsed.chars() {
        // Values are collapsed to one line and rendered after our own list marker, so
        // block-prefix characters such as '-', '+', '#', and '!' are harmless here.
        // Escape only inline syntax that can still alter or inject rendered Markdown.
        if matches!(
            character,
            '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '>' | '|'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn digest_category_label(category: Option<&str>, russian: bool) -> Option<&'static str> {
    match (category, russian) {
        (Some("completed_or_recent"), true) => Some("сделано"),
        (Some("next"), true) => Some("дальше"),
        (Some("blockers"), true) => Some("блокер"),
        (Some("completed_or_recent"), false) => Some("completed"),
        (Some("next"), false) => Some("next"),
        (Some("blockers"), false) => Some("blocker"),
        _ => None,
    }
}

fn render_digest_section(
    markdown: &mut String,
    title: &str,
    items: &[SeriesDigestItem],
    russian: bool,
) {
    markdown.push_str(&format!("## {title}\n\n"));
    if items.is_empty() {
        markdown.push_str("—\n\n");
        return;
    }
    for item in items {
        let category = digest_category_label(item.category.as_deref(), russian)
            .map(|value| format!("**[{value}]** "))
            .unwrap_or_default();
        let participant = item
            .participant
            .as_deref()
            .map(|value| format!("**{}:** ", escape_digest_markdown(value)))
            .unwrap_or_default();
        let owner = item
            .owner
            .as_deref()
            .map(|value| {
                let label = if russian {
                    "исполнитель"
                } else {
                    "owner"
                };
                format!(" · {label}: {}", escape_digest_markdown(value))
            })
            .unwrap_or_default();
        let due = item
            .due_date
            .as_deref()
            .map(|value| {
                let label = if russian { "срок" } else { "due" };
                format!(" · {label}: {}", escape_digest_markdown(value))
            })
            .unwrap_or_default();
        markdown.push_str(&format!(
            "- {category}{participant}{}{}{} {}\n",
            escape_digest_markdown(&item.text),
            owner,
            due,
            markdown_evidence(item)
        ));
    }
    markdown.push('\n');
}

fn render_series_digest_markdown(
    digest: &StandupSeriesDigest,
    output_language: Option<&str>,
) -> String {
    let russian = output_language
        .map(|value| value.to_lowercase().starts_with("ru"))
        .unwrap_or(false);
    let (title, coverage, highlights, updates, open, done, decisions, risks, deep_dives, parking) =
        if russian {
            (
                "Дайджест серии стендапов",
                "Покрытие",
                "Главное",
                "Изменения по участникам",
                "Открытые действия",
                "Завершённые действия",
                "Решения",
                "Риски и блокеры",
                "Технические разборы",
                "Отложенные темы",
            )
        } else {
            (
                "Standup series digest",
                "Coverage",
                "Highlights",
                "Participant updates",
                "Open actions",
                "Completed actions",
                "Decisions",
                "Risks and blockers",
                "Deep dives",
                "Parking lot",
            )
        };
    let mut markdown = format!(
        "# {title}: {}\n\n",
        escape_digest_markdown(&digest.series_name)
    );
    let (meetings_label, accepted_label, pending_label) = if russian {
        (
            "Встречи",
            "Встречи с принятыми записями",
            "Ожидают проверки",
        )
    } else {
        (
            "Meetings",
            "Meetings with accepted records",
            "Pending review",
        )
    };
    let window = match digest.window_days {
        Some(days) => match (&digest.period_start, &digest.period_end) {
            (Some(start), Some(end)) if russian => {
                format!("Окно: последние {days} дн. ({start} — {end})")
            }
            (Some(start), Some(end)) => format!("Window: last {days} days ({start} – {end})"),
            _ if russian => format!("Окно: последние {days} дн."),
            _ => format!("Window: last {days} days"),
        },
        None if russian => "Окно: вся история".to_string(),
        None => "Window: all history".to_string(),
    };
    markdown.push_str(&format!(
        "## {coverage}\n\n- {window}\n- {meetings_label}: {}\n- {accepted_label}: {}\n- {pending_label}: {}\n\n",
        digest.meeting_count,
        digest.meetings_with_accepted_records,
        digest.pending_review_count
    ));
    render_digest_section(&mut markdown, highlights, &digest.highlights, russian);
    render_digest_section(&mut markdown, updates, &digest.updates, russian);
    render_digest_section(&mut markdown, open, &digest.open_actions, russian);
    render_digest_section(&mut markdown, done, &digest.done_actions, russian);
    render_digest_section(&mut markdown, decisions, &digest.decisions, russian);
    render_digest_section(&mut markdown, risks, &digest.risks, russian);
    render_digest_section(&mut markdown, deep_dives, &digest.deep_dives, russian);
    render_digest_section(&mut markdown, parking, &digest.parking_lot, russian);
    markdown
}

pub async fn get_series_digest(
    pool: &SqlitePool,
    collection_id: i64,
    window_days: Option<u32>,
    output_language: Option<&str>,
) -> Result<StandupSeriesDigest, String> {
    if matches!(window_days, Some(0 | 3651..)) {
        return Err("Digest window must be between 1 and 3650 days".to_string());
    }
    let collection: Option<(String, String)> =
        sqlx::query_as("SELECT name, kind FROM collections WHERE id = ?")
            .bind(collection_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    let (series_name, kind) = collection.ok_or_else(|| "Standup series not found".to_string())?;
    if kind != "series" {
        return Err("Digest is available only for a series collection".to_string());
    }

    let meetings: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.id, COALESCE(m.occurred_at, m.created_at) \
         FROM meetings m JOIN meeting_collections mc ON mc.meeting_id = m.id \
         WHERE mc.collection_id = ? ORDER BY COALESCE(m.occurred_at, m.created_at), m.id",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let anchor = meetings
        .iter()
        .filter_map(|(_, value)| parse_digest_datetime(value))
        .max();
    let cutoff = match window_days {
        Some(days) => Some(
            anchor.ok_or_else(|| {
                "Cannot apply a digest window because the series has no valid meeting dates"
                    .to_string()
            })? - chrono::Duration::days(i64::from(days)),
        ),
        None => None,
    };
    let included_meetings: HashMap<String, String> = meetings
        .into_iter()
        .filter(|(_, occurred_at)| {
            cutoff.map_or(true, |cutoff| {
                parse_digest_datetime(occurred_at)
                    .map(|value| value >= cutoff)
                    .unwrap_or(false)
            })
        })
        .collect();
    let period_start = included_meetings
        .values()
        .filter_map(|value| parse_digest_datetime(value))
        .min()
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let period_end = included_meetings
        .values()
        .filter_map(|value| parse_digest_datetime(value))
        .max()
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));

    let rows: Vec<DigestRecordRow> = sqlx::query_as(
        "SELECT sr.id, m.id AS meeting_id, m.title AS meeting_title, \
                COALESCE(m.occurred_at, m.created_at) AS occurred_at, sr.kind, \
                COALESCE(sr.reviewed_payload, sr.payload) AS payload, sr.review_status, \
                ai.status AS action_status \
         FROM standup_records sr \
         JOIN meetings m ON m.id = sr.meeting_id \
         JOIN meeting_collections mc ON mc.meeting_id = m.id \
         LEFT JOIN action_items ai ON ai.standup_record_id = sr.id \
         WHERE mc.collection_id = ? \
         ORDER BY COALESCE(m.occurred_at, m.created_at) DESC, sr.id DESC",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut digest = StandupSeriesDigest {
        collection_id,
        series_name,
        window_days,
        period_start,
        period_end,
        meeting_count: included_meetings.len(),
        ..StandupSeriesDigest::default()
    };
    let mut accepted_meetings = HashSet::new();
    for row in rows {
        if !included_meetings.contains_key(&row.meeting_id) {
            continue;
        }
        if row.review_status == "pending" {
            digest.pending_review_count += 1;
            continue;
        }
        if row.review_status != "accepted" {
            continue;
        }
        let payload: Value = serde_json::from_str(&row.payload)
            .map_err(|error| format!("Stored standup record is invalid: {error}"))?;
        let Some(item) = digest_item(&row, &payload) else {
            continue;
        };
        if row.kind == "action" && item.action_status.as_deref() == Some("cancelled") {
            digest.cancelled_actions.push(item);
            continue;
        }
        accepted_meetings.insert(row.meeting_id);
        match row.kind.as_str() {
            "overview" | "unattributed_fact" => digest.highlights.push(item),
            "participant_update" => digest.updates.push(item),
            "decision" => digest.decisions.push(item),
            "risk" => digest.risks.push(item),
            "deep_dive" if item.parking_lot => digest.parking_lot.push(item),
            "deep_dive" => digest.deep_dives.push(item),
            "action" => match item.action_status.as_deref().unwrap_or("open") {
                "done" => digest.done_actions.push(item),
                "cancelled" => unreachable!("cancelled actions are handled before coverage"),
                _ => digest.open_actions.push(item),
            },
            _ => {}
        }
    }
    digest.meetings_with_accepted_records = accepted_meetings.len();
    digest.markdown = render_series_digest_markdown(&digest, output_language);
    Ok(digest)
}

#[tauri::command]
pub async fn get_standup_series_digest(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
    window_days: Option<u32>,
    output_language: Option<String>,
) -> Result<StandupSeriesDigest, String> {
    get_series_digest(
        state.db_manager.pool(),
        collection_id,
        window_days,
        output_language.as_deref(),
    )
    .await
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
        EvidenceRef, StandupAction, StandupDecision, StandupDeepDive, StandupReport, StandupRisk,
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
    async fn series_digest_is_windowed_reviewed_and_evidence_linked() {
        let pool = test_pool().await;
        for (id, title, created_at, occurred_at) in [
            (
                "old",
                "Standup old",
                "2026-05-01T10:00:00Z",
                Some("2026-05-01T10:00:00Z"),
            ),
            (
                "accepted",
                "Standup accepted",
                "2026-07-15T09:00:00Z",
                Some("2026-07-15T09:00:00Z"),
            ),
            ("pending", "Standup pending", "2026-07-15 10:00:00", None),
        ] {
            sqlx::query(
                "INSERT INTO meetings(id, title, created_at, occurred_at) VALUES(?, ?, ?, ?)",
            )
            .bind(id)
            .bind(title)
            .bind(created_at)
            .bind(occurred_at)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO collections VALUES(1, 'Команда', 'series')")
            .execute(&pool)
            .await
            .unwrap();
        for meeting_id in ["old", "accepted", "pending"] {
            sqlx::query("INSERT INTO meeting_collections VALUES(?, 1)")
                .bind(meeting_id)
                .execute(&pool)
                .await
                .unwrap();
            let mut source_report = report();
            if meeting_id != "pending" {
                source_report.deep_dives.push(StandupDeepDive {
                    topic: "Сверить архитектуру авторизации".into(),
                    outcome: None,
                    parking_lot: true,
                    evidence: evidence("[04:05]", "вернёмся после стендапа"),
                });
            }
            sync_standup_records(&pool, meeting_id, &source_report)
                .await
                .unwrap();
        }
        for meeting_id in ["old", "accepted"] {
            for record in list_records(&pool, meeting_id).await.unwrap() {
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
        }
        let accepted_action: i64 =
            sqlx::query_scalar("SELECT id FROM action_items WHERE meeting_id = 'accepted'")
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query("UPDATE action_items SET status = 'done' WHERE id = ?")
            .bind(accepted_action)
            .execute(&pool)
            .await
            .unwrap();

        let digest = get_series_digest(&pool, 1, Some(14), Some("ru-RU"))
            .await
            .unwrap();
        assert_eq!(digest.meeting_count, 2);
        assert_eq!(digest.period_start.as_deref(), Some("2026-07-15T09:00:00Z"));
        assert_eq!(digest.period_end.as_deref(), Some("2026-07-15T10:00:00Z"));
        assert_eq!(digest.meetings_with_accepted_records, 1);
        assert_eq!(digest.pending_review_count, 3);
        assert!(digest.open_actions.is_empty());
        assert_eq!(digest.done_actions.len(), 1);
        assert_eq!(digest.decisions.len(), 1);
        assert_eq!(digest.risks.len(), 1);
        assert_eq!(digest.parking_lot.len(), 1);
        assert!(digest.deep_dives.is_empty());
        assert!(digest
            .markdown
            .contains("/meeting-details?id=accepted&t=62"));
        assert!(digest.markdown.contains("- Встречи: 2"));
        assert!(digest
            .markdown
            .contains("Окно: последние 14 дн. (2026-07-15T09:00:00Z — 2026-07-15T10:00:00Z)"));
        assert!(digest.markdown.contains("Ожидают проверки: 3"));
        assert!(!digest.markdown.contains("Pending review:"));
        assert!(!digest.markdown.contains("Standup old"));
    }

    #[test]
    fn digest_categories_preserve_update_semantics() {
        assert_eq!(
            digest_category_label(Some("completed_or_recent"), false),
            Some("completed")
        );
        assert_eq!(digest_category_label(Some("next"), false), Some("next"));
        assert_eq!(
            digest_category_label(Some("blockers"), false),
            Some("blocker")
        );
        assert_eq!(
            digest_category_label(Some("blockers"), true),
            Some("блокер")
        );
        assert_eq!(digest_category_label(None, true), None);
    }

    #[test]
    fn digest_markdown_escapes_user_controlled_text() {
        let item = SeriesDigestItem {
            record_id: 1,
            kind: "overview".into(),
            text: "[fake](https://example.test)\n# heading".into(),
            participant: Some("**Anna**".into()),
            category: None,
            owner: Some("[owner](https://example.test)".into()),
            due_date: Some("2026-07-15 | now".into()),
            action_status: None,
            parking_lot: false,
            source_meeting_id: "meeting id&unsafe".into(),
            source_meeting_title: "[meeting](https://example.test)".into(),
            source_occurred_at: "2026-07-15T10:00:00Z".into(),
            source_start_ms: Some(1_000),
        };
        let mut digest = StandupSeriesDigest {
            series_name: "[series](https://example.test)".into(),
            highlights: vec![item],
            ..StandupSeriesDigest::default()
        };
        digest.markdown = render_series_digest_markdown(&digest, Some("en"));

        assert!(!digest.markdown.contains("[fake](https://example.test)"));
        assert!(!digest.markdown.contains("\n# heading"));
        assert!(!digest.markdown.contains("[meeting](https://example.test)"));
        assert!(digest.markdown.contains("meeting+id%26unsafe"));
        assert!(digest.markdown.contains("2026-07-15"));
        assert!(!digest.markdown.contains("2026\\-07\\-15"));
        assert!(digest.markdown.contains("Window: all history"));
    }

    #[tokio::test]
    async fn windowed_digest_rejects_series_without_valid_dates() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO meetings(id, title, created_at) VALUES('bad', 'Bad', 'not-a-date')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO collections VALUES(1, 'Series', 'series')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meeting_collections VALUES('bad', 1)")
            .execute(&pool)
            .await
            .unwrap();

        let error = get_series_digest(&pool, 1, Some(14), Some("en"))
            .await
            .unwrap_err();
        assert!(error.contains("no valid meeting dates"));
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
