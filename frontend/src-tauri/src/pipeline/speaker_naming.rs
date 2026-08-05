//! Automatic speaker naming: an LLM pass over the diarized transcript.
//!
//! Diarization separates voices; this pass puts names to them. It runs unattended right
//! after diarization — nobody is asked to confirm anything — so the safety comes from
//! being strict about what it accepts:
//!
//!   * a name is applied only above [`DEFAULT_MIN_CONFIDENCE`]; a weaker guess is dropped
//!     rather than applied tentatively, and that speaker keeps its `Speaker N` label;
//!   * a name the user typed is never overwritten, and an automatic name is never marked
//!     confirmed, so a later rename still wins (and still teaches the voice — see
//!     [`crate::learning::identity::learn_named_speaker`]);
//!   * two voices are merged only when both sides are still automatic, so a merge can
//!     never swallow a speaker the user named.
//!
//! The local regex pass in [`super::speaker_names`] runs first and handles explicit
//! self-introductions; this pass reads the whole conversation and catches the names that
//! only follow from context.

use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::database::repositories::speaker::{is_automatic_speaker_name, SpeakersRepository};
use crate::llm::{complete_routed, router::Scope, LlmError, Purpose};
use crate::report::dynamics;
use crate::report::pipeline::{build_segment_views, strip_json_fences};
use crate::report::prompts::{self, SpeakerGuesses};

/// Confidence a guessed name needs before it is applied without review. The prompt tells
/// the model that 0.9+ means an explicit introduction, so this bar also admits a name that
/// follows unambiguously from how the room addresses someone.
pub const DEFAULT_MIN_CONFIDENCE: f32 = 0.85;

/// Runtime override, for tuning without a rebuild.
const MIN_CONFIDENCE_KEY: &str = "speaker_names.llm_min_confidence";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NamingOutcome {
    pub named: usize,
    pub merged: usize,
}

impl NamingOutcome {
    fn is_empty(self) -> bool {
        self.named == 0 && self.merged == 0
    }
}

/// One speaker as the decision logic sees it: id, current label, and whether the user has
/// spoken for it.
#[derive(Debug, Clone)]
pub struct RosterEntry {
    pub id: i64,
    pub display_name: String,
    pub is_confirmed: bool,
}

/// What the pass decided to change. Empty vectors mean "leave everything as it is".
#[derive(Debug, Default, PartialEq, Eq)]
pub struct NamingPlan {
    /// (speaker id, new name)
    pub renames: Vec<(i64, String)>,
    /// (speaker to keep, speakers folded into it)
    pub merges: Vec<(i64, Vec<i64>)>,
}

async fn min_confidence(pool: &SqlitePool) -> f32 {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings_kv WHERE key=?")
        .bind(MIN_CONFIDENCE_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| (0.0..=1.0).contains(value))
        .unwrap_or(DEFAULT_MIN_CONFIDENCE)
}

