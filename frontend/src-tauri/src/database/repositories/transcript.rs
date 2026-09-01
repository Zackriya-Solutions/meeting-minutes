use crate::api::{TranscriptAnnotationInput, TranscriptSearchResult, TranscriptSegment};
use super::annotation::{annotation_directory, AnnotationsRepository};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use std::path::Path;
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
        annotations: &[TranscriptAnnotationInput],
        app_data_dir: &Path,
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
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, source, translation, translation_target_language)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.source)
            .bind(&segment.translation)
            .bind(&segment.translation_target_language)
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

        // Keep the meeting, transcript rows, and annotation rows in one transaction.
        let directory = annotation_directory(app_data_dir, &meeting_id, folder_path.as_deref())?;
        let (_, files) = match AnnotationsRepository::save_in_transaction(
            &mut transaction,
            &meeting_id,
            annotations,
            &directory,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        };

        if let Err(error) = transaction.commit().await {
            files.rollback().await;
            return Err(error);
        }
        files.commit().await;

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
    use crate::api::TranscriptAnnotationInput;
    use crate::database::models::Transcript;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    const SOURCE_MIGRATION_VERSION: i64 = 20260817000000;
    const TRANSLATION_MIGRATION_VERSION: i64 = 20260831110000;

    async fn in_memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to open in-memory sqlite")
    }

    fn segment(id: &str, text: &str, start: f64, source: Option<&str>) -> TranscriptSegment {
        TranscriptSegment {
            id: id.to_string(),
            text: text.to_string(),
            timestamp: "2026-08-17T10:00:00Z".to_string(),
            audio_start_time: Some(start),
            audio_end_time: Some(start + 1.0),
            duration: Some(1.0),
            source: source.map(|s| s.to_string()),
            translation: None,
            translation_target_language: None,
        }
    }

    #[tokio::test]
    async fn migrated_db_persists_and_reloads_channel_sources_and_translations() {
        let pool = in_memory_pool().await;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations must apply to a fresh database");

        let mut segments = vec![
            segment("seg-mic", "fala do microfone", 0.0, Some("Você")),
            segment("seg-sys", "fala do sistema", 1.0, Some("Outros")),
            segment("seg-legacy", "segmento antigo", 2.0, None),
        ];
        segments[0].translation = Some("tradução da fala".to_string());
        segments[0].translation_target_language = Some("pt-BR".to_string());
        let meeting_id =
            TranscriptsRepository::save_transcript(&pool, "Reunião de teste", &segments, None, &[], std::path::Path::new("/tmp"))
                .await
                .expect("save_transcript must succeed");

        let rows = sqlx::query_as::<_, Transcript>(
            "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time",
        )
        .bind(&meeting_id)
        .fetch_all(&pool)
        .await
        .expect("reload transcripts");

        assert_eq!(rows[0].translation.as_deref(), Some("tradução da fala"));
        assert_eq!(rows[0].translation_target_language.as_deref(), Some("pt-BR"));

        let sources: Vec<Option<String>> = rows.into_iter().map(|t| t.source).collect();
        assert_eq!(
            sources,
            vec![
                Some("Você".to_string()),
                Some("Outros".to_string()),
                None,
            ],
            "microphone/system labels must round-trip through SQLite and legacy segments stay NULL"
        );
    }

    #[tokio::test]
    async fn source_migration_upgrades_populated_legacy_database() {
        let pool = in_memory_pool().await;
        let migrator = sqlx::migrate!("./migrations");

        for migration in migrator.iter() {
            if migration.version >= SOURCE_MIGRATION_VERSION {
                continue;
            }
            sqlx::raw_sql(migration.sql.as_ref())
                .execute(&pool)
                .await
                .unwrap_or_else(|e| panic!("legacy migration {} failed: {}", migration.version, e));
        }

        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind("meeting-legacy")
        .bind("Reunião antiga")
        .bind("2025-01-01T00:00:00Z")
        .bind("2025-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("insert legacy meeting");
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES (?, ?, ?, ?)",
        )
        .bind("transcript-legacy")
        .bind("meeting-legacy")
        .bind("gravado antes da migração")
        .bind("2025-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .expect("insert legacy transcript row");

        let source_migration = migrator
            .iter()
            .find(|m| m.version == SOURCE_MIGRATION_VERSION)
            .expect("add_transcript_source migration must exist");
        sqlx::raw_sql(source_migration.sql.as_ref())
            .execute(&pool)
            .await
            .expect("source migration must apply to a populated legacy database");

        // The current row model includes the later nullable translation fields;
        // apply that migration too so this test can verify loading the upgraded
        // row while retaining the source migration's legacy NULL behavior.
        let translation_migration = migrator
            .iter()
            .find(|m| m.version == TRANSLATION_MIGRATION_VERSION)
            .expect("translation migration must exist");
        sqlx::raw_sql(translation_migration.sql.as_ref())
            .execute(&pool)
            .await
            .expect("translation migration must apply to a populated legacy database");

        let legacy: Transcript = sqlx::query_as("SELECT * FROM transcripts WHERE id = ?")
            .bind("transcript-legacy")
            .fetch_one(&pool)
            .await
            .expect("legacy row must still load after migration");
        assert_eq!(legacy.source, None, "pre-migration rows must load with NULL source");
        assert_eq!(legacy.transcript, "gravado antes da migração");
    }

    #[tokio::test]
    async fn failed_annotation_batch_rolls_back_the_meeting_and_staged_images() {
        let pool = in_memory_pool().await;
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let app_data_dir = tempdir().unwrap();
        let transcripts = vec![segment("seg-1", "hello", 0.0, None)];
        let annotations = vec![
            TranscriptAnnotationInput {
                id: Some("annotation-image".to_string()),
                annotation_type: "image".to_string(),
                anchor_time: 1.0,
                created_at: Some("2026-08-31T10:00:01Z".to_string()),
                text: None,
                image_file: None,
                image_data: Some(vec![1, 2, 3]),
                image_mime: Some("image/png".to_string()),
            },
            TranscriptAnnotationInput {
                id: Some("annotation-invalid".to_string()),
                annotation_type: "note".to_string(),
                anchor_time: 2.0,
                created_at: None,
                text: Some("   ".to_string()),
                image_file: None,
                image_data: None,
                image_mime: None,
            },
        ];

        let result = TranscriptsRepository::save_transcript(
            &pool,
            "Atomic meeting",
            &transcripts,
            None,
            &annotations,
            app_data_dir.path(),
        )
        .await;

        assert!(result.is_err());
        let meeting_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings")
            .fetch_one(&pool)
            .await
            .unwrap();
        let annotation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transcript_annotations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(meeting_count, 0);
        assert_eq!(annotation_count, 0);
        if let Ok(meetings) = std::fs::read_dir(app_data_dir.path().join("meetings")) {
            for meeting in meetings {
                let meeting = meeting.unwrap();
                assert!(!meeting.path().join("annotations").exists());
            }
        }
    }
}
