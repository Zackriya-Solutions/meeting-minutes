use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use log::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting with id: {}", meeting_id);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.speaker)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!("Failed to save transcript segment for meeting {}", meeting_id);
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Inserts `segment` as the transcript row `id` of the meeting.
    pub async fn insert_row(
        conn: &mut sqlx::SqliteConnection,
        id: &str,
        meeting_id: &str,
        segment: &TranscriptSegment,
    ) -> Result<(), SqlxError> {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .bind(&segment.speaker)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::meeting::MeetingsRepository;
    use crate::database::test_support::{migrated_pool, seed_person};

    fn segment(id: &str, start: f64, speaker: Option<&str>) -> TranscriptSegment {
        TranscriptSegment {
            id: id.to_string(),
            text: format!("text {id}"),
            timestamp: "2026-09-27T10:00:00Z".to_string(),
            audio_start_time: Some(start),
            audio_end_time: Some(start + 1.0),
            duration: Some(1.0),
            speaker: speaker.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn saved_speakers_round_trip_through_pagination() {
        let pool = migrated_pool().await;
        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Standup",
            &[segment("a", 0.0, Some("spk_1")), segment("b", 2.0, None)],
            None,
        )
        .await
        .unwrap();

        let (rows, total) =
            MeetingsRepository::get_meeting_transcripts_paginated(&pool, &meeting_id, 10, 0)
                .await
                .unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows[0].speaker.as_deref(), Some("spk_1"));
        assert_eq!(rows[1].speaker, None);
    }

    #[tokio::test]
    async fn per_meeting_speaker_queries_use_an_index() {
        let pool = migrated_pool().await;
        let plan: Vec<(i64, i64, i64, String)> =
            sqlx::query_as("EXPLAIN QUERY PLAN SELECT DISTINCT speaker FROM transcripts WHERE meeting_id = ?")
                .bind("m")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(plan.iter().any(|row| row.3.contains("idx_transcripts_meeting_speaker")), "{plan:?}");
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_its_speakers() {
        let pool = migrated_pool().await;
        let meeting_id = TranscriptsRepository::save_transcript(&pool, "M", &[segment("a", 0.0, Some("spk_0"))], None)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meeting_speakers (meeting_id, speaker_key, created_at) VALUES (?, 'spk_0', '2026-09-27T10:00:00Z')")
            .bind(&meeting_id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(MeetingsRepository::delete_meeting(&pool, &meeting_id).await.unwrap());

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_speakers WHERE meeting_id = ?")
            .bind(&meeting_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_its_rejections() {
        let pool = migrated_pool().await;
        // The delete must not rely on the cascade. One connection, so the pragma holds.
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&pool).await.unwrap();
        let meeting_id = TranscriptsRepository::save_transcript(&pool, "M", &[segment("a", 0.0, Some("spk_0"))], None)
            .await
            .unwrap();
        seed_person(&pool, "person-noah", "Noah").await;
        sqlx::query("INSERT INTO speaker_rejections (meeting_id, speaker_key, person_id) VALUES (?, 'spk_0', 'person-noah')")
            .bind(&meeting_id)
            .execute(&pool)
            .await
            .unwrap();

        assert!(MeetingsRepository::delete_meeting(&pool, &meeting_id).await.unwrap());

        let (rejections,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM speaker_rejections WHERE meeting_id = ?")
            .bind(&meeting_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rejections, 0);
        let (people,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM people").fetch_one(&pool).await.unwrap();
        assert_eq!(people, 1, "the person outlives the meeting");
    }
}