/// Turn the model's guesses into the changes worth making unattended.
///
/// Everything the model says about a speaker the user already named is discarded, as is
/// any name that does not clear `threshold`, any name identical to the current label, and
/// any merge that would chain, self-reference, or absorb a confirmed speaker.
pub fn plan_changes(
    roster: &[RosterEntry],
    guesses: &SpeakerGuesses,
    threshold: f32,
) -> NamingPlan {
    let by_id: HashMap<i64, &RosterEntry> = roster.iter().map(|entry| (entry.id, entry)).collect();
    let mut plan = NamingPlan::default();

    let mut decided: HashSet<i64> = HashSet::new();
    for guess in &guesses.names {
        let name = guess.name.trim();
        let Some(entry) = by_id.get(&guess.speaker_id) else {
            continue;
        };
        // First guess per speaker wins, matching the order the model ranked them in.
        if name.is_empty() || !decided.insert(guess.speaker_id) {
            continue;
        }
        if entry.is_confirmed || !is_automatic_speaker_name(&entry.display_name) {
            continue;
        }
        if guess.confidence < threshold || name == entry.display_name {
            continue;
        }
        // A model that echoes the placeholder back has not recognised anybody.
        if is_automatic_speaker_name(name) {
            continue;
        }
        plan.renames.push((guess.speaker_id, name.to_string()));
    }

    let mut absorbed: HashSet<i64> = HashSet::new();
    let mut keepers: HashSet<i64> = HashSet::new();
    for merge in &guesses.merges {
        let Some(keeper) = by_id.get(&merge.keep_id) else {
            continue;
        };
        if absorbed.contains(&merge.keep_id) {
            continue;
        }
        let mut group = Vec::new();
        for id in &merge.merge_ids {
            let Some(entry) = by_id.get(id) else { continue };
            // Never fold away a speaker the user named, and never chain merges.
            if *id == merge.keep_id
                || entry.is_confirmed
                || absorbed.contains(id)
                || keepers.contains(id)
            {
                continue;
            }
            absorbed.insert(*id);
            group.push(*id);
        }
        if group.is_empty() {
            continue;
        }
        keepers.insert(merge.keep_id);
        log::info!(
            "[speaker-naming] merging {group:?} into {} ({})",
            keeper.id,
            merge.reason.trim()
        );
        plan.merges.push((merge.keep_id, group));
    }

    // A speaker that is about to disappear must not also be renamed.
    plan.renames.retain(|(id, _)| !absorbed.contains(id));
    plan
}

/// Apply a name without claiming the user chose it: `is_confirmed` stays 0, so a manual
/// rename still overrides this and the row is still garbage-collectable. Returns whether
/// the row actually changed (a confirmed row is left alone by the WHERE clause).
async fn apply_provisional_name(
    pool: &SqlitePool,
    speaker_id: i64,
    name: &str,
) -> Result<bool, String> {
    let result = sqlx::query("UPDATE speakers SET display_name=? WHERE id=? AND is_confirmed=0")
        .bind(name)
        .bind(speaker_id)
        .execute(pool)
        .await
        .map_err(|error| format!("Failed to apply automatic speaker name: {error}"))?;
    Ok(result.rows_affected() > 0)
}

