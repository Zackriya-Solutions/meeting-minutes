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
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
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

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
        tag_ids: &[String],
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());
        let mut tags: Vec<&str> = Vec::new();
        for id in tag_ids {
            if !tags.contains(&id.as_str()) {
                tags.push(id.as_str());
            }
        }

        let mut sql = String::from(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ? AND m.deleted_at IS NULL",
        );
        if !tags.is_empty() {
            // Meetings carrying every selected tag (AND).
            let placeholders = vec!["?"; tags.len()].join(", ");
            sql.push_str(&format!(
                " AND m.id IN (
                    SELECT mt.meeting_id FROM meeting_tags mt
                    JOIN tags tg ON tg.id = mt.tag_id
                    WHERE mt.deleted_at IS NULL AND tg.deleted_at IS NULL AND mt.tag_id IN ({})
                    GROUP BY mt.meeting_id
                    HAVING COUNT(DISTINCT mt.tag_id) = ?)",
                placeholders
            ));
        }

        let mut statement =
            sqlx::query_as::<_, (String, String, String, String)>(&sql).bind(&search_query);
        for id in &tags {
            statement = statement.bind(*id);
        }
        if !tags.is_empty() {
            statement = statement.bind(tags.len() as i64);
        }
        let rows = statement.fetch_all(pool).await?;

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
mod search_tests {
    use super::*;
    use crate::database::repositories::tag::TagRepository;
    use crate::database::test_support::{insert_meeting, insert_transcript, test_pool};

    fn ids(results: &[TranscriptSearchResult]) -> Vec<String> {
        let mut ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    #[tokio::test]
    async fn search_requires_all_selected_tags_and_skips_trash() {
        let pool = test_pool().await;
        for id in ["meeting-1", "meeting-2", "meeting-3"] {
            insert_meeting(&pool, id, id, None).await;
            insert_transcript(&pool, id, "we discussed the budget").await;
        }
        let a = TagRepository::create_or_get(&pool, "a").await.unwrap();
        let b = TagRepository::create_or_get(&pool, "b").await.unwrap();
        TagRepository::set_meeting_tags(&pool, "meeting-1", &[a.id.clone(), b.id.clone()]).await.unwrap();
        TagRepository::set_meeting_tags(&pool, "meeting-2", &[a.id.clone()]).await.unwrap();

        let all = TranscriptsRepository::search_transcripts(&pool, "budget", &[]).await.unwrap();
        assert_eq!(ids(&all), vec!["meeting-1", "meeting-2", "meeting-3"]);

        let only_a = TranscriptsRepository::search_transcripts(&pool, "budget", &[a.id.clone()])
            .await
            .unwrap();
        assert_eq!(ids(&only_a), vec!["meeting-1", "meeting-2"]);

        let a_and_b = TranscriptsRepository::search_transcripts(
            &pool,
            "budget",
            &[a.id.clone(), b.id.clone(), a.id.clone()],
        )
        .await
        .unwrap();
        assert_eq!(ids(&a_and_b), vec!["meeting-1"]);

        sqlx::query("UPDATE meetings SET deleted_at = datetime('now') WHERE id = 'meeting-3'")
            .execute(&pool)
            .await
            .unwrap();
        let after_trash = TranscriptsRepository::search_transcripts(&pool, "budget", &[]).await.unwrap();
        assert_eq!(ids(&after_trash), vec!["meeting-1", "meeting-2"]);
    }
}
