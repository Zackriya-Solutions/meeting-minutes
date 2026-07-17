//! Raw-preserving transcript corrections and scoped terminology memory.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCorrectionInput {
    pub transcript_id: String,
    pub corrected_text: String,
}

#[derive(Debug, Serialize)]
pub struct TranscriptCorrectionResult {
    pub correction_id: i64,
    pub meeting_id: String,
    pub transcript_id: String,
    pub raw_text: String,
    pub normalized_text: String,
    pub version: i64,
    pub terminology_candidate_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TerminologyTermRow {
    pub id: i64,
    pub scope_kind: String,
    pub scope_id: Option<i64>,
    pub canonical: String,
    pub aliases: Vec<String>,
    pub status: String,
    pub confidence: f64,
    pub support_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTerminologyInput {
    pub term_id: i64,
    pub status: String,
    #[serde(default)]
    pub canonical: Option<String>,
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn changed_phrase(before: &str, after: &str) -> Option<(String, String)> {
    let before_words: Vec<&str> = before.split_whitespace().collect();
    let after_words: Vec<&str> = after.split_whitespace().collect();
    let mut prefix = 0usize;
    while prefix < before_words.len()
        && prefix < after_words.len()
        && normalize(before_words[prefix]) == normalize(after_words[prefix])
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < before_words.len().saturating_sub(prefix)
        && suffix < after_words.len().saturating_sub(prefix)
        && normalize(before_words[before_words.len() - 1 - suffix])
            == normalize(after_words[after_words.len() - 1 - suffix])
    {
        suffix += 1;
    }
    let before_end = before_words.len().saturating_sub(suffix);
    let after_end = after_words.len().saturating_sub(suffix);
    let alias = before_words[prefix..before_end].join(" ");
    let canonical = after_words[prefix..after_end].join(" ");
    let alias_normalized = normalize(&alias);
    let canonical_normalized = normalize(&canonical);
    if alias_normalized.is_empty()
        || canonical_normalized.is_empty()
        || alias_normalized == canonical_normalized
        || alias.chars().count() > 80
        || canonical.chars().count() > 80
    {
        return None;
    }
    Some((alias, canonical))
}

async fn terminology_scope(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
) -> Result<(String, Option<i64>), String> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.id, c.kind FROM meeting_collections mc \
         JOIN collections c ON c.id=mc.collection_id \
         WHERE mc.meeting_id=? ORDER BY CASE c.kind WHEN 'series' THEN 0 ELSE 1 END, c.id",
    )
    .bind(meeting_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("Failed to resolve terminology scope: {error}"))?;
    if let Some((id, kind)) = rows.first() {
        return Ok((
            if kind == "series" {
                "series"
            } else {
                "collection"
            }
            .to_string(),
            Some(*id),
        ));
    }
    Ok(("global".to_string(), None))
}

async fn upsert_terminology_candidate(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
    transcript_id: &str,
    correction_id: i64,
    alias: &str,
    canonical: &str,
) -> Result<i64, String> {
    let (scope_kind, scope_id) = terminology_scope(tx, meeting_id).await?;
    let normalized_canonical = normalize(canonical);
    let term_id: i64 = sqlx::query_scalar(
        "INSERT INTO terminology_terms( \
            scope_kind, scope_id, canonical, normalized_canonical, confidence, support_count \
         ) VALUES(?, ?, ?, ?, 0.60, 1) \
         ON CONFLICT(scope_kind, COALESCE(scope_id, -1), normalized_canonical) DO UPDATE SET \
            support_count=terminology_terms.support_count+1, \
            confidence=MIN(0.95, terminology_terms.confidence+0.08), \
            last_seen_at=datetime('now') \
         RETURNING id",
    )
    .bind(&scope_kind)
    .bind(scope_id)
    .bind(canonical.trim())
    .bind(&normalized_canonical)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| format!("Failed to store terminology candidate: {error}"))?;
    let normalized_alias = normalize(alias);
    let alias_id: i64 = sqlx::query_scalar(
        "INSERT INTO terminology_aliases(term_id, alias, normalized_alias) \
         VALUES(?, ?, ?) \
         ON CONFLICT(term_id, normalized_alias) DO UPDATE SET \
            support_count=terminology_aliases.support_count+1 \
         RETURNING id",
    )
    .bind(term_id)
    .bind(alias.trim())
    .bind(normalized_alias)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| format!("Failed to store terminology alias: {error}"))?;
    sqlx::query(
        "INSERT INTO terminology_evidence( \
            term_id, alias_id, meeting_id, transcript_id, correction_id, source_kind \
         ) VALUES(?, ?, ?, ?, ?, 'correction') \
         ON CONFLICT(term_id, meeting_id, transcript_id, source_kind) DO NOTHING",
    )
    .bind(term_id)
    .bind(alias_id)
    .bind(meeting_id)
    .bind(transcript_id)
    .bind(correction_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to store terminology evidence: {error}"))?;
    Ok(term_id)
}