/// Whether this meeting's transcript may be read by a model at all.
///
/// Naming sends the conversation to a cloud model, so it honours the same per-meeting
/// switch extraction does. The check lives here rather than in the caller so the
/// guarantee holds for every caller, present and future.
async fn naming_allowed(pool: &SqlitePool, meeting_id: &str) -> Result<bool, String> {
    let cloud_allowed: Option<i64> =
        sqlx::query_scalar("SELECT cloud_processing_allowed FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("Failed to read the meeting privacy policy: {error}"))?;
    Ok(cloud_allowed != Some(0))
}

/// Name the speakers of one meeting from its transcript, unattended.
///
/// Degrades quietly: no diarized speakers, no transcript, a privacy policy that blocks
/// outbound calls, or an unparsable answer all leave the meeting exactly as it was.
pub async fn infer_and_apply(pool: &SqlitePool, meeting_id: &str) -> Result<NamingOutcome, String> {
    let roster = SpeakersRepository::meeting_speakers(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load meeting speakers: {error}"))?;
    let entries: Vec<RosterEntry> = roster
        .iter()
        .map(|speaker| RosterEntry {
            id: speaker.id,
            display_name: speaker.display_name.clone(),
            is_confirmed: speaker.is_confirmed,
        })
        .collect();
    // Nothing to do when every voice already carries a name the user stands behind.
    if entries
        .iter()
        .all(|entry| entry.is_confirmed || !is_automatic_speaker_name(&entry.display_name))
    {
        return Ok(NamingOutcome::default());
    }

    if !naming_allowed(pool, meeting_id).await? {
        log::info!(
            "[speaker-naming] meeting {meeting_id}: naming disabled by memory privacy policy"
        );
        return Ok(NamingOutcome::default());
    }

    let segments = SpeakersRepository::meeting_transcript_segments(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load transcript: {error}"))?;
    if segments.is_empty() {
        return Ok(NamingOutcome::default());
    }

    // The transcript is labeled with stable ids so the model answers about `id N` rather
    // than display names, which collide and get renumbered.
    let (dyn_segments, _, texts) = build_segment_views(&segments);
    let timed = dynamics::timeline(&dyn_segments);
    let id_labels: Vec<String> = segments
        .iter()
        .zip(dyn_segments.iter())
        .map(|(segment, view)| match segment.speaker_id {
            Some(id) => format!("{} [id {id}]", view.speaker_label),
            None => view.speaker_label.clone(),
        })
        .collect();
    let transcript =
        prompts::truncate_transcript(&prompts::format_transcript(&timed, &id_labels, &texts));
    let roster_lines: Vec<(i64, String, i64)> = roster
        .iter()
        .map(|speaker| {
            (
                speaker.id,
                speaker.display_name.clone(),
                speaker.segment_count,
            )
        })
        .collect();
    let (system, user) = prompts::speakers(&transcript, &prompts::speaker_roster(&roster_lines));

    let Some(guesses) = ask_model(pool, meeting_id, &system, &user, transcript.len()).await else {
        return Ok(NamingOutcome::default());
    };

    let threshold = min_confidence(pool).await;
    let plan = plan_changes(&entries, &guesses, threshold);
    if plan.renames.is_empty() && plan.merges.is_empty() {
        return Ok(NamingOutcome::default());
    }

    let mut outcome = NamingOutcome::default();
    for (keep_id, group) in &plan.merges {
        match SpeakersRepository::merge_meeting_speakers(pool, meeting_id, *keep_id, group).await {
            Ok(0) => {}
            Ok(segments) => {
                outcome.merged += group.len();
                log::info!(
                    "[speaker-naming] meeting {meeting_id}: merged {group:?} into {keep_id} \
                     ({segments} segment(s) reattributed)"
                );
            }
            Err(error) => {
                log::warn!("[speaker-naming] merge {group:?} -> {keep_id} failed: {error}")
            }
        }
    }
    for (speaker_id, name) in &plan.renames {
        match apply_provisional_name(pool, *speaker_id, name).await {
            Ok(true) => outcome.named += 1,
            Ok(false) => {}
            Err(error) => {
                log::warn!("[speaker-naming] naming speaker {speaker_id} failed: {error}")
            }
        }
    }

    if !outcome.is_empty() {
        // Merged-away rows may now be unreferenced (same GC the diarization run does).
        match SpeakersRepository::delete_orphaned_unconfirmed(pool).await {
            Ok(0) => {}
            Ok(removed) => {
                log::info!("[speaker-naming] GC removed {removed} orphaned speaker profile(s)")
            }
            Err(error) => log::warn!("[speaker-naming] orphaned-speaker GC failed: {error}"),
        }
    }
    Ok(outcome)
}

/// One LLM call, retried once with a stricter instruction when the answer will not parse.
/// Returns `None` for every non-answer — a blocked purpose, a provider error, or JSON the
/// model never got right — because an unattended pass has nothing to fall back to.
async fn ask_model(
    pool: &SqlitePool,
    meeting_id: &str,
    system: &str,
    user: &str,
    query_chars: usize,
) -> Option<SpeakerGuesses> {
    for attempt in 1..=2 {
        let prompt = if attempt == 1 {
            user.to_string()
        } else {
            prompts::retry_suffix(user)
        };
        let raw = match complete_routed(
            pool,
            Purpose::Extract,
            Scope::SingleMeeting,
            query_chars,
            system,
            &prompt,
        )
        .await
        {
            Ok(raw) => raw,
            Err(LlmError::Provider(error)) => {
                log::warn!("[speaker-naming] meeting {meeting_id}: provider error: {error}");
                return None;
            }
            Err(error) => {
                log::info!("[speaker-naming] meeting {meeting_id}: skipped ({error})");
                return None;
            }
        };
        match serde_json::from_str::<SpeakerGuesses>(&strip_json_fences(&raw)) {
            Ok(guesses) => return Some(guesses),
            Err(error) => log::warn!(
                "[speaker-naming] meeting {meeting_id}: invalid JSON (attempt {attempt}): {error}"
            ),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::prompts::{SpeakerMergeGuess, SpeakerNameGuess};

    fn roster() -> Vec<RosterEntry> {
        vec![
            RosterEntry {
                id: 1,
                display_name: "Speaker 1".to_string(),
                is_confirmed: false,
            },
            RosterEntry {
                id: 2,
                display_name: "Speaker 2".to_string(),
                is_confirmed: false,
            },
            RosterEntry {
                id: 3,
                display_name: "Наташа".to_string(),
                is_confirmed: true,
            },
        ]
    }

    fn guess(speaker_id: i64, name: &str, confidence: f32) -> SpeakerNameGuess {
        SpeakerNameGuess {
            speaker_id,
            name: name.to_string(),
            confidence,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn a_meeting_that_forbids_cloud_processing_is_never_sent_to_the_model() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for (id, allowed) in [("open-meeting", 1), ("private-meeting", 0)] {
            sqlx::query(
                "INSERT INTO meetings(id, title, created_at, updated_at, cloud_processing_allowed) \
                 VALUES(?, 'Sync', datetime('now'), datetime('now'), ?)",
            )
            .bind(id)
            .bind(allowed)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert!(naming_allowed(&pool, "open-meeting").await.unwrap());
        assert!(!naming_allowed(&pool, "private-meeting").await.unwrap());
        // An unknown meeting cannot be named either way; it must not look forbidden.
        assert!(naming_allowed(&pool, "missing-meeting").await.unwrap());
    }

    #[test]
    fn only_confident_names_are_applied() {
        let guesses = SpeakerGuesses {
            names: vec![guess(1, "Андрей", 0.92), guess(2, "Карим", 0.60)],
            merges: vec![],
        };
        let plan = plan_changes(&roster(), &guesses, DEFAULT_MIN_CONFIDENCE);
        assert_eq!(plan.renames, vec![(1, "Андрей".to_string())]);
    }

    #[test]
    fn a_name_the_user_typed_is_never_overwritten() {
        let guesses = SpeakerGuesses {
            names: vec![guess(3, "Анастасия", 0.99)],
            merges: vec![],
        };
        assert!(plan_changes(&roster(), &guesses, DEFAULT_MIN_CONFIDENCE)
            .renames
            .is_empty());
    }

    #[test]
    fn unknown_ids_placeholders_and_repeats_are_ignored() {
        let guesses = SpeakerGuesses {
            names: vec![
                guess(99, "Кто-то", 0.99),
                guess(1, "Speaker 4", 0.99),
                guess(2, "Карим", 0.95),
                guess(2, "Максим", 0.99),
            ],
            merges: vec![],
        };
        let plan = plan_changes(&roster(), &guesses, DEFAULT_MIN_CONFIDENCE);
        assert_eq!(plan.renames, vec![(2, "Карим".to_string())]);
    }

    #[test]
    fn merges_skip_confirmed_speakers_and_never_chain() {
        let guesses = SpeakerGuesses {
            names: vec![],
            merges: vec![
                SpeakerMergeGuess {
                    keep_id: 1,
                    merge_ids: vec![2, 3, 1, 99],
                    reason: "одна манера речи".to_string(),
                },
                SpeakerMergeGuess {
                    keep_id: 2,
                    merge_ids: vec![1],
                    reason: "цепочка".to_string(),
                },
            ],
        };
        let plan = plan_changes(&roster(), &guesses, DEFAULT_MIN_CONFIDENCE);
        assert_eq!(plan.merges, vec![(1, vec![2])]);
    }

    #[test]
    fn a_speaker_being_merged_away_is_not_also_renamed() {
        let guesses = SpeakerGuesses {
            names: vec![guess(2, "Карим", 0.99)],
            merges: vec![SpeakerMergeGuess {
                keep_id: 1,
                merge_ids: vec![2],
                reason: "тот же голос".to_string(),
            }],
        };
        let plan = plan_changes(&roster(), &guesses, DEFAULT_MIN_CONFIDENCE);
        assert!(plan.renames.is_empty());
        assert_eq!(plan.merges, vec![(1, vec![2])]);
    }
}
