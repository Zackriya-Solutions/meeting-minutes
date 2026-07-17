//! Review-before-apply backfill for identities and transcript terminology.
//!
//! A newly trusted profile may explain older observations, but it never rewrites
//! history silently. Reconciliation stores versioned proposals with the exact
//! previous value, then waits for a user decision. Applied proposals can be
//! rolled back by creating a new corrective event.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::database::repositories::speaker::decode_embedding;
use crate::pipeline::diarization::cosine_similarity;

const BACKFILL_VOICE_FLOOR: f32 = 0.80;
const BACKFILL_MARGIN: f32 = 0.05;
const MAX_PROPOSALS_PER_RUN: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationSuggestion {
    pub id: i64,
    pub run_id: i64,
    pub meeting_id: Option<String>,
    pub target_type: String,
    pub target_id: String,
    pub suggestion_kind: String,
    pub previous_value: Value,
    pub proposed_value: Value,
    pub confidence: f64,
    pub evidence: Value,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewReconciliationInput {
    pub suggestion_id: i64,
    pub decision: String,
}

#[derive(Debug)]
struct ActiveCentroid {
    speaker_id: i64,
    embedding: Vec<f32>,
    dispersion: f32,
}

async fn create_run(
    pool: &SqlitePool,
    trigger_kind: &str,
    trigger_ref: &str,
    snapshot: Value,
) -> Result<i64, String> {
    sqlx::query_scalar(
        "INSERT INTO reconciliation_runs( \
            run_uuid, trigger_kind, trigger_ref, input_snapshot_json \
         ) VALUES(?, ?, ?, ?) RETURNING id",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(trigger_kind)
    .bind(trigger_ref)
    .bind(snapshot.to_string())
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to create reconciliation run: {error}"))
}

async fn active_centroids(pool: &SqlitePool) -> Result<Vec<ActiveCentroid>, String> {
    let rows = sqlx::query(
        "SELECT vc.speaker_id, vc.embedding, vc.dispersion \
         FROM voice_centroids vc JOIN speakers s ON s.id=vc.speaker_id \
         WHERE vc.is_active=1 AND s.learning_enabled=1 \
           AND s.consent_state='granted' AND s.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load active voice centroids: {error}"))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            decode_embedding(&row.get::<Vec<u8>, _>("embedding")).map(|embedding| ActiveCentroid {
                speaker_id: row.get("speaker_id"),
                embedding,
                dispersion: row.get::<f64, _>("dispersion") as f32,
            })
        })
        .collect())
}