pub async fn correct_transcript(
    pool: &SqlitePool,
    input: TranscriptCorrectionInput,
) -> Result<TranscriptCorrectionResult, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin transcript correction: {error}"))?;
    let result = correct_transcript_in_transaction(&mut tx, input).await?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit transcript correction: {error}"))?;
    Ok(result)
}

pub(crate) async fn correct_transcript_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    input: TranscriptCorrectionInput,
) -> Result<TranscriptCorrectionResult, String> {
    let corrected = input.corrected_text.trim();
    if corrected.is_empty() || corrected.chars().count() > 20_000 {
        return Err("Corrected transcript must contain 1–20000 characters".to_string());
    }
    let row = sqlx::query(
        "SELECT meeting_id, transcript, COALESCE(raw_transcript, transcript) AS raw_transcript, \
                transcript_version FROM transcripts WHERE id=?",
    )
    .bind(&input.transcript_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| format!("Failed to load transcript: {error}"))?
    .ok_or_else(|| format!("Transcript {} not found", input.transcript_id))?;
    let meeting_id: String = row.get("meeting_id");
    let previous: String = row.get("transcript");
    let raw: String = row.get("raw_transcript");
    let previous_version: i64 = row.get("transcript_version");
    if previous == corrected {
        return Ok(TranscriptCorrectionResult {
            correction_id: 0,
            meeting_id,
            transcript_id: input.transcript_id,
            raw_text: raw,
            normalized_text: previous,
            version: previous_version,
            terminology_candidate_id: None,
        });
    }
    let new_version = previous_version + 1;
    let correction_uuid = Uuid::new_v4().to_string();
    let update = sqlx::query(
        "UPDATE transcripts SET raw_transcript=COALESCE(raw_transcript, transcript), \
                transcript=?, transcript_version=? WHERE id=? AND transcript_version=?",
    )
    .bind(corrected)
    .bind(new_version)
    .bind(&input.transcript_id)
    .bind(previous_version)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to apply transcript correction: {error}"))?;
    if update.rows_affected() != 1 {
        return Err(
            "Transcript changed while this correction was being saved; reload and try again"
                .to_string(),
        );
    }
    let correction_id: i64 = sqlx::query_scalar(
        "INSERT INTO transcript_corrections( \
            correction_uuid, transcript_id, meeting_id, previous_text, corrected_text, \
            previous_version, new_version \
         ) VALUES(?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&correction_uuid)
    .bind(&input.transcript_id)
    .bind(&meeting_id)
    .bind(&previous)
    .bind(corrected)
    .bind(previous_version)
    .bind(new_version)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| format!("Failed to log transcript correction: {error}"))?;
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, \
            trust_tier, scope, payload_json \
         ) VALUES(?, ?, 'transcript_corrected', 'transcript', ?, 'user', 'trusted', \
                  'segment', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&meeting_id)
    .bind(&input.transcript_id)
    .bind(
        json!({
            "correction_id": correction_id,
            "previous_version": previous_version,
            "new_version": new_version
        })
        .to_string(),
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to append correction event: {error}"))?;
    sqlx::query(
        "UPDATE artifact_versions SET stale=1 WHERE meeting_id=? \
         AND artifact_kind IN ('summary', 'embedding', 'glossary')",
    )
    .bind(&meeting_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to mark derived artifacts stale: {error}"))?;
    sqlx::query(
        "INSERT INTO artifact_versions( \
            meeting_id, artifact_kind, version, source_versions_json \
         ) VALUES(?, 'transcript', ?, ?) \
         ON CONFLICT(meeting_id, artifact_kind, version) DO NOTHING",
    )
    .bind(&meeting_id)
    .bind(new_version)
    .bind(json!({"correction_uuid": correction_uuid}).to_string())
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to version transcript artifact: {error}"))?;
    let terminology_candidate_id = match changed_phrase(&previous, corrected) {
        Some((alias, canonical)) => Some(
            upsert_terminology_candidate(
                tx,
                &meeting_id,
                &input.transcript_id,
                correction_id,
                &alias,
                &canonical,
            )
            .await?,
        ),
        None => None,
    };
    Ok(TranscriptCorrectionResult {
        correction_id,
        meeting_id,
        transcript_id: input.transcript_id,
        raw_text: raw,
        normalized_text: corrected.to_string(),
        version: new_version,
        terminology_candidate_id,
    })
}

pub async fn list_terms(
    pool: &SqlitePool,
    status: Option<&str>,
    meeting_id: Option<&str>,
) -> Result<Vec<TerminologyTermRow>, String> {
    let rows = sqlx::query(
        "SELECT tt.id, tt.scope_kind, tt.scope_id, tt.canonical, tt.status, \
                tt.confidence, tt.support_count, \
                COALESCE(json_group_array(ta.alias) FILTER (WHERE ta.id IS NOT NULL), '[]') AS aliases \
         FROM terminology_terms tt \
         LEFT JOIN terminology_aliases ta ON ta.term_id=tt.id AND ta.status<>'rejected' \
         WHERE (? IS NULL OR tt.status=?) \
           AND (? IS NULL OR EXISTS( \
               SELECT 1 FROM terminology_evidence te \
               WHERE te.term_id=tt.id AND te.meeting_id=? \
           )) \
         GROUP BY tt.id ORDER BY tt.support_count DESC, tt.last_seen_at DESC",
    )
    .bind(status)
    .bind(status)
    .bind(meeting_id)
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load terminology: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| TerminologyTermRow {
            id: row.get("id"),
            scope_kind: row.get("scope_kind"),
            scope_id: row.get("scope_id"),
            canonical: row.get("canonical"),
            aliases: serde_json::from_str(&row.get::<String, _>("aliases")).unwrap_or_default(),
            status: row.get("status"),
            confidence: row.get("confidence"),
            support_count: row.get("support_count"),
        })
        .collect())
}

