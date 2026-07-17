//! Opt-in shadow profiles for language and conversation dynamics.
//!
//! These features are intentionally not fused into identity decisions yet. They
//! provide local, inspectable drift/evaluation signals after at least three
//! reviewed meetings and can be deleted with the speaker's voice profile.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};

const MIN_SUPPORT_MEETINGS: usize = 3;

#[derive(Debug, Serialize)]
pub struct AdvancedLearningProfile {
    pub speaker_id: i64,
    pub enabled: bool,
    pub support_meetings: i64,
    pub language_features: serde_json::Value,
    pub dynamics_features: serde_json::Value,
    pub model_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAdvancedLearningInput {
    pub speaker_id: i64,
    pub enabled: bool,
}

fn normalized_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

pub async fn rebuild_shadow_profiles(
    pool: &SqlitePool,
    speaker_id: i64,
) -> Result<AdvancedLearningProfile, String> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM speakers WHERE id=? AND is_confirmed=1 \
         AND learning_enabled=1 AND consent_state='granted' AND deleted_at IS NULL)",
    )
    .bind(speaker_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("Failed to verify advanced-learning consent: {error}"))?;
    if !allowed {
        return Err("Speaker learning must be explicitly enabled first".to_string());
    }
    let rows = sqlx::query(
        "SELECT t.meeting_id, t.transcript, t.audio_start_time, t.audio_end_time \
         FROM transcripts t WHERE t.speaker_id=? AND EXISTS( \
             SELECT 1 FROM speaker_cluster_segments scs \
             JOIN identity_assertions ia ON ia.cluster_id=scs.cluster_id \
             WHERE scs.transcript_id=t.id AND ia.speaker_id=? \
               AND ia.polarity='positive' AND ia.trust_tier='trusted' \
         ) ORDER BY t.meeting_id, t.audio_start_time",
    )
    .bind(speaker_id)
    .bind(speaker_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("Failed to load reviewed speaker turns: {error}"))?;
    let meeting_ids: HashSet<String> = rows
        .iter()
        .map(|row| row.get::<String, _>("meeting_id"))
        .collect();
    if meeting_ids.len() < MIN_SUPPORT_MEETINGS {
        return Err(format!(
            "At least {MIN_SUPPORT_MEETINGS} reviewed meetings are required for advanced profiles"
        ));
    }
    let mut vocabulary = HashSet::new();
    let mut word_count = 0usize;
    let mut filler_count = 0usize;
    let mut total_turn_seconds = 0.0f64;
    let mut duration_count = 0usize;
    let fillers = ["эм", "ну", "значит", "uh", "um", "like"];
    for row in &rows {
        let words = normalized_words(&row.get::<String, _>("transcript"));
        word_count += words.len();
        filler_count += words
            .iter()
            .filter(|word| fillers.contains(&word.as_str()))
            .count();
        vocabulary.extend(words);
        let start: f64 = row.get("audio_start_time");
        let end: f64 = row.get("audio_end_time");
        if end >= start {
            total_turn_seconds += end - start;
            duration_count += 1;
        }
    }
    let support_meetings = meeting_ids.len() as i64;
    let language = json!({
        "average_words_per_turn": word_count as f64 / rows.len().max(1) as f64,
        "vocabulary_size": vocabulary.len(),
        "filler_rate": filler_count as f64 / word_count.max(1) as f64,
        "turn_count": rows.len()
    });
    let dynamics = json!({
        "average_turn_seconds": total_turn_seconds / duration_count.max(1) as f64,
        "turns_per_meeting": rows.len() as f64 / meeting_ids.len() as f64,
        "support_meetings": support_meetings
    });
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("Failed to begin advanced profile update: {error}"))?;
    sqlx::query(
        "INSERT INTO language_profiles( \
            speaker_id, enabled, support_meetings, features_json, model_version, updated_at \
         ) VALUES(?, 1, ?, ?, 'language_shadow_v1', datetime('now')) \
         ON CONFLICT(speaker_id) DO UPDATE SET enabled=1, \
            support_meetings=excluded.support_meetings, features_json=excluded.features_json, \
            model_version=excluded.model_version, updated_at=datetime('now')",
    )
    .bind(speaker_id)
    .bind(support_meetings)
    .bind(language.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to store language shadow profile: {error}"))?;
    sqlx::query(
        "INSERT INTO conversation_dynamics_profiles( \
            speaker_id, enabled, support_meetings, features_json, model_version, updated_at \
         ) VALUES(?, 1, ?, ?, 'dynamics_shadow_v1', datetime('now')) \
         ON CONFLICT(speaker_id) DO UPDATE SET enabled=1, \
            support_meetings=excluded.support_meetings, features_json=excluded.features_json, \
            model_version=excluded.model_version, updated_at=datetime('now')",
    )
    .bind(speaker_id)
    .bind(support_meetings)
    .bind(dynamics.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("Failed to store dynamics shadow profile: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit advanced profile update: {error}"))?;
    Ok(AdvancedLearningProfile {
        speaker_id,
        enabled: true,
        support_meetings,
        language_features: language,
        dynamics_features: dynamics,
        model_version: "shadow_v1".to_string(),
    })
}

