//! Speaker-profile persistence for diarization + cross-meeting identity resolution
//! (PLAN.md Phase 2).
//!
//! This repository owns the `speakers` table plus the diarization-specific reads/writes of
//! `transcripts.speaker_id` (the resolved speaker profile — DISTINCT from the
//! `transcripts.speaker` TEXT channel tag 'mic'/'system', which is never touched here).
//!
//! ## Voice-embedding BLOB format
//! `speakers.voice_embedding` stores an averaged speaker embedding as **raw little-endian
//! `f32` bytes, concatenated** — 4 bytes per dimension, no header/length prefix. A 512-dim
//! WeSpeaker CAM++ embedding is therefore a 2048-byte BLOB. Decoding is the exact inverse:
//! consecutive 4-byte little-endian groups become `f32`s. A BLOB whose length is not a
//! multiple of 4 is treated as corrupt (`decode_embedding` returns `None`) and skipped.
//! See [`encode_embedding`] / [`decode_embedding`] and their roundtrip unit test.

use serde::Serialize;
use sqlx::{Error as SqlxError, SqlitePool};
use std::collections::HashMap;

/// A speaker referenced by a specific meeting's transcripts, with that meeting's segment
/// count. Field names are snake_case on the wire (the diarization UI codes against these).
#[derive(Debug, Clone, Serialize)]
pub struct MeetingSpeaker {
    pub id: i64,
    pub display_name: String,
    /// Previous and alternative names tied to this stable speaker id.
    ///
    /// Summaries are persisted as prose, so the UI uses these aliases to resolve an old
    /// rendered name back to the current `display_name` after a manual rename.
    pub aliases: Vec<String>,
    pub is_confirmed: bool,
    /// User-confirmed owner identity. Derived from diarization/voice matching, never channel.
    pub is_self: bool,
    pub segment_count: i64,
    /// Summed speech time across this speaker's segments in the meeting, in seconds.
    ///
    /// Per-segment durations are summed as-is, so simultaneous speech on the mic and system
    /// channels counts once per channel and the total across speakers can exceed the meeting's
    /// wall-clock length. The UI only ever divides it by that total (see
    /// `SummaryMessage.engagementPercentages`), so it is a relative share of speaking time,
    /// not an absolute claim about elapsed time.
    pub speech_duration_seconds: f64,
}

/// One transcript segment with its resolved speaker display name (LEFT JOINed), ordered as
/// the UI shows it. Used to rebuild speaker-labeled summary input (see
/// `summary::transcript_labeling`). `display_name` is present only when `speaker_id` resolves
/// to an existing `speakers` row.
#[derive(Debug, Clone)]
pub struct TranscriptSpeakerSegment {
    pub text: String,
    /// Wall-clock timestamp string (the frontend's `formatTime` fallback when no audio time).
    pub timestamp: String,
    /// Seconds from recording start; NULL when unknown.
    pub audio_start_time: Option<f64>,
    /// Audio-channel tag: 'mic' | 'system' | NULL.
    pub speaker: Option<String>,
    /// Resolved diarized speaker profile id; NULL until diarization runs.
    pub speaker_id: Option<i64>,
    /// speakers.display_name when speaker_id resolves, else NULL.
    pub display_name: Option<String>,
    /// Whether the resolved diarized profile is the local user.
    pub is_self: bool,
}

/// Encode an embedding as raw little-endian f32 bytes (see module docs). 4 bytes/dim.
pub fn encode_embedding(v: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(v.len() * 4);
    for x in v {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    bytes
}

/// Decode a `voice_embedding` BLOB back into an embedding. Returns `None` if the byte
/// length is not a multiple of 4 (corrupt) — the inverse of [`encode_embedding`].
pub fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

pub struct SpeakersRepository;

pub(crate) fn is_automatic_speaker_name(display_name: &str) -> bool {
    display_name.strip_prefix("Speaker ").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
    })
}

/// Automatic speaker rows are global database identities, but their labels are meeting UI.
/// Never expose the global/autoincrement id as the participant number: repeated diarization
/// runs create fresh rows and would otherwise turn two people into "Speaker 37/38/…".
fn apply_meeting_local_automatic_names(speakers: &mut [MeetingSpeaker]) {
    let mut automatic_ids = speakers
        .iter()
        .filter(|speaker| !speaker.is_confirmed && is_automatic_speaker_name(&speaker.display_name))
        .map(|speaker| speaker.id)
        .collect::<Vec<_>>();
    automatic_ids.sort_unstable();
    automatic_ids.dedup();
    let ordinals = automatic_ids
        .into_iter()
        .enumerate()
        .map(|(index, speaker_id)| (speaker_id, index + 1))
        .collect::<HashMap<_, _>>();

    for speaker in speakers {
        if let Some(ordinal) = ordinals.get(&speaker.id) {
            speaker.display_name = format!("Speaker {ordinal}");
        }
    }
}

