//! Conservative, local-only speaker-name suggestions from explicit transcript evidence.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;

use crate::state::AppState;

static SELF_INTRO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?iu)(?:меня\s+зовут|my\s+name\s+is)\s+([\p{L}][\p{L}'’\-]{1,31})")
        .expect("valid self-introduction regex")
});
static EXPLICIT_INTRO: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?iu)(?:с\s+нами(?:\s+сегодня)?|представлю(?:\s+вам)?|познакомьтесь\s*[,—-]?\s*это|это\s+(?:наш|наша|новый|новая)(?:\s+коллега)?)\s+([\p{L}][\p{L}'’\-]{1,31})",
    )
    .expect("valid introduction regex")
});
static DIRECT_ADDRESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?iu)^\s*([\p{L}][\p{L}'’\-]{1,31})\s*[,!]").expect("valid direct-address regex")
});

const BLOCKED_EXACT: &[&str] = &[
    "коллега",
    "коллеги",
    "друзья",
    "ребята",
    "команда",
    "разработчик",
    "аналитик",
    "менеджер",
    "руководитель",
    "директор",
    "заказчик",
    "клиент",
    "спикер",
    "участник",
    "доктор",
    "девушка",
    "мужчина",
    "женщина",
    "человек",
    "начальник",
    "босс",
    "автор",
    "всем",
    "кто",
    "что",
    "привет",
    "спасибо",
    "слушай",
    "слушайте",
    "смотри",
    "смотрите",
    "подожди",
    "подождите",
    "давай",
    "будет",
    "может",
    "просто",
    "кстати",
    "короче",
    "например",
    "итак",
    "конечно",
    "возможно",
    "видимо",
    "наверное",
    "значит",
    "вообще",
    "впрочем",
    "однако",
    "правда",
    "ладно",
    "окей",
    "хорошо",
    "понятно",
    "алло",
    "ага",
    "угу",
    "сегодня",
    "завтра",
    "все",
    "всё",
    "результат",
    "результаты",
    "новость",
    "hello",
    "team",
    "guys",
    "manager",
    "developer",
    "speaker",
    "doctor",
    "client",
    "boss",
];
const BLOCKED_ABUSIVE_EXACT: &[&str] = &[
    "хер",
    "чмо",
    "лох",
    "падла",
    "гнида",
    "урод",
    "дебил",
    "идиот",
    "мудак",
    "мразь",
    "сука",
    "гандон",
    "пидор",
    "пидар",
    "шлюха",
    "тупица",
    "козел",
    "козёл",
    "сволочь",
    "fuck",
    "shit",
    "bitch",
    "asshole",
];
// Stems are checked only to reject a candidate. Rejected raw strings and quotes are never stored.
const BLOCKED_STEMS: &[&str] = &[
    "бляд",
    "блят",
    "сука",
    "пизд",
    "хуй",
    "хуе",
    "хуё",
    "говн",
    "дерьм",
    "сран",
    "ебан",
    "ёбан",
    "ебат",
    "долбо",
    "ублюд",
    "мудак",
    "мраз",
    "гандон",
    "пидор",
    "пидар",
    "шлюх",
    "урод",
    "дебил",
    "идиот",
    "тупиц",
    "козел",
    "козёл",
    "сволоч",
    "fuck",
    "shit",
    "bitch",
    "asshole",
];

