//! Persistence and human review for One-on-One Memory V1.

use crate::database::repositories::meeting::{
    MeetingsRepository, MEMORY_TYPE_GENERAL, SENSITIVITY_SENSITIVE, TEMPLATE_ONE_ON_ONE,
};
use crate::state::AppState;
use crate::summary::one_on_one::OneOnOneReport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOneConfig {
    pub meeting_id: String,
    pub pair_id: Option<i64>,
    pub participant_a: Option<String>,
    pub participant_a_role: Option<String>,
    pub participant_b: Option<String>,
    pub participant_b_role: Option<String>,
    #[serde(default)]
    pub shared_agenda: Vec<String>,
    pub target_minutes: i64,
    pub facilitation_enabled: bool,
    pub occurred_at: Option<String>,
    pub occurred_at_confirmed: bool,
}

impl OneOnOneConfig {
    fn empty(meeting_id: String) -> Self {
        Self {
            meeting_id,
            pair_id: None,
            participant_a: None,
            participant_a_role: None,
            participant_b: None,
            participant_b_role: None,
            shared_agenda: Vec::new(),
            target_minutes: 30,
            facilitation_enabled: false,
            occurred_at: None,
            occurred_at_confirmed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OneOnOnePrivateNoteRow {
    pub id: i64,
    pub meeting_id: String,
    pub participant_slot: String,
    pub note_kind: String,
    pub content: String,
    pub shared_to_agenda: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOneOnOnePrivateNoteInput {
    pub note_id: Option<i64>,
    pub meeting_id: String,
    pub participant_slot: String,
    pub note_kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OneOnOneLiveMarkerRow {
    pub id: i64,
    pub meeting_id: String,
    pub marker_kind: String,
    pub elapsed_seconds: i64,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddOneOnOneLiveMarkerInput {
    pub meeting_id: String,
    pub marker_kind: String,
    pub elapsed_seconds: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOneSeriesMeeting {
    pub meeting_id: String,
    pub title: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOneCarryRecord {
    pub source_meeting_id: String,
    pub source_title: String,
    pub source_occurred_at: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOnePrebrief {
    pub ready: bool,
    pub reason: Option<String>,
    pub previous_meeting: Option<OneOnOneSeriesMeeting>,
    pub open_commitments: Vec<OneOnOneCommitmentRow>,
    pub accepted_carry: Vec<OneOnOneCarryRecord>,
    pub changes_since_previous: Vec<OneOnOneSeriesChange>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOneSeriesChange {
    pub state: String,
    pub kind: String,
    pub payload: Value,
    pub source_meeting_id: String,
    pub comparison_meeting_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOneRecurringSuggestion {
    pub canonical_topic: String,
    pub occurrences: i64,
    pub source_record_ids: Vec<i64>,
    pub confirmation_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmOneOnOneRecurringTopicInput {
    pub meeting_id: String,
    pub canonical_topic: String,
    pub source_record_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneOnOnePrivacy {
    pub cloud_processing_allowed: bool,
    pub indexing_allowed: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OneOnOneRecordRow {
    pub id: i64,
    pub meeting_id: String,
    pub kind: String,
    #[sqlx(json)]
    pub payload: Value,
    #[sqlx(json(nullable))]
    pub reviewed_payload: Option<Value>,
    pub review_status: String,
    pub carry_status: String,
    pub source_schema_version: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOneOnOneRecordInput {
    pub record_id: i64,
    pub status: String,
    #[serde(default)]
    pub edited_payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OneOnOneCommitmentRow {
    pub id: i64,
    pub source_record_id: i64,
    pub meeting_id: String,
    pub task: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub status: String,
    #[sqlx(json)]
    pub evidence: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOneOnOneCommitmentStatusInput {
    pub commitment_id: i64,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOneOnOneTopicStatusInput {
    pub record_id: i64,
    pub status: String,
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_text(label: &str, value: &str) -> Result<(), String> {
    let length = value.trim().chars().count();
    if length == 0 || length > 4_000 {
        return Err(format!(
            "{label} must contain between 1 and 4000 characters"
        ));
    }
    Ok(())
}

fn validate_string_list(values: &[String]) -> Result<Vec<String>, String> {
    if values.len() > 50 {
        return Err("sharedAgenda cannot contain more than 50 items".to_string());
    }
    let mut result = Vec::new();
    for value in values {
        validate_text("sharedAgenda item", value)?;
        let value = value.trim().to_string();
        if !result.contains(&value) {
            result.push(value);
        }
    }
    Ok(result)
}

pub async fn get_config(pool: &SqlitePool, meeting_id: &str) -> Result<OneOnOneConfig, String> {
    let row: Option<(
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        i64,
        bool,
        Option<String>,
        bool,
    )> = sqlx::query_as(
        "SELECT c.pair_id,c.participant_a,c.participant_a_role,c.participant_b,c.participant_b_role,\
                c.shared_agenda_json,c.target_minutes,c.facilitation_enabled,m.occurred_at,\
                m.occurred_at_confirmed FROM one_on_one_configs c JOIN meetings m ON m.id=c.meeting_id \
         WHERE c.meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        let meeting: Option<(Option<String>, bool)> =
            sqlx::query_as("SELECT occurred_at,occurred_at_confirmed FROM meetings WHERE id=?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| error.to_string())?;
        let mut config = OneOnOneConfig::empty(meeting_id.to_string());
        if let Some((occurred_at, confirmed)) = meeting {
            config.occurred_at = occurred_at;
            config.occurred_at_confirmed = confirmed;
        }
        return Ok(config);
    };
    Ok(OneOnOneConfig {
        meeting_id: meeting_id.to_string(),
        pair_id: row.0,
        participant_a: row.1,
        participant_a_role: row.2,
        participant_b: row.3,
        participant_b_role: row.4,
        shared_agenda: serde_json::from_str(&row.5).unwrap_or_default(),
        target_minutes: row.6,
        facilitation_enabled: row.7,
        occurred_at: row.8,
        occurred_at_confirmed: row.9,
    })
}

fn pair_key(participant_a: &str, participant_b: &str) -> String {
    let mut names = [normalized(participant_a), normalized(participant_b)];
    names.sort();
    names.join("\u{1f}")
}

fn validate_confirmed_date(value: Option<&str>, confirmed: bool) -> Result<Option<String>, String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    if !confirmed {
        return Ok(value.map(str::to_string));
    }
    let value = value.ok_or_else(|| "A confirmed meeting date is required".to_string())?;
    let valid = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        || chrono::DateTime::parse_from_rfc3339(value).is_ok()
        || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok();
    if !valid {
        return Err("occurredAt must be YYYY-MM-DD, RFC3339, or YYYY-MM-DD HH:MM:SS".to_string());
    }
    Ok(Some(value.to_string()))
}

pub async fn save_config(
    pool: &SqlitePool,
    mut config: OneOnOneConfig,
) -> Result<OneOnOneConfig, String> {
    if !(10..=180).contains(&config.target_minutes) {
        return Err("targetMinutes must be between 10 and 180".to_string());
    }
    config.participant_a = clean(config.participant_a);
    config.participant_a_role = clean(config.participant_a_role);
    config.participant_b = clean(config.participant_b);
    config.participant_b_role = clean(config.participant_b_role);
    config.occurred_at =
        validate_confirmed_date(config.occurred_at.as_deref(), config.occurred_at_confirmed)?;
    for (label, value) in [
        ("participantA", config.participant_a.as_deref()),
        ("participantARole", config.participant_a_role.as_deref()),
        ("participantB", config.participant_b.as_deref()),
        ("participantBRole", config.participant_b_role.as_deref()),
    ] {
        if let Some(value) = value {
            validate_text(label, value)?;
        }
    }
    config.shared_agenda = validate_string_list(&config.shared_agenda)?;

    if config.participant_a.is_some() != config.participant_b.is_some() {
        return Err("Both participants are required to create a confirmed pair".to_string());
    }
    if let (Some(participant_a), Some(participant_b)) = (
        config.participant_a.as_deref(),
        config.participant_b.as_deref(),
    ) {
        if normalized(participant_a) == normalized(participant_b) {
            return Err("Participants must be different people".to_string());
        }
    }

    let current = MeetingsRepository::get_memory_config(pool, &config.meeting_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Meeting not found".to_string())?;
    let entering_private = current.1 != SENSITIVITY_SENSITIVE;
    MeetingsRepository::set_memory_and_template_config(
        pool,
        &config.meeting_id,
        MEMORY_TYPE_GENERAL,
        SENSITIVITY_SENSITIVE,
        TEMPLATE_ONE_ON_ONE,
    )
    .await
    .map_err(|error| error.to_string())?;
    if entering_private {
        crate::summary::interview_workflow::delete_search_index(pool, &config.meeting_id).await?;
    }

    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    config.pair_id = if let (Some(participant_a), Some(participant_b)) = (
        config.participant_a.as_deref(),
        config.participant_b.as_deref(),
    ) {
        let key = pair_key(participant_a, participant_b);
        let (stored_a, stored_a_role, stored_b, stored_b_role) =
            if normalized(participant_a) <= normalized(participant_b) {
                (
                    participant_a,
                    &config.participant_a_role,
                    participant_b,
                    &config.participant_b_role,
                )
            } else {
                (
                    participant_b,
                    &config.participant_b_role,
                    participant_a,
                    &config.participant_a_role,
                )
            };
        sqlx::query(
            "INSERT INTO one_on_one_participant_pairs(pair_key,participant_a,participant_b,participant_a_role,participant_b_role) \
             VALUES(?,?,?,?,?) ON CONFLICT(pair_key) DO UPDATE SET \
             participant_a=excluded.participant_a,participant_b=excluded.participant_b,\
             participant_a_role=excluded.participant_a_role,participant_b_role=excluded.participant_b_role,\
             updated_at=datetime('now')",
        )
        .bind(&key)
        .bind(stored_a)
        .bind(stored_b)
        .bind(stored_a_role)
        .bind(stored_b_role)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query_scalar::<_, i64>("SELECT id FROM one_on_one_participant_pairs WHERE pair_key=?")
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO one_on_one_configs(meeting_id,pair_id,participant_a,participant_a_role,participant_b,\
         participant_b_role,shared_agenda_json,target_minutes,facilitation_enabled) VALUES(?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(meeting_id) DO UPDATE SET participant_a=excluded.participant_a,\
         pair_id=excluded.pair_id,participant_a_role=excluded.participant_a_role,participant_b=excluded.participant_b,\
         participant_b_role=excluded.participant_b_role,shared_agenda_json=excluded.shared_agenda_json,\
         target_minutes=excluded.target_minutes,facilitation_enabled=excluded.facilitation_enabled,\
         updated_at=datetime('now')",
    )
    .bind(&config.meeting_id)
    .bind(config.pair_id)
    .bind(&config.participant_a)
    .bind(&config.participant_a_role)
    .bind(&config.participant_b)
    .bind(&config.participant_b_role)
    .bind(serde_json::to_string(&config.shared_agenda).map_err(|error| error.to_string())?)
    .bind(config.target_minutes)
    .bind(config.facilitation_enabled)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE meetings SET occurred_at=?,occurred_at_confirmed=? WHERE id=?")
        .bind(&config.occurred_at)
        .bind(config.occurred_at_confirmed)
        .bind(&config.meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    if let (Some(pair_id), true, Some(occurred_at)) = (
        config.pair_id,
        config.occurred_at_confirmed,
        config.occurred_at.as_deref(),
    ) {
        sqlx::query(
            "INSERT INTO one_on_one_series(meeting_id,pair_id,confirmed_occurred_at) VALUES(?,?,?) \
             ON CONFLICT(meeting_id) DO UPDATE SET pair_id=excluded.pair_id,\
             confirmed_occurred_at=excluded.confirmed_occurred_at,updated_at=datetime('now')",
        )
        .bind(&config.meeting_id)
        .bind(pair_id)
        .bind(occurred_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query("DELETE FROM one_on_one_series WHERE meeting_id=?")
            .bind(&config.meeting_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    audit(
        pool,
        &config.meeting_id,
        "config",
        &config.meeting_id,
        "saved",
        None,
    )
    .await?;
    Ok(config)
}

pub async fn extraction_context(pool: &SqlitePool, meeting_id: &str) -> Result<String, String> {
    let config = get_config(pool, meeting_id).await?;
    let confirmed_attribution = if let (
        Some(participant_a),
        Some(participant_a_role),
        Some(participant_b),
        Some(participant_b_role),
    ) = (
        config.participant_a.as_deref(),
        config.participant_a_role.as_deref(),
        config.participant_b.as_deref(),
        config.participant_b_role.as_deref(),
    ) {
        let confirmed_names: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT s.id) FROM speakers s JOIN transcripts t ON t.speaker_id=s.id \
             WHERE t.meeting_id=? AND s.is_confirmed=1 \
             AND lower(trim(s.display_name)) IN (lower(trim(?)),lower(trim(?)))",
        )
        .bind(meeting_id)
        .bind(participant_a)
        .bind(participant_b)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
        !participant_a_role.trim().is_empty()
            && !participant_b_role.trim().is_empty()
            && confirmed_names == 2
    } else {
        false
    };
    Ok([
        Some(format!(
            "CONFIRMED_ATTRIBUTION={}",
            if confirmed_attribution {
                "true"
            } else {
                "false"
            }
        )),
        config.participant_a.map(|value| {
            format!(
                "Participant A: {value}{}",
                config
                    .participant_a_role
                    .map(|role| format!(" ({role})"))
                    .unwrap_or_default()
            )
        }),
        config.participant_b.map(|value| {
            format!(
                "Participant B: {value}{}",
                config
                    .participant_b_role
                    .map(|role| format!(" ({role})"))
                    .unwrap_or_default()
            )
        }),
        (!config.shared_agenda.is_empty())
            .then(|| format!("Shared agenda: {}", config.shared_agenda.join(" | "))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n"))
}

pub async fn get_privacy(pool: &SqlitePool, meeting_id: &str) -> Result<OneOnOnePrivacy, String> {
    let row: Option<(bool, bool)> =
        sqlx::query_as("SELECT cloud_processing_allowed,indexing_allowed FROM meetings WHERE id=?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    row.map(|row| OneOnOnePrivacy {
        cloud_processing_allowed: row.0,
        indexing_allowed: row.1,
    })
    .ok_or_else(|| "Meeting not found".to_string())
}

pub async fn save_privacy(
    pool: &SqlitePool,
    meeting_id: &str,
    privacy: OneOnOnePrivacy,
) -> Result<OneOnOnePrivacy, String> {
    let template: Option<String> =
        sqlx::query_scalar("SELECT summary_template_id FROM meetings WHERE id=?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    if template.as_deref() != Some(TEMPLATE_ONE_ON_ONE) {
        return Err("Privacy settings can only be changed for One-on-One Memory".to_string());
    }
    sqlx::query("UPDATE meetings SET cloud_processing_allowed=?,indexing_allowed=? WHERE id=?")
        .bind(privacy.cloud_processing_allowed)
        .bind(privacy.indexing_allowed)
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    if !privacy.indexing_allowed {
        crate::summary::interview_workflow::delete_search_index(pool, meeting_id).await?;
    }
    audit(
        pool,
        meeting_id,
        "privacy",
        meeting_id,
        "explicit_consent_updated",
        Some(serde_json::json!({
            "cloudProcessingAllowed": privacy.cloud_processing_allowed,
            "indexingAllowed": privacy.indexing_allowed,
        })),
    )
    .await?;
    Ok(privacy)
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn primary_text(kind: &str, payload: &Value) -> String {
    let field = match kind {
        "check_in" | "progress" | "decision" => "text",
        "previous_follow_up" => "commitment",
        "challenge_support" => "challenge",
        "feedback" => "observation",
        "growth" | "open_topic" => "topic",
        "commitment" => "task",
        _ => return String::new(),
    };
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(normalized)
        .unwrap_or_default()
}

fn record_key(kind: &str, payload: &Value) -> String {
    let identity = match kind {
        "feedback" => {
            serde_json::to_string(&(payload.get("direction"), primary_text(kind, payload)))
        }
        "commitment" => serde_json::to_string(&(
            primary_text(kind, payload),
            payload.get("owner").and_then(Value::as_str).map(normalized),
        )),
        _ => serde_json::to_string(&primary_text(kind, payload)),
    }
    .unwrap_or_default();
    format!("{kind}:{identity}")
}

fn flatten(report: &OneOnOneReport) -> Result<Vec<(String, Value)>, serde_json::Error> {
    let mut records = Vec::new();
    macro_rules! add {
        ($kind:literal, $items:expr) => {
            for item in $items {
                records.push(($kind.to_string(), serde_json::to_value(item)?));
            }
        };
    }
    add!("check_in", &report.check_in);
    add!("previous_follow_up", &report.previous_follow_ups);
    add!("progress", &report.progress);
    add!("challenge_support", &report.challenges_and_support);
    add!("feedback", &report.feedback);
    add!("growth", &report.growth);
    add!("decision", &report.decisions);
    add!("commitment", &report.commitments);
    add!("open_topic", &report.open_topics);
    Ok(records)
}

pub async fn sync_records(
    pool: &SqlitePool,
    meeting_id: &str,
    report: &OneOnOneReport,
) -> anyhow::Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM one_on_one_records WHERE meeting_id=? AND review_status='pending'")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    let mut seen = HashSet::new();
    let mut count = 0;
    for (kind, payload) in flatten(report)? {
        if primary_text(&kind, &payload).is_empty() {
            continue;
        }
        let key = record_key(&kind, &payload);
        if !seen.insert(key.clone()) {
            continue;
        }
        count += sqlx::query(
            "INSERT INTO one_on_one_records(meeting_id,record_key,kind,payload,source_schema_version) \
             VALUES(?,?,?,?,?) ON CONFLICT(meeting_id,record_key) DO NOTHING",
        )
        .bind(meeting_id)
        .bind(key)
        .bind(kind)
        .bind(serde_json::to_string(&payload)?)
        .bind(&report.schema_version)
        .execute(&mut *tx)
        .await?
        .rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(count)
}

pub async fn list_records(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<OneOnOneRecordRow>, String> {
    sqlx::query_as(
        "SELECT id,meeting_id,kind,payload,reviewed_payload,review_status,carry_status,source_schema_version,\
         created_at,updated_at FROM one_on_one_records WHERE meeting_id=? \
         ORDER BY CASE review_status WHEN 'pending' THEN 0 WHEN 'accepted' THEN 1 ELSE 2 END,id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

fn valid_edited_payload(original: &Value, edited: &Value) -> Result<(), String> {
    if !edited.is_object() {
        return Err("editedPayload must be an object".to_string());
    }
    if serde_json::to_string(edited)
        .map_err(|error| error.to_string())?
        .chars()
        .count()
        > 32_000
    {
        return Err("editedPayload is too large".to_string());
    }
    if edited.get("evidence") != original.get("evidence") {
        return Err("Transcript evidence cannot be changed during review".to_string());
    }
    Ok(())
}

async fn sync_commitment(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record_id: i64,
    meeting_id: &str,
    payload: &Value,
) -> Result<(), String> {
    let task = payload
        .get("task")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Accepted commitment must have a task".to_string())?;
    sqlx::query(
        "INSERT INTO one_on_one_commitments(source_record_id,meeting_id,task,owner,due_date,evidence_json) \
         VALUES(?,?,?,?,?,?) ON CONFLICT(source_record_id) DO UPDATE SET task=excluded.task,\
         owner=excluded.owner,due_date=excluded.due_date,evidence_json=excluded.evidence_json,\
         updated_at=datetime('now')",
    )
    .bind(record_id)
    .bind(meeting_id)
    .bind(task)
    .bind(payload.get("owner").and_then(Value::as_str))
    .bind(payload.get("due_date").and_then(Value::as_str))
    .bind(payload.get("evidence").cloned().unwrap_or(Value::Array(vec![])).to_string())
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn review_record(
    pool: &SqlitePool,
    input: ReviewOneOnOneRecordInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "pending" | "accepted" | "rejected") {
        return Err("Review status must be pending, accepted, or rejected".to_string());
    }
    if input.status != "accepted" && input.edited_payload.is_some() {
        return Err("Edits can only be saved with an accepted record".to_string());
    }
    let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT meeting_id,kind,payload,reviewed_payload FROM one_on_one_records WHERE id=?",
    )
    .bind(input.record_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let (meeting_id, kind, original_raw, reviewed_raw) =
        row.ok_or_else(|| "One-on-one record not found".to_string())?;
    let original: Value = serde_json::from_str(&original_raw).map_err(|error| error.to_string())?;
    let previous_reviewed = reviewed_raw
        .as_deref()
        .map(serde_json::from_str::<Value>)
        .transpose()
        .map_err(|error| error.to_string())?;
    if let Some(edited) = input.edited_payload.as_ref() {
        valid_edited_payload(&original, edited)?;
    }
    let effective = input
        .edited_payload
        .as_ref()
        .or(previous_reviewed.as_ref())
        .unwrap_or(&original);
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE one_on_one_records SET review_status=?,reviewed_payload=CASE WHEN ? IS NULL \
         THEN reviewed_payload ELSE ? END,updated_at=datetime('now') WHERE id=?",
    )
    .bind(&input.status)
    .bind(input.edited_payload.as_ref().map(Value::to_string))
    .bind(input.edited_payload.as_ref().map(Value::to_string))
    .bind(input.record_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if kind == "commitment" {
        if input.status == "accepted" {
            sync_commitment(&mut tx, input.record_id, &meeting_id, effective).await?;
        } else {
            sqlx::query("DELETE FROM one_on_one_commitments WHERE source_record_id=?")
                .bind(input.record_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    audit(
        pool,
        &meeting_id,
        "record",
        &input.record_id.to_string(),
        &input.status,
        input.edited_payload,
    )
    .await
}

pub async fn list_commitments(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<OneOnOneCommitmentRow>, String> {
    sqlx::query_as(
        "SELECT id,source_record_id,meeting_id,task,owner,due_date,status,evidence_json AS evidence,\
         created_at,updated_at FROM one_on_one_commitments WHERE meeting_id=? ORDER BY id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn set_commitment_status(
    pool: &SqlitePool,
    input: SetOneOnOneCommitmentStatusInput,
) -> Result<(), String> {
    if !matches!(
        input.status.as_str(),
        "open" | "done" | "cancelled" | "superseded"
    ) {
        return Err("Unsupported commitment status".to_string());
    }
    let meeting_id: Option<String> = sqlx::query_scalar(
        "UPDATE one_on_one_commitments SET status=?,updated_at=datetime('now') WHERE id=? \
         RETURNING meeting_id",
    )
    .bind(&input.status)
    .bind(input.commitment_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let meeting_id = meeting_id.ok_or_else(|| "One-on-one commitment not found".to_string())?;
    audit(
        pool,
        &meeting_id,
        "commitment",
        &input.commitment_id.to_string(),
        &input.status,
        None,
    )
    .await
}

pub async fn set_topic_status(
    pool: &SqlitePool,
    input: SetOneOnOneTopicStatusInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "open" | "closed") {
        return Err("Topic status must be open or closed".to_string());
    }
    let meeting_id: Option<String> = sqlx::query_scalar(
        "UPDATE one_on_one_records SET carry_status=?,updated_at=datetime('now') \
         WHERE id=? AND kind='open_topic' AND review_status='accepted' RETURNING meeting_id",
    )
    .bind(&input.status)
    .bind(input.record_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let meeting_id = meeting_id.ok_or_else(|| "Accepted open topic not found".to_string())?;
    audit(
        pool,
        &meeting_id,
        "open_topic",
        &input.record_id.to_string(),
        &input.status,
        None,
    )
    .await
}

pub async fn list_private_notes(
    pool: &SqlitePool,
    meeting_id: &str,
    participant_slot: &str,
) -> Result<Vec<OneOnOnePrivateNoteRow>, String> {
    if !matches!(participant_slot, "participant_a" | "participant_b") {
        return Err("Unsupported participant slot".to_string());
    }
    sqlx::query_as(
        "SELECT id,meeting_id,participant_slot,note_kind,content,shared_to_agenda,created_at,updated_at \
         FROM one_on_one_private_notes WHERE meeting_id=? AND participant_slot=? ORDER BY id",
    )
    .bind(meeting_id)
    .bind(participant_slot)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn save_private_note(
    pool: &SqlitePool,
    mut input: SaveOneOnOnePrivateNoteInput,
) -> Result<i64, String> {
    if !matches!(
        input.participant_slot.as_str(),
        "participant_a" | "participant_b"
    ) {
        return Err("Unsupported participant slot".to_string());
    }
    if !matches!(input.note_kind.as_str(), "agenda_draft" | "scratchpad") {
        return Err("Unsupported private note kind".to_string());
    }
    input.content = input.content.trim().to_string();
    validate_text("private note", &input.content)?;
    let id = if let Some(note_id) = input.note_id {
        let result = sqlx::query(
            "UPDATE one_on_one_private_notes SET content=?,updated_at=datetime('now') \
             WHERE id=? AND meeting_id=? AND participant_slot=?",
        )
        .bind(&input.content)
        .bind(note_id)
        .bind(&input.meeting_id)
        .bind(&input.participant_slot)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
        if result.rows_affected() == 0 {
            return Err("Private note not found".to_string());
        }
        note_id
    } else {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO one_on_one_private_notes(meeting_id,participant_slot,note_kind,content) \
             VALUES(?,?,?,?) RETURNING id",
        )
        .bind(&input.meeting_id)
        .bind(&input.participant_slot)
        .bind(&input.note_kind)
        .bind(&input.content)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?
    };
    audit(
        pool,
        &input.meeting_id,
        "private_note",
        &id.to_string(),
        "saved",
        None,
    )
    .await?;
    Ok(id)
}

pub async fn delete_private_note(
    pool: &SqlitePool,
    meeting_id: &str,
    note_id: i64,
) -> Result<(), String> {
    sqlx::query("DELETE FROM one_on_one_private_notes WHERE id=? AND meeting_id=?")
        .bind(note_id)
        .bind(meeting_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    audit(
        pool,
        meeting_id,
        "private_note",
        &note_id.to_string(),
        "deleted",
        None,
    )
    .await
}

pub async fn share_private_note_to_agenda(
    pool: &SqlitePool,
    meeting_id: &str,
    note_id: i64,
) -> Result<OneOnOneConfig, String> {
    let note: Option<(String,)> = sqlx::query_as(
        "SELECT content FROM one_on_one_private_notes WHERE id=? AND meeting_id=? AND note_kind='agenda_draft'",
    )
    .bind(note_id)
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let content = note
        .ok_or_else(|| "Private agenda draft not found".to_string())?
        .0;
    let mut config = get_config(pool, meeting_id).await?;
    if !config.shared_agenda.contains(&content) {
        config.shared_agenda.push(content);
    }
    let saved = save_config(pool, config).await?;
    sqlx::query("UPDATE one_on_one_private_notes SET shared_to_agenda=1,updated_at=datetime('now') WHERE id=?")
        .bind(note_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    audit(
        pool,
        meeting_id,
        "private_note",
        &note_id.to_string(),
        "shared_to_agenda",
        None,
    )
    .await?;
    Ok(saved)
}

pub async fn list_live_markers(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<OneOnOneLiveMarkerRow>, String> {
    sqlx::query_as(
        "SELECT id,meeting_id,marker_kind,elapsed_seconds,note,created_at \
         FROM one_on_one_live_markers WHERE meeting_id=? ORDER BY elapsed_seconds,id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn add_live_marker(
    pool: &SqlitePool,
    mut input: AddOneOnOneLiveMarkerInput,
) -> Result<i64, String> {
    if !matches!(
        input.marker_kind.as_str(),
        "feedback" | "support" | "growth" | "follow_up" | "return_later" | "deep_dive"
    ) {
        return Err("Unsupported live marker kind".to_string());
    }
    if !(0..=86_400).contains(&input.elapsed_seconds) {
        return Err("elapsedSeconds must be between 0 and 86400".to_string());
    }
    input.note = clean(input.note);
    if let Some(note) = input.note.as_deref() {
        validate_text("marker note", note)?;
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO one_on_one_live_markers(meeting_id,marker_kind,elapsed_seconds,note) \
         VALUES(?,?,?,?) RETURNING id",
    )
    .bind(&input.meeting_id)
    .bind(&input.marker_kind)
    .bind(input.elapsed_seconds)
    .bind(&input.note)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(id)
}

pub async fn get_prebrief(pool: &SqlitePool, meeting_id: &str) -> Result<OneOnOnePrebrief, String> {
    let current: Option<(i64, String)> = sqlx::query_as(
        "SELECT pair_id,confirmed_occurred_at FROM one_on_one_series WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some((pair_id, occurred_at)) = current else {
        return Ok(OneOnOnePrebrief {
            ready: false,
            reason: Some(
                "Confirm both participants and the meeting date to enable series memory"
                    .to_string(),
            ),
            previous_meeting: None,
            open_commitments: vec![],
            accepted_carry: vec![],
            changes_since_previous: vec![],
        });
    };
    let previous: Option<(String, String, String)> = sqlx::query_as(
        "SELECT s.meeting_id,m.title,s.confirmed_occurred_at FROM one_on_one_series s \
         JOIN meetings m ON m.id=s.meeting_id WHERE s.pair_id=? AND s.meeting_id<>? \
         AND s.confirmed_occurred_at<? ORDER BY s.confirmed_occurred_at DESC LIMIT 1",
    )
    .bind(pair_id)
    .bind(meeting_id)
    .bind(&occurred_at)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let open_commitments = sqlx::query_as(
        "SELECT c.id,c.source_record_id,c.meeting_id,c.task,c.owner,c.due_date,c.status,\
                c.evidence_json AS evidence,c.created_at,c.updated_at \
         FROM one_on_one_commitments c JOIN one_on_one_series s ON s.meeting_id=c.meeting_id \
         WHERE s.pair_id=? AND s.confirmed_occurred_at<? AND c.status='open' \
         ORDER BY s.confirmed_occurred_at DESC,c.id",
    )
    .bind(pair_id)
    .bind(&occurred_at)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let carry_rows: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT r.meeting_id,m.title,s.confirmed_occurred_at,r.kind,r.payload,r.reviewed_payload \
         FROM one_on_one_records r JOIN one_on_one_series s ON s.meeting_id=r.meeting_id \
         JOIN meetings m ON m.id=r.meeting_id WHERE s.pair_id=? AND s.confirmed_occurred_at<? \
         AND r.review_status='accepted' AND (r.kind IN ('growth','feedback') \
             OR (r.kind='open_topic' AND r.carry_status='open')) \
         ORDER BY s.confirmed_occurred_at DESC,r.id LIMIT 50",
    )
    .bind(pair_id)
    .bind(&occurred_at)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let accepted_carry = carry_rows
        .into_iter()
        .filter_map(|row| {
            serde_json::from_str(row.5.as_deref().unwrap_or(&row.4))
                .ok()
                .map(|payload| OneOnOneCarryRecord {
                    source_meeting_id: row.0,
                    source_title: row.1,
                    source_occurred_at: row.2,
                    kind: row.3,
                    payload,
                })
        })
        .collect();
    let changes_since_previous = if let Some((previous_id, _, _)) = previous.as_ref() {
        let prior_keys: HashSet<String> = sqlx::query_scalar(
            "SELECT record_key FROM one_on_one_records WHERE meeting_id=? AND review_status='accepted'",
        )
        .bind(previous_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .collect();
        let current_rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT record_key,payload,reviewed_payload FROM one_on_one_records \
             WHERE meeting_id=? AND review_status='accepted' ORDER BY id",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
        let mut changes = current_rows
            .into_iter()
            .filter(|row| !prior_keys.contains(&row.0))
            .filter_map(|row| serde_json::from_str(row.2.as_deref().unwrap_or(&row.1)).ok())
            .map(|payload| OneOnOneSeriesChange {
                state: "accepted_in_current".to_string(),
                kind: "record".to_string(),
                payload,
                source_meeting_id: meeting_id.to_string(),
                comparison_meeting_id: previous_id.clone(),
            })
            .collect::<Vec<_>>();
        let closed: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT task,status,owner,due_date FROM one_on_one_commitments \
             WHERE meeting_id=? AND status IN ('done','cancelled','superseded') ORDER BY id",
        )
        .bind(previous_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
        changes.extend(closed.into_iter().map(|row| OneOnOneSeriesChange {
            state: row.1,
            kind: "commitment".to_string(),
            payload: serde_json::json!({"task": row.0, "owner": row.2, "due_date": row.3}),
            source_meeting_id: previous_id.clone(),
            comparison_meeting_id: meeting_id.to_string(),
        }));
        changes
    } else {
        vec![]
    };
    Ok(OneOnOnePrebrief {
        ready: true,
        reason: None,
        previous_meeting: previous.map(|row| OneOnOneSeriesMeeting {
            meeting_id: row.0,
            title: row.1,
            occurred_at: row.2,
        }),
        open_commitments,
        accepted_carry,
        changes_since_previous,
    })
}

pub async fn recurring_suggestions(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<OneOnOneRecurringSuggestion>, String> {
    let pair_id: Option<i64> =
        sqlx::query_scalar("SELECT pair_id FROM one_on_one_series WHERE meeting_id=?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    let Some(pair_id) = pair_id else {
        return Ok(vec![]);
    };
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT r.id,COALESCE(r.reviewed_payload,r.payload) FROM one_on_one_records r \
         JOIN one_on_one_series s ON s.meeting_id=r.meeting_id WHERE s.pair_id=? \
         AND r.review_status='accepted' AND r.kind='open_topic' ORDER BY r.id",
    )
    .bind(pair_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut grouped: std::collections::BTreeMap<String, Vec<i64>> =
        std::collections::BTreeMap::new();
    for (id, raw) in rows {
        if let Ok(payload) = serde_json::from_str::<Value>(&raw) {
            if let Some(topic) = payload.get("topic").and_then(Value::as_str) {
                let topic = normalized(topic);
                if !topic.is_empty() {
                    grouped.entry(topic).or_default().push(id);
                }
            }
        }
    }
    Ok(grouped
        .into_iter()
        .filter(|(_, ids)| ids.len() >= 2)
        .map(
            |(canonical_topic, source_record_ids)| OneOnOneRecurringSuggestion {
                occurrences: source_record_ids.len() as i64,
                canonical_topic,
                source_record_ids,
                confirmation_required: true,
            },
        )
        .collect())
}

pub async fn confirm_recurring_topic(
    pool: &SqlitePool,
    mut input: ConfirmOneOnOneRecurringTopicInput,
) -> Result<(), String> {
    input.canonical_topic = normalized(&input.canonical_topic);
    validate_text("canonical topic", &input.canonical_topic)?;
    if input.source_record_ids.len() < 2 || input.source_record_ids.len() > 100 {
        return Err("A recurring topic needs between 2 and 100 source records".to_string());
    }
    let pair_id: i64 =
        sqlx::query_scalar("SELECT pair_id FROM one_on_one_series WHERE meeting_id=?")
            .bind(&input.meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "Confirm the pair and date before creating a recurring topic".to_string()
            })?;
    let mut query = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) FROM one_on_one_records r JOIN one_on_one_series s ON s.meeting_id=r.meeting_id \
         WHERE s.pair_id=",
    );
    query
        .push_bind(pair_id)
        .push(" AND r.review_status='accepted' AND r.id IN (");
    let mut separated = query.separated(",");
    for id in &input.source_record_ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");
    let count: i64 = query
        .build_query_scalar()
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    if count != input.source_record_ids.len() as i64 {
        return Err(
            "All recurring-topic sources must be accepted records from this pair".to_string(),
        );
    }
    sqlx::query(
        "INSERT INTO one_on_one_recurring_topics(pair_id,canonical_topic,source_record_ids_json) \
         VALUES(?,?,?) ON CONFLICT(pair_id,canonical_topic) DO UPDATE SET \
         source_record_ids_json=excluded.source_record_ids_json,status='open',updated_at=datetime('now')",
    )
    .bind(pair_id)
    .bind(&input.canonical_topic)
    .bind(serde_json::to_string(&input.source_record_ids).map_err(|error| error.to_string())?)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    audit(
        pool,
        &input.meeting_id,
        "recurring_topic",
        &input.canonical_topic,
        "confirmed",
        None,
    )
    .await
}

pub async fn export_accepted_memory(pool: &SqlitePool, meeting_id: &str) -> Result<Value, String> {
    let config = get_config(pool, meeting_id).await?;
    let records = list_records(pool, meeting_id)
        .await?
        .into_iter()
        .filter(|record| record.review_status == "accepted")
        .map(|record| {
            serde_json::json!({
                "kind": record.kind,
                "payload": record.reviewed_payload.unwrap_or(record.payload),
                "sourceSchemaVersion": record.source_schema_version,
            })
        })
        .collect::<Vec<_>>();
    let commitments = list_commitments(pool, meeting_id).await?;
    Ok(serde_json::json!({
        "schemaVersion": "one_on_one_export_v1",
        "meetingId": meeting_id,
        "occurredAt": config.occurred_at.filter(|_| config.occurred_at_confirmed),
        "participants": [config.participant_a, config.participant_b],
        "acceptedRecords": records,
        "commitments": commitments,
        "privateNotesIncluded": false,
    }))
}

pub async fn delete_series_memory(
    pool: &SqlitePool,
    meeting_id: &str,
    confirm_pair_id: i64,
) -> Result<usize, String> {
    let pair_id: i64 =
        sqlx::query_scalar("SELECT pair_id FROM one_on_one_configs WHERE meeting_id=?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "This meeting is not linked to a confirmed one-on-one pair".to_string()
            })?;
    if pair_id != confirm_pair_id {
        return Err("Pair confirmation does not match".to_string());
    }
    let meeting_ids: Vec<String> = sqlx::query_scalar(
        "SELECT meeting_id FROM one_on_one_configs WHERE pair_id=? ORDER BY meeting_id",
    )
    .bind(pair_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    for id in &meeting_ids {
        crate::summary::interview_workflow::delete_search_index(pool, id).await?;
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    for id in &meeting_ids {
        for table in [
            "one_on_one_private_notes",
            "one_on_one_live_markers",
            "one_on_one_commitments",
            "one_on_one_records",
            "one_on_one_audit_log",
            "one_on_one_configs",
            "one_on_one_series",
            "summary_processes",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE meeting_id=?"))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        }
        sqlx::query("UPDATE meetings SET summary_template_id='standard_meeting' WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    sqlx::query("DELETE FROM one_on_one_recurring_topics WHERE pair_id=?")
        .bind(pair_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM one_on_one_participant_pairs WHERE id=?")
        .bind(pair_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(meeting_ids.len())
}

async fn audit(
    pool: &SqlitePool,
    meeting_id: &str,
    entity_type: &str,
    entity_id: &str,
    action: &str,
    payload: Option<Value>,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO one_on_one_audit_log(meeting_id,entity_type,entity_id,action,payload) \
         VALUES(?,?,?,?,?)",
    )
    .bind(meeting_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(action)
    .bind(payload.map(|value| value.to_string()))
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_one_on_one_config(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<OneOnOneConfig, String> {
    get_config(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn save_one_on_one_config(
    state: tauri::State<'_, AppState>,
    config: OneOnOneConfig,
) -> Result<OneOnOneConfig, String> {
    save_config(state.db_manager.pool(), config).await
}

#[tauri::command]
pub async fn get_one_on_one_privacy(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<OneOnOnePrivacy, String> {
    get_privacy(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn save_one_on_one_privacy(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    privacy: OneOnOnePrivacy,
) -> Result<OneOnOnePrivacy, String> {
    save_privacy(state.db_manager.pool(), &meeting_id, privacy).await
}

#[tauri::command]
pub async fn list_one_on_one_records(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<OneOnOneRecordRow>, String> {
    list_records(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn review_one_on_one_record(
    state: tauri::State<'_, AppState>,
    input: ReviewOneOnOneRecordInput,
) -> Result<(), String> {
    review_record(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn list_one_on_one_commitments(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<OneOnOneCommitmentRow>, String> {
    list_commitments(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn set_one_on_one_commitment_status(
    state: tauri::State<'_, AppState>,
    input: SetOneOnOneCommitmentStatusInput,
) -> Result<(), String> {
    set_commitment_status(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn set_one_on_one_topic_status(
    state: tauri::State<'_, AppState>,
    input: SetOneOnOneTopicStatusInput,
) -> Result<(), String> {
    set_topic_status(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn list_one_on_one_private_notes(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    participant_slot: String,
) -> Result<Vec<OneOnOnePrivateNoteRow>, String> {
    list_private_notes(state.db_manager.pool(), &meeting_id, &participant_slot).await
}

#[tauri::command]
pub async fn save_one_on_one_private_note(
    state: tauri::State<'_, AppState>,
    input: SaveOneOnOnePrivateNoteInput,
) -> Result<i64, String> {
    save_private_note(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn delete_one_on_one_private_note(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    note_id: i64,
) -> Result<(), String> {
    delete_private_note(state.db_manager.pool(), &meeting_id, note_id).await
}

#[tauri::command]
pub async fn share_one_on_one_private_note_to_agenda(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    note_id: i64,
) -> Result<OneOnOneConfig, String> {
    share_private_note_to_agenda(state.db_manager.pool(), &meeting_id, note_id).await
}

#[tauri::command]
pub async fn list_one_on_one_live_markers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<OneOnOneLiveMarkerRow>, String> {
    list_live_markers(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn add_one_on_one_live_marker(
    state: tauri::State<'_, AppState>,
    input: AddOneOnOneLiveMarkerInput,
) -> Result<i64, String> {
    add_live_marker(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn get_one_on_one_prebrief(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<OneOnOnePrebrief, String> {
    get_prebrief(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn list_one_on_one_recurring_suggestions(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<OneOnOneRecurringSuggestion>, String> {
    recurring_suggestions(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn confirm_one_on_one_recurring_topic(
    state: tauri::State<'_, AppState>,
    input: ConfirmOneOnOneRecurringTopicInput,
) -> Result<(), String> {
    confirm_recurring_topic(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn export_one_on_one_accepted_memory(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Value, String> {
    export_accepted_memory(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn delete_one_on_one_series_memory(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    confirm_pair_id: i64,
) -> Result<usize, String> {
    delete_series_memory(state.db_manager.pool(), &meeting_id, confirm_pair_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::one_on_one::{FeedbackRecord, OneOnOneCommitment, OneOnOneReport};
    use crate::summary::standup::EvidenceRef;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY,title TEXT DEFAULT '',occurred_at TEXT,occurred_at_confirmed INTEGER DEFAULT 0,memory_type TEXT DEFAULT 'general',sensitivity TEXT DEFAULT 'standard',cloud_processing_allowed INTEGER DEFAULT 1,indexing_allowed INTEGER DEFAULT 1,summary_template_id TEXT DEFAULT 'standard_meeting')").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_participant_pairs(id INTEGER PRIMARY KEY,pair_key TEXT UNIQUE,participant_a TEXT,participant_b TEXT,participant_a_role TEXT,participant_b_role TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_configs(meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,pair_id INTEGER,participant_a TEXT,participant_a_role TEXT,participant_b TEXT,participant_b_role TEXT,shared_agenda_json TEXT DEFAULT '[]',target_minutes INTEGER DEFAULT 30,facilitation_enabled INTEGER DEFAULT 0,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_series(meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,pair_id INTEGER,confirmed_occurred_at TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_private_notes(id INTEGER PRIMARY KEY,meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,participant_slot TEXT,note_kind TEXT,content TEXT,shared_to_agenda INTEGER DEFAULT 0,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_live_markers(id INTEGER PRIMARY KEY,meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,marker_kind TEXT,elapsed_seconds INTEGER,note TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_recurring_topics(id INTEGER PRIMARY KEY,pair_id INTEGER,canonical_topic TEXT,status TEXT DEFAULT 'open',confirmed_by_user INTEGER DEFAULT 1,source_record_ids_json TEXT DEFAULT '[]',created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,UNIQUE(pair_id,canonical_topic))").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE speakers(id INTEGER PRIMARY KEY,display_name TEXT,is_confirmed INTEGER DEFAULT 0)").execute(&pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(id TEXT PRIMARY KEY,meeting_id TEXT,speaker_id INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE one_on_one_records(id INTEGER PRIMARY KEY,meeting_id TEXT,record_key TEXT,kind TEXT,payload TEXT,reviewed_payload TEXT,review_status TEXT DEFAULT 'pending',carry_status TEXT DEFAULT 'open',source_schema_version TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,UNIQUE(meeting_id,record_key))").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_commitments(id INTEGER PRIMARY KEY,source_record_id INTEGER UNIQUE,meeting_id TEXT,task TEXT,owner TEXT,due_date TEXT,status TEXT DEFAULT 'open',evidence_json TEXT DEFAULT '[]',created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE one_on_one_audit_log(id INTEGER PRIMARY KEY,meeting_id TEXT,entity_type TEXT,entity_id TEXT,action TEXT,payload TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY,meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE jobs(id INTEGER PRIMARY KEY,kind TEXT,meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE summary_processes(id INTEGER PRIMARY KEY,meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO meetings(id,title) VALUES('m1','First'),('m2','Second'),('m3','Third')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn report() -> OneOnOneReport {
        OneOnOneReport {
            commitments: vec![OneOnOneCommitment {
                task: "Prepare a proposal".to_string(),
                owner: Some("Participant A".to_string()),
                due_date: None,
                evidence: vec![EvidenceRef {
                    timestamp: "[01:00]".to_string(),
                    quote: Some("prepare a proposal".to_string()),
                }],
            }],
            ..Default::default()
        }
    }

    fn config(meeting_id: &str, occurred_at: &str) -> OneOnOneConfig {
        OneOnOneConfig {
            meeting_id: meeting_id.to_string(),
            pair_id: None,
            participant_a: Some("Alex".to_string()),
            participant_a_role: Some("manager".to_string()),
            participant_b: Some("Sam".to_string()),
            participant_b_role: Some("direct report".to_string()),
            shared_agenda: vec![],
            target_minutes: 30,
            facilitation_enabled: false,
            occurred_at: Some(occurred_at.to_string()),
            occurred_at_confirmed: true,
        }
    }

    #[tokio::test]
    async fn pair_roles_remain_attached_to_names_when_slots_are_swapped() {
        let pool = pool().await;
        save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        let mut swapped = config("m2", "2026-02-01");
        swapped.participant_a = Some("Sam".to_string());
        swapped.participant_a_role = Some("direct report".to_string());
        swapped.participant_b = Some("Alex".to_string());
        swapped.participant_b_role = Some("manager".to_string());
        save_config(&pool, swapped).await.unwrap();

        let pair: (String, Option<String>, String, Option<String>) = sqlx::query_as(
            "SELECT participant_a,participant_a_role,participant_b,participant_b_role \
             FROM one_on_one_participant_pairs",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            pair,
            (
                "Alex".to_string(),
                Some("manager".to_string()),
                "Sam".to_string(),
                Some("direct report".to_string()),
            )
        );
    }

    #[tokio::test]
    async fn sync_records_skips_empty_primary_text_for_tuple_keys() {
        let pool = pool().await;
        let empty = OneOnOneReport {
            schema_version: "one_on_one_v1".to_string(),
            feedback: vec![FeedbackRecord {
                direction: "participant_a_to_b".to_string(),
                ..Default::default()
            }],
            commitments: vec![OneOnOneCommitment {
                owner: Some("Participant A".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        assert_eq!(sync_records(&pool, "m1", &empty).await.unwrap(), 0);
        assert!(list_records(&pool, "m1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accepted_commitment_enters_lifecycle() {
        let pool = pool().await;
        sync_records(&pool, "m1", &report()).await.unwrap();
        let record = list_records(&pool, "m1").await.unwrap().remove(0);
        review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: record.id,
                status: "accepted".to_string(),
                edited_payload: None,
            },
        )
        .await
        .unwrap();
        let commitments = list_commitments(&pool, "m1").await.unwrap();
        assert_eq!(commitments.len(), 1);
        assert_eq!(commitments[0].task, "Prepare a proposal");
    }

    #[tokio::test]
    async fn review_cannot_change_evidence() {
        let pool = pool().await;
        sync_records(&pool, "m1", &report()).await.unwrap();
        let record = list_records(&pool, "m1").await.unwrap().remove(0);
        let mut edited = record.payload.clone();
        edited["evidence"] = serde_json::json!([]);
        assert!(review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: record.id,
                status: "accepted".to_string(),
                edited_payload: Some(edited)
            }
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn only_currently_accepted_commitments_exist_and_human_edits_survive_rereview() {
        let pool = pool().await;
        sync_records(&pool, "m1", &report()).await.unwrap();
        let record = list_records(&pool, "m1").await.unwrap().remove(0);
        let mut edited = record.payload.clone();
        edited["task"] = Value::String("Prepare the reviewed proposal".to_string());
        review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: record.id,
                status: "accepted".into(),
                edited_payload: Some(edited),
            },
        )
        .await
        .unwrap();
        review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: record.id,
                status: "pending".into(),
                edited_payload: None,
            },
        )
        .await
        .unwrap();
        assert!(list_commitments(&pool, "m1").await.unwrap().is_empty());
        review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: record.id,
                status: "accepted".into(),
                edited_payload: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            list_commitments(&pool, "m1").await.unwrap()[0].task,
            "Prepare the reviewed proposal"
        );
    }

    #[tokio::test]
    async fn series_prebrief_is_confirmed_and_accepted_only() {
        let pool = pool().await;
        save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        save_config(&pool, config("m2", "2026-02-01"))
            .await
            .unwrap();
        sync_records(&pool, "m1", &report()).await.unwrap();
        let commitment = list_records(&pool, "m1").await.unwrap().remove(0);
        review_record(
            &pool,
            ReviewOneOnOneRecordInput {
                record_id: commitment.id,
                status: "accepted".to_string(),
                edited_payload: None,
            },
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO one_on_one_records(meeting_id,record_key,kind,payload,review_status,source_schema_version) VALUES('m1','accepted-topic','open_topic','{\"topic\":\"Architecture\"}','accepted','one_on_one_v1'),('m1','pending-topic','open_topic','{\"topic\":\"Private rumor\"}','pending','one_on_one_v1')")
            .execute(&pool).await.unwrap();

        let prebrief = get_prebrief(&pool, "m2").await.unwrap();
        assert!(prebrief.ready);
        assert_eq!(prebrief.previous_meeting.unwrap().meeting_id, "m1");
        assert_eq!(prebrief.open_commitments.len(), 1);
        assert_eq!(prebrief.accepted_carry.len(), 1);
        assert_eq!(prebrief.accepted_carry[0].payload["topic"], "Architecture");

        let topic_id: i64 = sqlx::query_scalar(
            "SELECT id FROM one_on_one_records WHERE record_key='accepted-topic'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        set_topic_status(
            &pool,
            SetOneOnOneTopicStatusInput {
                record_id: topic_id,
                status: "closed".into(),
            },
        )
        .await
        .unwrap();
        assert!(get_prebrief(&pool, "m2")
            .await
            .unwrap()
            .accepted_carry
            .is_empty());

        let unconfirmed = get_prebrief(&pool, "m3").await.unwrap();
        assert!(!unconfirmed.ready);
        assert!(unconfirmed.accepted_carry.is_empty());
    }

    #[tokio::test]
    async fn private_notes_never_enter_context_or_export_and_cascade_on_delete() {
        let pool = pool().await;
        save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        save_private_note(
            &pool,
            SaveOneOnOnePrivateNoteInput {
                note_id: None,
                meeting_id: "m1".to_string(),
                participant_slot: "participant_a".to_string(),
                note_kind: "scratchpad".to_string(),
                content: "SECRET PRIVATE THOUGHT".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(!extraction_context(&pool, "m1")
            .await
            .unwrap()
            .contains("SECRET"));
        assert!(!export_accepted_memory(&pool, "m1")
            .await
            .unwrap()
            .to_string()
            .contains("SECRET"));
        sqlx::query("DELETE FROM meetings WHERE id='m1'")
            .execute(&pool)
            .await
            .unwrap();
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM one_on_one_private_notes WHERE meeting_id='m1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn recurring_topics_require_explicit_confirmation() {
        let pool = pool().await;
        save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        save_config(&pool, config("m2", "2026-02-01"))
            .await
            .unwrap();
        sqlx::query("INSERT INTO one_on_one_records(meeting_id,record_key,kind,payload,review_status,source_schema_version) VALUES('m1','topic-1','open_topic','{\"topic\":\"Architecture\"}','accepted','one_on_one_v1'),('m2','topic-2','open_topic','{\"topic\":\" architecture \"}','accepted','one_on_one_v1')")
            .execute(&pool).await.unwrap();
        let suggestions = recurring_suggestions(&pool, "m2").await.unwrap();
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].confirmation_required);
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM one_on_one_recurring_topics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 0);
        confirm_recurring_topic(
            &pool,
            ConfirmOneOnOneRecurringTopicInput {
                meeting_id: "m2".to_string(),
                canonical_topic: suggestions[0].canonical_topic.clone(),
                source_record_ids: suggestions[0].source_record_ids.clone(),
            },
        )
        .await
        .unwrap();
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM one_on_one_recurring_topics")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(after, 1);
    }

    #[tokio::test]
    async fn attribution_requires_both_manually_named_confirmed_speakers_and_roles() {
        let pool = pool().await;
        save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        assert!(extraction_context(&pool, "m1")
            .await
            .unwrap()
            .starts_with("CONFIRMED_ATTRIBUTION=false"));
        sqlx::query(
            "INSERT INTO speakers(id,display_name,is_confirmed) VALUES(1,'Alex',1),(2,'Sam',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts(id,meeting_id,speaker_id) VALUES('t1','m1',1),('t2','m1',2)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(extraction_context(&pool, "m1")
            .await
            .unwrap()
            .starts_with("CONFIRMED_ATTRIBUTION=true"));
    }

    #[tokio::test]
    async fn deleting_pair_memory_removes_all_derived_and_private_data_but_keeps_transcripts() {
        let pool = pool().await;
        let first = save_config(&pool, config("m1", "2026-01-01"))
            .await
            .unwrap();
        save_config(&pool, config("m2", "2026-02-01"))
            .await
            .unwrap();
        save_private_note(
            &pool,
            SaveOneOnOnePrivateNoteInput {
                note_id: None,
                meeting_id: "m2".into(),
                participant_slot: "participant_b".into(),
                note_kind: "scratchpad".into(),
                content: "private".into(),
            },
        )
        .await
        .unwrap();
        sync_records(&pool, "m1", &report()).await.unwrap();
        sqlx::query("INSERT INTO summary_processes(meeting_id) VALUES('m1'),('m2')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO transcripts(id,meeting_id) VALUES('keep','m1')")
            .execute(&pool)
            .await
            .unwrap();
        let deleted = delete_series_memory(&pool, "m1", first.pair_id.unwrap())
            .await
            .unwrap();
        assert_eq!(deleted, 2);
        for table in [
            "one_on_one_configs",
            "one_on_one_records",
            "one_on_one_private_notes",
            "one_on_one_participant_pairs",
            "summary_processes",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table} should be empty");
        }
        let transcripts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transcripts WHERE meeting_id='m1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(transcripts, 1);
    }
}