impl SpeakersRepository {
    /// Load every speaker profile that has a stored voice embedding, decoded. Rows whose
    /// BLOB is corrupt (length not a multiple of 4) are skipped with a warning.
    pub async fn list_with_embeddings(
        pool: &SqlitePool,
    ) -> Result<Vec<(i64, Vec<f32>)>, SqlxError> {
        let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT id, voice_embedding FROM speakers WHERE voice_embedding IS NOT NULL",
        )
        .fetch_all(pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (id, blob) in rows {
            match decode_embedding(&blob) {
                Some(emb) => out.push((id, emb)),
                None => log::warn!(
                    "[speakers] speaker {id} has a corrupt voice_embedding BLOB ({} bytes); skipping",
                    blob.len()
                ),
            }
        }
        Ok(out)
    }

    /// Total number of speaker profiles (used to derive human-friendly "Speaker N" names).
    pub async fn count(pool: &SqlitePool) -> Result<i64, SqlxError> {
        sqlx::query_scalar("SELECT COUNT(*) FROM speakers")
            .fetch_one(pool)
            .await
    }

    /// Insert a new speaker profile with an initial voice embedding. Returns the new id.
    pub async fn insert(
        pool: &SqlitePool,
        display_name: &str,
        embedding: &[f32],
        is_confirmed: bool,
    ) -> Result<i64, SqlxError> {
        let blob = encode_embedding(embedding);
        sqlx::query_scalar(
            "INSERT INTO speakers (display_name, voice_embedding, is_confirmed) \
             VALUES (?, ?, ?) RETURNING id",
        )
        .bind(display_name)
        .bind(blob)
        .bind(if is_confirmed { 1 } else { 0 })
        .fetch_one(pool)
        .await
    }