#[derive(Debug, Clone)]
struct Segment {
    text: String,
    speaker_id: Option<i64>,
    start_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct ExtractedCandidate {
    text: String,
    speaker_id: Option<i64>,
    evidence_kind: &'static str,
    evidence_quote: String,
    start_ms: Option<i64>,
    confidence: f64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SpeakerNameCandidateRow {
    pub id: i64,
    pub meeting_id: String,
    pub proposed_speaker_id: Option<i64>,
    pub candidate_text: Option<String>,
    pub evidence_kind: String,
    pub evidence_quote: Option<String>,
    pub evidence_start_ms: Option<i64>,
    pub confidence: f64,
    pub occurrence_count: i64,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSpeakerNameCandidateInput {
    pub candidate_id: i64,
    pub status: String,
    #[serde(default)]
    pub speaker_id: Option<i64>,
    #[serde(default)]
    pub set_as_display_name: bool,
}

fn normalize_name(value: &str) -> String {
    value
        .trim_matches(|character: char| {
            !character.is_alphabetic() && character != '-' && character != '\''
        })
        .to_lowercase()
        .replace('ё', "е")
}

fn display_name(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    let mut output = String::with_capacity(normalized.len());
    let mut capitalize_next = true;
    for character in normalized.chars() {
        if capitalize_next && character.is_alphabetic() {
            output.extend(character.to_uppercase());
            capitalize_next = false;
        } else {
            output.push(character);
        }
        if matches!(character, '-' | '\'' | '’') {
            capitalize_next = true;
        }
    }
    output
}

fn has_name_like_capitalization(value: &str) -> bool {
    value
        .chars()
        .find(|character| character.is_alphabetic())
        .is_some_and(char::is_uppercase)
}

fn validate_candidate(value: &str) -> Result<String, &'static str> {
    let trimmed = value.trim();
    let normalized = normalize_name(trimmed);
    let length = normalized.chars().count();
    if !(2..=32).contains(&length) {
        return Err("invalid_length");
    }
    if !trimmed
        .chars()
        .all(|character| character.is_alphabetic() || matches!(character, '-' | '\'' | '’'))
    {
        return Err("invalid_shape");
    }
    let abuse_normalized = normalized
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect::<String>();
    if BLOCKED_ABUSIVE_EXACT.contains(&abuse_normalized.as_str())
        || BLOCKED_STEMS
            .iter()
            .any(|stem| abuse_normalized.starts_with(stem))
    {
        return Err("abusive_or_profane");
    }
    if BLOCKED_EXACT.contains(&normalized.as_str()) {
        return Err("role_or_generic_word");
    }
    let mut previous = None;
    let mut repeated = 0;
    for character in normalized.chars() {
        if Some(character) == previous {
            repeated += 1;
            if repeated >= 3 {
                return Err("implausible_shape");
            }
        } else {
            repeated = 0;
            previous = Some(character);
        }
    }
    Ok(normalized)
}

fn extract_candidates(segments: &[Segment]) -> Vec<ExtractedCandidate> {
    let mut extracted = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        for capture in SELF_INTRO.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            extracted.push(ExtractedCandidate {
                text: display_name(&capture[1]),
                speaker_id: segment.speaker_id,
                evidence_kind: "self_introduction",
                evidence_quote: segment.text.clone(),
                start_ms: segment.start_ms,
                confidence: 0.95,
            });
        }
        for capture in EXPLICIT_INTRO.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            extracted.push(ExtractedCandidate {
                text: display_name(&capture[1]),
                speaker_id: None,
                evidence_kind: "explicit_introduction",
                evidence_quote: segment.text.clone(),
                start_ms: segment.start_ms,
                confidence: 0.75,
            });
        }
        if let Some(capture) = DIRECT_ADDRESS.captures(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            let candidate_text = display_name(&capture[1]);
            let Some(addressing_speaker_id) = segment.speaker_id else {
                // Without an attributed addressing turn, a later speaker is not
                // reliable evidence of who the name referred to.
                continue;
            };
            let next = segments.iter().skip(index + 1).find(|next| {
                next.speaker_id.is_some()
                    && next.speaker_id != Some(addressing_speaker_id)
                    && match (segment.start_ms, next.start_ms) {
                        (Some(start), Some(end)) => (0..=15_000).contains(&(end - start)),
                        _ => false,
                    }
            });
            if let Some(next) = next {
                extracted.push(ExtractedCandidate {
                    text: candidate_text,
                    speaker_id: next.speaker_id,
                    evidence_kind: "direct_address",
                    evidence_quote: segment.text.clone(),
                    start_ms: segment.start_ms,
                    confidence: 0.60,
                });
            }
        }
    }
    extracted
}

#[cfg(not(test))]
const REJECTION_SALT_SERVICE: &str = "meetily.speaker-names";
#[cfg(not(test))]
const REJECTION_SALT_ACCOUNT: &str = "rejection-salt-v1";

