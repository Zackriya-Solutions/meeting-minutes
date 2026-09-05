use crate::database::models::DiarizationSetting;
use serde::Serialize;
use sqlx::{Sqlite, SqlitePool};

pub struct DiarizationRepository;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSpeaker {
    pub speaker_id: String,
    pub speaker_label: String,
    pub speaker_color: Option<String>,
    pub segment_count: i64,
}

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

    pub async fn get_meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, sqlx::Error> {
        sqlx::query_as::<_, MeetingSpeaker>(
            "SELECT
                speaker_id AS speaker_id,
                COALESCE(NULLIF(TRIM(MAX(speaker_label)), ''), speaker_id) AS speaker_label,
                MAX(speaker_color) AS speaker_color,
                COUNT(*) AS segment_count
             FROM transcripts
             WHERE meeting_id = ?
                AND speaker_id IS NOT NULL
                AND TRIM(speaker_id) <> ''
             GROUP BY speaker_id
             ORDER BY MIN(COALESCE(audio_start_time, 0)), speaker_id",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    pub async fn rename_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
        speaker_id: &str,
        speaker_label: &str,
    ) -> Result<u64, sqlx::Error> {
        let mut transaction = pool.begin().await?;

        let transcript_result = sqlx::query(
            "UPDATE transcripts
             SET speaker_label = ?
             WHERE meeting_id = ? AND speaker_id = ?",
        )
        .bind(speaker_label)
        .bind(meeting_id)
        .bind(speaker_id)
        .execute(&mut *transaction)
        .await?;

        let segment_result = sqlx::query(
            "UPDATE speaker_segments
             SET speaker_label = ?
             WHERE meeting_id = ? AND speaker_id = ?",
        )
        .bind(speaker_label)
        .bind(meeting_id)
        .bind(speaker_id)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "UPDATE meeting_diarization_status
             SET updated_at = CURRENT_TIMESTAMP
             WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(transcript_result.rows_affected() + segment_result.rows_affected())
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

    async fn speaker_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("test pool should connect");

        sqlx::query(
            "CREATE TABLE transcripts (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                speaker_id TEXT,
                speaker_label TEXT,
                speaker_color TEXT,
                audio_start_time REAL
            )",
        )
        .execute(&pool)
        .await
        .expect("transcripts table should be created");

        sqlx::query(
            "CREATE TABLE speaker_segments (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                source TEXT NOT NULL,
                start_time REAL NOT NULL,
                end_time REAL NOT NULL,
                speaker_id TEXT,
                speaker_label TEXT,
                confidence REAL,
                is_overlap INTEGER NOT NULL DEFAULT 0,
                diarization_status TEXT NOT NULL,
                diarization_method TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await
        .expect("speaker_segments table should be created");

        sqlx::query(
            "CREATE TABLE meeting_diarization_status (
                meeting_id TEXT PRIMARY KEY NOT NULL,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&pool)
        .await
        .expect("meeting_diarization_status table should be created");

        sqlx::query(
            "INSERT INTO meeting_diarization_status (meeting_id) VALUES ('meeting-1')",
        )
        .execute(&pool)
        .await
        .expect("meeting diarization status should be inserted");

        for (id, meeting_id, speaker_id, speaker_label, speaker_color, audio_start_time) in [
            (
                "t-1",
                "meeting-1",
                Some("speaker-1"),
                Some("Speaker 1"),
                Some("#2563eb"),
                0.0,
            ),
            (
                "t-2",
                "meeting-1",
                Some("speaker-1"),
                Some("Speaker 1"),
                Some("#2563eb"),
                1.0,
            ),
            (
                "t-3",
                "meeting-1",
                Some("speaker-2"),
                Some("Speaker 2"),
                Some("#16a34a"),
                2.0,
            ),
            (
                "t-4",
                "meeting-2",
                Some("speaker-1"),
                Some("Other meeting speaker"),
                Some("#dc2626"),
                3.0,
            ),
            ("t-5", "meeting-1", None, None, None, 4.0),
        ] {
            sqlx::query(
                "INSERT INTO transcripts (
                    id, meeting_id, speaker_id, speaker_label, speaker_color, audio_start_time
                 )
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(meeting_id)
            .bind(speaker_id)
            .bind(speaker_label)
            .bind(speaker_color)
            .bind(audio_start_time)
            .execute(&pool)
            .await
            .expect("transcript row should be inserted");
        }

        for (id, meeting_id, speaker_id, speaker_label) in [
            ("s-1", "meeting-1", Some("speaker-1"), Some("Speaker 1")),
            ("s-2", "meeting-1", Some("speaker-1"), Some("Speaker 1")),
            ("s-3", "meeting-1", Some("speaker-2"), Some("Speaker 2")),
            (
                "s-4",
                "meeting-2",
                Some("speaker-1"),
                Some("Other meeting speaker"),
            ),
        ] {
            sqlx::query(
                "INSERT INTO speaker_segments (
                    id, meeting_id, source, start_time, end_time,
                    speaker_id, speaker_label, diarization_status
                 )
                 VALUES (?, ?, 'unit_test', 0, 1, ?, ?, 'final')",
            )
            .bind(id)
            .bind(meeting_id)
            .bind(speaker_id)
            .bind(speaker_label)
            .execute(&pool)
            .await
            .expect("speaker segment row should be inserted");
        }

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

    #[tokio::test]
    async fn get_meeting_speakers_returns_distinct_speakers_for_full_meeting() {
        let pool = speaker_test_pool().await;

        let speakers = DiarizationRepository::get_meeting_speakers(&pool, "meeting-1")
            .await
            .expect("speakers should be loaded");

        assert_eq!(speakers.len(), 2);
        assert_eq!(speakers[0].speaker_id, "speaker-1");
        assert_eq!(speakers[0].speaker_label, "Speaker 1");
        assert_eq!(speakers[0].speaker_color.as_deref(), Some("#2563eb"));
        assert_eq!(speakers[0].segment_count, 2);
        assert_eq!(speakers[1].speaker_id, "speaker-2");
        assert_eq!(speakers[1].speaker_label, "Speaker 2");
        assert_eq!(speakers[1].segment_count, 1);
    }

    #[tokio::test]
    async fn rename_speaker_updates_transcripts_and_segments_for_one_meeting() {
        let pool = speaker_test_pool().await;

        let updated_count =
            DiarizationRepository::rename_speaker(&pool, "meeting-1", "speaker-1", "Alice")
                .await
                .expect("speaker should be renamed");

        assert_eq!(updated_count, 4);

        let meeting_one_labels: Vec<String> = sqlx::query_scalar(
            "SELECT speaker_label FROM transcripts
             WHERE meeting_id = 'meeting-1' AND speaker_id = 'speaker-1'
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("meeting one transcript labels should be fetched");

        assert_eq!(meeting_one_labels, vec!["Alice", "Alice"]);

        let meeting_two_label: String = sqlx::query_scalar(
            "SELECT speaker_label FROM transcripts
             WHERE meeting_id = 'meeting-2' AND speaker_id = 'speaker-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("meeting two transcript label should be fetched");

        assert_eq!(meeting_two_label, "Other meeting speaker");

        let segment_labels: Vec<String> = sqlx::query_scalar(
            "SELECT speaker_label FROM speaker_segments
             WHERE meeting_id = 'meeting-1' AND speaker_id = 'speaker-1'
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("speaker segment labels should be fetched");

        assert_eq!(segment_labels, vec!["Alice", "Alice"]);
    }
}
