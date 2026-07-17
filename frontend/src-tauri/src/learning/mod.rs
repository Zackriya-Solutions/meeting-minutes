//! Human-governed, local long-term learning.
//!
//! Predictions are observations, never training truth. Only explicit trusted
//! assertions can create voice samples or rebuild a speaker profile.

pub mod advanced;
pub mod classification;
pub mod identity;
pub mod reconciliation;
pub mod terminology;

#[cfg(test)]
mod integration_tests {
    use sqlx::sqlite::SqlitePoolOptions;

    async fn migrated_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_meeting(pool: &sqlx::SqlitePool, id: &str, title: &str) {
        sqlx::query(
            "INSERT INTO meetings(id, title, created_at, updated_at) \
             VALUES(?, ?, datetime('now'), datetime('now'))",
        )
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_transcript(
        pool: &sqlx::SqlitePool,
        id: &str,
        meeting_id: &str,
        text: &str,
        start: f64,
        end: f64,
    ) {
        sqlx::query(
            "INSERT INTO transcripts( \
                id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time \
             ) VALUES(?, ?, ?, datetime('now'), ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(text)
        .bind(start)
        .bind(end)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn confirmed_eligible_voice_builds_profile_but_prediction_alone_does_not() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "voice-meeting", "Team sync").await;
        insert_transcript(
            &pool,
            "voice-segment",
            "voice-meeting",
            "Обсудили релиз",
            0.0,
            20.0,
        )
        .await;
        let turns = vec![crate::pipeline::diarization::SpeakerTurn {
            start_ms: 0,
            end_ms: 20_000,
            cluster_id: 0,
        }];
        let mapping = super::identity::resolve_clusters(
            &pool,
            "voice-meeting",
            "run-1",
            &turns,
            &[(0, vec![1.0, 0.0, 0.0])],
        )
        .await
        .unwrap();
        let (placeholder, cluster_id) = mapping[&0];
        super::identity::link_cluster_segment(&pool, cluster_id, "voice-segment", 1.0)
            .await
            .unwrap();
        let samples_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voice_samples")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            samples_before, 0,
            "model observation must not train a profile"
        );

        super::identity::review_identity(
            &pool,
            super::identity::ReviewIdentityInput {
                cluster_id,
                decision: "confirm".to_string(),
                speaker_id: None,
                display_name: Some("Anna".to_string()),
                rejected_speaker_id: None,
                allow_learning: true,
                scope: "cluster".to_string(),
            },
        )
        .await
        .unwrap();
        let profile: (i64, i64, String) = sqlx::query_as(
            "SELECT learning_enabled, profile_version, consent_state FROM speakers WHERE id=?",
        )
        .bind(placeholder)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(profile, (1, 1, "granted".to_string()));
        let trusted_samples: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM voice_samples WHERE speaker_id=? AND eligibility='trusted'",
        )
        .bind(placeholder)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(trusted_samples, 1);
        let active_centroids: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM voice_centroids WHERE speaker_id=? AND is_active=1",
        )
        .bind(placeholder)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(active_centroids, 1);

        super::identity::delete_speaker_learning_data(&pool, placeholder)
            .await
            .unwrap();
        let remaining_derivatives: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM voice_samples WHERE speaker_id=?) + \
                    (SELECT COUNT(*) FROM voice_centroids WHERE speaker_id=?) + \
                    (SELECT COUNT(*) FROM speaker_profile_versions WHERE speaker_id=?)",
        )
        .bind(placeholder)
        .bind(placeholder)
        .bind(placeholder)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(remaining_derivatives, 0);
        let transcript_speaker: Option<i64> =
            sqlx::query_scalar("SELECT speaker_id FROM transcripts WHERE id='voice-segment'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(transcript_speaker, None);
        let operational_speaker: Option<i64> =
            sqlx::query_scalar("SELECT operational_speaker_id FROM speaker_clusters WHERE id=?")
                .bind(cluster_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(operational_speaker, None);
    }

    #[tokio::test]
    async fn confirmed_term_creates_reviewable_historical_backfill() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "term-source", "Product sync").await;
        insert_meeting(&pool, "term-history", "Older product sync").await;
        insert_transcript(
            &pool,
            "term-correction",
            "term-source",
            "обновили гига тул сегодня",
            0.0,
            3.0,
        )
        .await;
        insert_transcript(
            &pool,
            "term-old",
            "term-history",
            "проверили гига тул вчера",
            0.0,
            3.0,
        )
        .await;
        let correction = super::terminology::correct_transcript(
            &pool,
            super::terminology::TranscriptCorrectionInput {
                transcript_id: "term-correction".to_string(),
                corrected_text: "обновили GigaTool сегодня".to_string(),
            },
        )
        .await
        .unwrap();
        let term_id = correction.terminology_candidate_id.unwrap();
        super::terminology::review_term(
            &pool,
            super::terminology::ReviewTerminologyInput {
                term_id,
                status: "confirmed".to_string(),
                canonical: None,
            },
        )
        .await
        .unwrap();
        let proposals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reconciliation_suggestions \
             WHERE target_id='term-old' AND suggestion_kind='terminology_backfill' \
               AND status='pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(proposals, 1);
        let unchanged: String =
            sqlx::query_scalar("SELECT transcript FROM transcripts WHERE id='term-old'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, "проверили гига тул вчера");
    }
}
