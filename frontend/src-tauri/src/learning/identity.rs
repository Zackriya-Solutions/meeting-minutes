use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::database::repositories::speaker::{decode_embedding, encode_embedding};
use crate::pipeline::diarization::{cosine_similarity, SpeakerTurn};

pub const VOICE_MODEL_VERSION: &str = "wespeaker_campp_v1";
pub const FUSION_MODEL_VERSION: &str = "voice_series_v1";
const MAX_CENTROIDS: usize = 5;
const NEW_MODE_THRESHOLD: f32 = 0.78;
const MIN_TRUSTED_DURATION_MS: i64 = 15_000;
const MIN_TRUSTED_QUALITY: f64 = 0.65;
const MAX_TRUSTED_OVERLAP: f64 = 0.10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityPolicy {
    pub auto_assign_enabled: bool,
    pub voice_floor: f32,
    pub confirm_threshold: f32,
    pub auto_assign_threshold: f32,
    pub min_confirm_margin: f32,
    pub min_auto_margin: f32,
    pub min_duration_ms: i64,
    pub min_quality: f64,
    pub max_context_boost: f32,
}

impl Default for IdentityPolicy {
    fn default() -> Self {
        Self {
            auto_assign_enabled: false,
            voice_floor: 0.72,
            confirm_threshold: 0.76,
            auto_assign_threshold: 0.90,
            min_confirm_margin: 0.03,
            min_auto_margin: 0.08,
            min_duration_ms: MIN_TRUSTED_DURATION_MS,
            min_quality: MIN_TRUSTED_QUALITY,
            max_context_boost: 0.05,
        }
    }
}

