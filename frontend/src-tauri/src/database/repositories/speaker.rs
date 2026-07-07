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

/// A speaker referenced by a specific meeting's transcripts, with that meeting's segment
/// count. Field names are snake_case on the wire (the diarization UI codes against these).
#[derive(Debug, Clone, Serialize)]
pub struct MeetingSpeaker {
    pub id: i64,
    pub display_name: String,
    pub is_confirmed: bool,
    pub segment_count: i64,
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
        sqlx::query_scalar("SELECT COUNT(*) FROM speakers").fetch_one(pool).await
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
        let res = sqlx::query("UPDATE speakers SET display_name = ?, is_confirmed = 1 WHERE id = ?")
            .bind(display_name)
            .bind(speaker_id)
            .execute(pool)
            .await?;
        Ok(res.rows_affected())
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

    /// Speakers referenced by a meeting's transcripts, with per-meeting segment counts,
    /// most-spoken first.
    pub async fn meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        let rows: Vec<(i64, String, i64, i64)> = sqlx::query_as(
            "SELECT s.id, s.display_name, s.is_confirmed, COUNT(t.id) AS segment_count \
             FROM speakers s \
             JOIN transcripts t ON t.speaker_id = s.id \
             WHERE t.meeting_id = ? \
             GROUP BY s.id, s.display_name, s.is_confirmed \
             ORDER BY segment_count DESC, s.id ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, display_name, is_confirmed, segment_count)| MeetingSpeaker {
                id,
                display_name,
                is_confirmed: is_confirmed != 0,
                segment_count,
            })
            .collect())
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
}