    /// Replace a speaker's stored voice embedding (after folding a new observation in).
    pub async fn update_embedding(
        pool: &SqlitePool,
        speaker_id: i64,
        embedding: &[f32],
    ) -> Result<(), SqlxError> {
        let blob = encode_embedding(embedding);
        sqlx::query("UPDATE speakers SET voice_embedding = ? WHERE id = ?")
            .bind(blob)
            .bind(speaker_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Rename a speaker and mark it confirmed. Returns rows affected (0 = no such speaker).
    pub async fn rename(
        pool: &SqlitePool,
        speaker_id: i64,
        display_name: &str,
    ) -> Result<u64, SqlxError> {
        let mut transaction = pool.begin().await?;
        let current: Option<(String, i64)> =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id = ?")
                .bind(speaker_id)
                .fetch_optional(&mut *transaction)
                .await?;
        let Some((current_name, current_is_confirmed)) = current else {
            transaction.rollback().await?;
            return Ok(0);
        };

        let normalize_alias = |value: &str| value.trim().to_lowercase().replace('ё', "е");
        if !current_name.trim().is_empty()
            && normalize_alias(&current_name) != normalize_alias(display_name)
        {
            // Preserve the label that may already be embedded in persisted summaries. This is
            // historical identity metadata, not a request to regenerate or rewrite user prose.
            sqlx::query(
                "INSERT INTO speaker_aliases \
                 (speaker_id, alias, normalized_alias, is_confirmed) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(speaker_id, normalized_alias) DO NOTHING",
            )
            .bind(speaker_id)
            .bind(current_name.trim())
            .bind(normalize_alias(&current_name))
            .bind(current_is_confirmed)
            .execute(&mut *transaction)
            .await?;
        }

        let res =
            sqlx::query("UPDATE speakers SET display_name = ?, is_confirmed = 1 WHERE id = ?")
                .bind(display_name)
                .bind(speaker_id)
                .execute(&mut *transaction)
                .await?;
        transaction.commit().await?;
        Ok(res.rows_affected())
    }

    /// Mark or unmark a diarized voice profile as the local user.
    ///
    /// The operation is transactional because only one profile may be `is_self`.
    /// Returns 0 when the requested speaker does not exist, 1 otherwise.
    pub async fn set_self(
        pool: &SqlitePool,
        speaker_id: i64,
        is_self: bool,
    ) -> Result<u64, SqlxError> {
        let mut transaction = pool.begin().await?;
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM speakers WHERE id = ?)")
            .bind(speaker_id)
            .fetch_one(&mut *transaction)
            .await?;
        if exists == 0 {
            transaction.rollback().await?;
            return Ok(0);
        }

        if is_self {
            sqlx::query("UPDATE speakers SET is_self = 0 WHERE is_self = 1")
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("UPDATE speakers SET is_self = ? WHERE id = ?")
            .bind(if is_self { 1 } else { 0 })
            .bind(speaker_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(1)
    }

    /// A meeting's transcript segments ordered by recording time: (id, start_secs, end_secs).
    /// Timing is the SECONDS-based `audio_start_time`/`audio_end_time` (NULL when unknown).
    pub async fn list_meeting_segments(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<(String, Option<f64>, Option<f64>)>, SqlxError> {
        sqlx::query_as(
            "SELECT id, audio_start_time, audio_end_time FROM transcripts \
             WHERE meeting_id = ? ORDER BY COALESCE(audio_start_time, 0.0), rowid",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Clear diarized speaker attributions for a meeting (idempotent re-runs start clean).
    /// Only touches `speaker_id`; the `speaker` channel tag ('mic'/'system') is left intact.
    pub async fn clear_meeting_speaker_ids(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<(), SqlxError> {
        sqlx::query("UPDATE transcripts SET speaker_id = NULL WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Attribute one transcript segment to a resolved speaker profile.
    pub async fn set_segment_speaker(
        pool: &SqlitePool,
        transcript_id: &str,
        speaker_id: i64,
    ) -> Result<(), SqlxError> {
        sqlx::query("UPDATE transcripts SET speaker_id = ? WHERE id = ?")
            .bind(speaker_id)
            .bind(transcript_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Merge speaker attributions WITHIN one meeting: every transcript row of `meeting_id`
    /// currently attributed to one of `merged_ids` is reattributed to `keep_id`. Speaker
    /// rows themselves are NOT deleted here — profiles are global (other meetings may
    /// reference them); callers follow up with [`Self::delete_orphaned_unconfirmed`] so
    /// now-unreferenced unconfirmed profiles are collected. Returns rows reattributed.
    pub async fn merge_meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
        keep_id: i64,
        merged_ids: &[i64],
    ) -> Result<u64, SqlxError> {
        if merged_ids.is_empty() {
            return Ok(0);
        }
        let placeholders = vec!["?"; merged_ids.len()].join(", ");
        let mut transaction = pool.begin().await?;

        // The owner voice has to follow its segments. Merging the `is_self` profile away
        // would otherwise strand the flag on a row that no longer has any transcript rows,
        // and every "You" label in the meeting would silently disappear — GC keeps that row
        // alive precisely so the flag survives, so nothing would ever repair it either.
        let merges_owner_voice: i64 = {
            let sql = format!(
                "SELECT EXISTS(SELECT 1 FROM speakers \
                 WHERE is_self = 1 AND id IN ({placeholders}))"
            );
            let mut query = sqlx::query_scalar(&sql);
            for id in merged_ids {
                query = query.bind(id);
            }
            query.fetch_one(&mut *transaction).await?
        };
        if merges_owner_voice != 0 {
            // Clear before setting: `idx_speakers_single_self` allows only one owner row.
            sqlx::query("UPDATE speakers SET is_self = 0 WHERE is_self = 1")
                .execute(&mut *transaction)
                .await?;
            sqlx::query("UPDATE speakers SET is_self = 1 WHERE id = ?")
                .bind(keep_id)
                .execute(&mut *transaction)
                .await?;
        }

        let sql = format!(
            "UPDATE transcripts SET speaker_id = ? \
             WHERE meeting_id = ? AND speaker_id IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql).bind(keep_id).bind(meeting_id);
        for id in merged_ids {
            query = query.bind(id);
        }
        let res = query.execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(res.rows_affected())
    }

    /// Speakers referenced by a meeting's transcripts, with per-meeting segment counts,
    /// most-spoken first.
    ///
    /// The `JOIN` is deliberate: this is the meeting's speaker roster, so a profile with no
    /// segments in THIS meeting is absent — including the owner voice, whose flag is global.
    /// Callers must not treat "no `is_self` row here" as "no owner voice configured".
    ///
    /// The duration branches read the same quantity, not two different ones: the transcription
    /// worker writes `audio_end_time = audio_start_time + duration`
    /// (`audio/transcription/worker.rs`), so `duration` is only reached for rows whose audio
    /// times were never populated (legacy and imported transcripts).
    pub async fn meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        let rows: Vec<(i64, String, i64, i64, i64, f64)> = sqlx::query_as(
            "SELECT s.id, s.display_name, s.is_confirmed, s.is_self, COUNT(t.id) AS segment_count, \
                    COALESCE(SUM(CASE \
                        WHEN t.audio_start_time IS NOT NULL AND t.audio_end_time IS NOT NULL \
                             AND t.audio_end_time >= t.audio_start_time \
                            THEN t.audio_end_time - t.audio_start_time \
                        WHEN t.duration > 0 THEN t.duration \
                        ELSE 0 END), 0) AS speech_duration_seconds \
             FROM speakers s \
             JOIN transcripts t ON t.speaker_id = s.id \
             WHERE t.meeting_id = ? \
             GROUP BY s.id, s.display_name, s.is_confirmed, s.is_self \
             ORDER BY segment_count DESC, s.id ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        let mut speakers = rows
            .into_iter()
            .map(
                |(
                    id,
                    display_name,
                    is_confirmed,
                    is_self,
                    segment_count,
                    speech_duration_seconds,
                )| MeetingSpeaker {
                    id,
                    display_name,
                    aliases: Vec::new(),
                    is_confirmed: is_confirmed != 0,
                    is_self: is_self != 0,
                    segment_count,
                    speech_duration_seconds,
                },
            )
            .collect::<Vec<_>>();

        let alias_rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT DISTINCT sa.speaker_id, sa.alias \
             FROM speaker_aliases sa \
             JOIN transcripts t ON t.speaker_id = sa.speaker_id \
             WHERE t.meeting_id = ? \
             ORDER BY sa.speaker_id ASC, sa.id ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        let mut aliases_by_speaker: HashMap<i64, Vec<String>> = HashMap::new();
        for (speaker_id, alias) in alias_rows {
            aliases_by_speaker
                .entry(speaker_id)
                .or_default()
                .push(alias);
        }
        for speaker in &mut speakers {
            speaker.aliases = aliases_by_speaker.remove(&speaker.id).unwrap_or_default();
        }
        apply_meeting_local_automatic_names(&mut speakers);
        Ok(speakers)
    }

    /// A meeting's transcript segments with resolved speaker display names, ordered as the UI
    /// shows them (`audio_start_time ASC`, rowid tiebreak — matching
    /// `MeetingsRepository::get_meeting_transcripts_paginated`). Read-only. LEFT JOIN so
    /// segments without a resolved `speaker_id` are still returned (with `display_name` NULL).
    /// Used by `summary::transcript_labeling` to rebuild speaker-labeled summary input.
    pub async fn meeting_transcript_segments(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<TranscriptSpeakerSegment>, SqlxError> {
        let rows: Vec<(String, String, Option<f64>, Option<String>, Option<i64>, Option<String>, i64)> =
            sqlx::query_as(
                "SELECT t.transcript, t.timestamp, t.audio_start_time, t.speaker, t.speaker_id, s.display_name, COALESCE(s.is_self, 0) \
                 FROM transcripts t \
                 LEFT JOIN speakers s ON s.id = t.speaker_id \
                 WHERE t.meeting_id = ? \
                 ORDER BY t.audio_start_time ASC, t.rowid ASC",
            )
            .bind(meeting_id)
            .fetch_all(pool)
            .await?;

        let meeting_labels = Self::meeting_speakers(pool, meeting_id)
            .await?
            .into_iter()
            .map(|speaker| (speaker.id, speaker.display_name))
            .collect::<HashMap<_, _>>();

        Ok(rows
            .into_iter()
            .map(
                |(
                    text,
                    timestamp,
                    audio_start_time,
                    speaker,
                    speaker_id,
                    display_name,
                    is_self,
                )| {
                    let display_name = speaker_id
                        .and_then(|id| meeting_labels.get(&id).cloned())
                        .or(display_name);
                    TranscriptSpeakerSegment {
                        text,
                        timestamp,
                        audio_start_time,
                        speaker,
                        speaker_id,
                        display_name,
                        is_self: is_self != 0,
                    }
                },
            )
            .collect())
    }

    /// Garbage-collect orphaned speaker profiles: delete **unconfirmed** speakers no longer
    /// referenced by ANY transcript row. Re-runs clear a meeting's `speaker_id`s and may
    /// resolve clusters to different/new profiles, stranding the old auto-created
    /// "Speaker N" rows; without GC they accumulate forever. Confirmed (user-renamed)
    /// speakers are NEVER deleted, even when currently unreferenced — the user's label and
    /// voice profile must survive re-runs. Returns the number of rows deleted.
    ///
    /// The owner voice (`is_self = 1`) is retained on the same grounds even when unconfirmed
    /// and unreferenced: it is the voice profile future runs match the user against, so
    /// deleting it would silently drop the "You" labels. Retention is bounded to a single row
    /// by `idx_speakers_single_self`, and [`Self::set_self`] / [`Self::merge_meeting_speakers`]
    /// are the only ways the flag moves — so this cannot accumulate.
    pub async fn delete_orphaned_unconfirmed(pool: &SqlitePool) -> Result<u64, SqlxError> {
        const ORPHANS: &str = "SELECT id FROM speakers \
             WHERE is_confirmed = 0 \
               AND is_self = 0 \
               AND id NOT IN (SELECT DISTINCT speaker_id FROM transcripts \
                              WHERE speaker_id IS NOT NULL)";

        let mut transaction = pool.begin().await?;
        // `action_items.owner_speaker_id` is the one reference to a speaker that was
        // declared without an ON DELETE rule, so a single action item owned by a phantom
        // voice made this DELETE fail — and, being one statement, it took every other
        // orphan down with it. Observed live: "orphaned-speaker GC failed (non-fatal):
        // FOREIGN KEY constraint failed" while 7 orphans survived. The item itself is the
        // user's, so it stays; what goes is the pointer to a voice that no longer exists.
        sqlx::query(&format!(
            "UPDATE action_items SET owner_speaker_id = NULL \
             WHERE owner_speaker_id IN ({ORPHANS})"
        ))
        .execute(&mut *transaction)
        .await?;

        let res = sqlx::query(&format!("DELETE FROM speakers WHERE id IN ({ORPHANS})"))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(res.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_blob_roundtrips() {
        let emb = vec![0.0f32, 1.5, -2.25, 3.125, f32::MIN_POSITIVE, -0.0];
        let bytes = encode_embedding(&emb);
        // 4 bytes per dimension, no header.
        assert_eq!(bytes.len(), emb.len() * 4);
        let decoded = decode_embedding(&bytes).expect("valid blob decodes");
        assert_eq!(decoded, emb);
    }

    #[test]
    fn embedding_blob_is_little_endian() {
        // 1.0f32 == 0x3F800000; little-endian bytes are [0x00, 0x00, 0x80, 0x3F].
        let bytes = encode_embedding(&[1.0]);
        assert_eq!(bytes, vec![0x00, 0x00, 0x80, 0x3F]);
    }

    #[test]
    fn empty_embedding_roundtrips() {
        assert_eq!(encode_embedding(&[]), Vec::<u8>::new());
        assert_eq!(decode_embedding(&[]), Some(Vec::<f32>::new()));
    }

    #[test]
    fn corrupt_blob_length_is_rejected() {
        // 5 bytes is not a whole number of f32s.
        assert_eq!(decode_embedding(&[1, 2, 3, 4, 5]), None);
        assert_eq!(decode_embedding(&[1]), None);
    }

    #[test]
    fn automatic_names_are_numbered_inside_the_meeting_not_from_global_ids() {
        let mut speakers = vec![
            MeetingSpeaker {
                id: 64,
                display_name: "Speaker 41".into(),
                aliases: Vec::new(),
                is_confirmed: false,
                is_self: false,
                segment_count: 72,
                speech_duration_seconds: 0.0,
            },
            MeetingSpeaker {
                id: 60,
                display_name: "Speaker 37".into(),
                aliases: Vec::new(),
                is_confirmed: false,
                is_self: false,
                segment_count: 4,
                speech_duration_seconds: 0.0,
            },
            MeetingSpeaker {
                id: 62,
                display_name: "Андрей".into(),
                aliases: Vec::new(),
                is_confirmed: true,
                is_self: true,
                segment_count: 20,
                speech_duration_seconds: 0.0,
            },
            MeetingSpeaker {
                id: 61,
                display_name: "Speaker 38".into(),
                aliases: Vec::new(),
                is_confirmed: false,
                is_self: false,
                segment_count: 17,
                speech_duration_seconds: 0.0,
            },
        ];

        apply_meeting_local_automatic_names(&mut speakers);

        assert_eq!(speakers[0].display_name, "Speaker 3");
        assert_eq!(speakers[1].display_name, "Speaker 1");
        assert_eq!(speakers[2].display_name, "Андрей");
        assert_eq!(speakers[3].display_name, "Speaker 2");
    }

    /// In-memory pool with just the columns the GC SQL touches (mirrors the real schema's
    /// relevant subset: speakers.id/display_name/voice_embedding/is_confirmed,
    /// transcripts.id/meeting_id/speaker_id).
    async fn gc_test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory db");
        sqlx::query(
            "CREATE TABLE speakers (
                id INTEGER PRIMARY KEY,
                display_name TEXT NOT NULL,
                voice_embedding BLOB,
                is_confirmed INTEGER NOT NULL DEFAULT 0,
                is_self INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        // The real migration's single-owner constraint, so tests fail the same way production
        // would if an owner reassignment ever set the new row before clearing the old one.
        sqlx::query(
            "CREATE UNIQUE INDEX idx_speakers_single_self ON speakers(is_self) WHERE is_self = 1",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                speaker_id INTEGER,
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE speaker_aliases (
                id INTEGER PRIMARY KEY,
                speaker_id INTEGER NOT NULL,
                alias TEXT NOT NULL,
                normalized_alias TEXT NOT NULL,
                source_candidate_id INTEGER,
                is_confirmed INTEGER NOT NULL DEFAULT 1,
                UNIQUE(speaker_id, normalized_alias)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Declared exactly as the real migration declares it: a reference to a speaker with
        // no ON DELETE rule. That omission is what broke the collector in production, so a
        // test pool without it would prove nothing.
        sqlx::query(
            "CREATE TABLE action_items (
                id INTEGER PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                text TEXT NOT NULL,
                owner_speaker_id INTEGER REFERENCES speakers(id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    /// One action item owned by a phantom voice used to fail the whole collection — a
    /// single DELETE, so every other orphan survived with it. Observed live as
    /// "orphaned-speaker GC failed (non-fatal): FOREIGN KEY constraint failed" while seven
    /// phantom profiles stayed in the archive.
    #[tokio::test]
    async fn an_action_item_owned_by_a_phantom_voice_no_longer_blocks_the_collection() {
        let pool = gc_test_pool().await;
        sqlx::query(
            "INSERT INTO speakers (id, display_name, is_confirmed) \
             VALUES (1, 'Speaker 1', 0), (2, 'Speaker 2', 0), (3, 'Анна', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO action_items (id, meeting_id, text, owner_speaker_id) \
             VALUES (1, 'm1', 'прислать отчёт', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            SpeakersRepository::delete_orphaned_unconfirmed(&pool)
                .await
                .unwrap(),
            2
        );
        // The item is the user's and stays; only the pointer to a voice that no longer
        // exists is dropped.
        let owner: (String, Option<i64>) =
            sqlx::query_as("SELECT text, owner_speaker_id FROM action_items WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner, ("прислать отчёт".to_string(), None));
        // A confirmed speaker is never an orphan, however unreferenced.
        let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM speakers ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(left, vec![3]);
    }

    #[tokio::test]
    async fn rename_preserves_the_previous_display_name_as_an_alias() {
        let pool = gc_test_pool().await;
        sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (7, 'Блин', 0)")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(
            SpeakersRepository::rename(&pool, 7, "Зуйзуй")
                .await
                .unwrap(),
            1
        );

        let renamed: (String, i64) =
            sqlx::query_as("SELECT display_name, is_confirmed FROM speakers WHERE id = 7")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(renamed, ("Зуйзуй".to_string(), 1));

        let alias: (String, String, i64) = sqlx::query_as(
            "SELECT alias, normalized_alias, is_confirmed \
             FROM speaker_aliases WHERE speaker_id = 7",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(alias, ("Блин".to_string(), "блин".to_string(), 0));
    }

    #[tokio::test]
    async fn meeting_speakers_returns_aliases_for_summary_name_resolution() {
        let pool = gc_test_pool().await;
        sqlx::query(
            "INSERT INTO speakers (id, display_name, is_confirmed) VALUES (7, 'Зуйзуй', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts \
             (id, meeting_id, speaker_id, audio_start_time, audio_end_time, duration) \
             VALUES ('t1', 'm1', 7, 0.0, 1.0, 1.0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO speaker_aliases \
             (speaker_id, alias, normalized_alias) VALUES (7, 'Блин', 'блин')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let speakers = SpeakersRepository::meeting_speakers(&pool, "m1")
            .await
            .unwrap();
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].display_name, "Зуйзуй");
        assert_eq!(speakers[0].aliases, vec!["Блин"]);
    }

    async fn speaker_ids(pool: &SqlitePool) -> Vec<i64> {
        sqlx::query_scalar("SELECT id FROM speakers ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn gc_deletes_only_unreferenced_unconfirmed_speakers() {
        let pool = gc_test_pool().await;
        // 1: unconfirmed + referenced  -> kept
        // 2: unconfirmed + unreferenced -> DELETED
        // 3: confirmed   + unreferenced -> kept (user-renamed profiles are never GC'd)
        // 4: confirmed   + referenced  -> kept
        for (id, confirmed) in [(1, 0), (2, 0), (3, 1), (4, 1)] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, ?)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .bind(confirmed)
                .execute(&pool)
                .await
                .unwrap();
        }
        for (tid, sid) in [("t1", Some(1)), ("t2", Some(4)), ("t3", None)] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES (?, 'm1', ?)")
                .bind(tid)
                .bind(sid)
                .execute(&pool)
                .await
                .unwrap();
        }

        let deleted = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(
            deleted, 1,
            "only the orphaned unconfirmed speaker is removed"
        );
        assert_eq!(speaker_ids(&pool).await, vec![1, 3, 4]);

        // Idempotent: nothing left to collect.
        let again = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(again, 0);
    }

    #[tokio::test]
    async fn merge_reattributes_only_this_meetings_segments() {
        let pool = gc_test_pool().await;
        for id in [1, 2, 3] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, 0)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        // m1: segments for speakers 1, 2, 3; m2: a segment for speaker 2 that must survive.
        for (tid, mid, sid) in [
            ("a", "m1", 1),
            ("b", "m1", 2),
            ("c", "m1", 3),
            ("d", "m2", 2),
        ] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES (?, ?, ?)")
                .bind(tid)
                .bind(mid)
                .bind(sid)
                .execute(&pool)
                .await
                .unwrap();
        }

        let moved = SpeakersRepository::merge_meeting_speakers(&pool, "m1", 1, &[2, 3])
            .await
            .unwrap();
        assert_eq!(moved, 2);

        let m1_ids: Vec<i64> =
            sqlx::query_scalar("SELECT speaker_id FROM transcripts WHERE meeting_id = 'm1'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(m1_ids, vec![1, 1, 1]);
        // The other meeting's attribution is untouched.
        let m2_id: i64 =
            sqlx::query_scalar("SELECT speaker_id FROM transcripts WHERE meeting_id = 'm2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(m2_id, 2);

        // GC then removes only speaker 3 (unreferenced); 2 is still used by m2.
        let deleted = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(speaker_ids(&pool).await, vec![1, 2]);
    }

    #[tokio::test]
    async fn merge_moves_the_owner_voice_to_the_surviving_profile() {
        let pool = gc_test_pool().await;
        for id in [1, 2] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, 0)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        for (tid, sid) in [("a", 1), ("b", 2)] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES (?, 'm1', ?)")
                .bind(tid)
                .bind(sid)
                .execute(&pool)
                .await
                .unwrap();
        }
        // The user marked speaker 2 as their own voice; the report pass then decides 2 is the
        // same person as 1 and merges it away.
        SpeakersRepository::set_self(&pool, 2, true).await.unwrap();

        SpeakersRepository::merge_meeting_speakers(&pool, "m1", 1, &[2])
            .await
            .unwrap();

        let owners: Vec<i64> = sqlx::query_scalar("SELECT id FROM speakers WHERE is_self = 1")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(
            owners,
            vec![1],
            "the owner voice follows its segments into the surviving profile"
        );
    }

    #[tokio::test]
    async fn merge_leaves_the_owner_voice_alone_when_it_is_not_merged_away() {
        let pool = gc_test_pool().await;
        for id in [1, 2, 3] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, 0)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        for (tid, sid) in [("a", 2), ("b", 3)] {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES (?, 'm1', ?)")
                .bind(tid)
                .bind(sid)
                .execute(&pool)
                .await
                .unwrap();
        }
        SpeakersRepository::set_self(&pool, 1, true).await.unwrap();

        SpeakersRepository::merge_meeting_speakers(&pool, "m1", 2, &[3])
            .await
            .unwrap();

        let owners: Vec<i64> = sqlx::query_scalar("SELECT id FROM speakers WHERE is_self = 1")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(owners, vec![1], "an unrelated merge does not move the flag");
    }

    #[tokio::test]
    async fn gc_retains_an_unconfirmed_owner_voice_with_no_segments() {
        let pool = gc_test_pool().await;
        // The abandoned-run case: the user marked a placeholder as their own voice and it never
        // got a transcript row. It is kept — it is the profile future runs match against — and
        // `idx_speakers_single_self` bounds that retention to exactly one row.
        sqlx::query(
            "INSERT INTO speakers (id, display_name, is_confirmed) VALUES (7, 'Speaker 7', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        SpeakersRepository::set_self(&pool, 7, true).await.unwrap();

        let deleted = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(speaker_ids(&pool).await, vec![7]);

        // Clearing the flag makes it collectable again, so the retention is not permanent.
        SpeakersRepository::set_self(&pool, 7, false).await.unwrap();
        let deleted = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(speaker_ids(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn merge_with_no_ids_is_a_noop() {
        let pool = gc_test_pool().await;
        let moved = SpeakersRepository::merge_meeting_speakers(&pool, "m1", 1, &[])
            .await
            .unwrap();
        assert_eq!(moved, 0);
    }

    #[tokio::test]
    async fn meeting_speakers_sums_audio_times_and_falls_back_to_duration() {
        let pool = gc_test_pool().await;
        sqlx::query("DROP TABLE transcripts")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                speaker_id INTEGER,
                audio_start_time REAL,
                audio_end_time REAL,
                duration REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        for id in [1, 2] {
            sqlx::query("INSERT INTO speakers (id, display_name, is_confirmed) VALUES (?, ?, 1)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        // Speaker 1 mixes the two sources the way a partly-imported meeting does: a row the
        // live worker wrote (audio times present, and `duration` agreeing with them), and a
        // legacy row carrying only `duration`. Both must contribute their 4s.
        for (tid, sid, start, end, duration) in [
            ("a", 1, Some(0.0), Some(4.0), Some(4.0)),
            ("b", 1, None, None, Some(4.0)),
            // Zero-length and negative-length rows contribute nothing rather than skewing.
            ("c", 2, Some(10.0), Some(10.0), Some(0.0)),
            ("d", 2, Some(30.0), Some(20.0), None),
            ("e", 2, Some(0.0), Some(1.5), Some(1.5)),
        ] {
            sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, speaker_id, audio_start_time, audio_end_time, duration) \
                 VALUES (?, 'm1', ?, ?, ?, ?)",
            )
            .bind(tid)
            .bind(sid)
            .bind(start)
            .bind(end)
            .bind(duration)
            .execute(&pool)
            .await
            .unwrap();
        }

        let speakers = SpeakersRepository::meeting_speakers(&pool, "m1")
            .await
            .unwrap();
        let duration_of = |id: i64| {
            speakers
                .iter()
                .find(|s| s.id == id)
                .expect("speaker in roster")
                .speech_duration_seconds
        };
        assert_eq!(duration_of(1), 8.0);
        assert_eq!(duration_of(2), 1.5);
    }

    #[tokio::test]
    async fn gc_respects_references_from_any_meeting() {
        let pool = gc_test_pool().await;
        // Unconfirmed speaker referenced only by ANOTHER meeting's transcript must be kept:
        // the GC criterion is "unreferenced by ANY transcript row", not per-meeting.
        sqlx::query(
            "INSERT INTO speakers (id, display_name, is_confirmed) VALUES (5, 'Speaker 5', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, speaker_id) VALUES ('x', 'other-meeting', 5)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let deleted = SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(speaker_ids(&pool).await, vec![5]);
    }

    #[tokio::test]
    async fn self_identity_is_unique_reassignable_and_protected_from_gc() {
        let pool = gc_test_pool().await;
        for id in [1, 2] {
            sqlx::query("INSERT INTO speakers (id, display_name) VALUES (?, ?)")
                .bind(id)
                .bind(format!("Speaker {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        assert_eq!(
            SpeakersRepository::set_self(&pool, 1, true).await.unwrap(),
            1
        );
        assert_eq!(
            SpeakersRepository::set_self(&pool, 2, true).await.unwrap(),
            1
        );
        let owners: Vec<i64> = sqlx::query_scalar("SELECT id FROM speakers WHERE is_self = 1")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(owners, vec![2]);

        SpeakersRepository::delete_orphaned_unconfirmed(&pool)
            .await
            .unwrap();
        assert_eq!(speaker_ids(&pool).await, vec![2]);
        assert_eq!(
            SpeakersRepository::set_self(&pool, 999, true)
                .await
                .unwrap(),
            0
        );
    }
}