impl IdentityPolicy {
    async fn load(pool: &SqlitePool) -> Self {
        let mut policy = Self::default();
        let settings: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM app_settings_kv WHERE key LIKE 'identity.%'")
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        for (key, value) in settings {
            match key.as_str() {
                "identity.auto_assign_enabled" => {
                    policy.auto_assign_enabled = value.eq_ignore_ascii_case("true")
                }
                "identity.voice_floor" => {
                    if let Ok(parsed) = value.parse() {
                        policy.voice_floor = parsed;
                    }
                }
                "identity.confirm_threshold" => {
                    if let Ok(parsed) = value.parse() {
                        policy.confirm_threshold = parsed;
                    }
                }
                "identity.auto_assign_threshold" => {
                    if let Ok(parsed) = value.parse() {
                        policy.auto_assign_threshold = parsed;
                    }
                }
                _ => {}
            }
        }
        policy
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCandidate {
    pub speaker_id: i64,
    pub display_name: String,
    pub voice_score: f32,
    pub context_boost: f32,
    pub combined_score: f32,
    pub confidence_band: String,
    pub explanation_factors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityDecision {
    AutoAssign,
    Confirm,
    Unknown,
}

impl IdentityDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::AutoAssign => "auto_assign",
            Self::Confirm => "confirm",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct CentroidCandidate {
    speaker_id: i64,
    display_name: String,
    embedding: Vec<f32>,
    dispersion: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityReviewItem {
    pub cluster_id: i64,
    pub local_cluster_id: i64,
    pub placeholder_speaker_id: Option<i64>,
    pub operational_speaker_id: Option<i64>,
    pub speech_duration_ms: i64,
    pub speech_quality: Option<f64>,
    pub policy_result: String,
    pub candidates: Vec<IdentityCandidate>,
    pub latest_assertion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerProfileVersionRow {
    pub version: i64,
    pub is_active: bool,
    pub sample_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewIdentityInput {
    pub cluster_id: i64,
    pub decision: String,
    #[serde(default)]
    pub speaker_id: Option<i64>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub rejected_speaker_id: Option<i64>,
    #[serde(default)]
    pub allow_learning: bool,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "cluster".to_string()
}

fn l2_normalize(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in values {
            *value /= norm;
        }
    }
}

fn confidence_band(score: f32, margin: f32) -> String {
    if score >= 0.90 && margin >= 0.08 {
        "high"
    } else if score >= 0.76 && margin >= 0.03 {
        "medium"
    } else {
        "low"
    }
    .to_string()
}

fn decide_identity(
    policy: &IdentityPolicy,
    candidates: &[IdentityCandidate],
    duration_ms: i64,
    quality: f64,
) -> IdentityDecision {
    let Some(top) = candidates.first() else {
        return IdentityDecision::Unknown;
    };
    let margin = top.combined_score
        - candidates
            .get(1)
            .map(|candidate| candidate.combined_score)
            .unwrap_or(0.0);
    if duration_ms < policy.min_duration_ms
        || quality < policy.min_quality
        || top.voice_score < policy.voice_floor
    {
        return IdentityDecision::Unknown;
    }
    if policy.auto_assign_enabled
        && top.combined_score >= policy.auto_assign_threshold
        && margin >= policy.min_auto_margin
    {
        return IdentityDecision::AutoAssign;
    }
    if top.combined_score >= policy.confirm_threshold && margin >= policy.min_confirm_margin {
        return IdentityDecision::Confirm;
    }
    IdentityDecision::Unknown
}

fn cluster_duration_ms(cluster_id: i64, turns: &[SpeakerTurn]) -> i64 {
    turns
        .iter()
        .filter(|turn| turn.cluster_id == cluster_id)
        .map(|turn| (turn.end_ms - turn.start_ms).max(0))
        .sum()
}

fn quality_from_duration(duration_ms: i64) -> f64 {
    match duration_ms {
        30_000.. => 0.90,
        15_000.. => 0.80,
        5_000.. => 0.60,
        _ => 0.40,
    }
}

async fn active_centroids(pool: &SqlitePool) -> Result<Vec<CentroidCandidate>, String> {
    let rows = sqlx::query(
        "SELECT vc.speaker_id, s.display_name, vc.embedding, vc.dispersion \
         FROM voice_centroids vc \
         JOIN speakers s ON s.id=vc.speaker_id \
         WHERE vc.is_active=1 AND s.is_confirmed=1 AND s.learning_enabled=1 \
           AND s.consent_state='granted' AND s.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load voice centroids: {error}"))?;

    let mut centroids = Vec::with_capacity(rows.len());
    for row in rows {
        let blob: Vec<u8> = row.get("embedding");
        if let Some(embedding) = decode_embedding(&blob) {
            centroids.push(CentroidCandidate {
                speaker_id: row.get("speaker_id"),
                display_name: row.get("display_name"),
                embedding,
                dispersion: row.get::<f64, _>("dispersion") as f32,
            });
        }
    }
    Ok(centroids)
}

async fn series_context_weights(pool: &SqlitePool, meeting_id: &str) -> HashMap<i64, f32> {
    sqlx::query(
        "SELECT ce.source_speaker_id, MAX(ce.weight) AS weight \
         FROM meeting_collections mc \
         JOIN collections c ON c.id=mc.collection_id AND c.kind='series' \
         JOIN context_edges ce ON ce.collection_id=c.id \
            AND ce.edge_type='series_attendance' AND ce.valid_to IS NULL \
         WHERE mc.meeting_id=? AND ce.support_count >= 2 \
         GROUP BY ce.source_speaker_id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|row| {
        (
            row.get::<i64, _>("source_speaker_id"),
            row.get::<f64, _>("weight") as f32,
        )
    })
    .collect()
}

async fn negative_speakers(pool: &SqlitePool, cluster_id: i64) -> HashSet<i64> {
    sqlx::query_scalar(
        "SELECT DISTINCT speaker_id FROM identity_assertions \
         WHERE cluster_id=? AND polarity='negative' AND trust_tier='trusted' \
           AND speaker_id IS NOT NULL",
    )
    .bind(cluster_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect()
}

async fn score_candidates(
    pool: &SqlitePool,
    meeting_id: &str,
    cluster_id: i64,
    embedding: &[f32],
    policy: &IdentityPolicy,
) -> Result<Vec<IdentityCandidate>, String> {
    let centroids = active_centroids(pool).await?;
    let context = series_context_weights(pool, meeting_id).await;
    let negatives = negative_speakers(pool, cluster_id).await;
    let mut by_speaker: HashMap<i64, IdentityCandidate> = HashMap::new();

    for centroid in centroids {
        if negatives.contains(&centroid.speaker_id) {
            continue;
        }
        let similarity = cosine_similarity(embedding, &centroid.embedding);
        let voice_score = (similarity - centroid.dispersion * 0.10).clamp(-1.0, 1.0);
        let context_boost = context
            .get(&centroid.speaker_id)
            .copied()
            .unwrap_or(0.0)
            .mul_add(policy.max_context_boost, 0.0)
            .min(policy.max_context_boost);
        let combined_score = (voice_score + context_boost).clamp(-1.0, 1.0);
        let entry = by_speaker
            .entry(centroid.speaker_id)
            .or_insert_with(|| IdentityCandidate {
                speaker_id: centroid.speaker_id,
                display_name: centroid.display_name.clone(),
                voice_score,
                context_boost,
                combined_score,
                confidence_band: "low".to_string(),
                explanation_factors: Vec::new(),
            });
        if combined_score > entry.combined_score {
            entry.voice_score = voice_score;
            entry.context_boost = context_boost;
            entry.combined_score = combined_score;
        }
    }

    let mut candidates: Vec<_> = by_speaker.into_values().collect();
    candidates.sort_by(|left, right| {
        right
            .combined_score
            .total_cmp(&left.combined_score)
            .then_with(|| left.speaker_id.cmp(&right.speaker_id))
    });
    let second_score = candidates
        .get(1)
        .map(|candidate| candidate.combined_score)
        .unwrap_or(0.0);
    for candidate in &mut candidates {
        let margin = candidate.combined_score - second_score;
        candidate.confidence_band = confidence_band(candidate.combined_score, margin);
        candidate
            .explanation_factors
            .push("voice_match".to_string());
        if candidate.context_boost > 0.0 {
            candidate
                .explanation_factors
                .push("reviewed_series_attendance".to_string());
        }
    }
    candidates.truncate(5);
    Ok(candidates)
}

async fn insert_placeholder(tx: &mut Transaction<'_, Sqlite>, ordinal: i64) -> Result<i64, String> {
    sqlx::query_scalar(
        "INSERT INTO speakers(display_name, voice_embedding, is_confirmed) \
         VALUES(?, NULL, 0) RETURNING id",
    )
    .bind(format!("Speaker {ordinal}"))
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| format!("Failed to create Unknown speaker: {error}"))
}

/// Persist diarized clusters and return the operational speaker used for transcript labels.
/// Model matches never update a profile. With auto-assignment disabled (the default), even a
/// strong match remains a confirmation candidate and the transcript keeps a local Unknown.
pub async fn resolve_clusters(
    pool: &SqlitePool,
    meeting_id: &str,
    diarization_run_id: &str,
    turns: &[SpeakerTurn],
    cluster_embeddings: &[(i64, Vec<f32>)],
) -> Result<HashMap<i64, (i64, i64)>, String> {
    let policy = IdentityPolicy::load(pool).await;
    let mut mapping = HashMap::new();
    let base_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM speakers")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("Failed to count speakers: {error}"))?;

    for (index, (local_cluster_id, embedding)) in cluster_embeddings.iter().enumerate() {
        let duration_ms = cluster_duration_ms(*local_cluster_id, turns);
        let quality = quality_from_duration(duration_ms);
        let embedding_blob = encode_embedding(embedding);
        let cluster_id: i64 = sqlx::query_scalar(
            "INSERT INTO speaker_clusters( \
                meeting_id, diarization_run_id, local_cluster_id, embedding, \
                speech_duration_ms, speech_quality, overlap_ratio, model_version \
             ) VALUES(?, ?, ?, ?, ?, ?, 0.0, ?) RETURNING id",
        )
        .bind(meeting_id)
        .bind(diarization_run_id)
        .bind(local_cluster_id)
        .bind(embedding_blob)
        .bind(duration_ms)
        .bind(quality)
        .bind(VOICE_MODEL_VERSION)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("Failed to persist speaker cluster: {error}"))?;

        let candidates = score_candidates(pool, meeting_id, cluster_id, embedding, &policy).await?;
        let decision = decide_identity(&policy, &candidates, duration_ms, quality);
        let top_margin = candidates.first().map(|candidate| {
            candidate.combined_score
                - candidates
                    .get(1)
                    .map(|second| second.combined_score)
                    .unwrap_or(0.0)
        });
        let factors = candidates
            .first()
            .map(|candidate| candidate.explanation_factors.clone())
            .unwrap_or_default();

        sqlx::query(
            "INSERT INTO identity_inference_runs( \
                cluster_id, voice_model_version, fusion_model_version, candidate_scores_json, \
                policy_result, explanation_factors_json, top_score, top_margin, policy_snapshot_json \
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(cluster_id)
        .bind(VOICE_MODEL_VERSION)
        .bind(FUSION_MODEL_VERSION)
        .bind(serde_json::to_string(&candidates).unwrap_or_else(|_| "[]".to_string()))
        .bind(decision.as_str())
        .bind(serde_json::to_string(&factors).unwrap_or_else(|_| "[]".to_string()))
        .bind(candidates.first().map(|candidate| candidate.combined_score))
        .bind(top_margin)
        .bind(serde_json::to_string(&policy).unwrap_or_else(|_| "{}".to_string()))
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to persist identity inference: {error}"))?;
        for (metric_name, metric_value) in [
            (
                "top_score",
                candidates
                    .first()
                    .map(|candidate| candidate.combined_score as f64)
                    .unwrap_or(0.0),
            ),
            ("top2_margin", top_margin.unwrap_or(0.0) as f64),
            (
                "unknown_decision",
                if decision == IdentityDecision::Unknown {
                    1.0
                } else {
                    0.0
                },
            ),
        ] {
            sqlx::query(
                "INSERT INTO quality_observations( \
                    meeting_id, component, metric_name, metric_value, cohort_json, model_version \
                 ) VALUES(?, 'identity', ?, ?, ?, ?)",
            )
            .bind(meeting_id)
            .bind(metric_name)
            .bind(metric_value)
            .bind(json!({"channel": "unknown", "duration_band": if duration_ms >= 30_000 { "long" } else if duration_ms >= 15_000 { "medium" } else { "short" }}).to_string())
            .bind(FUSION_MODEL_VERSION)
            .execute(pool)
            .await
            .map_err(|error| format!("Failed to persist identity quality observation: {error}"))?;
        }

        let mut tx = pool
            .begin()
            .await
            .map_err(|error| format!("Failed to begin identity transaction: {error}"))?;
        let placeholder_id = insert_placeholder(&mut tx, base_count + index as i64 + 1).await?;
        let operational_id = if decision == IdentityDecision::AutoAssign {
            candidates
                .first()
                .map(|candidate| candidate.speaker_id)
                .unwrap_or(placeholder_id)
        } else {
            placeholder_id
        };
        sqlx::query(
            "UPDATE speaker_clusters SET placeholder_speaker_id=?, operational_speaker_id=? \
             WHERE id=?",
        )
        .bind(placeholder_id)
        .bind(operational_id)
        .bind(cluster_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to link speaker cluster: {error}"))?;

        let (polarity, actor, trust, reason, asserted_speaker) = match decision {
            IdentityDecision::AutoAssign => (
                "positive",
                "policy",
                "operational",
                "policy_auto_assign",
                Some(operational_id),
            ),
            IdentityDecision::Confirm => (
                "positive",
                "model",
                "untrusted",
                "candidate_requires_confirmation",
                candidates.first().map(|candidate| candidate.speaker_id),
            ),
            IdentityDecision::Unknown => (
                "unknown",
                "system",
                "operational",
                "insufficient_evidence",
                None,
            ),
        };
        sqlx::query(
            "INSERT INTO identity_assertions( \
                assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                trust_tier, confidence, reason, model_version \
             ) VALUES(?, ?, ?, ?, 'cluster', ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(cluster_id)
        .bind(asserted_speaker)
        .bind(polarity)
        .bind(actor)
        .bind(trust)
        .bind(candidates.first().map(|candidate| candidate.combined_score))
        .bind(reason)
        .bind(FUSION_MODEL_VERSION)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to persist identity assertion: {error}"))?;
        tx.commit()
            .await
            .map_err(|error| format!("Failed to commit identity transaction: {error}"))?;
        mapping.insert(*local_cluster_id, (operational_id, cluster_id));
    }
    Ok(mapping)
}

pub async fn link_cluster_segment(
    pool: &SqlitePool,
    cluster_id: i64,
    transcript_id: &str,
    overlap_ratio: f64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO speaker_cluster_segments(cluster_id, transcript_id, overlap_ratio) \
         VALUES(?, ?, ?) ON CONFLICT(cluster_id, transcript_id) DO UPDATE \
         SET overlap_ratio=excluded.overlap_ratio",
    )
    .bind(cluster_id)
    .bind(transcript_id)
    .bind(overlap_ratio.clamp(0.0, 1.0))
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to persist cluster segment: {error}"))?;
    Ok(())
}

pub async fn list_identity_review(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<IdentityReviewItem>, String> {
    let rows = sqlx::query(
        "SELECT sc.id, sc.local_cluster_id, sc.placeholder_speaker_id, \
                sc.operational_speaker_id, sc.speech_duration_ms, sc.speech_quality, \
                ir.policy_result, ir.candidate_scores_json, \
                (SELECT ia.polarity || ':' || ia.trust_tier || ':' || ia.reason \
                   FROM identity_assertions ia WHERE ia.cluster_id=sc.id \
                   ORDER BY ia.id DESC LIMIT 1) AS latest_assertion \
         FROM speaker_clusters sc \
         JOIN identity_inference_runs ir ON ir.id=( \
             SELECT id FROM identity_inference_runs WHERE cluster_id=sc.id ORDER BY id DESC LIMIT 1 \
         ) \
         WHERE sc.meeting_id=? AND sc.diarization_run_id=( \
             SELECT diarization_run_id FROM speaker_clusters WHERE meeting_id=? \
             ORDER BY id DESC LIMIT 1 \
         ) ORDER BY sc.local_cluster_id",
    )
    .bind(meeting_id)
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load identity review: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| IdentityReviewItem {
            cluster_id: row.get("id"),
            local_cluster_id: row.get("local_cluster_id"),
            placeholder_speaker_id: row.get("placeholder_speaker_id"),
            operational_speaker_id: row.get("operational_speaker_id"),
            speech_duration_ms: row.get("speech_duration_ms"),
            speech_quality: row.get("speech_quality"),
            policy_result: row.get("policy_result"),
            candidates: serde_json::from_str(&row.get::<String, _>("candidate_scores_json"))
                .unwrap_or_default(),
            latest_assertion: row.get("latest_assertion"),
        })
        .collect())
}

async fn create_or_select_speaker(
    tx: &mut Transaction<'_, Sqlite>,
    placeholder_speaker_id: Option<i64>,
    speaker_id: Option<i64>,
    display_name: Option<&str>,
) -> Result<i64, String> {
    if let Some(speaker_id) = speaker_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM speakers WHERE id=? AND deleted_at IS NULL)",
        )
        .bind(speaker_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| format!("Failed to validate speaker: {error}"))?;
        return exists
            .then_some(speaker_id)
            .ok_or_else(|| format!("Speaker {speaker_id} not found"));
    }
    let name = display_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "A display name or existing speaker is required".to_string())?;
    if let Some(placeholder_id) = placeholder_speaker_id {
        sqlx::query("UPDATE speakers SET display_name=?, is_confirmed=1 WHERE id=?")
            .bind(name)
            .bind(placeholder_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("Failed to confirm speaker: {error}"))?;
        Ok(placeholder_id)
    } else {
        sqlx::query_scalar(
            "INSERT INTO speakers(display_name, is_confirmed) VALUES(?, 1) RETURNING id",
        )
        .bind(name)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| format!("Failed to create speaker: {error}"))
    }
}