/// Rescore older unknown clusters after a trusted profile changes. This only
/// creates proposals; it does not update transcripts or voice samples.
pub async fn propose_identity_backfill(
    pool: &SqlitePool,
    trigger_speaker_id: i64,
) -> Result<i64, String> {
    let centroids = active_centroids(pool).await?;
    let profile_version: i64 = sqlx::query_scalar(
        "SELECT profile_version FROM speakers WHERE id=? AND deleted_at IS NULL",
    )
    .bind(trigger_speaker_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load speaker profile version: {error}"))?
    .ok_or_else(|| format!("Speaker {trigger_speaker_id} not found"))?;
    let run_id = create_run(
        pool,
        "speaker_profile_updated",
        &trigger_speaker_id.to_string(),
        json!({
            "speaker_id": trigger_speaker_id,
            "profile_version": profile_version,
            "voice_floor": BACKFILL_VOICE_FLOOR,
            "margin": BACKFILL_MARGIN
        }),
    )
    .await?;

    let clusters = sqlx::query(
        "SELECT sc.id, sc.meeting_id, sc.operational_speaker_id, sc.embedding, \
                sc.speech_duration_ms, sc.speech_quality \
         FROM speaker_clusters sc \
         WHERE sc.embedding IS NOT NULL \
           AND NOT EXISTS( \
             SELECT 1 FROM identity_assertions ia WHERE ia.cluster_id=sc.id \
               AND ia.id=(SELECT MAX(latest.id) FROM identity_assertions latest \
                          WHERE latest.cluster_id=sc.id) \
               AND ia.trust_tier='trusted' AND ia.polarity='positive' \
           ) ORDER BY sc.id DESC LIMIT 2000",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load historical speaker clusters: {error}"))?;

    let mut inserted = 0usize;
    for row in clusters {
        if inserted >= MAX_PROPOSALS_PER_RUN {
            break;
        }
        let cluster_id: i64 = row.get("id");
        let Some(embedding) = decode_embedding(&row.get::<Vec<u8>, _>("embedding")) else {
            continue;
        };
        let mut per_speaker: HashMap<i64, f32> = HashMap::new();
        for centroid in &centroids {
            let score = (cosine_similarity(&embedding, &centroid.embedding)
                - centroid.dispersion * 0.10)
                .clamp(-1.0, 1.0);
            per_speaker
                .entry(centroid.speaker_id)
                .and_modify(|best| *best = best.max(score))
                .or_insert(score);
        }
        let mut ranked: Vec<_> = per_speaker.into_iter().collect();
        ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
        let Some((top_speaker, top_score)) = ranked.first().copied() else {
            continue;
        };
        let margin = top_score - ranked.get(1).map(|item| item.1).unwrap_or(0.0);
        if top_speaker != trigger_speaker_id
            || top_score < BACKFILL_VOICE_FLOOR
            || margin < BACKFILL_MARGIN
        {
            continue;
        }
        let current_speaker: Option<i64> = row.get("operational_speaker_id");
        if current_speaker == Some(trigger_speaker_id) {
            continue;
        }
        sqlx::query(
            "INSERT INTO reconciliation_suggestions( \
                run_id, meeting_id, target_type, target_id, suggestion_kind, \
                previous_value_json, proposed_value_json, confidence, evidence_json \
             ) VALUES(?, ?, 'speaker_cluster', ?, 'identity_backfill', ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(row.get::<String, _>("meeting_id"))
        .bind(cluster_id.to_string())
        .bind(json!({"speaker_id": current_speaker}).to_string())
        .bind(json!({"speaker_id": trigger_speaker_id}).to_string())
        .bind(top_score as f64)
        .bind(
            json!({
                "voice_score": top_score,
                "top2_margin": margin,
                "duration_ms": row.get::<i64, _>("speech_duration_ms"),
                "speech_quality": row.get::<Option<f64>, _>("speech_quality"),
                "profile_version": profile_version
            })
            .to_string(),
        )
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to store identity backfill proposal: {error}"))?;
        inserted += 1;
    }
    if inserted == 0 {
        sqlx::query(
            "UPDATE reconciliation_runs SET status='applied', completed_at=datetime('now') WHERE id=?",
        )
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to close empty reconciliation run: {error}"))?;
    }
    Ok(run_id)
}

fn replace_case_insensitive(text: &str, needle: &str, replacement: &str) -> Option<String> {
    if needle.trim().is_empty() {
        return None;
    }
    let lower_text = text.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let start = lower_text.find(&lower_needle)?;
    let end = start + lower_needle.len();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    let mut result = String::with_capacity(text.len() + replacement.len());
    result.push_str(&text[..start]);
    result.push_str(replacement);
    result.push_str(&text[end..]);
    Some(result)
}

/// Find older transcripts containing a confirmed alias. Each replacement is a
/// separate review item and preserves the raw ASR text.
pub async fn propose_terminology_backfill(pool: &SqlitePool, term_id: i64) -> Result<i64, String> {
    let term = sqlx::query(
        "SELECT scope_kind, scope_id, canonical, version FROM terminology_terms \
         WHERE id=? AND status='confirmed'",
    )
    .bind(term_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load confirmed term: {error}"))?
    .ok_or_else(|| format!("Confirmed terminology term {term_id} not found"))?;
    let scope_kind: String = term.get("scope_kind");
    let scope_id: Option<i64> = term.get("scope_id");
    let canonical: String = term.get("canonical");
    let version: i64 = term.get("version");
    let aliases: Vec<String> = sqlx::query_scalar(
        "SELECT alias FROM terminology_aliases WHERE term_id=? AND status='confirmed'",
    )
    .bind(term_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load confirmed aliases: {error}"))?;
    let run_id = create_run(
        pool,
        "terminology_confirmed",
        &term_id.to_string(),
        json!({"term_id": term_id, "version": version, "scope_kind": scope_kind, "scope_id": scope_id}),
    )
    .await?;

    let rows = sqlx::query(
        "SELECT t.id, t.meeting_id, t.transcript, t.transcript_version \
         FROM transcripts t WHERE ( \
           ?='global' OR EXISTS(SELECT 1 FROM meeting_collections mc \
                                WHERE mc.meeting_id=t.meeting_id AND mc.collection_id=?) \
         ) ORDER BY t.timestamp ASC LIMIT 10000",
    )
    .bind(&scope_kind)
    .bind(scope_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to scan transcript terminology: {error}"))?;
    let mut inserted = 0usize;
    for row in rows {
        if inserted >= MAX_PROPOSALS_PER_RUN {
            break;
        }
        let previous: String = row.get("transcript");
        let mut proposed = None;
        let mut matched_alias = None;
        for alias in &aliases {
            if let Some(candidate) = replace_case_insensitive(&previous, alias, &canonical) {
                if candidate != previous {
                    proposed = Some(candidate);
                    matched_alias = Some(alias.clone());
                    break;
                }
            }
        }
        let (Some(corrected_text), Some(alias)) = (proposed, matched_alias) else {
            continue;
        };
        let transcript_id: String = row.get("id");
        sqlx::query(
            "INSERT INTO reconciliation_suggestions( \
                run_id, meeting_id, target_type, target_id, suggestion_kind, \
                previous_value_json, proposed_value_json, confidence, evidence_json \
             ) VALUES(?, ?, 'transcript', ?, 'terminology_backfill', ?, ?, 0.90, ?)",
        )
        .bind(run_id)
        .bind(row.get::<String, _>("meeting_id"))
        .bind(&transcript_id)
        .bind(
            json!({
                "text": previous,
                "version": row.get::<i64, _>("transcript_version")
            })
            .to_string(),
        )
        .bind(json!({"text": corrected_text, "term_id": term_id}).to_string())
        .bind(json!({"alias": alias, "canonical": canonical, "term_version": version}).to_string())
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to store terminology backfill proposal: {error}"))?;
        inserted += 1;
    }
    if inserted == 0 {
        sqlx::query(
            "UPDATE reconciliation_runs SET status='applied', completed_at=datetime('now') WHERE id=?",
        )
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to close empty reconciliation run: {error}"))?;
    }
    Ok(run_id)
}

pub async fn list_suggestions(
    pool: &SqlitePool,
    meeting_id: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<ReconciliationSuggestion>, String> {
    let rows = sqlx::query(
        "SELECT id, run_id, meeting_id, target_type, target_id, suggestion_kind, \
                previous_value_json, proposed_value_json, confidence, evidence_json, \
                status, created_at FROM reconciliation_suggestions \
         WHERE (? IS NULL OR meeting_id=?) AND (? IS NULL OR status=?) \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(meeting_id)
    .bind(meeting_id)
    .bind(status)
    .bind(status)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load reconciliation suggestions: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| ReconciliationSuggestion {
            id: row.get("id"),
            run_id: row.get("run_id"),
            meeting_id: row.get("meeting_id"),
            target_type: row.get("target_type"),
            target_id: row.get("target_id"),
            suggestion_kind: row.get("suggestion_kind"),
            previous_value: serde_json::from_str(&row.get::<String, _>("previous_value_json"))
                .unwrap_or(Value::Null),
            proposed_value: serde_json::from_str(&row.get::<String, _>("proposed_value_json"))
                .unwrap_or(Value::Null),
            confidence: row.get("confidence"),
            evidence: serde_json::from_str(&row.get::<String, _>("evidence_json"))
                .unwrap_or(Value::Null),
            status: row.get("status"),
            created_at: row.get("created_at"),
        })
        .collect())
}

async fn refresh_run_status(pool: &SqlitePool, run_id: i64) -> Result<(), String> {
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reconciliation_suggestions WHERE run_id=? AND status='pending'",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to count pending reconciliation: {error}"))?;
    let applied: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reconciliation_suggestions WHERE run_id=? AND status='applied'",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to count applied reconciliation: {error}"))?;
    let rolled_back: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reconciliation_suggestions WHERE run_id=? AND status='rolled_back'",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to count rolled-back reconciliation: {error}"))?;
    let status = if pending > 0 {
        if applied > 0 {
            "partially_applied"
        } else {
            "proposed"
        }
    } else if applied > 0 {
        "applied"
    } else if rolled_back > 0 {
        "rolled_back"
    } else {
        "rejected"
    };
    sqlx::query(
        "UPDATE reconciliation_runs SET status=?, completed_at=CASE WHEN ?='proposed' \
         OR ?='partially_applied' THEN NULL ELSE datetime('now') END WHERE id=?",
    )
    .bind(status)
    .bind(status)
    .bind(status)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to update reconciliation run: {error}"))?;
    Ok(())
}

