use crate::database::models::DiarizationSetting;
use sqlx::{Sqlite, SqlitePool};

pub struct DiarizationRepository;

impl DiarizationRepository {
    pub async fn get_settings(pool: &SqlitePool) -> Result<DiarizationSetting, sqlx::Error> {
        let existing = sqlx::query_as::<_, DiarizationSetting>(
            "SELECT * FROM diarization_settings WHERE id = '1'",
        )
        .fetch_optional(pool)
        .await?;

        if let Some(settings) = existing {
            return Ok(settings);
        }

        sqlx::query("INSERT INTO diarization_settings (id) VALUES ('1')")
            .execute(pool)
            .await?;

        sqlx::query_as::<_, DiarizationSetting>("SELECT * FROM diarization_settings WHERE id = '1'")
            .fetch_one(pool)
            .await
    }

    pub async fn save_settings(
        pool: &SqlitePool,
        enabled: bool,
        mode: &str,
        show_provisional_labels: bool,
        post_call_refinement_enabled: bool,
        overlap_handling: &str,
        speaker_review_enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO diarization_settings (
                id, enabled, mode, show_provisional_labels,
                post_call_refinement_enabled, overlap_handling,
                speaker_review_enabled, updated_at
             )
             VALUES ('1', ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                mode = excluded.mode,
                show_provisional_labels = excluded.show_provisional_labels,
                post_call_refinement_enabled = excluded.post_call_refinement_enabled,
                overlap_handling = excluded.overlap_handling,
                speaker_review_enabled = excluded.speaker_review_enabled,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(enabled as i64)
        .bind(mode)
        .bind(show_provisional_labels as i64)
        .bind(post_call_refinement_enabled as i64)
        .bind(overlap_handling)
        .bind(speaker_review_enabled as i64)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn update_transcript_assignment<'e, E>(
        executor: E,
        transcript_id: &str,
        assignment: &crate::diarization::types::SpeakerAssignment,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        let diarization_status =
            diarization_status_to_database_value(assignment.diarization_status);

        sqlx::query(
            "UPDATE transcripts SET
                speaker_id = ?,
                speaker_label = ?,
                speaker_color = ?,
                is_overlap = ?,
                diarization_status = ?,
                diarization_method = ?,
                diarization_confidence = ?
             WHERE id = ?",
        )
        .bind(&assignment.speaker_id)
        .bind(&assignment.speaker_label)
        .bind(&assignment.speaker_color)
        .bind(assignment.is_overlap as i64)
        .bind(diarization_status)
        .bind(&assignment.diarization_method)
        .bind(assignment.diarization_confidence)
        .bind(transcript_id)
        .execute(executor)
        .await?;

        Ok(())
    }
}

pub(crate) fn diarization_status_to_database_value(
    status: crate::diarization::types::DiarizationStatus,
) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diarization::types::{DiarizationStatus, SpeakerAssignment};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn transcript_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("test pool should connect");

        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                speaker_id TEXT,
                speaker_label TEXT,
                speaker_color TEXT,
                is_overlap INTEGER NOT NULL DEFAULT 0,
                diarization_status TEXT NOT NULL DEFAULT 'none',
                diarization_method TEXT,
                diarization_confidence REAL
            )",
        )
        .execute(&pool)
        .await
        .expect("transcripts table should be created");

        sqlx::query("INSERT INTO transcripts (id) VALUES ('transcript-1')")
            .execute(&pool)
            .await
            .expect("transcript row should be inserted");

        pool
    }

    #[tokio::test]
    async fn update_transcript_assignment_persists_assignment_fields() {
        let pool = transcript_test_pool().await;
        let assignment = SpeakerAssignment {
            speaker_id: Some("speaker-1".to_string()),
            speaker_label: Some("Speaker 1".to_string()),
            speaker_color: Some("#2563eb".to_string()),
            is_overlap: true,
            diarization_status: DiarizationStatus::Final,
            diarization_method: Some("pyannote-rs".to_string()),
            diarization_confidence: Some(0.82),
        };

        DiarizationRepository::update_transcript_assignment(&pool, "transcript-1", &assignment)
            .await
            .expect("assignment should be persisted");

        let row: (
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            String,
            Option<String>,
            Option<f64>,
        ) = sqlx::query_as(
            "SELECT
                speaker_id,
                speaker_label,
                speaker_color,
                is_overlap,
                diarization_status,
                diarization_method,
                diarization_confidence
             FROM transcripts WHERE id = 'transcript-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("updated transcript row should be fetched");

        assert_eq!(row.0.as_deref(), Some("speaker-1"));
        assert_eq!(row.1.as_deref(), Some("Speaker 1"));
        assert_eq!(row.2.as_deref(), Some("#2563eb"));
        assert_eq!(row.3, 1);
        assert_eq!(row.4, "final");
        assert_eq!(row.5.as_deref(), Some("pyannote-rs"));
        assert_eq!(row.6, Some(0.82));
    }
}
