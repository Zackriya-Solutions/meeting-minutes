//! Conservative, local-only speaker-name suggestions from explicit transcript evidence.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::collections::{HashMap, HashSet};
#[cfg(not(test))]
use std::time::{Duration, Instant};

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
    Regex::new(r"(?iu)^\s*(?:(?:так|ну|слушай|смотри)\s*,\s*)?([\p{L}][\p{L}'’\-]{1,31})\s*[,!]")
        .expect("valid direct-address regex")
});
static STRONG_DIRECT_ADDRESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?iu)^\s*(?:(?:так|ну|слушай|смотри)\s*,\s*)?([\p{L}][\p{L}'’\-]{1,31})\s*[,!]\s*(?:ты|вы|тебе|вам|к\s+тебе|к\s+вам|давай|давайте|расскажи|расскажите|скажи|скажите|подскажи|подскажите|можешь|можете|посмотри|посмотрите|продолжай|продолжайте|думаешь|думаете|слышишь|слышите|что|ч[её]|где|как|you|can\s+you|could\s+you)\b",
    )
    .expect("valid strong direct-address regex")
});
static CONTEXTUAL_DIRECT_ADDRESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?iu)[.!?]\s+([\p{L}][\p{L}'’\-]{1,31})\s*[,!]\s*(?:ты|вы|тебе|вам|к\s+тебе|к\s+вам|давай|давайте|расскажи|расскажите|скажи|скажите|подскажи|подскажите|можешь|можете|посмотри|посмотрите|продолжай|продолжайте|думаешь|думаете|слышишь|слышите|что|ч[её]|где|как|you|can\s+you|could\s+you)\b",
    )
    .expect("valid contextual direct-address regex")
});
static GREETING_ADDRESS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?iu)(?:^|[.!?]\s*|\bвсем\s+)(?:привет|здравствуй|здравствуйте|доброе\s+утро|добрый\s+(?:день|вечер)|hello|hi)\s*[,!—-]?\s+([\p{L}][\p{L}'’\-]{1,31})(?:\s*[,!.?]|$)",
    )
    .expect("valid greeting-address regex")
});
static MEETING_TITLE_PERSON: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?iu)(?:\b1\s*(?:to|[-–—])\s*1\b|\b1to1\b|\b1-1\b)[^\p{L}]{0,16}(?:w(?:ith)?\s+|с\s+)?([\p{L}][\p{L}'’\-]{1,31})",
    )
    .expect("valid meeting-title person regex")
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
    "расскажи",
    "расскажите",
    "скажи",
    "скажите",
    "подскажи",
    "подскажите",
    "будет",
    "может",
    "просто",
    "типа",
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
    "хуя",
    "хую",
    "хуем",
    "хуём",
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
    /// Whether the word is a name we recognise (see
    /// [`crate::pipeline::person_names::is_known_given_name`]). Not persisted — it is
    /// computed per read so the lexicon can grow without a migration. The review screen
    /// leads with these: on this archive they are one or two per meeting, while the rest
    /// of the address slot is filled by particles and verbs a reviewer should not have to
    /// wade through to find them.
    #[sqlx(default)]
    pub is_recognized_name: bool,
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
    // The shared gate runs last so the specific reasons above keep their labels: profanity
    // truncations, discourse markers, hesitation noise and verb morphology that the lists
    // in this file predate. Both automatic writers clear the same bar — see
    // [`crate::pipeline::person_names`].
    if !crate::pipeline::person_names::is_plausible_person_name(trimmed) {
        return Err("not_name_like");
    }
    Ok(normalized)
}

