use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use std::collections::HashSet;
use tracing::{error, info};
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
            error!("Failed to create meeting '{}': {}", meeting_title, e);
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
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
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
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let trimmed_query = query.trim();
        let normalized_query = trimmed_query.to_lowercase();
        let rows = if trimmed_query.is_ascii() {
            let escaped_query = normalized_query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT m.id, m.title, t.transcript, t.timestamp
                 FROM meetings m
                 JOIN transcripts t ON m.id = t.meeting_id
                 WHERE LOWER(t.transcript) LIKE ? ESCAPE '\\'
                 ORDER BY m.created_at DESC, t.audio_start_time ASC",
            )
            .bind(format!("%{}%", escaped_query))
            .fetch_all(pool)
            .await?
        } else {
            // SQLite's built-in LOWER()/NOCASE only understands ASCII. For Cyrillic
            // and other Unicode scripts, filter with Rust's Unicode lowercase rules.
            sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT m.id, m.title, t.transcript, t.timestamp
                 FROM meetings m
                 JOIN transcripts t ON m.id = t.meeting_id
                 ORDER BY m.created_at DESC, t.audio_start_time ASC",
            )
            .fetch_all(pool)
            .await?
        };

        let mut seen_meetings = HashSet::new();
        let results = rows
            .into_iter()
            .filter(|(_, _, transcript, _)| transcript.to_lowercase().contains(&normalized_query))
            .filter_map(|(id, title, transcript, timestamp)| {
                if !seen_meetings.insert(id.clone()) {
                    return None;
                }
                let match_context = Self::get_match_context(&transcript, trimmed_query);
                Some(TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                })
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        match Self::case_insensitive_char_range(transcript, query) {
            Some((match_start, match_end)) => {
                let chars: Vec<char> = transcript.chars().collect();
                let start_index = match_start.saturating_sub(80);
                let end_index = (match_end + 80).min(chars.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.extend(chars[start_index..end_index].iter());
                if end_index < chars.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }

    fn case_insensitive_char_range(text: &str, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() {
            return None;
        }

        let mut folded = String::new();
        let mut spans = Vec::new();
        for (char_index, character) in text.chars().enumerate() {
            let start = folded.len();
            folded.extend(character.to_lowercase());
            spans.push((start, folded.len(), char_index));
        }

        let folded_query = query.to_lowercase();
        let byte_start = folded.find(&folded_query)?;
        let byte_end = byte_start + folded_query.len();
        let start_char = spans
            .iter()
            .find(|(start, end, _)| *start <= byte_start && byte_start < *end)?
            .2;
        let end_char = spans
            .iter()
            .find(|(start, end, _)| *start < byte_end && byte_end <= *end)
            .map(|(_, _, index)| index + 1)
            .unwrap_or_else(|| text.chars().count());
        Some((start_char, end_char))
    }
}

#[cfg(test)]
mod tests {
    use super::TranscriptsRepository;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn unicode_context_matches_cyrillic_without_byte_slicing_panics() {
        let context = TranscriptsRepository::get_match_context(
            "Команда обсудила Обзор встречи и следующие шаги.",
            "обзор",
        );
        assert!(context.contains("Обзор встречи"));
    }

    #[tokio::test]
    async fn transcript_search_is_case_insensitive_for_cyrillic() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE meetings(
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                audio_start_time REAL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO meetings(id, title, created_at) VALUES('m1', 'Тест', '2026-07-16')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts(id, meeting_id, transcript, timestamp, audio_start_time)
             VALUES('t1', 'm1', 'Обзор встречи и планы', '00:10', 10)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let lower = TranscriptsRepository::search_transcripts(&pool, "обзор")
            .await
            .unwrap();
        let upper = TranscriptsRepository::search_transcripts(&pool, "ОБЗОР")
            .await
            .unwrap();

        assert_eq!(lower.len(), 1);
        assert_eq!(upper.len(), 1);
        assert_eq!(lower[0].id, upper[0].id);

        sqlx::query(
            "INSERT INTO meetings(id, title, created_at) VALUES('m2', 'ASCII', '2026-07-15')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts(id, meeting_id, transcript, timestamp, audio_start_time)
             VALUES('t2', 'm2', 'Product Overview', '00:05', 5)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ascii = TranscriptsRepository::search_transcripts(&pool, "overview")
            .await
            .unwrap();
        assert_eq!(ascii.len(), 1);
        assert_eq!(ascii[0].id, "m2");
    }
}
