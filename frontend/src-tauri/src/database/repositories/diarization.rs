use crate::database::models::DiarizationSetting;
use sqlx::SqlitePool;

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
}