#[cfg(not(test))]
async fn rejection_salt(pool: &SqlitePool) -> Result<String, String> {
    let entry = keyring::Entry::new(REJECTION_SALT_SERVICE, REJECTION_SALT_ACCOUNT)
        .map_err(|error| format!("credential vault unavailable: {error}"))?;
    if let Ok(value) = entry.get_password() {
        if !value.is_empty() {
            // Remove the legacy database copy after a successful vault read. This
            // keeps future database backups from containing both salt and hashes.
            sqlx::query(
                "DELETE FROM app_settings_kv \
                 WHERE key='speaker_alias.rejection_salt.secret'",
            )
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
            return Ok(value);
        }
    }

    let legacy = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key='speaker_alias.rejection_salt.secret'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let salt = legacy.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    entry
        .set_password(&salt)
        .map_err(|error| format!("cannot save speaker-name rejection salt: {error}"))?;
    sqlx::query(
        "DELETE FROM app_settings_kv \
         WHERE key='speaker_alias.rejection_salt.secret'",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(salt)
}

#[cfg(test)]
async fn rejection_salt(_pool: &SqlitePool) -> Result<String, String> {
    Ok("speaker-name-test-salt".to_string())
}

fn candidate_hash(salt: &str, normalized: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update([0]);
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn store_rejection(
    pool: &SqlitePool,
    meeting_id: &str,
    hash: &str,
    reason: &str,
    evidence_start_ms: Option<i64>,
    evidence_kind: &str,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let observation = sqlx::query(
        "INSERT OR IGNORE INTO rejected_speaker_name_observations \
         (meeting_id, candidate_hash, evidence_start_ms, evidence_kind) VALUES(?, ?, ?, ?)",
    )
    .bind(meeting_id)
    .bind(hash)
    .bind(evidence_start_ms.unwrap_or(-1))
    .bind(evidence_kind)
    .execute(&mut *transaction)
    .await?;
    if observation.rows_affected() == 0 {
        transaction.commit().await?;
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO rejected_speaker_name_fingerprints(candidate_hash, reason) VALUES(?, ?) \
         ON CONFLICT(candidate_hash) DO UPDATE SET occurrence_count=occurrence_count+1, \
         last_seen_at=datetime('now')",
    )
    .bind(hash)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn scan_candidates(pool: &SqlitePool, meeting_id: &str) -> Result<usize, String> {
    let rows: Vec<(String, Option<i64>, Option<f64>)> = sqlx::query_as(
        "SELECT transcript, speaker_id, audio_start_time FROM transcripts \
         WHERE meeting_id=? ORDER BY COALESCE(audio_start_time, 0), rowid",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let segments = rows
        .into_iter()
        .map(|(text, speaker_id, start)| Segment {
            text,
            speaker_id,
            start_ms: start.map(|seconds| (seconds * 1_000.0).round() as i64),
        })
        .collect::<Vec<_>>();
    let salt = rejection_salt(pool)
        .await
        .map_err(|error| error.to_string())?;
    let mut grouped: HashMap<(String, Option<i64>, &'static str), (ExtractedCandidate, i64)> =
        HashMap::new();
    for candidate in extract_candidates(&segments) {
        let normalized = match validate_candidate(&candidate.text) {
            Ok(value) => value,
            Err(reason) => {
                let hash = candidate_hash(&salt, &normalize_name(&candidate.text));
                store_rejection(
                    pool,
                    meeting_id,
                    &hash,
                    reason,
                    candidate.start_ms,
                    candidate.evidence_kind,
                )
                .await
                .map_err(|error| error.to_string())?;
                continue;
            }
        };
        let hash = candidate_hash(&salt, &normalized);
        let speaker_key = candidate.speaker_id.unwrap_or(-1);
        let rejected: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM rejected_speaker_name_fingerprints \
             WHERE candidate_hash=?) \
             OR EXISTS(SELECT 1 FROM rejected_speaker_name_candidate_instances \
             WHERE meeting_id=? AND candidate_hash=? AND proposed_speaker_key=? \
               AND evidence_kind=?)",
        )
        .bind(&hash)
        .bind(meeting_id)
        .bind(&hash)
        .bind(speaker_key)
        .bind(candidate.evidence_kind)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
        if rejected != 0 {
            continue;
        }
        let key = (normalized, candidate.speaker_id, candidate.evidence_kind);
        grouped
            .entry(key)
            .and_modify(|(_, count)| *count += 1)
            .or_insert((candidate, 1));
    }

    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM speaker_name_candidates WHERE meeting_id=? AND status='pending'")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    let mut inserted = 0;
    for ((normalized, speaker_id, evidence_kind), (candidate, occurrence_count)) in grouped {
        if evidence_kind == "direct_address" && occurrence_count < 2 {
            continue;
        }
        let hash = candidate_hash(&salt, &normalized);
        let speaker_key = candidate.speaker_id.unwrap_or(-1);
        let result = sqlx::query(
            "INSERT INTO speaker_name_candidates \
             (meeting_id, proposed_speaker_id, proposed_speaker_key, candidate_text, \
              normalized_name, candidate_hash, evidence_kind, evidence_quote, \
              evidence_start_ms, confidence, occurrence_count) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(meeting_id, candidate_hash, proposed_speaker_key, evidence_kind) \
             DO UPDATE SET candidate_text=excluded.candidate_text, \
             evidence_quote=excluded.evidence_quote, \
             evidence_start_ms=excluded.evidence_start_ms, \
             confidence=excluded.confidence, \
             occurrence_count=excluded.occurrence_count, \
             updated_at=datetime('now') \
             WHERE status='pending'",
        )
        .bind(meeting_id)
        .bind(speaker_id)
        .bind(speaker_key)
        .bind(&candidate.text)
        .bind(&normalized)
        .bind(hash)
        .bind(evidence_kind)
        .bind(&candidate.evidence_quote)
        .bind(candidate.start_ms)
        .bind(candidate.confidence)
        .bind(occurrence_count)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        inserted += result.rows_affected() as usize;
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(inserted)
}

#[tauri::command]
pub async fn scan_speaker_name_candidates(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<usize, String> {
    scan_candidates(state.db_manager.pool(), &meeting_id).await
}

async fn list_candidates(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<SpeakerNameCandidateRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, meeting_id, proposed_speaker_id, candidate_text, evidence_kind, \
                evidence_quote, evidence_start_ms, confidence, occurrence_count, status \
         FROM speaker_name_candidates WHERE meeting_id=? AND status='pending' \
           AND (evidence_kind!='direct_address' OR occurrence_count>=2) \
         ORDER BY confidence DESC, occurrence_count DESC, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
}

#[tauri::command]
pub async fn list_speaker_name_candidates(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<SpeakerNameCandidateRow>, String> {
    list_candidates(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn review_candidate(
    pool: &SqlitePool,
    input: ReviewSpeakerNameCandidateInput,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "accepted" | "rejected") {
        return Err("Candidate status must be accepted or rejected".to_string());
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let row: Option<(
        String,
        Option<i64>,
        i64,
        Option<String>,
        Option<String>,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT meeting_id, proposed_speaker_id, proposed_speaker_key, candidate_text, \
                normalized_name, candidate_hash, evidence_kind \
         FROM speaker_name_candidates WHERE id=? AND status='pending'",
    )
    .bind(input.candidate_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let (
        meeting_id,
        proposed_speaker_id,
        proposed_speaker_key,
        candidate_text,
        normalized_name,
        hash,
        evidence_kind,
    ) = row.ok_or_else(|| "Pending speaker name candidate not found".to_string())?;

    if input.status == "rejected" {
        sqlx::query(
            "INSERT INTO rejected_speaker_name_candidate_instances \
             (meeting_id, candidate_hash, proposed_speaker_key, evidence_kind) \
             VALUES(?, ?, ?, ?) \
             ON CONFLICT(meeting_id, candidate_hash, proposed_speaker_key, evidence_kind) \
             DO UPDATE SET occurrence_count=occurrence_count+1, last_seen_at=datetime('now')",
        )
        .bind(&meeting_id)
        .bind(&hash)
        .bind(proposed_speaker_key)
        .bind(&evidence_kind)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE speaker_name_candidates SET candidate_text=NULL, normalized_name=NULL, \
             evidence_quote=NULL, status='rejected', updated_at=datetime('now') WHERE id=?",
        )
        .bind(input.candidate_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        return tx.commit().await.map_err(|error| error.to_string());
    }

    let speaker_id = input
        .speaker_id
        .or(proposed_speaker_id)
        .ok_or_else(|| "Choose which speaker this name belongs to".to_string())?;
    let candidate_text = candidate_text.ok_or_else(|| "Candidate text was removed".to_string())?;
    let normalized_name =
        normalized_name.ok_or_else(|| "Candidate name was removed".to_string())?;
    validate_candidate(&candidate_text).map_err(|reason| format!("Unsafe candidate: {reason}"))?;
    let speaker_belongs_to_meeting: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transcripts WHERE meeting_id=? AND speaker_id=?)",
    )
    .bind(&meeting_id)
    .bind(speaker_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if speaker_belongs_to_meeting == 0 {
        return Err("Speaker does not belong to this meeting".to_string());
    }
    sqlx::query(
        "INSERT INTO speaker_aliases(speaker_id, alias, normalized_alias, source_candidate_id) \
         VALUES(?, ?, ?, ?) ON CONFLICT(speaker_id, normalized_alias) DO NOTHING",
    )
    .bind(speaker_id)
    .bind(&candidate_text)
    .bind(&normalized_name)
    .bind(input.candidate_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if input.set_as_display_name {
        sqlx::query("UPDATE speakers SET display_name=?, is_confirmed=1 WHERE id=?")
            .bind(&candidate_text)
            .bind(speaker_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    sqlx::query(
        "UPDATE speaker_name_candidates SET proposed_speaker_id=?, \
         status='accepted', \
         updated_at=datetime('now') WHERE id=?",
    )
    .bind(speaker_id)
    .bind(input.candidate_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn review_speaker_name_candidate(
    state: tauri::State<'_, AppState>,
    input: ReviewSpeakerNameCandidateInput,
) -> Result<(), String> {
    review_candidate(state.db_manager.pool(), input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, speaker_id: Option<i64>, start_ms: i64) -> Segment {
        Segment {
            text: text.into(),
            speaker_id,
            start_ms: Some(start_ms),
        }
    }

    #[test]
    fn validation_rejects_roles_abuse_and_implausible_shapes() {
        assert_eq!(validate_candidate("Анна"), Ok("анна".into()));
        assert_eq!(validate_candidate("Мурод"), Ok("мурод".into()));
        assert_eq!(
            validate_candidate("разработчик"),
            Err("role_or_generic_word")
        );
        assert_eq!(validate_candidate("мудак"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Говно"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Дерьмо"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Хер"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("му-дак"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("ху-й"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("м'удак"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Иван123"), Err("invalid_shape"));
        assert_eq!(validate_candidate("Ааааа"), Err("implausible_shape"));
    }

    #[test]
    fn display_name_normalizes_asr_casing() {
        assert_eq!(display_name("АННА"), "Анна");
        assert_eq!(display_name("иВаН"), "Иван");
        assert_eq!(display_name("МАРИЯ-АННА"), "Мария-Анна");
        assert_eq!(display_name("o'NEILL"), "O'Neill");
    }

    #[test]
    fn extraction_links_only_strong_self_intro_and_repeated_address_target() {
        let rows = vec![
            segment("Меня зовут Анна", Some(7), 0),
            segment("Иван, расскажи про релиз", Some(7), 5_000),
            segment("Сборка готова", Some(8), 7_000),
        ];
        let extracted = extract_candidates(&rows);
        assert!(extracted.iter().any(|item| item.text == "Анна"
            && item.speaker_id == Some(7)
            && item.evidence_kind == "self_introduction"));
        assert!(extracted.iter().any(|item| item.text == "Иван"
            && item.speaker_id == Some(8)
            && item.evidence_kind == "direct_address"));
    }

    #[test]
    fn extraction_does_not_link_direct_address_from_unattributed_turn() {
        let rows = vec![
            segment("Иван, расскажи про релиз", None, 5_000),
            segment("Сборка готова", Some(8), 7_000),
        ];

        assert!(extract_candidates(&rows)
            .iter()
            .all(|item| item.evidence_kind != "direct_address"));
    }

    #[test]
    fn extraction_rejects_discourse_markers_and_bare_eto_phrases() {
        let rows = vec![
            segment("Это хорошо", Some(7), 0),
            segment("Кстати, расскажи про релиз", Some(7), 1_000),
            segment("Первый ответ", Some(8), 2_000),
            segment("Кстати, продолжай", Some(7), 3_000),
            segment("Второй ответ", Some(8), 4_000),
        ];

        assert!(extract_candidates(&rows)
            .iter()
            .all(|candidate| validate_candidate(&candidate.text).is_err()));
    }

    #[test]
    fn extraction_rejects_lowercase_trigger_completions_that_are_not_names() {
        let rows = vec![
            segment("С нами сегодня всё в порядке", Some(7), 0),
            segment("Представлю вам результаты", Some(7), 1_000),
            segment("Слушайте, у меня новость", Some(7), 2_000),
            segment("Первый ответ", Some(8), 3_000),
            segment("Слушайте, продолжим", Some(7), 4_000),
            segment("Второй ответ", Some(8), 5_000),
        ];

        assert!(extract_candidates(&rows)
            .iter()
            .all(|candidate| validate_candidate(&candidate.text).is_err()));
    }

    #[test]
    fn salted_hash_is_stable_per_install_and_changes_with_salt() {
        assert_eq!(candidate_hash("a", "анна"), candidate_hash("a", "анна"));
        assert_ne!(candidate_hash("a", "анна"), candidate_hash("b", "анна"));
    }

    #[tokio::test]
    async fn scan_is_idempotent_and_review_never_keeps_rejected_raw_text() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE app_settings_kv(key TEXT PRIMARY KEY, value TEXT, updated_at TEXT)",
            "CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, speaker_id INTEGER, audio_start_time REAL)",
            "CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT, is_confirmed INTEGER)",
            "CREATE TABLE speaker_name_candidates(id INTEGER PRIMARY KEY, meeting_id TEXT, proposed_speaker_id INTEGER, proposed_speaker_key INTEGER NOT NULL, candidate_text TEXT, normalized_name TEXT, candidate_hash TEXT, evidence_kind TEXT, evidence_quote TEXT, evidence_start_ms INTEGER, confidence REAL, occurrence_count INTEGER DEFAULT 1, status TEXT DEFAULT 'pending', created_at TEXT DEFAULT CURRENT_TIMESTAMP, updated_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(meeting_id,candidate_hash,proposed_speaker_key,evidence_kind))",
            "CREATE TABLE speaker_aliases(id INTEGER PRIMARY KEY, speaker_id INTEGER, alias TEXT, normalized_alias TEXT, source_candidate_id INTEGER, is_confirmed INTEGER DEFAULT 1, created_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(speaker_id,normalized_alias))",
            "CREATE TABLE rejected_speaker_name_fingerprints(candidate_hash TEXT PRIMARY KEY, reason TEXT, occurrence_count INTEGER DEFAULT 1, last_seen_at TEXT DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE rejected_speaker_name_observations(meeting_id TEXT, candidate_hash TEXT, evidence_start_ms INTEGER, evidence_kind TEXT, PRIMARY KEY(meeting_id,candidate_hash,evidence_start_ms,evidence_kind))",
            "CREATE TABLE rejected_speaker_name_candidate_instances(meeting_id TEXT, candidate_hash TEXT, proposed_speaker_key INTEGER, evidence_kind TEXT, occurrence_count INTEGER DEFAULT 1, last_seen_at TEXT DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(meeting_id,candidate_hash,proposed_speaker_key,evidence_kind))",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO speakers VALUES(7,'Speaker 7',0),(8,'Speaker 8',0),(9,'Speaker 9',0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, text, speaker_id, start) in [
            ("1", "Меня зовут Анна", 7, 0.0),
            ("2", "Иван, расскажи", 7, 5.0),
            ("3", "Первый ответ", 8, 6.0),
            ("4", "Иван, продолжай", 7, 10.0),
            ("5", "Второй ответ", 8, 11.0),
            ("6", "Меня зовут мудак", 7, 15.0),
            ("6b", "Слушаю", 9, 16.0),
            ("6c", "Дебил, расскажи", 7, 20.0),
            ("6d", "Отвечаю", 8, 21.0),
        ] {
            sqlx::query("INSERT INTO transcripts VALUES(?, 'm1', ?, ?, ?)")
                .bind(id)
                .bind(text)
                .bind(speaker_id)
                .bind(start)
                .execute(&pool)
                .await
                .unwrap();
        }

        scan_candidates(&pool, "m1").await.unwrap();
        scan_candidates(&pool, "m1").await.unwrap();
        let candidates = list_candidates(&pool, "m1").await.unwrap();
        assert_eq!(candidates.len(), 2);
        let anna = candidates
            .iter()
            .find(|candidate| candidate.candidate_text.as_deref() == Some("Анна"))
            .unwrap();
        let ivan = candidates
            .iter()
            .find(|candidate| candidate.candidate_text.as_deref() == Some("Иван"))
            .unwrap();
        assert_eq!(ivan.occurrence_count, 2);
        let anna_id = anna.id;
        let ivan_id = ivan.id;

        // Preserve the extracted identity key when the reviewer intentionally maps a
        // candidate to another speaker. Rewriting it to the target would collide with
        // an existing candidate for that same name/evidence kind.
        let anna_hash: String =
            sqlx::query_scalar("SELECT candidate_hash FROM speaker_name_candidates WHERE id=?")
                .bind(anna_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO speaker_name_candidates \
             (meeting_id, proposed_speaker_id, proposed_speaker_key, candidate_text, \
              normalized_name, candidate_hash, evidence_kind, confidence) \
             VALUES('m1', 9, 9, 'Анна', 'анна', ?, 'self_introduction', 0.9)",
        )
        .bind(&anna_hash)
        .execute(&pool)
        .await
        .unwrap();

        review_candidate(
            &pool,
            ReviewSpeakerNameCandidateInput {
                candidate_id: anna_id,
                status: "accepted".into(),
                speaker_id: Some(9),
                set_as_display_name: true,
            },
        )
        .await
        .unwrap();
        let renamed: (String, i64) =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id=9")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(renamed, ("Анна".into(), 1));
        let accepted_link: (Option<i64>, i64) = sqlx::query_as(
            "SELECT proposed_speaker_id, proposed_speaker_key \
             FROM speaker_name_candidates WHERE id=?",
        )
        .bind(anna_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(accepted_link, (Some(9), 7));
        sqlx::query(
            "DELETE FROM speaker_name_candidates \
             WHERE candidate_hash=? AND proposed_speaker_key=9",
        )
        .bind(&anna_hash)
        .execute(&pool)
        .await
        .unwrap();

        review_candidate(
            &pool,
            ReviewSpeakerNameCandidateInput {
                candidate_id: ivan_id,
                status: "rejected".into(),
                speaker_id: None,
                set_as_display_name: false,
            },
        )
        .await
        .unwrap();
        let removed: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT candidate_text, evidence_quote, status FROM speaker_name_candidates WHERE id=?",
        )
        .bind(ivan_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(removed, (None, None, "rejected".into()));
        scan_candidates(&pool, "m1").await.unwrap();
        assert!(list_candidates(&pool, "m1").await.unwrap().is_empty());

        for (id, text, speaker_id, start) in [
            ("7", "Иван, расскажи", 7, 5.0),
            ("8", "Первый ответ", 8, 6.0),
            ("9", "Иван, продолжай", 7, 10.0),
            ("10", "Второй ответ", 8, 11.0),
        ] {
            sqlx::query("INSERT INTO transcripts VALUES(?, 'm2', ?, ?, ?)")
                .bind(id)
                .bind(text)
                .bind(speaker_id)
                .bind(start)
                .execute(&pool)
                .await
                .unwrap();
        }
        scan_candidates(&pool, "m2").await.unwrap();
        let m2_candidates = list_candidates(&pool, "m2").await.unwrap();
        assert!(m2_candidates
            .iter()
            .any(|candidate| candidate.candidate_text.as_deref() == Some("Иван")));

        let raw_rejections: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM rejected_speaker_name_fingerprints WHERE candidate_hash LIKE '%мудак%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(raw_rejections, 0);

        let stored_salt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM app_settings_kv \
             WHERE key='speaker_alias.rejection_salt.secret'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_salt_count, 0);
        let rejected_hash = candidate_hash("speaker-name-test-salt", "дебил");
        let direct_address_rejections: i64 = sqlx::query_scalar(
            "SELECT occurrence_count FROM rejected_speaker_name_fingerprints \
             WHERE candidate_hash=? AND reason='abusive_or_profane'",
        )
        .bind(&rejected_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(direct_address_rejections, 1);
        scan_candidates(&pool, "m1").await.unwrap();
        let after_rescan: i64 = sqlx::query_scalar(
            "SELECT occurrence_count FROM rejected_speaker_name_fingerprints \
             WHERE candidate_hash=? AND reason='abusive_or_profane'",
        )
        .bind(&rejected_hash)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after_rescan, direct_address_rejections);
    }

    #[tokio::test]
    async fn rescan_removes_direct_address_after_evidence_drops_below_threshold() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE app_settings_kv(key TEXT PRIMARY KEY, value TEXT, updated_at TEXT)",
            "CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, speaker_id INTEGER, audio_start_time REAL)",
            "CREATE TABLE speaker_name_candidates(id INTEGER PRIMARY KEY, meeting_id TEXT, proposed_speaker_id INTEGER, proposed_speaker_key INTEGER NOT NULL, candidate_text TEXT, normalized_name TEXT, candidate_hash TEXT, evidence_kind TEXT, evidence_quote TEXT, evidence_start_ms INTEGER, confidence REAL, occurrence_count INTEGER DEFAULT 1, status TEXT DEFAULT 'pending', created_at TEXT DEFAULT CURRENT_TIMESTAMP, updated_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(meeting_id,candidate_hash,proposed_speaker_key,evidence_kind))",
            "CREATE TABLE rejected_speaker_name_fingerprints(candidate_hash TEXT PRIMARY KEY, reason TEXT, occurrence_count INTEGER DEFAULT 1, last_seen_at TEXT DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE rejected_speaker_name_observations(meeting_id TEXT, candidate_hash TEXT, evidence_start_ms INTEGER, evidence_kind TEXT, PRIMARY KEY(meeting_id,candidate_hash,evidence_start_ms,evidence_kind))",
            "CREATE TABLE rejected_speaker_name_candidate_instances(meeting_id TEXT, candidate_hash TEXT, proposed_speaker_key INTEGER, evidence_kind TEXT, occurrence_count INTEGER DEFAULT 1, last_seen_at TEXT DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(meeting_id,candidate_hash,proposed_speaker_key,evidence_kind))",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        for (id, text, speaker_id, start) in [
            ("1", "Иван, первый вопрос", 7, 5.0),
            ("2", "Первый ответ", 8, 6.0),
            ("3", "Иван, второй вопрос", 7, 10.0),
            ("4", "Второй ответ", 8, 11.0),
        ] {
            sqlx::query("INSERT INTO transcripts VALUES(?, 'm1', ?, ?, ?)")
                .bind(id)
                .bind(text)
                .bind(speaker_id)
                .bind(start)
                .execute(&pool)
                .await
                .unwrap();
        }

        scan_candidates(&pool, "m1").await.unwrap();
        let initial = list_candidates(&pool, "m1").await.unwrap();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].occurrence_count, 2);
        assert_eq!(
            initial[0].evidence_quote.as_deref(),
            Some("Иван, первый вопрос")
        );

        sqlx::query("UPDATE transcripts SET transcript='Иван, новый вопрос' WHERE id='1'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM transcripts WHERE id IN ('3', '4')")
            .execute(&pool)
            .await
            .unwrap();
        scan_candidates(&pool, "m1").await.unwrap();

        assert!(list_candidates(&pool, "m1").await.unwrap().is_empty());
        let pending_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) \
             FROM speaker_name_candidates WHERE meeting_id='m1' AND status='pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending_count, 0);
    }

    #[tokio::test]
    async fn review_rejects_speaker_from_another_meeting() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, speaker_id INTEGER, audio_start_time REAL)",
            "CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT, is_confirmed INTEGER)",
            "CREATE TABLE speaker_name_candidates(id INTEGER PRIMARY KEY, meeting_id TEXT, proposed_speaker_id INTEGER, proposed_speaker_key INTEGER NOT NULL, candidate_text TEXT, normalized_name TEXT, candidate_hash TEXT, evidence_kind TEXT, evidence_quote TEXT, evidence_start_ms INTEGER, confidence REAL, occurrence_count INTEGER DEFAULT 1, status TEXT DEFAULT 'pending', created_at TEXT DEFAULT CURRENT_TIMESTAMP, updated_at TEXT DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE speaker_aliases(id INTEGER PRIMARY KEY, speaker_id INTEGER, alias TEXT, normalized_alias TEXT, source_candidate_id INTEGER, is_confirmed INTEGER DEFAULT 1, created_at TEXT DEFAULT CURRENT_TIMESTAMP, UNIQUE(speaker_id,normalized_alias))",
            "CREATE TABLE rejected_speaker_name_candidate_instances(meeting_id TEXT, candidate_hash TEXT, proposed_speaker_key INTEGER, evidence_kind TEXT, occurrence_count INTEGER DEFAULT 1, last_seen_at TEXT DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY(meeting_id,candidate_hash,proposed_speaker_key,evidence_kind))",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO speakers VALUES(7,'Speaker 7',0),(8,'Speaker 8',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO transcripts VALUES('t1','m1','Меня зовут Анна',7,0),('t2','m2','Ответ',8,0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO speaker_name_candidates \
             (id,meeting_id,proposed_speaker_id,proposed_speaker_key,candidate_text,normalized_name,candidate_hash,evidence_kind) \
             VALUES(1,'m1',7,7,'Анна','анна','hash','self_introduction')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = review_candidate(
            &pool,
            ReviewSpeakerNameCandidateInput {
                candidate_id: 1,
                status: "accepted".into(),
                speaker_id: Some(8),
                set_as_display_name: true,
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error, "Speaker does not belong to this meeting");
        let untouched: (String, i64) =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id=8")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(untouched, ("Speaker 8".into(), 0));
        let aliases: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM speaker_aliases")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(aliases, 0);
    }
}