fn address_candidate(
    segments: &[Segment],
    index: usize,
    candidate_text: String,
    confidence: f64,
) -> ExtractedCandidate {
    let segment = &segments[index];
    let next = segments.iter().skip(index + 1).find(|next| {
        next.speaker_id.is_some()
            && segment.speaker_id.map_or(true, |addressing_speaker_id| {
                next.speaker_id != Some(addressing_speaker_id)
            })
            && match (segment.start_ms, next.start_ms) {
                (Some(start), Some(end)) => (0..=15_000).contains(&(end - start)),
                _ => false,
            }
    });
    if let Some(next) = next {
        ExtractedCandidate {
            text: candidate_text,
            speaker_id: next.speaker_id,
            evidence_kind: "direct_address",
            evidence_quote: segment.text.clone(),
            start_ms: segment.start_ms,
            confidence,
        }
    } else {
        // The name itself is useful evidence even when diarization cannot safely determine
        // who was addressed. Keep speaker selection manual.
        ExtractedCandidate {
            text: candidate_text,
            speaker_id: None,
            evidence_kind: "direct_address_unassigned",
            evidence_quote: segment.text.clone(),
            start_ms: segment.start_ms,
            confidence: 0.45,
        }
    }
}

fn extract_candidates(segments: &[Segment]) -> Vec<ExtractedCandidate> {
    let mut extracted = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let mut strong_addresses = HashSet::new();
        for capture in SELF_INTRO.captures_iter(&segment.text) {
            // Strong grammatical evidence is safe even when ASR lowercases a proper name.
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
            // ASR capitalization is inconsistent; the explicit introduction phrase is
            // the safety signal, not the casing of the captured token.
            extracted.push(ExtractedCandidate {
                text: display_name(&capture[1]),
                speaker_id: None,
                evidence_kind: "explicit_introduction",
                evidence_quote: segment.text.clone(),
                start_ms: segment.start_ms,
                confidence: 0.75,
            });
        }
        for capture in STRONG_DIRECT_ADDRESS.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            strong_addresses.insert(normalize_name(&capture[1]));
            extracted.push(address_candidate(
                segments,
                index,
                display_name(&capture[1]),
                0.85,
            ));
        }
        for capture in DIRECT_ADDRESS.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            let candidate_text = display_name(&capture[1]);
            // STRONG_DIRECT_ADDRESS is a strict subset of this pattern. Count the
            // evidence once, keeping the stronger confidence assigned above.
            if strong_addresses.contains(&normalize_name(&candidate_text)) {
                continue;
            }
            extracted.push(address_candidate(segments, index, candidate_text, 0.60));
        }
        for capture in CONTEXTUAL_DIRECT_ADDRESS.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            extracted.push(address_candidate(
                segments,
                index,
                display_name(&capture[1]),
                0.85,
            ));
        }
        for capture in GREETING_ADDRESS.captures_iter(&segment.text) {
            if !has_name_like_capitalization(&capture[1]) {
                continue;
            }
            extracted.push(address_candidate(
                segments,
                index,
                display_name(&capture[1]),
                0.70,
            ));
        }
    }
    extracted
}

fn extract_title_candidates(title: &str) -> Vec<ExtractedCandidate> {
    MEETING_TITLE_PERSON
        .captures_iter(title)
        .filter_map(|capture| {
            let raw = capture.get(1)?.as_str();
            has_name_like_capitalization(raw).then(|| ExtractedCandidate {
                text: display_name(raw),
                speaker_id: None,
                evidence_kind: "meeting_title",
                evidence_quote: title.to_string(),
                start_ms: None,
                confidence: 0.55,
            })
        })
        .collect()
}

#[cfg(not(test))]
const REJECTION_SALT_SERVICE: &str = "meetily.speaker-names";
#[cfg(not(test))]
const REJECTION_SALT_ACCOUNT: &str = "rejection-salt-v1";
#[cfg(not(test))]
static REJECTION_SALT_CACHE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
#[cfg(not(test))]
static REJECTION_SALT_ATTEMPT: Lazy<tokio::sync::Mutex<Option<(Instant, String)>>> =
    Lazy::new(|| tokio::sync::Mutex::new(None));
#[cfg(not(test))]
const REJECTION_SALT_RETRY_COOLDOWN: Duration = Duration::from_secs(30);

