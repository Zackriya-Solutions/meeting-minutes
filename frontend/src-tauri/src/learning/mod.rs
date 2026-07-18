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
    async fn identity_review_exposes_transcript_label_and_playable_evidence() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "review-meeting", "Product sync").await;
        for (id, text, start, end) in [
            ("long", "Длинная вводная реплика", 0.0, 30.0),
            ("best", "Короткий узнаваемый фрагмент", 40.0, 48.0),
            ("second", "Ещё один фрагмент голоса", 60.0, 70.0),
            ("third", "Третий фрагмент голоса", 80.0, 91.0),
            ("fourth", "Лишний четвёртый фрагмент", 100.0, 112.0),
        ] {
            insert_transcript(&pool, id, "review-meeting", text, start, end).await;
        }
        let turns = vec![crate::pipeline::diarization::SpeakerTurn {
            start_ms: 0,
            end_ms: 120_000,
            cluster_id: 0,
        }];
        let mapping = super::identity::resolve_clusters(
            &pool,
            "review-meeting",
            "review-run",
            &turns,
            &[(0, vec![1.0, 0.0, 0.0])],
        )
        .await
        .unwrap();
        let (speaker_id, cluster_id) = mapping[&0];
        for transcript_id in ["long", "best", "second", "third", "fourth"] {
            super::identity::link_cluster_segment(&pool, cluster_id, transcript_id, 1.0)
                .await
                .unwrap();
            sqlx::query("UPDATE transcripts SET speaker_id=? WHERE id=?")
                .bind(speaker_id)
                .bind(transcript_id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let review = super::identity::list_identity_review(&pool, "review-meeting")
            .await
            .unwrap();
        assert_eq!(review.len(), 1);
        assert_eq!(
            review[0].operational_display_name.as_deref(),
            Some("Speaker 1")
        );
        assert_eq!(review[0].samples.len(), 3);
        assert_eq!(review[0].samples[0].transcript_id, "best");
        assert_eq!(review[0].samples[0].start_seconds, 40.0);
        assert!(review[0]
            .samples
            .iter()
            .all(|sample| sample.transcript_id != "long"));
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

    #[tokio::test]
    async fn global_terminology_deduplicates_and_failed_candidate_rolls_back_correction() {
        let pool = migrated_pool().await;
        for (meeting_id, transcript_id, text) in [
            ("term-one", "segment-one", "обновили гига тул сегодня"),
            ("term-two", "segment-two", "проверили гига тул вчера"),
        ] {
            insert_meeting(&pool, meeting_id, "Product sync").await;
            insert_transcript(&pool, transcript_id, meeting_id, text, 0.0, 3.0).await;
            super::terminology::correct_transcript(
                &pool,
                super::terminology::TranscriptCorrectionInput {
                    transcript_id: transcript_id.to_string(),
                    corrected_text: text.replace("гига тул", "GigaTool"),
                },
            )
            .await
            .unwrap();
        }
        let terms: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id, support_count FROM terminology_terms \
             WHERE scope_kind='global' AND scope_id IS NULL AND normalized_canonical='gigatool'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].1, 2);

        insert_meeting(&pool, "term-failure", "Product sync").await;
        insert_transcript(
            &pool,
            "segment-failure",
            "term-failure",
            "обновили альфа тул",
            0.0,
            3.0,
        )
        .await;
        sqlx::raw_sql(
            "CREATE TRIGGER reject_terminology_candidate \
             BEFORE INSERT ON terminology_terms BEGIN SELECT RAISE(ABORT, 'forced failure'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();
        let result = super::terminology::correct_transcript(
            &pool,
            super::terminology::TranscriptCorrectionInput {
                transcript_id: "segment-failure".to_string(),
                corrected_text: "обновили AlphaTool".to_string(),
            },
        )
        .await;
        assert!(result.is_err());
        let transcript: (String, i64) = sqlx::query_as(
            "SELECT transcript, transcript_version FROM transcripts WHERE id='segment-failure'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(transcript, ("обновили альфа тул".to_string(), 1));
        let corrections: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transcript_corrections WHERE transcript_id='segment-failure'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(corrections, 0);
    }

    #[tokio::test]
    async fn profile_rollback_restores_the_exact_published_embedding() {
        let pool = migrated_pool().await;
        let version_one: Vec<u8> = [0.25_f32, 0.75_f32]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let version_two: Vec<u8> = [0.9_f32, 0.1_f32]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let speaker_id: i64 = sqlx::query_scalar(
            "INSERT INTO speakers(display_name, voice_embedding, is_confirmed, profile_version) \
             VALUES('Anna', ?, 1, 2) RETURNING id",
        )
        .bind(&version_two)
        .fetch_one(&pool)
        .await
        .unwrap();
        for (version, parent, embedding, active) in [
            (1_i64, None, version_one.clone(), 0_i64),
            (2_i64, Some(1_i64), version_two.clone(), 1_i64),
        ] {
            sqlx::query(
                "INSERT INTO speaker_profile_versions( \
                    speaker_id, version, parent_version, build_reason, snapshot_json, \
                    published_embedding, model_version, is_active \
                 ) VALUES(?, ?, ?, 'test', '{}', ?, 'test-v1', ?)",
            )
            .bind(speaker_id)
            .bind(version)
            .bind(parent)
            .bind(embedding)
            .bind(active)
            .execute(&pool)
            .await
            .unwrap();
        }

        super::identity::rollback_profile(&pool, speaker_id, 1)
            .await
            .unwrap();
        let restored: (Vec<u8>, i64) =
            sqlx::query_as("SELECT voice_embedding, profile_version FROM speakers WHERE id=?")
                .bind(speaker_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(restored, (version_one, 1));
    }

    #[tokio::test]
    async fn identity_reconciliation_rollback_supersedes_trust_without_rolling_back_the_run() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "reconcile-meeting", "Team sync").await;
        let previous_speaker: i64 = sqlx::query_scalar(
            "INSERT INTO speakers(display_name, is_confirmed) VALUES('Speaker old', 0) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let proposed_speaker: i64 = sqlx::query_scalar(
            "INSERT INTO speakers(display_name, is_confirmed) VALUES('Anna', 1) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cluster_id: i64 = sqlx::query_scalar(
            "INSERT INTO speaker_clusters( \
                meeting_id, diarization_run_id, local_cluster_id, placeholder_speaker_id, \
                operational_speaker_id, speech_duration_ms, model_version \
             ) VALUES('reconcile-meeting', 'run-1', 0, ?, ?, 20000, 'voice-v1') RETURNING id",
        )
        .bind(previous_speaker)
        .bind(previous_speaker)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO identity_assertions( \
                assertion_uuid, cluster_id, polarity, scope, actor_kind, trust_tier, reason \
             ) VALUES('initial-unknown', ?, 'unknown', 'cluster', 'policy', 'operational', 'initial')",
        )
        .bind(cluster_id)
        .execute(&pool)
        .await
        .unwrap();
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO reconciliation_runs( \
                run_uuid, trigger_kind, input_snapshot_json, status \
             ) VALUES('reconcile-run', 'speaker_profile_updated', '{}', 'proposed') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let suggestion_id: i64 = sqlx::query_scalar(
            "INSERT INTO reconciliation_suggestions( \
                run_id, meeting_id, target_type, target_id, suggestion_kind, \
                previous_value_json, proposed_value_json, confidence \
             ) VALUES(?, 'reconcile-meeting', 'speaker_cluster', ?, 'identity_backfill', \
                       ?, ?, 0.95) RETURNING id",
        )
        .bind(run_id)
        .bind(cluster_id.to_string())
        .bind(serde_json::json!({"speaker_id": previous_speaker}).to_string())
        .bind(serde_json::json!({"speaker_id": proposed_speaker}).to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO reconciliation_suggestions( \
                run_id, meeting_id, target_type, target_id, suggestion_kind, \
                previous_value_json, proposed_value_json, confidence, status \
             ) VALUES(?, 'reconcile-meeting', 'transcript', 'already-applied', \
                       'terminology_backfill', '{}', '{}', 0.9, 'applied')",
        )
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();

        super::reconciliation::review_suggestion(
            &pool,
            super::reconciliation::ReviewReconciliationInput {
                suggestion_id,
                decision: "accepted".to_string(),
            },
        )
        .await
        .unwrap();
        super::reconciliation::rollback_suggestion(&pool, suggestion_id)
            .await
            .unwrap();

        let cluster_speaker: i64 =
            sqlx::query_scalar("SELECT operational_speaker_id FROM speaker_clusters WHERE id=?")
                .bind(cluster_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cluster_speaker, previous_speaker);
        let assertion: (String, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT polarity, speaker_id, supersedes_id FROM identity_assertions \
             WHERE cluster_id=? ORDER BY id DESC LIMIT 1",
        )
        .bind(cluster_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(assertion.0, "unknown");
        assert_eq!(assertion.1, None);
        assert!(assertion.2.is_some());
        let run_status: String =
            sqlx::query_scalar("SELECT status FROM reconciliation_runs WHERE id=?")
                .bind(run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(run_status, "applied");
    }
}