pub async fn review_suggestion(
    pool: &SqlitePool,
    input: ReviewReconciliationInput,
) -> Result<(), String> {
    if !matches!(input.decision.as_str(), "accepted" | "rejected") {
        return Err("decision must be accepted or rejected".to_string());
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin reconciliation review: {error}"))?;
    let row = sqlx::query(
        "SELECT run_id, meeting_id, target_type, target_id, suggestion_kind, \
                proposed_value_json \
         FROM reconciliation_suggestions WHERE id=? AND status='pending'",
    )
    .bind(input.suggestion_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("Failed to load reconciliation suggestion: {error}"))?
    .ok_or_else(|| format!("Pending suggestion {} not found", input.suggestion_id))?;
    let run_id: i64 = row.get("run_id");
    if input.decision == "accepted" {
        let proposed: Value = serde_json::from_str(&row.get::<String, _>("proposed_value_json"))
            .map_err(|error| format!("Invalid proposed value: {error}"))?;
        match row.get::<String, _>("suggestion_kind").as_str() {
            "identity_backfill" => {
                let cluster_id = row
                    .get::<String, _>("target_id")
                    .parse::<i64>()
                    .map_err(|_| "Invalid speaker cluster id".to_string())?;
                let speaker_id = proposed["speaker_id"]
                    .as_i64()
                    .ok_or_else(|| "Identity proposal has no speaker_id".to_string())?;
                let speaker_exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM speakers WHERE id=? AND deleted_at IS NULL)",
                )
                .bind(speaker_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| format!("Failed to validate proposed speaker: {error}"))?;
                if !speaker_exists {
                    return Err(format!("Speaker {speaker_id} is unavailable"));
                }
                let latest_assertion: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM identity_assertions WHERE cluster_id=? ORDER BY id DESC LIMIT 1",
                )
                .bind(cluster_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| format!("Failed to load prior identity assertion: {error}"))?;
                sqlx::query(
                    "INSERT INTO identity_assertions( \
                        assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                        trust_tier, confidence, reason, supersedes_id \
                     ) VALUES(?, ?, ?, 'positive', 'cluster', 'user', 'trusted', 1.0, \
                              'reconciliation_confirmed_identity', ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(cluster_id)
                .bind(speaker_id)
                .bind(latest_assertion)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to apply identity reconciliation: {error}"))?;
                sqlx::query("UPDATE speaker_clusters SET operational_speaker_id=? WHERE id=?")
                    .bind(speaker_id)
                    .bind(cluster_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("Failed to update reconciled cluster: {error}"))?;
                sqlx::query(
                    "UPDATE transcripts SET speaker_id=? WHERE id IN( \
                        SELECT transcript_id FROM speaker_cluster_segments WHERE cluster_id=? \
                     )",
                )
                .bind(speaker_id)
                .bind(cluster_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to update reconciled transcripts: {error}"))?;
                sqlx::query(
                    "INSERT INTO learning_events( \
                        event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, \
                        trust_tier, scope, payload_json \
                     ) VALUES(?, ?, 'reconciliation_applied', 'speaker_cluster', ?, 'user', \
                              'trusted', 'cluster', ?)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(row.get::<Option<String>, _>("meeting_id"))
                .bind(cluster_id.to_string())
                .bind(
                    json!({"speaker_id": speaker_id, "suggestion_id": input.suggestion_id})
                        .to_string(),
                )
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to audit identity reconciliation: {error}"))?;
            }
            "terminology_backfill" => {
                let corrected_text = proposed["text"]
                    .as_str()
                    .ok_or_else(|| "Terminology proposal has no text".to_string())?;
                crate::learning::terminology::correct_transcript_in_transaction(
                    &mut tx,
                    crate::learning::terminology::TranscriptCorrectionInput {
                        transcript_id: row.get("target_id"),
                        corrected_text: corrected_text.to_string(),
                    },
                )
                .await?;
            }
            other => return Err(format!("Unsupported reconciliation kind: {other}")),
        }
    }
    sqlx::query(
        "UPDATE reconciliation_suggestions SET status=?, reviewed_at=datetime('now') WHERE id=?",
    )
    .bind(if input.decision == "accepted" {
        "applied"
    } else {
        "rejected"
    })
    .bind(input.suggestion_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to save reconciliation review: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit reconciliation review: {error}"))?;
    refresh_run_status(pool, run_id).await
}

pub async fn rollback_suggestion(pool: &SqlitePool, suggestion_id: i64) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin reconciliation rollback: {error}"))?;
    let row = sqlx::query(
        "SELECT run_id, meeting_id, target_id, suggestion_kind, previous_value_json \
         FROM reconciliation_suggestions WHERE id=? AND status='applied'",
    )
    .bind(suggestion_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("Failed to load applied suggestion: {error}"))?
    .ok_or_else(|| format!("Applied suggestion {suggestion_id} not found"))?;
    let previous: Value = serde_json::from_str(&row.get::<String, _>("previous_value_json"))
        .map_err(|error| format!("Invalid previous value: {error}"))?;
    match row.get::<String, _>("suggestion_kind").as_str() {
        "terminology_backfill" => {
            let text = previous["text"]
                .as_str()
                .ok_or_else(|| "Previous transcript text missing".to_string())?;
            crate::learning::terminology::correct_transcript_in_transaction(
                &mut tx,
                crate::learning::terminology::TranscriptCorrectionInput {
                    transcript_id: row.get("target_id"),
                    corrected_text: text.to_string(),
                },
            )
            .await?;
        }
        "identity_backfill" => {
            let cluster_id = row
                .get::<String, _>("target_id")
                .parse::<i64>()
                .map_err(|_| "Invalid speaker cluster id".to_string())?;
            let speaker_id = previous["speaker_id"].as_i64();
            let fallback: Option<i64> = sqlx::query_scalar(
                "SELECT placeholder_speaker_id FROM speaker_clusters WHERE id=?",
            )
            .bind(cluster_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("Failed to load Unknown speaker: {error}"))?
            .flatten();
            let restored = speaker_id.or(fallback).ok_or_else(|| {
                "Cannot restore identity because its previous speaker was deleted".to_string()
            })?;
            let latest_assertion: Option<(i64, Option<i64>)> = sqlx::query_as(
                "SELECT id, supersedes_id FROM identity_assertions \
                 WHERE cluster_id=? ORDER BY id DESC LIMIT 1",
            )
            .bind(cluster_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("Failed to load applied identity assertion: {error}"))?;
            let prior_assertion: Option<(Option<i64>, String, String)> =
                if let Some((_, Some(prior_id))) = latest_assertion {
                    sqlx::query_as(
                        "SELECT speaker_id, polarity, scope FROM identity_assertions WHERE id=?",
                    )
                    .bind(prior_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|error| format!("Failed to load superseded identity: {error}"))?
                } else {
                    None
                };
            let (asserted_speaker, polarity, scope) = prior_assertion
                .unwrap_or_else(|| (None, "unknown".to_string(), "cluster".to_string()));
            sqlx::query("UPDATE speaker_clusters SET operational_speaker_id=? WHERE id=?")
                .bind(restored)
                .bind(cluster_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to restore cluster identity: {error}"))?;
            sqlx::query(
                "UPDATE transcripts SET speaker_id=? WHERE id IN( \
                    SELECT transcript_id FROM speaker_cluster_segments WHERE cluster_id=? \
                 )",
            )
            .bind(restored)
            .bind(cluster_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to restore transcript identity: {error}"))?;
            sqlx::query(
                "INSERT INTO identity_assertions( \
                    assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                    trust_tier, confidence, reason, supersedes_id \
                 ) VALUES(?, ?, ?, ?, ?, 'user', 'trusted', 1.0, \
                          'reconciliation_identity_rolled_back', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(cluster_id)
            .bind(asserted_speaker)
            .bind(polarity)
            .bind(scope)
            .bind(latest_assertion.map(|(id, _)| id))
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to supersede reconciled identity: {error}"))?;
            sqlx::query(
                "INSERT INTO learning_events( \
                    event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, trust_tier, \
                    scope, payload_json \
                 ) VALUES(?, ?, 'reconciliation_rolled_back', 'speaker_cluster', ?, \
                          'user', 'trusted', 'cluster', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(row.get::<Option<String>, _>("meeting_id"))
            .bind(cluster_id.to_string())
            .bind(
                json!({"restored_speaker_id": restored, "suggestion_id": suggestion_id})
                    .to_string(),
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to log identity rollback: {error}"))?;
        }
        other => return Err(format!("Unsupported reconciliation kind: {other}")),
    }
    let run_id: i64 = row.get("run_id");
    sqlx::query(
        "UPDATE reconciliation_suggestions SET status='rolled_back', reviewed_at=datetime('now') \
         WHERE id=?",
    )
    .bind(suggestion_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to mark reconciliation rollback: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit reconciliation rollback: {error}"))?;
    refresh_run_status(pool, run_id).await
}

#[tauri::command]
pub async fn list_reconciliation_suggestions(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: Option<String>,
    status: Option<String>,
) -> Result<Vec<ReconciliationSuggestion>, String> {
    list_suggestions(
        state.db_manager.pool(),
        meeting_id.as_deref(),
        status.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn review_reconciliation_suggestion(
    state: tauri::State<'_, crate::state::AppState>,
    input: ReviewReconciliationInput,
) -> Result<(), String> {
    review_suggestion(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn rollback_reconciliation_suggestion(
    state: tauri::State<'_, crate::state::AppState>,
    suggestion_id: i64,
) -> Result<(), String> {
    rollback_suggestion(state.db_manager.pool(), suggestion_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_is_case_insensitive_and_bounded() {
        assert_eq!(
            replace_case_insensitive("Обновили гига тул сегодня", "ГИГА ТУЛ", "GigaTool"),
            Some("Обновили GigaTool сегодня".to_string())
        );
        assert_eq!(replace_case_insensitive("alpha", "beta", "x"), None);
    }
}