#[cfg(not(test))]
async fn rejection_salt_from_vault(pool: &SqlitePool) -> Result<String, String> {
    let stored = tauri::async_runtime::spawn_blocking(|| {
        let entry = keyring::Entry::new(REJECTION_SALT_SERVICE, REJECTION_SALT_ACCOUNT)
            .map_err(|error| format!("credential vault unavailable: {error}"))?;
        match entry.get_password() {
            Ok(value) if !value.is_empty() => Ok(Some(value)),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(format!("cannot read speaker-name rejection salt: {error}")),
        }
    })
    .await
    .map_err(|error| format!("credential task failed: {error}"))??;

    if let Some(value) = stored {
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

    let legacy = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings_kv WHERE key='speaker_alias.rejection_salt.secret'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let salt = legacy.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let salt_to_store = salt.clone();
    tauri::async_runtime::spawn_blocking(move || {
        keyring::Entry::new(REJECTION_SALT_SERVICE, REJECTION_SALT_ACCOUNT)
            .map_err(|error| format!("credential vault unavailable: {error}"))?
            .set_password(&salt_to_store)
            .map_err(|error| format!("cannot save speaker-name rejection salt: {error}"))
    })
    .await
    .map_err(|error| format!("credential task failed: {error}"))??;
    sqlx::query(
        "DELETE FROM app_settings_kv \
         WHERE key='speaker_alias.rejection_salt.secret'",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(salt)
}

#[cfg(not(test))]
async fn rejection_salt(pool: &SqlitePool) -> Result<String, String> {
    if let Some(value) = REJECTION_SALT_CACHE.get() {
        return Ok(value.clone());
    }

    // Serialize the OS dialog and apply only a short failure cooldown. Successful
    // retrieval is process-cached, while a transient denial can recover without restart.
    let mut failure = REJECTION_SALT_ATTEMPT.lock().await;
    if let Some(value) = REJECTION_SALT_CACHE.get() {
        return Ok(value.clone());
    }
    if let Some((retry_after, message)) = failure.as_ref() {
        if Instant::now() < *retry_after {
            return Err(message.clone());
        }
    }

    match rejection_salt_from_vault(pool).await {
        Ok(value) => {
            let _ = REJECTION_SALT_CACHE.set(value.clone());
            *failure = None;
            Ok(value)
        }
        Err(error) => {
            *failure = Some((
                Instant::now() + REJECTION_SALT_RETRY_COOLDOWN,
                error.clone(),
            ));
            Err(error)
        }
    }
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
    let mut extracted = extract_candidates(&segments);
    // Titles such as "1to1 with Andrew" are useful, reviewable evidence for old
    // imported archives even when the transcript never contains an introduction.
    // Keep the target unassigned so the user must map it to a detected speaker.
    if let Some(title) = sqlx::query_scalar::<_, String>("SELECT title FROM meetings WHERE id=?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    {
        extracted.extend(extract_title_candidates(&title));
    }
    for candidate in extracted {
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

/// Find speaker names from explicit transcript evidence and apply only unambiguous mappings.
///
/// This is deliberately local and provisional: it never marks a speaker as user-confirmed.
/// A confirmed/manual name always wins. Automatic mappings are limited to self-introductions
/// and direct addresses followed by a response from one diarized voice; if one voice has
/// multiple possible names, or one name points at multiple voices, nothing is applied.
pub async fn infer_and_apply_names(pool: &SqlitePool, meeting_id: &str) -> Result<usize, String> {
    scan_candidates(pool, meeting_id).await?;

    type CandidateRow = (i64, i64, String, String, String, f64, i64);
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "SELECT id, proposed_speaker_id, candidate_text, normalized_name, evidence_kind, \
                confidence, occurrence_count \
         FROM speaker_name_candidates \
         WHERE meeting_id=? AND status='pending' AND proposed_speaker_id IS NOT NULL \
           AND candidate_text IS NOT NULL AND normalized_name IS NOT NULL",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let eligible = rows
        .into_iter()
        .filter(|row| match row.4.as_str() {
            // «Меня зовут Гурген» says what the word is. Rare and foreign names must
            // survive that, so recognition is not required here.
            "self_introduction" => row.5 >= 0.90,
            // Being addressed is inference, not statement: the name belongs to whoever
            // answered next, which is only probably the person meant — an echo, an
            // interruption or somebody else answering all land it on the wrong voice.
            // Two independent addresses converging on the same voice is what makes it a
            // fact rather than a guess, and the word still has to be a name we recognise.
            // A single address is not lost: it stays a pending candidate for review and
            // for the context-aware LLM pass.
            // `occurrence_count` counts occurrences per (name, speaker, evidence kind) —
            // see the grouping key in `scan_candidates` and the table's UNIQUE constraint —
            // so two here means two addresses that landed on the SAME voice, not two
            // people who happen to share a first name.
            "direct_address" => {
                row.5 >= 0.80
                    && row.6 >= 2
                    && crate::pipeline::person_names::is_known_given_name(&row.3)
            }
            _ => false,
        })
        .collect::<Vec<_>>();

    let mut names_by_speaker: HashMap<i64, HashSet<String>> = HashMap::new();
    let mut speakers_by_name: HashMap<String, HashSet<i64>> = HashMap::new();
    for (_, speaker_id, _, normalized_name, _, _, _) in &eligible {
        names_by_speaker
            .entry(*speaker_id)
            .or_default()
            .insert(normalized_name.clone());
        speakers_by_name
            .entry(normalized_name.clone())
            .or_default()
            .insert(*speaker_id);
    }

    let mut selected = Vec::new();
    for (speaker_id, names) in names_by_speaker {
        if names.len() != 1 {
            continue;
        }
        let Some(normalized_name) = names.into_iter().next() else {
            continue;
        };
        if speakers_by_name
            .get(&normalized_name)
            .is_some_and(|speakers| speakers.len() != 1)
        {
            continue;
        }

        let best = eligible
            .iter()
            .filter(|row| row.1 == speaker_id && row.3 == normalized_name)
            .max_by(|left, right| {
                let left_self_intro = left.4 == "self_introduction";
                let right_self_intro = right.4 == "self_introduction";
                left_self_intro
                    .cmp(&right_self_intro)
                    .then_with(|| left.6.cmp(&right.6))
                    .then_with(|| left.5.total_cmp(&right.5))
            });
        if let Some(candidate) = best {
            selected.push((
                candidate.0,
                speaker_id,
                candidate.2.clone(),
                normalized_name,
            ));
        }
    }

    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    let mut applied = 0;
    for (candidate_id, speaker_id, candidate_text, normalized_name) in selected {
        let current: Option<(String, i64)> =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id=?")
                .bind(speaker_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?;
        let Some((current_name, is_confirmed)) = current else {
            continue;
        };
        if is_confirmed != 0
            || (!crate::database::repositories::speaker::is_automatic_speaker_name(&current_name)
                && current_name != candidate_text)
        {
            continue;
        }

        let result =
            sqlx::query("UPDATE speakers SET display_name=? WHERE id=? AND is_confirmed=0")
                .bind(&candidate_text)
                .bind(speaker_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?;
        if result.rows_affected() == 0 {
            continue;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO speaker_aliases \
             (speaker_id, alias, normalized_alias, source_candidate_id, is_confirmed) \
             VALUES(?, ?, ?, ?, 0)",
        )
        .bind(speaker_id)
        .bind(&candidate_text)
        .bind(&normalized_name)
        .bind(candidate_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE speaker_name_candidates SET status='accepted', updated_at=datetime('now') \
             WHERE id=? AND status='pending'",
        )
        .bind(candidate_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        applied += 1;
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(applied)
}

/// Re-run the local, conservative name resolver for every already-diarized meeting.
/// New meetings call the same resolver at the end of diarization; this sweep only repairs
/// archives created before automatic naming existed or interrupted between the two passes.
pub async fn backfill_existing_speaker_names(pool: &SqlitePool) -> Result<(usize, usize), String> {
    // This is a migration sweep for archives created before automatic naming existed,
    // not recurring startup work. Claim it before touching the Keychain so a denial or
    // interrupted scan cannot ask again on every launch; new meetings run the resolver
    // directly after diarization and do not depend on this marker.
    let claimed = sqlx::query(
        "INSERT OR IGNORE INTO app_settings_kv(key, value, updated_at) \
         VALUES('speaker_names.archive_backfill_v1_attempted', 'true', CURRENT_TIMESTAMP)",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    if claimed == 0 {
        return Ok((0, 0));
    }

    let meeting_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT meeting_id FROM transcripts \
         WHERE speaker_id IS NOT NULL ORDER BY meeting_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let mut applied = 0usize;
    for meeting_id in &meeting_ids {
        applied += infer_and_apply_names(pool, meeting_id).await?;
    }
    Ok((meeting_ids.len(), applied))
}

/// Take back the names the gate would refuse today.
///
/// Archives recorded before [`crate::pipeline::person_names`] existed carry speakers called
/// *Назови* or *Бля* — an imperative and a swear word that the address patterns accepted.
/// They are automatic names, so nothing the user chose is at risk: reset them to their
/// placeholder and drop the alias that would otherwise keep the word alive in prose. A
/// speaker the user confirmed is never touched, whatever it is called.
///
/// Returns how many names were taken back.
pub async fn repair_implausible_automatic_names(pool: &SqlitePool) -> Result<usize, String> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, display_name FROM speakers WHERE is_confirmed=0")
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?;

    let junk: Vec<i64> = rows
        .into_iter()
        .filter(|(_, display_name)| {
            !crate::database::repositories::speaker::is_automatic_speaker_name(display_name)
                && !crate::pipeline::person_names::is_plausible_person_name(display_name)
        })
        .map(|(speaker_id, _)| speaker_id)
        .collect();
    if junk.is_empty() {
        return Ok(0);
    }

    // One transaction for the whole repair: this runs inside app startup, and a commit per
    // speaker would put the launch behind however much junk an archive accumulated.
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    for speaker_id in &junk {
        sqlx::query("UPDATE speakers SET display_name=? WHERE id=? AND is_confirmed=0")
            .bind(format!("Speaker {speaker_id}"))
            .bind(speaker_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        // Only the aliases that are junk themselves. A speaker can carry a second automatic
        // alias that is a perfectly good name — dropping it along with the bad one would
        // throw away the evidence that could have named this voice correctly.
        let aliases: Vec<(i64, String)> = sqlx::query_as(
            "SELECT id, alias FROM speaker_aliases WHERE speaker_id=? AND is_confirmed=0",
        )
        .bind(speaker_id)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        for (alias_id, alias) in aliases {
            if crate::pipeline::person_names::is_plausible_person_name(&alias) {
                continue;
            }
            sqlx::query("DELETE FROM speaker_aliases WHERE id=?")
                .bind(alias_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    log::info!(
        "[speaker-names] took back {} implausible automatic name(s): {junk:?}",
        junk.len()
    );
    Ok(junk.len())
}

#[tauri::command]
pub async fn scan_speaker_name_candidates(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<usize, String> {
    scan_candidates(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn infer_meeting_speaker_names(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<usize, String> {
    infer_and_apply_names(state.db_manager.pool(), &meeting_id).await
}

async fn list_candidates(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<SpeakerNameCandidateRow>, sqlx::Error> {
    let mut rows: Vec<SpeakerNameCandidateRow> = sqlx::query_as(
        "SELECT id, meeting_id, proposed_speaker_id, candidate_text, evidence_kind, \
                evidence_quote, evidence_start_ms, confidence, occurrence_count, status \
         FROM speaker_name_candidates WHERE meeting_id=? AND status='pending' \
         ORDER BY confidence DESC, occurrence_count DESC, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?;
    for row in &mut rows {
        row.is_recognized_name = row
            .candidate_text
            .as_deref()
            .is_some_and(crate::pipeline::person_names::is_known_given_name);
    }
    rows.sort_by(|left, right| {
        right
            .is_recognized_name
            .cmp(&left.is_recognized_name)
            .then(right.confidence.total_cmp(&left.confidence))
            .then(right.occurrence_count.cmp(&left.occurrence_count))
            .then(left.id.cmp(&right.id))
    });
    Ok(rows)
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
        assert_eq!(validate_candidate("Расскажи"), Err("role_or_generic_word"));
        assert_eq!(validate_candidate("Типа"), Err("role_or_generic_word"));
        assert_eq!(validate_candidate("мудак"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Говно"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Дерьмо"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("Хер"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("му-дак"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("ху-й"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("хуя"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("хую"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("хуем"), Err("abusive_or_profane"));
        assert_eq!(validate_candidate("хуём"), Err("abusive_or_profane"));
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
    fn extraction_links_direct_address_from_unattributed_turn_to_the_response() {
        let rows = vec![
            segment("Иван, расскажи про релиз", None, 5_000),
            segment("Сборка готова", Some(8), 7_000),
        ];

        let extracted = extract_candidates(&rows);
        assert!(extracted.iter().any(|item| {
            item.text == "Иван"
                && item.speaker_id == Some(8)
                && item.evidence_kind == "direct_address"
        }));
    }

    #[test]
    fn extraction_links_contextual_address_to_the_response() {
        let rows = vec![
            segment(
                "Это отличная идея. Андрей, ты нас слышишь? Всё хорошо?",
                None,
                21_210,
            ),
            segment("Да-да, слышно", Some(4), 27_480),
        ];

        assert!(extract_candidates(&rows).iter().any(|item| {
            item.text == "Андрей"
                && item.speaker_id == Some(4)
                && item.evidence_kind == "direct_address"
        }));
    }

    /// The tables the candidate flow touches, for tests that drive it end to end.
    async fn candidate_pool() -> SqlitePool {
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
        pool
    }

    #[tokio::test]
    /// Two addresses converging on one voice name it; one address does not.
    ///
    /// This test used to assert the opposite — a single address applied a name — which is
    /// what put «Миша» on whoever happened to answer after «Миша, присаживайся». The
    /// evidence is the same either way; what changed is that being answered next is now
    /// treated as the guess it is until a second address agrees with it.
    async fn automatic_inference_applies_one_unambiguous_provisional_name() {
        let pool = candidate_pool().await;
        sqlx::query("INSERT INTO speakers VALUES(4,'Speaker 4',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO transcripts VALUES \
             ('t1','m1','Это отличная идея. Андрей, ты нас слышишь?',NULL,21.21), \
             ('t2','m1','Да-да, слышно',4,27.48)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // One address: the name is evidence, not yet a fact. Nothing is applied, and the
        // candidate stays pending for review.
        assert_eq!(infer_and_apply_names(&pool, "m1").await.unwrap(), 0);
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM speaker_name_candidates \
             WHERE meeting_id='m1' AND status='pending' AND normalized_name='андрей'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending, 1);

        // A second address to the same voice settles it.
        sqlx::query(
            "INSERT INTO transcripts VALUES \
             ('t3','m1','Хорошо. Андрей, ты посмотришь сборку?',NULL,60.0), \
             ('t4','m1','Да, посмотрю',4,63.0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(infer_and_apply_names(&pool, "m1").await.unwrap(), 1);
        let speaker: (String, i64) =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id=4")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(speaker, ("Андрей".into(), 0));
        let alias: (String, i64) =
            sqlx::query_as("SELECT alias, is_confirmed FROM speaker_aliases WHERE speaker_id=4")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(alias, ("Андрей".into(), 0));
    }

    /// Two addresses to two different voices are not two addresses to one voice.
    ///
    /// Guards the invariant the auto-apply gate rests on: occurrences are counted per
    /// (name, speaker), so a meeting with two people sharing a first name cannot pool
    /// their evidence into one confident-looking name on the wrong voice.
    #[tokio::test]
    async fn addresses_landing_on_different_voices_never_pool_into_one_name() {
        let pool = candidate_pool().await;
        sqlx::query("INSERT INTO speakers VALUES(4,'Speaker 4',0),(5,'Speaker 5',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO transcripts VALUES \
             ('t1','m1','Саша, ты посмотришь сборку?',NULL,10.0), \
             ('t2','m1','Да, посмотрю',4,12.0), \
             ('t3','m1','Саша, а ты что думаешь?',NULL,40.0), \
             ('t4','m1','Я согласен',5,42.0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(infer_and_apply_names(&pool, "m1").await.unwrap(), 0);
        let counts: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT proposed_speaker_id, occurrence_count FROM speaker_name_candidates \
             WHERE meeting_id='m1' AND normalized_name='саша' AND evidence_kind='direct_address' \
             ORDER BY proposed_speaker_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(counts, vec![(4, 1), (5, 1)]);

        // Nor do occurrences pool across meetings: the same voice addressed once here and
        // once in another meeting is still one address each time.
        sqlx::query(
            "INSERT INTO transcripts VALUES \
             ('t5','m2','Саша, ты посмотришь сборку?',NULL,10.0), \
             ('t6','m2','Да, посмотрю',4,12.0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(infer_and_apply_names(&pool, "m2").await.unwrap(), 0);
        assert_eq!(infer_and_apply_names(&pool, "m1").await.unwrap(), 0);
        let still_automatic: String =
            sqlx::query_scalar("SELECT display_name FROM speakers WHERE id=4")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(still_automatic, "Speaker 4");
    }

    #[test]
    fn extraction_accepts_russian_discourse_marker_before_address() {
        let rows = vec![
            segment("Так, Андрей, может быть, он должен?", Some(7), 5_000),
            segment("Да, я посмотрю", Some(8), 7_000),
        ];

        assert!(extract_candidates(&rows).iter().any(|item| {
            item.text == "Андрей"
                && item.speaker_id == Some(8)
                && item.evidence_kind == "direct_address"
        }));
    }

    #[test]
    fn extraction_finds_names_in_realistic_russian_intro_and_greeting() {
        let rows = vec![
            segment("Всем привет, Макс. Ага, привет.", Some(36), 52_000),
            segment("Привет, Андрей.", Some(37), 67_000),
            segment("Привет!", Some(38), 70_000),
            segment(
                "Окей, с этим решили. Давайте дальше. Значит, меня зовут Андрей. Я техлит в команде.",
                Some(38),
                145_950,
            ),
        ];
        let extracted = extract_candidates(&rows);

        assert!(extracted.iter().any(|candidate| {
            candidate.text == "Макс" && candidate.evidence_kind == "direct_address"
        }));

        assert!(extracted.iter().any(|candidate| {
            candidate.text == "Андрей"
                && candidate.speaker_id == Some(38)
                && candidate.evidence_kind == "direct_address"
        }));
        assert!(extracted.iter().any(|candidate| {
            candidate.text == "Андрей"
                && candidate.speaker_id == Some(38)
                && candidate.evidence_kind == "self_introduction"
        }));
    }

    #[test]
    fn strong_intro_survives_lowercase_asr_and_one_on_one_title_is_reviewable() {
        let rows = vec![segment("меня зовут андрей", Some(7), 0)];
        assert!(extract_candidates(&rows).iter().any(|candidate| {
            candidate.text == "Андрей" && candidate.speaker_id == Some(7)
        }));
        let title = extract_title_candidates("Thu 16.02 1to1 w Andrew, Mar 5, 2026");
        assert_eq!(title.len(), 1);
        assert_eq!(title[0].text, "Andrew");
        assert_eq!(title[0].evidence_kind, "meeting_title");
        assert!(title[0].speaker_id.is_none());
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

    /// Both lines are transcribed from a real meeting. The imperative and the swear word
    /// sit exactly where a name sits, are capitalised exactly like a name, and each named
    /// a speaker before the shared plausibility gate existed.
    #[test]
    fn imperatives_and_profanity_never_become_candidates() {
        let rows = vec![
            segment("Назови, как тебя зовут", Some(7), 0),
            segment("Меня зовут Анна", Some(8), 3_000),
            segment("Бля, что там с архивом", Some(7), 6_000),
            segment("Проблема в индексе", Some(8), 8_000),
        ];

        let accepted = extract_candidates(&rows)
            .into_iter()
            .filter(|candidate| validate_candidate(&candidate.text).is_ok())
            .map(|candidate| candidate.text)
            .collect::<Vec<_>>();
        assert_eq!(accepted, vec!["Анна".to_string()]);
    }

    /// Being addressed says who answered next, not who was meant. A word we do not
    /// recognise as a name may be reviewed, but never applied unattended.
    #[test]
    fn only_a_recognised_name_is_applied_from_a_direct_address() {
        assert!(crate::pipeline::person_names::is_known_given_name("андрей"));
        assert!(!crate::pipeline::person_names::is_known_given_name("назови"));
    }

    #[tokio::test]
    async fn repair_takes_back_junk_names_but_never_a_name_the_user_typed() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT, is_confirmed INTEGER)",
            "CREATE TABLE speaker_aliases(id INTEGER PRIMARY KEY, speaker_id INTEGER, alias TEXT, normalized_alias TEXT, source_candidate_id INTEGER, is_confirmed INTEGER DEFAULT 1)",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO speakers VALUES(1,'Назови',0),(2,'Бля',0),(3,'Миша',0), \
             (4,'Speaker 4',0),(5,'Бля',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO speaker_aliases(speaker_id, alias, normalized_alias, is_confirmed) \
             VALUES(1,'Назови','назови',0),(1,'Анна','анна',0),(2,'Бля','бля',1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(repair_implausible_automatic_names(&pool).await.unwrap(), 2);
        let names: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, display_name FROM speakers ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            names,
            vec![
                (1, "Speaker 1".to_string()),
                (2, "Speaker 2".to_string()),
                (3, "Миша".to_string()),
                (4, "Speaker 4".to_string()),
                // Whatever the user typed is theirs, including a word this gate rejects.
                (5, "Бля".to_string()),
            ]
        );
        // The junk alias goes with the junk name. What stays: a confirmed alias, because a
        // person accepted that word in the review screen and this repair only takes back what
        // the app decided on its own; and an automatic alias that is a real name, because it
        // is the evidence that could still name this voice correctly.
        let aliases: Vec<(i64, String, i64)> =
            sqlx::query_as("SELECT speaker_id, alias, is_confirmed FROM speaker_aliases ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            aliases,
            vec![(1, "Анна".to_string(), 0), (2, "Бля".to_string(), 1)]
        );
        // Nothing left to take back: a second pass is a no-op.
        assert_eq!(repair_implausible_automatic_names(&pool).await.unwrap(), 0);
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
    async fn rescan_updates_single_direct_address_candidate() {
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

        let remaining = list_candidates(&pool, "m1").await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].occurrence_count, 1);
        assert_eq!(
            remaining[0].evidence_quote.as_deref(),
            Some("Иван, новый вопрос")
        );
        let pending_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) \
             FROM speaker_name_candidates WHERE meeting_id='m1' AND status='pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending_count, 1);
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

    #[tokio::test]
    async fn archive_backfill_is_claimed_before_any_repeat_scan() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE app_settings_kv(\
                key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT\
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(\
                id TEXT PRIMARY KEY, meeting_id TEXT, speaker_id INTEGER\
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            backfill_existing_speaker_names(&pool).await.unwrap(),
            (0, 0)
        );
        let marker: String = sqlx::query_scalar(
            "SELECT value FROM app_settings_kv \
             WHERE key='speaker_names.archive_backfill_v1_attempted'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(marker, "true");

        // The second call must return before touching archive tables (and therefore
        // before the Keychain-backed salt), even if those tables are unavailable.
        sqlx::query("DROP TABLE transcripts")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            backfill_existing_speaker_names(&pool).await.unwrap(),
            (0, 0)
        );
    }
}