pub async fn review_term(pool: &SqlitePool, input: ReviewTerminologyInput) -> Result<(), String> {
    if !matches!(input.status.as_str(), "confirmed" | "rejected") {
        return Err("status must be confirmed or rejected".to_string());
    }
    let canonical = input
        .canonical
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin terminology review: {error}"))?;
    let existing =
        sqlx::query("SELECT canonical, status, version FROM terminology_terms WHERE id=?")
            .bind(input.term_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| format!("Failed to load terminology term: {error}"))?
            .ok_or_else(|| format!("Terminology term {} not found", input.term_id))?;
    let previous_canonical: String = existing.get("canonical");
    let previous_status: String = existing.get("status");
    let previous_version: i64 = existing.get("version");
    let next_canonical = canonical.unwrap_or(&previous_canonical);
    let new_version = previous_version + 1;
    let result = sqlx::query(
        "UPDATE terminology_terms SET status=?, canonical=COALESCE(?, canonical), \
                normalized_canonical=COALESCE(?, normalized_canonical), \
                confirmed_at=CASE WHEN ?='confirmed' THEN datetime('now') ELSE NULL END, \
                version=?, last_seen_at=datetime('now') WHERE id=? AND version=?",
    )
    .bind(&input.status)
    .bind(canonical)
    .bind(canonical.map(normalize))
    .bind(&input.status)
    .bind(new_version)
    .bind(input.term_id)
    .bind(previous_version)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to review terminology: {error}"))?;
    if result.rows_affected() != 1 {
        return Err(
            "Terminology changed while it was being reviewed; reload and retry".to_string(),
        );
    }
    sqlx::query(
        "INSERT INTO terminology_term_versions( \
            term_id, previous_canonical, new_canonical, previous_status, new_status, \
            previous_version, new_version \
         ) VALUES(?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.term_id)
    .bind(&previous_canonical)
    .bind(next_canonical)
    .bind(&previous_status)
    .bind(&input.status)
    .bind(previous_version)
    .bind(new_version)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to version terminology review: {error}"))?;
    sqlx::query("UPDATE terminology_aliases SET status=? WHERE term_id=? AND status='pending'")
        .bind(&input.status)
        .bind(input.term_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to review terminology aliases: {error}"))?;
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, event_kind, target_type, target_id, actor_kind, trust_tier, \
            scope, payload_json \
         ) VALUES(?, 'terminology_reviewed', 'terminology_term', ?, 'user', 'trusted', \
                  'terminology', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(input.term_id.to_string())
    .bind(
        json!({
            "previous_canonical": previous_canonical,
            "new_canonical": next_canonical,
            "previous_status": previous_status,
            "new_status": &input.status,
            "previous_version": previous_version,
            "new_version": new_version
        })
        .to_string(),
    )
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to audit terminology review: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit terminology review: {error}"))?;
    if input.status == "confirmed" {
        if let Err(error) =
            crate::learning::reconciliation::propose_terminology_backfill(pool, input.term_id).await
        {
            // The confirmed glossary entry remains useful for future meetings even if
            // historical scanning cannot be completed immediately.
            log::warn!("Could not prepare terminology backfill proposals: {error}");
        }
    }
    Ok(())
}

pub async fn context_for_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Option<String>, String> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT tt.canonical, \
                COALESCE((SELECT group_concat(ta.alias, ', ') FROM terminology_aliases ta \
                          WHERE ta.term_id=tt.id AND ta.status='confirmed'), '') \
         FROM terminology_terms tt \
         WHERE tt.status='confirmed' AND ( \
             tt.scope_kind='global' OR \
             (tt.scope_id IN (SELECT collection_id FROM meeting_collections WHERE meeting_id=?)) \
         ) ORDER BY tt.support_count DESC, tt.canonical LIMIT 100",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load terminology context: {error}"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let terms = rows
        .into_iter()
        .map(|(canonical, aliases)| {
            if aliases.is_empty() {
                canonical
            } else {
                format!("{canonical} (heard variants: {aliases})")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(format!(
        "<terminology_not_evidence>\n{terms}\n</terminology_not_evidence>\n\
         Use canonical spellings when the transcript supports the term. This glossary is context, not evidence."
    )))
}

/// A bounded, reviewed vocabulary hint for Whisper. This is spelling context,
/// not transcript evidence, and contains confirmed canonical forms only.
pub async fn asr_vocabulary_prompt(pool: &SqlitePool) -> Result<Option<String>, String> {
    let terms: Vec<String> = sqlx::query_scalar(
        "SELECT canonical FROM terminology_terms WHERE status='confirmed' \
         ORDER BY support_count DESC, last_seen_at DESC LIMIT 80",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load ASR terminology hints: {error}"))?;
    if terms.is_empty() {
        return Ok(None);
    }
    let mut prompt = String::from("Corporate vocabulary: ");
    for term in terms {
        if prompt.chars().count() + term.chars().count() + 2 > 1000 {
            break;
        }
        if !prompt.ends_with(' ') {
            prompt.push_str(", ");
        }
        prompt.push_str(term.trim());
    }
    Ok(Some(prompt))
}

#[tauri::command]
pub async fn correct_transcript_segment(
    state: tauri::State<'_, crate::state::AppState>,
    input: TranscriptCorrectionInput,
) -> Result<TranscriptCorrectionResult, String> {
    correct_transcript(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn list_terminology_memory(
    state: tauri::State<'_, crate::state::AppState>,
    status: Option<String>,
    meeting_id: Option<String>,
) -> Result<Vec<TerminologyTermRow>, String> {
    list_terms(
        state.db_manager.pool(),
        status.as_deref(),
        meeting_id.as_deref(),
    )
    .await
}

#[tauri::command]
pub async fn review_terminology_memory(
    state: tauri::State<'_, crate::state::AppState>,
    input: ReviewTerminologyInput,
) -> Result<(), String> {
    review_term(state.db_manager.pool(), input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_diff_extracts_only_the_changed_phrase() {
        assert_eq!(
            changed_phrase("обновили гига тул сегодня", "обновили GigaTool сегодня"),
            Some(("гига тул".to_string(), "GigaTool".to_string()))
        );
    }

    #[test]
    fn correction_diff_rejects_noop_and_unbounded_changes() {
        assert!(changed_phrase("same text", "same text").is_none());
        assert!(changed_phrase("a", &"x".repeat(100)).is_none());
    }
}