async fn append_learning_event(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: Option<&str>,
    event_kind: &str,
    target_type: &str,
    target_id: &str,
    payload: serde_json::Value,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, meeting_id, event_kind, target_type, target_id, actor_kind, \
            trust_tier, scope, payload_json \
         ) VALUES(?, ?, ?, ?, ?, 'user', 'trusted', 'cluster', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(meeting_id)
    .bind(event_kind)
    .bind(target_type)
    .bind(target_id)
    .bind(payload.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("Failed to append learning event: {error}"))?;
    Ok(())
}

pub async fn review_identity(pool: &SqlitePool, input: ReviewIdentityInput) -> Result<(), String> {
    if !matches!(input.scope.as_str(), "cluster" | "meeting" | "global") {
        return Err("scope must be cluster, meeting, or global".to_string());
    }
    let row = sqlx::query(
        "SELECT meeting_id, placeholder_speaker_id, operational_speaker_id, embedding, \
                speech_duration_ms, COALESCE(speech_quality, 0.0) AS speech_quality, \
                COALESCE(overlap_ratio, 1.0) AS overlap_ratio, channel_kind, model_version \
         FROM speaker_clusters WHERE id=?",
    )
    .bind(input.cluster_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load speaker cluster: {error}"))?
    .ok_or_else(|| format!("Speaker cluster {} not found", input.cluster_id))?;

    let meeting_id: String = row.get("meeting_id");
    let placeholder_id: Option<i64> = row.get("placeholder_speaker_id");
    let current_operational: Option<i64> = row.get("operational_speaker_id");
    let embedding_blob: Option<Vec<u8>> = row.get("embedding");
    let duration_ms: i64 = row.get("speech_duration_ms");
    let quality: f64 = row.get("speech_quality");
    let overlap: f64 = row.get("overlap_ratio");
    let channel_kind: String = row.get("channel_kind");
    let model_version: String = row.get("model_version");
    let mut confirmed_target = None;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin identity review: {error}"))?;
    let latest_assertion: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM identity_assertions WHERE cluster_id=? ORDER BY id DESC LIMIT 1",
    )
    .bind(input.cluster_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("Failed to load prior assertion: {error}"))?;

    match input.decision.as_str() {
        "confirm" => {
            let target = create_or_select_speaker(
                &mut tx,
                placeholder_id,
                input.speaker_id,
                input.display_name.as_deref(),
            )
            .await?;
            confirmed_target = Some(target);
            sqlx::query(
                "INSERT INTO identity_assertions( \
                    assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                    trust_tier, confidence, reason, supersedes_id \
                 ) VALUES(?, ?, ?, 'positive', ?, 'user', 'trusted', 1.0, \
                          'user_confirmed_identity', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(input.cluster_id)
            .bind(target)
            .bind(&input.scope)
            .bind(latest_assertion)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to confirm identity: {error}"))?;
            let assertion_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
                .fetch_one(&mut *tx)
                .await
                .map_err(|error| format!("Failed to read assertion id: {error}"))?;

            sqlx::query(
                "UPDATE transcripts SET speaker_id=? WHERE id IN ( \
                    SELECT transcript_id FROM speaker_cluster_segments WHERE cluster_id=? \
                 )",
            )
            .bind(target)
            .bind(input.cluster_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to apply confirmed identity: {error}"))?;
            sqlx::query("UPDATE speaker_clusters SET operational_speaker_id=? WHERE id=?")
                .bind(target)
                .bind(input.cluster_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to update speaker cluster: {error}"))?;

            if input.allow_learning {
                sqlx::query(
                    "UPDATE speakers SET is_confirmed=1, learning_enabled=1, \
                            consent_state='granted' WHERE id=?",
                )
                .bind(target)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to enable speaker learning: {error}"))?;
                if let Some(blob) = embedding_blob.as_ref() {
                    let (eligibility, exclusion_reason) = if duration_ms < MIN_TRUSTED_DURATION_MS {
                        ("excluded", Some("insufficient_duration"))
                    } else if quality < MIN_TRUSTED_QUALITY {
                        ("excluded", Some("insufficient_quality"))
                    } else if overlap > MAX_TRUSTED_OVERLAP {
                        ("excluded", Some("overlap"))
                    } else {
                        ("trusted", None)
                    };
                    sqlx::query(
                        "INSERT INTO voice_samples( \
                            speaker_id, cluster_id, assertion_id, embedding, duration_ms, \
                            speech_quality, overlap_ratio, channel_kind, eligibility, \
                            exclusion_reason, model_version \
                         ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                         ON CONFLICT(speaker_id, cluster_id, assertion_id) DO NOTHING",
                    )
                    .bind(target)
                    .bind(input.cluster_id)
                    .bind(assertion_id)
                    .bind(blob)
                    .bind(duration_ms)
                    .bind(quality)
                    .bind(overlap)
                    .bind(&channel_kind)
                    .bind(eligibility)
                    .bind(exclusion_reason)
                    .bind(&model_version)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| format!("Failed to store voice sample: {error}"))?;
                }
            }
            append_learning_event(
                &mut tx,
                Some(&meeting_id),
                "identity_confirmed",
                "speaker_cluster",
                &input.cluster_id.to_string(),
                json!({"speaker_id": target, "allow_learning": input.allow_learning}),
            )
            .await?;
        }
        "reject" => {
            let rejected = input
                .rejected_speaker_id
                .or(input.speaker_id)
                .ok_or_else(|| "rejectedSpeakerId is required".to_string())?;
            sqlx::query(
                "INSERT INTO identity_assertions( \
                    assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                    trust_tier, confidence, reason, supersedes_id \
                 ) VALUES(?, ?, ?, 'negative', ?, 'user', 'trusted', 1.0, \
                          'user_rejected_identity', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(input.cluster_id)
            .bind(rejected)
            .bind(&input.scope)
            .bind(latest_assertion)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to reject identity: {error}"))?;
            append_learning_event(
                &mut tx,
                Some(&meeting_id),
                "identity_rejected",
                "speaker_cluster",
                &input.cluster_id.to_string(),
                json!({"speaker_id": rejected, "scope": input.scope}),
            )
            .await?;
        }
        "unknown" => {
            sqlx::query(
                "INSERT INTO identity_assertions( \
                    assertion_uuid, cluster_id, speaker_id, polarity, scope, actor_kind, \
                    trust_tier, confidence, reason, supersedes_id \
                 ) VALUES(?, ?, NULL, 'unknown', ?, 'user', 'trusted', 1.0, \
                          'user_kept_unknown', ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(input.cluster_id)
            .bind(&input.scope)
            .bind(latest_assertion)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to keep identity Unknown: {error}"))?;
            let unknown_id = placeholder_id
                .or(current_operational)
                .ok_or_else(|| "Cluster has no local Unknown speaker".to_string())?;
            sqlx::query(
                "UPDATE transcripts SET speaker_id=? WHERE id IN ( \
                    SELECT transcript_id FROM speaker_cluster_segments WHERE cluster_id=? \
                 )",
            )
            .bind(unknown_id)
            .bind(input.cluster_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to restore Unknown identity: {error}"))?;
            sqlx::query("UPDATE speaker_clusters SET operational_speaker_id=? WHERE id=?")
                .bind(unknown_id)
                .bind(input.cluster_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to restore Unknown cluster: {error}"))?;
            append_learning_event(
                &mut tx,
                Some(&meeting_id),
                "identity_unknown",
                "speaker_cluster",
                &input.cluster_id.to_string(),
                json!({}),
            )
            .await?;
        }
        _ => return Err("decision must be confirm, reject, or unknown".to_string()),
    }
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit identity review: {error}"))?;

    if input.decision == "confirm" && input.allow_learning {
        if let Some(target) = confirmed_target {
            match build_profile(pool, target, "trusted_confirmation").await {
                Ok(_) => {
                    if let Err(error) = rebuild_series_context(pool, &meeting_id, target).await {
                        log::warn!("Could not rebuild reviewed series context: {error}");
                    }
                    if let Err(error) =
                        crate::learning::reconciliation::propose_identity_backfill(pool, target)
                            .await
                    {
                        log::warn!("Could not prepare identity backfill proposals: {error}");
                    }
                }
                Err(error) => {
                    // The identity confirmation is still valid. A short/noisy sample is
                    // deliberately excluded from biometric-like learning.
                    log::info!("Speaker confirmed without a profile rebuild: {error}");
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ModeAccumulator {
    centroid: Vec<f32>,
    weighted_sum: Vec<f32>,
    weight: f32,
    samples: Vec<Vec<f32>>,
    channel_hint: Option<String>,
}

impl ModeAccumulator {
    fn new(sample: Vec<f32>, weight: f32, channel: String) -> Self {
        let mut centroid = sample.clone();
        l2_normalize(&mut centroid);
        let weighted_sum = sample.iter().map(|value| value * weight).collect();
        Self {
            centroid,
            weighted_sum,
            weight,
            samples: vec![sample],
            channel_hint: Some(channel),
        }
    }

    fn add(&mut self, sample: Vec<f32>, weight: f32) {
        if sample.len() != self.weighted_sum.len() {
            return;
        }
        for (sum, value) in self.weighted_sum.iter_mut().zip(sample.iter()) {
            *sum += value * weight;
        }
        self.weight += weight;
        self.centroid = self
            .weighted_sum
            .iter()
            .map(|value| *value / self.weight.max(f32::EPSILON))
            .collect();
        l2_normalize(&mut self.centroid);
        self.samples.push(sample);
    }

    fn dispersion(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples
            .iter()
            .map(|sample| 1.0 - cosine_similarity(sample, &self.centroid))
            .sum::<f32>()
            / self.samples.len() as f32
    }
}

pub async fn build_profile(
    pool: &SqlitePool,
    speaker_id: i64,
    reason: &str,
) -> Result<i64, String> {
    let rows = sqlx::query(
        "SELECT embedding, speech_quality, channel_kind FROM voice_samples \
         WHERE speaker_id=? AND eligibility='trusted' ORDER BY id",
    )
    .bind(speaker_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load trusted voice samples: {error}"))?;
    if rows.is_empty() {
        return Err("No trusted eligible samples for this speaker".to_string());
    }

    let mut modes: Vec<ModeAccumulator> = Vec::new();
    for row in rows {
        let blob: Vec<u8> = row.get("embedding");
        let Some(mut sample) = decode_embedding(&blob) else {
            continue;
        };
        l2_normalize(&mut sample);
        let quality = row.get::<f64, _>("speech_quality") as f32;
        let channel: String = row.get("channel_kind");
        let best = modes
            .iter()
            .enumerate()
            .map(|(index, mode)| (index, cosine_similarity(&sample, &mode.centroid)))
            .max_by(|left, right| left.1.total_cmp(&right.1));
        match best {
            None => modes.push(ModeAccumulator::new(sample, quality, channel)),
            Some((_, score)) if score < NEW_MODE_THRESHOLD && modes.len() < MAX_CENTROIDS => {
                modes.push(ModeAccumulator::new(sample, quality, channel));
            }
            Some((index, _)) => modes[index].add(sample, quality),
        }
    }
    if modes.is_empty() {
        return Err("Trusted samples contain no valid embeddings".to_string());
    }

    let current_version: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) FROM speaker_profile_versions WHERE speaker_id=?",
    )
    .bind(speaker_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to load profile version: {error}"))?;
    let new_version = current_version + 1;
    let snapshot = json!({
        "mode_count": modes.len(),
        "sample_count": modes.iter().map(|mode| mode.samples.len()).sum::<usize>(),
        "reason": reason,
        "voice_model_version": VOICE_MODEL_VERSION,
    });
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin profile build: {error}"))?;
    sqlx::query("UPDATE voice_centroids SET is_active=0 WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to retire old centroids: {error}"))?;
    sqlx::query("UPDATE speaker_profile_versions SET is_active=0 WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to retire old profile: {error}"))?;
    sqlx::query(
        "INSERT INTO speaker_profile_versions( \
            speaker_id, version, parent_version, build_reason, snapshot_json, model_version \
         ) VALUES(?, ?, ?, ?, ?, ?)",
    )
    .bind(speaker_id)
    .bind(new_version)
    .bind((current_version > 0).then_some(current_version))
    .bind(reason)
    .bind(snapshot.to_string())
    .bind(VOICE_MODEL_VERSION)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to persist profile version: {error}"))?;

    let mut legacy = vec![0.0f32; modes[0].centroid.len()];
    let mut total_weight = 0.0f32;
    for (index, mode) in modes.iter().enumerate() {
        sqlx::query(
            "INSERT INTO voice_centroids( \
                speaker_id, profile_version, mode_index, embedding, dispersion, \
                sample_count, channel_hint, model_version \
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(speaker_id)
        .bind(new_version)
        .bind(index as i64)
        .bind(encode_embedding(&mode.centroid))
        .bind(mode.dispersion() as f64)
        .bind(mode.samples.len() as i64)
        .bind(&mode.channel_hint)
        .bind(VOICE_MODEL_VERSION)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to persist centroid: {error}"))?;
        let weight = mode.samples.len() as f32;
        for (target, value) in legacy.iter_mut().zip(mode.centroid.iter()) {
            *target += value * weight;
        }
        total_weight += weight;
    }
    for value in &mut legacy {
        *value /= total_weight.max(1.0);
    }
    l2_normalize(&mut legacy);
    sqlx::query(
        "UPDATE speakers SET voice_embedding=?, profile_version=?, is_confirmed=1 WHERE id=?",
    )
    .bind(encode_embedding(&legacy))
    .bind(new_version)
    .bind(speaker_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to publish speaker profile: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit profile build: {error}"))?;
    let average_dispersion =
        modes.iter().map(ModeAccumulator::dispersion).sum::<f32>() / modes.len().max(1) as f32;
    sqlx::query(
        "INSERT INTO quality_observations( \
            component, metric_name, metric_value, cohort_json, model_version \
         ) VALUES('identity', 'profile_dispersion', ?, ?, ?)",
    )
    .bind(average_dispersion as f64)
    .bind(json!({"speaker_id": speaker_id, "profile_version": new_version}).to_string())
    .bind(VOICE_MODEL_VERSION)
    .execute(pool)
    .await
    .map_err(|error| format!("Failed to persist profile quality observation: {error}"))?;
    Ok(new_version)
}

pub async fn rollback_profile(
    pool: &SqlitePool,
    speaker_id: i64,
    version: i64,
) -> Result<(), String> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM speaker_profile_versions \
         WHERE speaker_id=? AND version=?)",
    )
    .bind(speaker_id)
    .bind(version)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to validate profile version: {error}"))?;
    if !exists {
        return Err(format!("Profile version {version} not found"));
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin rollback: {error}"))?;
    sqlx::query("UPDATE voice_centroids SET is_active=(profile_version=?) WHERE speaker_id=?")
        .bind(version)
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to rollback centroids: {error}"))?;
    sqlx::query("UPDATE speaker_profile_versions SET is_active=(version=?) WHERE speaker_id=?")
        .bind(version)
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to rollback profile version: {error}"))?;
    let centroid_blob: Vec<u8> = sqlx::query_scalar(
        "SELECT embedding FROM voice_centroids WHERE speaker_id=? AND profile_version=? \
         ORDER BY sample_count DESC, mode_index LIMIT 1",
    )
    .bind(speaker_id)
    .bind(version)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| format!("Failed to load rollback centroid: {error}"))?;
    sqlx::query("UPDATE speakers SET voice_embedding=?, profile_version=? WHERE id=?")
        .bind(centroid_blob)
        .bind(version)
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to publish rollback: {error}"))?;
    append_learning_event(
        &mut tx,
        None,
        "speaker_profile_rolled_back",
        "speaker",
        &speaker_id.to_string(),
        json!({"version": version}),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit rollback: {error}"))?;
    Ok(())
}

pub async fn list_profile_versions(
    pool: &SqlitePool,
    speaker_id: i64,
) -> Result<Vec<SpeakerProfileVersionRow>, String> {
    let rows = sqlx::query(
        "SELECT version, is_active, sample_count, created_at \
         FROM speaker_profile_versions WHERE speaker_id=? ORDER BY version DESC",
    )
    .bind(speaker_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load speaker profile versions: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| SpeakerProfileVersionRow {
            version: row.get("version"),
            is_active: row.get::<i64, _>("is_active") != 0,
            sample_count: row.get("sample_count"),
            created_at: row.get("created_at"),
        })
        .collect())
}

async fn rebuild_series_context(
    pool: &SqlitePool,
    meeting_id: &str,
    speaker_id: i64,
) -> Result<(), String> {
    let collections: Vec<i64> = sqlx::query_scalar(
        "SELECT c.id FROM meeting_collections mc \
         JOIN collections c ON c.id=mc.collection_id AND c.kind='series' \
         WHERE mc.meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load meeting series: {error}"))?;
    for collection_id in collections {
        let support_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT t.meeting_id) FROM transcripts t \
             JOIN meeting_collections mc ON mc.meeting_id=t.meeting_id \
             WHERE t.speaker_id=? AND mc.collection_id=?",
        )
        .bind(speaker_id)
        .bind(collection_id)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("Failed to count reviewed series attendance: {error}"))?;
        let weight = (support_count as f64 / (support_count as f64 + 3.0)).min(1.0);
        sqlx::query(
            "INSERT INTO context_edges( \
                edge_type, source_speaker_id, target_speaker_id, collection_id, \
                support_count, weight \
             ) VALUES('series_attendance', ?, NULL, ?, ?, ?) \
             ON CONFLICT DO UPDATE SET support_count=excluded.support_count, \
                           weight=excluded.weight, \
                           updated_at=datetime('now'), valid_to=NULL",
        )
        .bind(speaker_id)
        .bind(collection_id)
        .bind(support_count.max(1))
        .bind(weight)
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to update series context: {error}"))?;
    }
    Ok(())
}

pub async fn delete_speaker_learning_data(
    pool: &SqlitePool,
    speaker_id: i64,
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin speaker deletion: {error}"))?;
    // Preserve the transcript and human-readable label, but remove every biometric-like
    // derivative and detach operational identity assignments.
    sqlx::query("UPDATE transcripts SET speaker_id=NULL WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to detach transcript identity: {error}"))?;
    let affected_clusters: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM speaker_clusters WHERE operational_speaker_id=?")
            .bind(speaker_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|error| format!("Failed to load affected speaker clusters: {error}"))?;
    sqlx::query(
        "UPDATE speaker_clusters SET operational_speaker_id=CASE \
             WHEN placeholder_speaker_id=? THEN NULL ELSE placeholder_speaker_id END \
         WHERE operational_speaker_id=?",
    )
    .bind(speaker_id)
    .bind(speaker_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to detach speaker clusters: {error}"))?;
    for cluster_id in affected_clusters {
        sqlx::query(
            "INSERT INTO identity_assertions( \
                assertion_uuid, cluster_id, polarity, scope, actor_kind, trust_tier, reason \
             ) VALUES(?, ?, 'unknown', 'cluster', 'user', 'trusted', \
                      'speaker_learning_data_deleted')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(cluster_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to revoke cluster identity: {error}"))?;
    }
    sqlx::query("DELETE FROM voice_samples WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete voice samples: {error}"))?;
    sqlx::query("DELETE FROM voice_centroids WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete centroids: {error}"))?;
    sqlx::query("DELETE FROM speaker_profile_versions WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete profile versions: {error}"))?;
    sqlx::query("DELETE FROM context_edges WHERE source_speaker_id=? OR target_speaker_id=?")
        .bind(speaker_id)
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete context edges: {error}"))?;
    sqlx::query("DELETE FROM language_profiles WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete language profile: {error}"))?;
    sqlx::query("DELETE FROM conversation_dynamics_profiles WHERE speaker_id=?")
        .bind(speaker_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("Failed to delete dynamics profile: {error}"))?;
    sqlx::query(
        "UPDATE speakers SET voice_embedding=NULL, profile_version=0, learning_enabled=0, \
         consent_state='revoked', deleted_at=datetime('now') WHERE id=?",
    )
    .bind(speaker_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to mark speaker learning data deleted: {error}"))?;
    append_learning_event(
        &mut tx,
        None,
        "speaker_learning_data_deleted",
        "speaker",
        &speaker_id.to_string(),
        json!({"verified": true}),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit speaker deletion: {error}"))?;
    Ok(())
}

#[tauri::command]
pub async fn get_identity_review(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<Vec<IdentityReviewItem>, String> {
    list_identity_review(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn review_speaker_identity(
    state: tauri::State<'_, crate::state::AppState>,
    input: ReviewIdentityInput,
) -> Result<(), String> {
    review_identity(state.db_manager.pool(), input).await
}

#[tauri::command]
pub async fn rollback_speaker_profile(
    state: tauri::State<'_, crate::state::AppState>,
    speaker_id: i64,
    version: i64,
) -> Result<(), String> {
    rollback_profile(state.db_manager.pool(), speaker_id, version).await
}

#[tauri::command]
pub async fn list_speaker_profile_versions(
    state: tauri::State<'_, crate::state::AppState>,
    speaker_id: i64,
) -> Result<Vec<SpeakerProfileVersionRow>, String> {
    list_profile_versions(state.db_manager.pool(), speaker_id).await
}

#[tauri::command]
pub async fn purge_speaker_learning_data(
    state: tauri::State<'_, crate::state::AppState>,
    speaker_id: i64,
) -> Result<(), String> {
    delete_speaker_learning_data(state.db_manager.pool(), speaker_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(score: f32) -> IdentityCandidate {
        IdentityCandidate {
            speaker_id: 1,
            display_name: "Anna".to_string(),
            voice_score: score,
            context_boost: 0.0,
            combined_score: score,
            confidence_band: "high".to_string(),
            explanation_factors: vec![],
        }
    }

    #[test]
    fn prediction_never_auto_assigns_when_policy_is_disabled() {
        let policy = IdentityPolicy::default();
        assert_eq!(
            decide_identity(&policy, &[candidate(0.99)], 60_000, 1.0),
            IdentityDecision::Confirm
        );
    }

    #[test]
    fn low_quality_and_short_samples_stay_unknown() {
        let mut policy = IdentityPolicy::default();
        policy.auto_assign_enabled = true;
        assert_eq!(
            decide_identity(&policy, &[candidate(0.99)], 1_000, 1.0),
            IdentityDecision::Unknown
        );
        assert_eq!(
            decide_identity(&policy, &[candidate(0.99)], 60_000, 0.2),
            IdentityDecision::Unknown
        );
    }

    #[test]
    fn auto_assign_requires_threshold_and_margin() {
        let mut policy = IdentityPolicy::default();
        policy.auto_assign_enabled = true;
        let mut top = candidate(0.95);
        top.speaker_id = 1;
        let mut close = candidate(0.91);
        close.speaker_id = 2;
        assert_eq!(
            decide_identity(&policy, &[top.clone(), close], 60_000, 1.0),
            IdentityDecision::Confirm
        );
        let mut distant = candidate(0.70);
        distant.speaker_id = 2;
        assert_eq!(
            decide_identity(&policy, &[top, distant], 60_000, 1.0),
            IdentityDecision::AutoAssign
        );
    }

    #[test]
    fn multiple_modes_are_preserved_instead_of_blurred() {
        let mut modes = vec![ModeAccumulator::new(vec![1.0, 0.0], 1.0, "mic".into())];
        let sample = vec![0.0, 1.0];
        let best = modes
            .iter()
            .enumerate()
            .map(|(index, mode)| (index, cosine_similarity(&sample, &mode.centroid)))
            .max_by(|left, right| left.1.total_cmp(&right.1));
        if best.is_some_and(|(_, score)| score < NEW_MODE_THRESHOLD) {
            modes.push(ModeAccumulator::new(sample, 1.0, "system".into()));
        }
        assert_eq!(modes.len(), 2);
    }
}