pub async fn load_profile(
    pool: &SqlitePool,
    speaker_id: i64,
) -> Result<AdvancedLearningProfile, String> {
    let row = sqlx::query(
        "SELECT COALESCE(lp.enabled, 0) AS enabled, \
                MAX(COALESCE(lp.support_meetings, 0), COALESCE(dp.support_meetings, 0)) AS support, \
                COALESCE(lp.features_json, '{}') AS language, \
                COALESCE(dp.features_json, '{}') AS dynamics \
         FROM speakers s LEFT JOIN language_profiles lp ON lp.speaker_id=s.id \
         LEFT JOIN conversation_dynamics_profiles dp ON dp.speaker_id=s.id WHERE s.id=?",
    )
    .bind(speaker_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("Failed to load advanced learning profile: {error}"))?
    .ok_or_else(|| format!("Speaker {speaker_id} not found"))?;
    Ok(AdvancedLearningProfile {
        speaker_id,
        enabled: row.get::<i64, _>("enabled") != 0,
        support_meetings: row.get("support"),
        language_features: serde_json::from_str(&row.get::<String, _>("language"))
            .unwrap_or_else(|_| json!({})),
        dynamics_features: serde_json::from_str(&row.get::<String, _>("dynamics"))
            .unwrap_or_else(|_| json!({})),
        model_version: "shadow_v1".to_string(),
    })
}

#[tauri::command]
pub async fn set_speaker_advanced_learning(
    state: tauri::State<'_, crate::state::AppState>,
    input: SetAdvancedLearningInput,
) -> Result<AdvancedLearningProfile, String> {
    let profile = if input.enabled {
        rebuild_shadow_profiles(state.db_manager.pool(), input.speaker_id).await
    } else {
        sqlx::query("UPDATE language_profiles SET enabled=0 WHERE speaker_id=?")
            .bind(input.speaker_id)
            .execute(state.db_manager.pool())
            .await
            .map_err(|error| error.to_string())?;
        sqlx::query("UPDATE conversation_dynamics_profiles SET enabled=0 WHERE speaker_id=?")
            .bind(input.speaker_id)
            .execute(state.db_manager.pool())
            .await
            .map_err(|error| error.to_string())?;
        load_profile(state.db_manager.pool(), input.speaker_id).await
    }?;
    sqlx::query(
        "INSERT INTO learning_events( \
            event_uuid, event_kind, target_type, target_id, actor_kind, trust_tier, \
            scope, payload_json \
         ) VALUES(?, 'advanced_learning_consent_changed', 'speaker', ?, 'user', \
                  'trusted', 'speaker', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(input.speaker_id.to_string())
    .bind(json!({"enabled": input.enabled}).to_string())
    .execute(state.db_manager.pool())
    .await
    .map_err(|error| format!("Failed to log advanced-learning consent: {error}"))?;
    Ok(profile)
}

#[tauri::command]
pub async fn get_speaker_advanced_learning(
    state: tauri::State<'_, crate::state::AppState>,
    speaker_id: i64,
) -> Result<AdvancedLearningProfile, String> {
    load_profile(state.db_manager.pool(), speaker_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenization_is_stable_for_mixed_corporate_terms() {
        assert_eq!(
            normalized_words("Обновили GigaTool-v2"),
            vec!["обновили", "gigatool", "v2"]
        );
    }
}
