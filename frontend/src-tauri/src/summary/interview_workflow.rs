//! Persistent workflow state for Interview Memory V1.

use crate::database::repositories::meeting::MeetingsRepository;
use crate::state::AppState;
use crate::summary::interview::InterviewReport;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;

const MAX_FIELD_CHARS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterviewConfig {
    pub meeting_id: String,
    pub candidate_name: Option<String>,
    pub role_title: Option<String>,
    pub interview_stage: Option<String>,
    #[serde(default)]
    pub interviewer_roles: Vec<String>,
    #[serde(default)]
    pub competencies: Vec<String>,
    pub success_criteria: Option<String>,
    #[serde(default)]
    pub question_plan: Vec<String>,
    #[serde(default)]
    pub glossary: Vec<String>,
    pub target_minutes: i64,
    pub candidate_questions_minutes: i64,
}

impl InterviewConfig {
    fn empty(meeting_id: String) -> Self {
        Self {
            meeting_id,
            candidate_name: None,
            role_title: None,
            interview_stage: None,
            interviewer_roles: Vec::new(),
            competencies: Vec::new(),
            success_criteria: None,
            question_plan: Vec::new(),
            glossary: Vec::new(),
            target_minutes: 60,
            candidate_questions_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterviewPrivacy {
    pub meeting_id: String,
    pub cloud_processing_allowed: bool,
    pub indexing_allowed: bool,
    pub retention_days: Option<i64>,
    pub retention_expires_at: Option<String>,
    pub candidate_export_allowed: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InterviewRecordRow {
    pub id: i64,
    pub meeting_id: String,
    pub kind: String,
    #[sqlx(json)]
    pub payload: Value,
    #[sqlx(json(nullable))]
    pub reviewed_payload: Option<Value>,
    pub review_status: String,
    pub source_schema_version: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewInterviewRecordInput {
    pub record_id: i64,
    pub status: String,
    #[serde(default)]
    pub edited_payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct InterviewDebrief {
    pub id: i64,
    pub meeting_id: String,
    pub reviewer_name: String,
    pub strengths: String,
    pub concerns: String,
    pub open_questions: String,
    pub recommendation: String,
    pub submitted_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveInterviewDebriefInput {
    pub meeting_id: String,
    pub reviewer_name: String,
    #[serde(default)]
    pub strengths: String,
    #[serde(default)]
    pub concerns: String,
    #[serde(default)]
    pub open_questions: String,
    #[serde(default = "default_recommendation")]
    pub recommendation: String,
}

fn default_recommendation() -> String {
    "pending".to_string()
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InterviewTrack {
    pub id: i64,
    pub candidate_name: String,
    pub role_title: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInterviewTrackInput {
    pub candidate_name: String,
    pub role_title: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignInterviewStageInput {
    pub track_id: i64,
    pub meeting_id: String,
    pub stage_order: i64,
    pub stage_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HandoffQuestion {
    pub source_meeting_id: String,
    pub source_meeting_title: String,
    pub stage_order: i64,
    pub question: String,
    pub reason: Option<String>,
    pub competency: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterviewHandoff {
    pub track_id: i64,
    pub current_stage_order: Option<i64>,
    pub previously_covered_competencies: Vec<String>,
    pub open_questions: Vec<HandoffQuestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterviewExport {
    pub filename: String,
    pub markdown: String,
}

fn validate_text(label: &str, value: &str, allow_empty: bool) -> Result<(), String> {
    let len = value.trim().chars().count();
    if (!allow_empty && len == 0) || len > MAX_FIELD_CHARS {
        return Err(format!(
            "{label} is empty or exceeds {MAX_FIELD_CHARS} characters"
        ));
    }
    Ok(())
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_string_list(label: &str, values: &[String], max: usize) -> Result<Vec<String>, String> {
    if values.len() > max {
        return Err(format!("{label} cannot contain more than {max} items"));
    }
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        validate_text(label, value, false)?;
        let value = value.trim().to_string();
        if seen.insert(value.to_lowercase()) {
            result.push(value);
        }
    }
    Ok(result)
}

fn parse_json_list(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub async fn get_config(pool: &SqlitePool, meeting_id: &str) -> Result<InterviewConfig, String> {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
        String,
        i64,
        i64,
    )> = sqlx::query_as(
        "SELECT candidate_name, role_title, interview_stage, interviewer_roles_json, \
                competencies_json, success_criteria, question_plan_json, glossary_json, \
                target_minutes, candidate_questions_minutes \
         FROM interview_configs WHERE meeting_id = ?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some(row) = row else {
        return Ok(InterviewConfig::empty(meeting_id.to_string()));
    };
    Ok(InterviewConfig {
        meeting_id: meeting_id.to_string(),
        candidate_name: row.0,
        role_title: row.1,
        interview_stage: row.2,
        interviewer_roles: parse_json_list(&row.3),
        competencies: parse_json_list(&row.4),
        success_criteria: row.5,
        question_plan: parse_json_list(&row.6),
        glossary: parse_json_list(&row.7),
        target_minutes: row.8,
        candidate_questions_minutes: row.9,
    })
}

pub async fn extraction_context(pool: &SqlitePool, meeting_id: &str) -> Result<String, String> {
    let config = get_config(pool, meeting_id).await?;
    Ok([
        config.role_title.map(|value| format!("Role: {value}")),
        config
            .interview_stage
            .map(|value| format!("Stage: {value}")),
        (!config.competencies.is_empty())
            .then(|| format!("Competencies: {}", config.competencies.join(", "))),
        config
            .success_criteria
            .map(|value| format!("Success criteria: {value}")),
        (!config.question_plan.is_empty())
            .then(|| format!("Question plan: {}", config.question_plan.join(" | "))),
        (!config.glossary.is_empty())
            .then(|| format!("Terminology/glossary: {}", config.glossary.join(", "))),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n"))
}

pub async fn save_config(
    pool: &SqlitePool,
    mut config: InterviewConfig,
) -> Result<InterviewConfig, String> {
    if config.target_minutes < 10 || config.target_minutes > 240 {
        return Err("targetMinutes must be between 10 and 240".to_string());
    }
    if config.candidate_questions_minutes < 0 || config.candidate_questions_minutes > 60 {
        return Err("candidateQuestionsMinutes must be between 0 and 60".to_string());
    }
    for (label, value) in [
        ("candidateName", config.candidate_name.as_deref()),
        ("roleTitle", config.role_title.as_deref()),
        ("interviewStage", config.interview_stage.as_deref()),
        ("successCriteria", config.success_criteria.as_deref()),
    ] {
        if let Some(value) = value {
            validate_text(label, value, true)?;
        }
    }
    config.candidate_name = clean(config.candidate_name);
    config.role_title = clean(config.role_title);
    config.interview_stage = clean(config.interview_stage);
    config.success_criteria = clean(config.success_criteria);
    config.interviewer_roles =
        validate_string_list("interviewerRoles", &config.interviewer_roles, 20)?;
    config.competencies = validate_string_list("competencies", &config.competencies, 30)?;
    config.question_plan = validate_string_list("questionPlan", &config.question_plan, 60)?;
    config.glossary = validate_string_list("glossary", &config.glossary, 100)?;

    let current_memory = MeetingsRepository::get_memory_config(pool, &config.meeting_id)
        .await
        .map_err(|e| e.to_string())?;
    let (current_memory_type, current_sensitivity) =
        current_memory.ok_or_else(|| "Meeting not found".to_string())?;
    let needs_identity_update = current_memory_type
        != crate::database::repositories::meeting::MEMORY_TYPE_INTERVIEW
        || current_sensitivity != crate::database::repositories::meeting::SENSITIVITY_SENSITIVE;
    let entering_private_memory = current_memory_type
        != crate::database::repositories::meeting::MEMORY_TYPE_INTERVIEW
        && current_sensitivity != crate::database::repositories::meeting::SENSITIVITY_SENSITIVE;

    sqlx::query(
        "INSERT INTO interview_configs(meeting_id, candidate_name, role_title, interview_stage, \
         interviewer_roles_json, competencies_json, success_criteria, question_plan_json, glossary_json, \
         target_minutes, candidate_questions_minutes) VALUES(?,?,?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(meeting_id) DO UPDATE SET candidate_name=excluded.candidate_name, \
         role_title=excluded.role_title, interview_stage=excluded.interview_stage, \
         interviewer_roles_json=excluded.interviewer_roles_json, competencies_json=excluded.competencies_json, \
         success_criteria=excluded.success_criteria, question_plan_json=excluded.question_plan_json, \
         glossary_json=excluded.glossary_json, target_minutes=excluded.target_minutes, \
         candidate_questions_minutes=excluded.candidate_questions_minutes, updated_at=datetime('now')",
    )
    .bind(&config.meeting_id).bind(&config.candidate_name).bind(&config.role_title)
    .bind(&config.interview_stage).bind(serde_json::to_string(&config.interviewer_roles).unwrap())
    .bind(serde_json::to_string(&config.competencies).unwrap()).bind(&config.success_criteria)
    .bind(serde_json::to_string(&config.question_plan).unwrap())
    .bind(serde_json::to_string(&config.glossary).unwrap()).bind(config.target_minutes)
    .bind(config.candidate_questions_minutes).execute(pool).await.map_err(|e| e.to_string())?;
    if needs_identity_update {
        let updated = MeetingsRepository::set_memory_config(
            pool,
            &config.meeting_id,
            crate::database::repositories::meeting::MEMORY_TYPE_INTERVIEW,
            crate::database::repositories::meeting::SENSITIVITY_SENSITIVE,
        )
        .await
        .map_err(|e| e.to_string())?;
        if !updated {
            return Err("Meeting not found".to_string());
        }
        if entering_private_memory {
            delete_search_index(pool, &config.meeting_id).await?;
        }
    }
    MeetingsRepository::set_summary_template_id(
        pool,
        &config.meeting_id,
        crate::database::repositories::meeting::TEMPLATE_INTERVIEW,
    )
    .await
    .map_err(|e| e.to_string())?;
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

pub async fn get_privacy(pool: &SqlitePool, meeting_id: &str) -> Result<InterviewPrivacy, String> {
    let row: Option<(i64, i64, Option<i64>, Option<String>, i64)> = sqlx::query_as(
        "SELECT cloud_processing_allowed, indexing_allowed, retention_days, \
                retention_expires_at, candidate_export_allowed FROM meetings WHERE id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let row = row.ok_or_else(|| "Meeting not found".to_string())?;
    Ok(InterviewPrivacy {
        meeting_id: meeting_id.to_string(),
        cloud_processing_allowed: row.0 != 0,
        indexing_allowed: row.1 != 0,
        retention_days: row.2,
        retention_expires_at: row.3,
        candidate_export_allowed: row.4 != 0,
    })
}

pub(crate) async fn delete_search_index(pool: &SqlitePool, meeting_id: &str) -> Result<(), String> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM chunks WHERE meeting_id=?")
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    for id in ids {
        let _ = sqlx::query("DELETE FROM chunk_embeddings WHERE chunk_id=?")
            .bind(id)
            .execute(pool)
            .await;
    }
    sqlx::query("DELETE FROM chunks WHERE meeting_id=?")
        .bind(meeting_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM jobs WHERE meeting_id=? AND kind IN ('chunk_embed','embedding_repair','extract')")
        .bind(meeting_id).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn save_privacy(
    pool: &SqlitePool,
    privacy: InterviewPrivacy,
) -> Result<InterviewPrivacy, String> {
    if privacy
        .retention_days
        .is_some_and(|days| !(1..=3650).contains(&days))
    {
        return Err("retentionDays must be between 1 and 3650".to_string());
    }
    let expires = privacy.retention_days.map(|days| format!("+{days} days"));
    let result = sqlx::query(
        "UPDATE meetings SET cloud_processing_allowed=?, indexing_allowed=?, retention_days=?, \
         retention_expires_at=CASE WHEN ? IS NULL THEN NULL ELSE datetime('now', ?) END, \
         candidate_export_allowed=? WHERE id=? AND memory_type='interview'",
    )
    .bind(privacy.cloud_processing_allowed)
    .bind(privacy.indexing_allowed)
    .bind(privacy.retention_days)
    .bind(privacy.retention_days)
    .bind(expires)
    .bind(privacy.candidate_export_allowed)
    .bind(&privacy.meeting_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    if result.rows_affected() == 0 {
        return Err("Interview meeting not found".to_string());
    }
    if !privacy.indexing_allowed {
        delete_search_index(pool, &privacy.meeting_id).await?;
    } else {
        crate::jobs::enqueue_post_meeting_pipeline(pool, &privacy.meeting_id)
            .await
            .map_err(|error| format!("Could not queue private-memory indexing: {error}"))?;
    }
    audit(
        pool,
        &privacy.meeting_id,
        "privacy",
        &privacy.meeting_id,
        "updated",
        Some(json!({
            "cloudProcessingAllowed": privacy.cloud_processing_allowed,
            "indexingAllowed": privacy.indexing_allowed,
            "retentionDays": privacy.retention_days,
            "candidateExportAllowed": privacy.candidate_export_allowed,
        })),
    )
    .await?;
    get_privacy(pool, &privacy.meeting_id).await
}

pub async fn cloud_processing_allowed(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, Option<i64>>(
        "SELECT cloud_processing_allowed FROM meetings WHERE id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?
    .flatten()
    .unwrap_or(1)
        != 0)
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn record_key(kind: &str, payload: &Value) -> String {
    if kind == "question_answer" {
        let question = payload
            .get("question")
            .and_then(Value::as_str)
            .map(normalized)
            .unwrap_or_default();
        let answer = payload
            .get("answer")
            .and_then(Value::as_str)
            .map(normalized)
            .unwrap_or_default();
        return format!(
            "{kind}:{}",
            serde_json::to_string(&(question, answer)).unwrap_or_default()
        );
    }
    if kind == "evidence" {
        let competency = payload
            .get("competency")
            .and_then(Value::as_str)
            .map(normalized)
            .unwrap_or_default();
        let observation = payload
            .get("observation")
            .and_then(Value::as_str)
            .map(normalized)
            .unwrap_or_default();
        return format!(
            "{kind}:{}",
            serde_json::to_string(&(competency, observation)).unwrap_or_default()
        );
    }
    let text = match kind {
        "conversation_block" => payload.get("topic"),
        "case_exercise" => payload.get("prompt"),
        "open_question" | "candidate_question" => payload.get("question"),
        "next_step" => payload.get("action"),
        _ => None,
    }
    .and_then(Value::as_str)
    .unwrap_or_default();
    format!("{kind}:{}", normalized(text))
}

fn flatten(report: &InterviewReport) -> Result<Vec<(String, Value)>, serde_json::Error> {
    let mut records = Vec::new();
    macro_rules! add {
        ($kind:literal, $items:expr) => {
            for item in $items {
                records.push(($kind.to_string(), serde_json::to_value(item)?));
            }
        };
    }
    add!("conversation_block", &report.conversation_blocks);
    add!("question_answer", &report.question_answers);
    add!("evidence", &report.evidence);
    add!("case_exercise", &report.case_exercises);
    add!("open_question", &report.open_questions);
    add!("candidate_question", &report.candidate_questions);
    add!("next_step", &report.next_steps);
    Ok(records)
}

pub async fn sync_records(
    pool: &SqlitePool,
    meeting_id: &str,
    report: &InterviewReport,
) -> anyhow::Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM interview_records WHERE meeting_id=? AND review_status='pending'")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    let mut seen = HashSet::new();
    let mut count = 0;
    for (kind, payload) in flatten(report)? {
        let key = record_key(&kind, &payload);
        if key.ends_with(':') || !seen.insert(key.clone()) {
            continue;
        }
        count += sqlx::query(
            "INSERT INTO interview_records(meeting_id,record_key,kind,payload,source_schema_version) \
             VALUES(?,?,?,?,?) ON CONFLICT(meeting_id,record_key) DO NOTHING",
        ).bind(meeting_id).bind(key).bind(kind).bind(serde_json::to_string(&payload)?)
        .bind(&report.schema_version).execute(&mut *tx).await?.rows_affected() as usize;
    }
    tx.commit().await?;
    Ok(count)
}

pub async fn list_records(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<InterviewRecordRow>, String> {
    sqlx::query_as(
        "SELECT id,meeting_id,kind,payload,reviewed_payload,review_status,source_schema_version,created_at,updated_at \
         FROM interview_records WHERE meeting_id=? ORDER BY CASE review_status WHEN 'pending' THEN 0 WHEN 'accepted' THEN 1 ELSE 2 END,id",
    ).bind(meeting_id).fetch_all(pool).await.map_err(|e| e.to_string())
}

fn valid_edited_payload(original: &Value, edited: &Value) -> Result<(), String> {
    if !edited.is_object() {
        return Err("editedPayload must be an object".to_string());
    }
    let encoded = serde_json::to_string(edited).map_err(|e| e.to_string())?;
    if encoded.chars().count() > 32_000 {
        return Err("editedPayload is too large".to_string());
    }
    if edited.get("evidence") != original.get("evidence") {
        return Err("Transcript evidence cannot be changed during review".to_string());
    }
    Ok(())
}

pub async fn review_record(
    pool: &SqlitePool,
    input: ReviewInterviewRecordInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "pending" | "accepted" | "rejected") {
        return Err("Review status must be pending, accepted, or rejected".to_string());
    }
    if input.status != "accepted" && input.edited_payload.is_some() {
        return Err("Edits can only be saved with an accepted record".to_string());
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT meeting_id,payload FROM interview_records WHERE id=?")
            .bind(input.record_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let (meeting_id, original_raw) = row.ok_or_else(|| "Interview record not found".to_string())?;
    let original: Value = serde_json::from_str(&original_raw).map_err(|e| e.to_string())?;
    if let Some(edited) = input.edited_payload.as_ref() {
        valid_edited_payload(&original, edited)?;
    }
    sqlx::query("UPDATE interview_records SET review_status=?,reviewed_payload=COALESCE(?,reviewed_payload),updated_at=datetime('now') WHERE id=?")
        .bind(&input.status).bind(input.edited_payload.as_ref().map(Value::to_string))
        .bind(input.record_id).execute(pool).await.map_err(|e| e.to_string())?;
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

async fn audit(
    pool: &SqlitePool,
    meeting_id: &str,
    entity_type: &str,
    entity_id: &str,
    action: &str,
    payload: Option<Value>,
) -> Result<(), String> {
    sqlx::query("INSERT INTO interview_audit_log(meeting_id,entity_type,entity_id,action,payload) VALUES(?,?,?,?,?)")
        .bind(meeting_id).bind(entity_type).bind(entity_id).bind(action)
        .bind(payload.map(|value| value.to_string())).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn save_debrief(
    pool: &SqlitePool,
    input: SaveInterviewDebriefInput,
) -> Result<InterviewDebrief, String> {
    validate_text("reviewerName", &input.reviewer_name, false)?;
    for (label, value) in [
        ("strengths", &input.strengths),
        ("concerns", &input.concerns),
        ("openQuestions", &input.open_questions),
    ] {
        validate_text(label, value, true)?;
    }
    if !matches!(
        input.recommendation.as_str(),
        "pending" | "advance" | "hold" | "decline"
    ) {
        return Err("Invalid human recommendation".to_string());
    }
    sqlx::query(
        "INSERT INTO interview_debriefs(meeting_id,reviewer_name,strengths,concerns,open_questions,recommendation) \
         VALUES(?,?,?,?,?,?) ON CONFLICT(meeting_id,reviewer_name) DO UPDATE SET strengths=excluded.strengths, \
         concerns=excluded.concerns,open_questions=excluded.open_questions,recommendation=excluded.recommendation,updated_at=datetime('now')",
    ).bind(&input.meeting_id).bind(input.reviewer_name.trim()).bind(input.strengths.trim())
    .bind(input.concerns.trim()).bind(input.open_questions.trim()).bind(&input.recommendation)
    .execute(pool).await.map_err(|e| e.to_string())?;
    audit(
        pool,
        &input.meeting_id,
        "debrief",
        input.reviewer_name.trim(),
        "saved",
        Some(json!({"recommendation":input.recommendation})),
    )
    .await?;
    sqlx::query_as("SELECT id,meeting_id,reviewer_name,strengths,concerns,open_questions,recommendation,submitted_at,updated_at FROM interview_debriefs WHERE meeting_id=? AND reviewer_name=?")
        .bind(&input.meeting_id).bind(input.reviewer_name.trim()).fetch_one(pool).await.map_err(|e| e.to_string())
}

pub async fn list_debriefs(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<InterviewDebrief>, String> {
    sqlx::query_as("SELECT id,meeting_id,reviewer_name,strengths,concerns,open_questions,recommendation,submitted_at,updated_at FROM interview_debriefs WHERE meeting_id=? ORDER BY submitted_at,id")
        .bind(meeting_id).fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn create_track(
    pool: &SqlitePool,
    input: CreateInterviewTrackInput,
) -> Result<InterviewTrack, String> {
    validate_text("candidateName", &input.candidate_name, false)?;
    validate_text("roleTitle", &input.role_title, false)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO interview_tracks(candidate_name,role_title) VALUES(?,?) RETURNING id",
    )
    .bind(input.candidate_name.trim())
    .bind(input.role_title.trim())
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query_as("SELECT id,candidate_name,role_title,status,created_at,updated_at FROM interview_tracks WHERE id=?")
        .bind(id).fetch_one(pool).await.map_err(|e| e.to_string())
}

pub async fn assign_stage(
    pool: &SqlitePool,
    input: AssignInterviewStageInput,
) -> Result<(), String> {
    if !(1..=100).contains(&input.stage_order) {
        return Err("stageOrder must be between 1 and 100".to_string());
    }
    sqlx::query(
        "INSERT INTO interview_track_meetings(track_id,meeting_id,stage_order,stage_name) VALUES(?,?,?,?) \
         ON CONFLICT(track_id,meeting_id) DO UPDATE SET stage_order=excluded.stage_order,stage_name=excluded.stage_name",
    ).bind(input.track_id).bind(&input.meeting_id).bind(input.stage_order).bind(clean(input.stage_name))
    .execute(pool).await.map_err(|e| e.to_string())?;
    audit(
        pool,
        &input.meeting_id,
        "track",
        &input.track_id.to_string(),
        "assigned",
        Some(json!({"stageOrder":input.stage_order})),
    )
    .await
}

pub async fn get_handoff(pool: &SqlitePool, meeting_id: &str) -> Result<InterviewHandoff, String> {
    let membership: Option<(i64, i64)> = sqlx::query_as(
        "SELECT track_id,stage_order FROM interview_track_meetings WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some((track_id, current_order)) = membership else {
        return Ok(InterviewHandoff {
            track_id: 0,
            current_stage_order: None,
            previously_covered_competencies: Vec::new(),
            open_questions: Vec::new(),
        });
    };
    let rows: Vec<(String, String, i64, String, String)> = sqlx::query_as(
        "SELECT m.id,m.title,itm.stage_order,ir.kind,COALESCE(ir.reviewed_payload,ir.payload) \
         FROM interview_track_meetings itm JOIN meetings m ON m.id=itm.meeting_id \
         JOIN interview_records ir ON ir.meeting_id=m.id AND ir.review_status='accepted' \
         WHERE itm.track_id=? AND itm.stage_order<? ORDER BY itm.stage_order,ir.id",
    )
    .bind(track_id)
    .bind(current_order)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut competencies = HashSet::new();
    let mut questions = Vec::new();
    let mut seen_questions = HashSet::new();
    for (source_id, title, order, kind, raw) in rows {
        let payload: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        if kind == "evidence" {
            if let Some(value) = payload.get("competency").and_then(Value::as_str) {
                competencies.insert(value.to_string());
            }
        } else if kind == "open_question" {
            let question = payload
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if !question.is_empty() && seen_questions.insert(normalized(question)) {
                questions.push(HandoffQuestion {
                    source_meeting_id: source_id,
                    source_meeting_title: title,
                    stage_order: order,
                    question: question.to_string(),
                    reason: payload
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    competency: payload
                        .get("competency")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        }
    }
    let mut competencies = competencies.into_iter().collect::<Vec<_>>();
    competencies.sort();
    Ok(InterviewHandoff {
        track_id,
        current_stage_order: Some(current_order),
        previously_covered_competencies: competencies,
        open_questions: questions,
    })
}

fn primary_text<'a>(kind: &str, payload: &'a Value) -> &'a str {
    let field = match kind {
        "conversation_block" => "topic",
        "question_answer" => "question",
        "evidence" => "observation",
        "case_exercise" => "prompt",
        "open_question" | "candidate_question" => "question",
        "next_step" => "action",
        _ => "",
    };
    payload
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

pub async fn build_export(
    pool: &SqlitePool,
    meeting_id: &str,
    audience: &str,
) -> Result<InterviewExport, String> {
    if !matches!(audience, "internal" | "candidate") {
        return Err("audience must be internal or candidate".to_string());
    }
    let config = get_config(pool, meeting_id).await?;
    let privacy = get_privacy(pool, meeting_id).await?;
    if audience == "candidate" && !privacy.candidate_export_allowed {
        return Err("Candidate export is disabled for this memory".to_string());
    }
    let records = list_records(pool, meeting_id).await?;
    let mut out = format!(
        "# Interview — {}\n\n",
        config.role_title.as_deref().unwrap_or("role not stated")
    );
    if audience == "internal" {
        out.push_str(&format!(
            "Candidate: {}\n\nStage: {}\n\n",
            config.candidate_name.as_deref().unwrap_or("not stated"),
            config.interview_stage.as_deref().unwrap_or("not stated")
        ));
    } else {
        out.push_str("This export contains the agreed interview record and next steps. Internal notes and reviewer recommendations are excluded.\n\n");
    }
    for record in records
        .into_iter()
        .filter(|record| record.review_status == "accepted")
    {
        if audience == "candidate" && matches!(record.kind.as_str(), "evidence" | "open_question") {
            continue;
        }
        let payload = record.reviewed_payload.as_ref().unwrap_or(&record.payload);
        out.push_str(&format!(
            "- **{}**: {}\n",
            record.kind.replace('_', " "),
            primary_text(&record.kind, payload)
        ));
    }
    if audience == "internal" {
        let debriefs = list_debriefs(pool, meeting_id).await?;
        if !debriefs.is_empty() {
            out.push_str("\n## Human debriefs\n\n");
        }
        for item in debriefs {
            out.push_str(&format!(
                "- {} — {}: strengths: {}; concerns: {}; open questions: {}\n",
                item.reviewer_name,
                item.recommendation,
                item.strengths,
                item.concerns,
                item.open_questions
            ));
        }
    }
    Ok(InterviewExport {
        filename: format!("interview-{meeting_id}-{audience}.md"),
        markdown: out.trim_end().to_string(),
    })
}

pub async fn purge_expired(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id,folder_path FROM meetings WHERE memory_type='interview' AND retention_expires_at IS NOT NULL \
         AND datetime(retention_expires_at)<=datetime('now') ORDER BY retention_expires_at",
    ).fetch_all(pool).await.map_err(|e| e.to_string())?;
    let mut deleted = Vec::new();
    for (id, folder_path) in rows {
        if let Some(folder_path) = folder_path.filter(|value| !value.trim().is_empty()) {
            if let Err(error) =
                crate::api::api::delete_recording_folder(std::path::Path::new(&folder_path))
            {
                log::warn!("Expired Interview Memory {id} was retained because its recording folder could not be safely deleted: {error}");
                continue;
            }
        }
        if MeetingsRepository::delete_meeting(pool, &id)
            .await
            .map_err(|e| e.to_string())?
        {
            deleted.push(id);
        }
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn get_interview_config(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<InterviewConfig, String> {
    get_config(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn save_interview_config(
    state: tauri::State<'_, AppState>,
    config: InterviewConfig,
) -> Result<InterviewConfig, String> {
    save_config(state.db_manager.pool(), config).await
}
#[tauri::command]
pub async fn get_interview_privacy(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<InterviewPrivacy, String> {
    get_privacy(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn save_interview_privacy(
    state: tauri::State<'_, AppState>,
    privacy: InterviewPrivacy,
) -> Result<InterviewPrivacy, String> {
    save_privacy(state.db_manager.pool(), privacy).await
}
#[tauri::command]
pub async fn list_interview_records(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<InterviewRecordRow>, String> {
    list_records(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn review_interview_record(
    state: tauri::State<'_, AppState>,
    input: ReviewInterviewRecordInput,
) -> Result<(), String> {
    review_record(state.db_manager.pool(), input).await
}
#[tauri::command]
pub async fn save_interview_debrief(
    state: tauri::State<'_, AppState>,
    input: SaveInterviewDebriefInput,
) -> Result<InterviewDebrief, String> {
    save_debrief(state.db_manager.pool(), input).await
}
#[tauri::command]
pub async fn list_interview_debriefs(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<InterviewDebrief>, String> {
    list_debriefs(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn create_interview_track(
    state: tauri::State<'_, AppState>,
    input: CreateInterviewTrackInput,
) -> Result<InterviewTrack, String> {
    create_track(state.db_manager.pool(), input).await
}
#[tauri::command]
pub async fn assign_interview_stage(
    state: tauri::State<'_, AppState>,
    input: AssignInterviewStageInput,
) -> Result<(), String> {
    assign_stage(state.db_manager.pool(), input).await
}
#[tauri::command]
pub async fn get_interview_handoff(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<InterviewHandoff, String> {
    get_handoff(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn export_interview_memory(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    audience: String,
) -> Result<InterviewExport, String> {
    build_export(state.db_manager.pool(), &meeting_id, &audience).await
}
#[tauri::command]
pub async fn purge_expired_interview_memories(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    purge_expired(state.db_manager.pool()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summary::interview::{InterviewEvidence, OpenQuestion, QuestionAnswer};
    use crate::summary::standup::EvidenceRef;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY,title TEXT DEFAULT '',memory_type TEXT DEFAULT 'interview',sensitivity TEXT DEFAULT 'sensitive',cloud_processing_allowed INTEGER DEFAULT 0,indexing_allowed INTEGER DEFAULT 0,retention_days INTEGER,retention_expires_at TEXT,candidate_export_allowed INTEGER DEFAULT 0,summary_template_id TEXT DEFAULT 'interview_memory')").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE interview_configs(meeting_id TEXT PRIMARY KEY,candidate_name TEXT,role_title TEXT,interview_stage TEXT,interviewer_roles_json TEXT DEFAULT '[]',competencies_json TEXT DEFAULT '[]',success_criteria TEXT,question_plan_json TEXT DEFAULT '[]',glossary_json TEXT DEFAULT '[]',target_minutes INTEGER DEFAULT 60,candidate_questions_minutes INTEGER DEFAULT 10,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE interview_records(id INTEGER PRIMARY KEY,meeting_id TEXT,record_key TEXT,kind TEXT,payload TEXT,reviewed_payload TEXT,review_status TEXT DEFAULT 'pending',source_schema_version TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP,updated_at TEXT DEFAULT CURRENT_TIMESTAMP,UNIQUE(meeting_id,record_key))").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE interview_audit_log(id INTEGER PRIMARY KEY,meeting_id TEXT,entity_type TEXT,entity_id TEXT,action TEXT,payload TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP)").execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY,meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE jobs(id INTEGER PRIMARY KEY,kind TEXT,meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings(id,title) VALUES('m1','Interview')")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn saving_interview_config_enforces_private_memory_defaults() {
        let pool = pool().await;
        sqlx::query(
            "UPDATE meetings SET memory_type='general', sensitivity='standard', \
             cloud_processing_allowed=1, indexing_allowed=1 WHERE id='m1'",
        )
        .execute(&pool)
        .await
        .unwrap();

        save_config(&pool, InterviewConfig::empty("m1".to_string()))
            .await
            .unwrap();

        let state: (String, String, i64, i64) = sqlx::query_as(
            "SELECT memory_type, sensitivity, cloud_processing_allowed, indexing_allowed \
             FROM meetings WHERE id='m1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("interview".into(), "sensitive".into(), 0, 0));
    }

    #[tokio::test]
    async fn resaving_interview_config_preserves_explicit_privacy_opt_in() {
        let pool = pool().await;
        let mut config = InterviewConfig::empty("m1".to_string());
        save_config(&pool, config.clone()).await.unwrap();
        sqlx::query(
            "UPDATE meetings SET cloud_processing_allowed=1, indexing_allowed=1 WHERE id='m1'",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO chunks(id, meeting_id) VALUES(99, 'm1')")
            .execute(&pool)
            .await
            .unwrap();

        config.question_plan.push("Add one follow-up".into());
        save_config(&pool, config).await.unwrap();

        let state: (i64, i64, i64) = sqlx::query_as(
            "SELECT cloud_processing_allowed, indexing_allowed, \
             (SELECT COUNT(*) FROM chunks WHERE meeting_id='m1') \
             FROM meetings WHERE id='m1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, (1, 1, 1));
    }

    #[tokio::test]
    async fn accepted_records_survive_regeneration() {
        let pool = pool().await;
        let make = |observation: &str| InterviewReport {
            evidence: vec![InterviewEvidence {
                competency: "Architecture".into(),
                evidence_type: "candidate_claim".into(),
                observation: observation.into(),
                evidence: vec![EvidenceRef {
                    timestamp: "[00:10]".into(),
                    quote: Some("quote".into()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        sync_records(&pool, "m1", &make("first")).await.unwrap();
        let id = list_records(&pool, "m1").await.unwrap()[0].id;
        review_record(
            &pool,
            ReviewInterviewRecordInput {
                record_id: id,
                status: "accepted".into(),
                edited_payload: None,
            },
        )
        .await
        .unwrap();
        sync_records(&pool, "m1", &make("second")).await.unwrap();
        let rows = list_records(&pool, "m1").await.unwrap();
        assert_eq!(
            rows.iter()
                .filter(|r| r.review_status == "accepted")
                .count(),
            1
        );
        assert_eq!(
            rows.iter().filter(|r| r.review_status == "pending").count(),
            1
        );
    }

    #[tokio::test]
    async fn rereview_without_a_new_edit_preserves_human_correction() {
        let pool = pool().await;
        let report = InterviewReport {
            evidence: vec![InterviewEvidence {
                competency: "Architecture".into(),
                evidence_type: "candidate_claim".into(),
                observation: "machine text".into(),
                evidence: vec![EvidenceRef {
                    timestamp: "[00:10]".into(),
                    quote: Some("quote".into()),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        sync_records(&pool, "m1", &report).await.unwrap();
        let record = list_records(&pool, "m1").await.unwrap().remove(0);
        let mut corrected = record.payload.clone();
        corrected["observation"] = Value::String("human correction".into());

        review_record(
            &pool,
            ReviewInterviewRecordInput {
                record_id: record.id,
                status: "accepted".into(),
                edited_payload: Some(corrected.clone()),
            },
        )
        .await
        .unwrap();
        for status in ["rejected", "accepted"] {
            review_record(
                &pool,
                ReviewInterviewRecordInput {
                    record_id: record.id,
                    status: status.into(),
                    edited_payload: None,
                },
            )
            .await
            .unwrap();
        }

        let rereviewed = list_records(&pool, "m1").await.unwrap().remove(0);
        assert_eq!(rereviewed.review_status, "accepted");
        assert_eq!(rereviewed.reviewed_payload, Some(corrected));
    }

    #[tokio::test]
    async fn handoff_deduplicates_open_questions() {
        let mut report = InterviewReport::default();
        report.open_questions.push(OpenQuestion {
            question: "What was your personal contribution?".into(),
            reason: "Claim needs detail".into(),
            competency: Some("Ownership".into()),
            evidence: vec![EvidenceRef {
                timestamp: "[00:10]".into(),
                quote: Some("we built it".into()),
            }],
        });
        assert_eq!(flatten(&report).unwrap().len(), 1);
        assert_eq!(
            record_key("open_question", &flatten(&report).unwrap()[0].1),
            "open_question:what was your personal contribution?"
        );
    }

    #[tokio::test]
    async fn persistence_keys_keep_distinct_answers_and_competencies() {
        let pool = pool().await;
        let evidence = vec![EvidenceRef {
            timestamp: "[00:10]".into(),
            quote: Some("quote".into()),
        }];
        let report = InterviewReport {
            question_answers: vec![
                QuestionAnswer {
                    question: "Tell me more".into(),
                    answer: "First answer".into(),
                    evidence: evidence.clone(),
                    ..Default::default()
                },
                QuestionAnswer {
                    question: "Tell me more".into(),
                    answer: "Second answer".into(),
                    evidence: evidence.clone(),
                    ..Default::default()
                },
            ],
            evidence: vec![
                InterviewEvidence {
                    competency: "Architecture".into(),
                    observation: "Used a trade-off matrix".into(),
                    evidence: evidence.clone(),
                    ..Default::default()
                },
                InterviewEvidence {
                    competency: "Communication".into(),
                    observation: "Used a trade-off matrix".into(),
                    evidence,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        assert_eq!(sync_records(&pool, "m1", &report).await.unwrap(), 4);
        let rows = list_records(&pool, "m1").await.unwrap();
        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == "question_answer")
                .count(),
            2
        );
        assert_eq!(rows.iter().filter(|row| row.kind == "evidence").count(), 2);
    }
}
