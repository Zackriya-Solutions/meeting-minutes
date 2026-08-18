use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExistingAudioMeeting {
    pub meeting_id: String,
    pub title: String,
    pub created_at: String,
    /// How that import was processed. `None` means the meeting predates the
    /// column: unknown, which is not the same as "no".
    pub denoise_applied: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityRegistration {
    Canonical,
    DuplicateCandidate { canonical_meeting_id: String },
}

pub async fn find_canonical_meeting(
    pool: &SqlitePool,
    sha256: &str,
) -> Result<Option<ExistingAudioMeeting>, sqlx::Error> {
    sqlx::query_as::<_, ExistingAudioMeeting>(
        "SELECT m.id AS meeting_id, m.title, CAST(m.created_at AS TEXT) AS created_at, \
                ai.denoise_applied \
         FROM audio_identities ai \
         JOIN meetings m ON m.id = ai.canonical_meeting_id \
         WHERE ai.sha256 = ?",
    )
    .bind(sha256)
    .fetch_optional(pool)
    .await
}

/// Return the already-imported take for one exact processing mode. A content
/// hash may have one raw and one denoised take, but repeating either mode is
/// idempotent and must resolve to the existing meeting.
pub async fn find_processing_variant(
    pool: &SqlitePool,
    sha256: &str,
    denoise_applied: bool,
) -> Result<Option<ExistingAudioMeeting>, sqlx::Error> {
    sqlx::query_as::<_, ExistingAudioMeeting>(
        "SELECT m.id AS meeting_id, m.title, CAST(m.created_at AS TEXT) AS created_at, \
                mai.denoise_applied \
         FROM meeting_audio_identities mai \
         JOIN meetings m ON m.id = mai.meeting_id \
         WHERE mai.sha256 = ? AND mai.denoise_applied = ? \
         ORDER BY CASE mai.role WHEN 'canonical' THEN 0 ELSE 1 END, m.created_at, m.id \
         LIMIT 1",
    )
    .bind(sha256)
    .bind(i64::from(denoise_applied))
    .fetch_optional(pool)
    .await
}

pub async fn register_import_identity(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
    sha256: &str,
    byte_size: u64,
    duration_ms: Option<i64>,
    denoise_applied: Option<bool>,
) -> Result<IdentityRegistration, sqlx::Error> {
    let existing_canonical: Option<String> =
        sqlx::query_scalar("SELECT canonical_meeting_id FROM audio_identities WHERE sha256 = ?")
            .bind(sha256)
            .fetch_optional(&mut **tx)
            .await?;

    if let Some(canonical_meeting_id) = existing_canonical {
        if canonical_meeting_id == meeting_id {
            sqlx::query(
                "INSERT INTO meeting_audio_identities \
                 (meeting_id, sha256, role, denoise_applied) \
                 VALUES (?, ?, 'canonical', ?) \
                 ON CONFLICT(meeting_id) DO UPDATE SET \
                   sha256=excluded.sha256, role='canonical', \
                   denoise_applied=excluded.denoise_applied",
            )
            .bind(meeting_id)
            .bind(sha256)
            .bind(denoise_applied.map(i64::from))
            .execute(&mut **tx)
            .await?;
            return Ok(IdentityRegistration::Canonical);
        }
        sqlx::query(
            "INSERT INTO meeting_audio_identities \
             (meeting_id, sha256, role, denoise_applied) \
             VALUES (?, ?, 'duplicate_candidate', ?) \
             ON CONFLICT(meeting_id) DO UPDATE SET \
               sha256=excluded.sha256, role='duplicate_candidate', \
               denoise_applied=excluded.denoise_applied, detected_at=datetime('now')",
        )
        .bind(meeting_id)
        .bind(sha256)
        .bind(denoise_applied.map(i64::from))
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO audio_duplicate_reviews \
             (duplicate_meeting_id, canonical_meeting_id, sha256, status) \
             VALUES (?, ?, ?, 'pending') \
             ON CONFLICT(duplicate_meeting_id) DO UPDATE SET \
               canonical_meeting_id=excluded.canonical_meeting_id, \
               sha256=excluded.sha256, status='pending', resolved_at=NULL",
        )
        .bind(meeting_id)
        .bind(&canonical_meeting_id)
        .bind(sha256)
        .execute(&mut **tx)
        .await?;
        return Ok(IdentityRegistration::DuplicateCandidate {
            canonical_meeting_id,
        });
    }

    sqlx::query(
        "INSERT INTO audio_identities \
         (sha256, canonical_meeting_id, byte_size, duration_ms, denoise_applied) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(sha256)
    .bind(meeting_id)
    .bind(i64::try_from(byte_size).unwrap_or(i64::MAX))
    .bind(duration_ms)
    .bind(denoise_applied.map(i64::from))
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO meeting_audio_identities \
         (meeting_id, sha256, role, denoise_applied) \
         VALUES (?, ?, 'canonical', ?)",
    )
    .bind(meeting_id)
    .bind(sha256)
    .bind(denoise_applied.map(i64::from))
    .execute(&mut **tx)
    .await?;
    Ok(IdentityRegistration::Canonical)
}

/// Register an existing meeting discovered by the migration audit. Unlike a
/// new import, legacy meetings may already contain different edits derived
/// from the same audio. The richer meeting becomes canonical, while every
/// other meeting remains intact and pending explicit review.
pub async fn register_backfilled_identity(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
    sha256: &str,
    byte_size: u64,
    duration_ms: Option<i64>,
) -> Result<IdentityRegistration, sqlx::Error> {
    let registration =
        register_import_identity(tx, meeting_id, sha256, byte_size, duration_ms, None).await?;
    let IdentityRegistration::DuplicateCandidate {
        canonical_meeting_id,
    } = registration
    else {
        return Ok(IdentityRegistration::Canonical);
    };

    let candidate_rank = meeting_content_rank(tx, meeting_id).await?;
    let canonical_rank = meeting_content_rank(tx, &canonical_meeting_id).await?;
    if candidate_rank <= canonical_rank {
        return Ok(IdentityRegistration::DuplicateCandidate {
            canonical_meeting_id,
        });
    }

    sqlx::query(
        "UPDATE audio_identities SET canonical_meeting_id = ?, denoise_applied = ( \
             SELECT denoise_applied FROM meeting_audio_identities WHERE meeting_id = ? \
         ) WHERE sha256 = ?",
    )
    .bind(meeting_id)
    .bind(meeting_id)
    .bind(sha256)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE meeting_audio_identities SET role = \
         CASE WHEN meeting_id = ? THEN 'canonical' ELSE 'duplicate_candidate' END \
         WHERE sha256 = ?",
    )
    .bind(meeting_id)
    .bind(sha256)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM audio_duplicate_reviews WHERE duplicate_meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE audio_duplicate_reviews SET canonical_meeting_id = ? WHERE sha256 = ?")
        .bind(meeting_id)
        .bind(sha256)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO audio_duplicate_reviews \
         (duplicate_meeting_id, canonical_meeting_id, sha256, status) \
         VALUES (?, ?, ?, 'pending') \
         ON CONFLICT(duplicate_meeting_id) DO UPDATE SET \
           canonical_meeting_id=excluded.canonical_meeting_id, \
           sha256=excluded.sha256, status='pending', resolved_at=NULL",
    )
    .bind(&canonical_meeting_id)
    .bind(meeting_id)
    .bind(sha256)
    .execute(&mut **tx)
    .await?;

    Ok(IdentityRegistration::Canonical)
}

async fn meeting_content_rank(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
) -> Result<(i64, std::cmp::Reverse<String>, std::cmp::Reverse<String>), sqlx::Error> {
    let mut score = 0_i64;
    if table_exists(tx, "transcripts").await? {
        score += sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(length(COALESCE(transcript, '')) + \
             4 * length(COALESCE(summary, '')) + \
             4 * length(COALESCE(action_items, '')) + \
             4 * length(COALESCE(key_points, ''))), 0) \
             FROM transcripts WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_one(&mut **tx)
        .await?;
    }
    for (table, weight) in [
        ("summary_processes", 100_i64),
        ("meeting_collections", 20),
        ("action_items", 40),
        ("standup_records", 40),
        ("standup_private_notes", 100),
        ("interview_track_meetings", 20),
    ] {
        if table_exists(tx, table).await? {
            let query = format!("SELECT COUNT(*) FROM {table} WHERE meeting_id = ?");
            let count: i64 = sqlx::query_scalar(&query)
                .bind(meeting_id)
                .fetch_one(&mut **tx)
                .await?;
            score = score.saturating_add(count.saturating_mul(weight));
        }
    }

    let created_at: String =
        sqlx::query_scalar("SELECT CAST(created_at AS TEXT) FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_one(&mut **tx)
            .await?;
    Ok((
        score,
        std::cmp::Reverse(created_at),
        std::cmp::Reverse(meeting_id.to_string()),
    ))
}

async fn table_exists(tx: &mut Transaction<'_, Sqlite>, table: &str) -> Result<bool, sqlx::Error> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
    )
    .bind(table)
    .fetch_one(&mut **tx)
    .await?;
    Ok(exists != 0)
}

/// Remove a meeting's identity while keeping the content hash usable. If the
/// meeting is canonical and a legacy duplicate exists, the oldest candidate is
/// promoted before the meeting row is deleted.
pub async fn release_meeting_identity(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
) -> Result<(), sqlx::Error> {
    let registry_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master \
         WHERE type='table' AND name='audio_identities')",
    )
    .fetch_one(&mut **tx)
    .await?;
    if registry_exists == 0 {
        return Ok(());
    }

    let canonical_hashes: Vec<String> =
        sqlx::query_scalar("SELECT sha256 FROM audio_identities WHERE canonical_meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut **tx)
            .await?;

    for sha256 in canonical_hashes {
        let replacement: Option<String> = sqlx::query_scalar(
            "SELECT meeting_id FROM meeting_audio_identities \
             WHERE sha256 = ? AND meeting_id <> ? \
             ORDER BY detected_at, meeting_id LIMIT 1",
        )
        .bind(&sha256)
        .bind(meeting_id)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(replacement_id) = replacement {
            sqlx::query(
                "UPDATE audio_identities SET canonical_meeting_id = ?, denoise_applied = ( \
                     SELECT denoise_applied FROM meeting_audio_identities WHERE meeting_id = ? \
                 ) WHERE sha256 = ?",
            )
            .bind(&replacement_id)
            .bind(&replacement_id)
            .bind(&sha256)
            .execute(&mut **tx)
            .await?;
            sqlx::query(
                "UPDATE meeting_audio_identities SET role='canonical' WHERE meeting_id = ?",
            )
            .bind(&replacement_id)
            .execute(&mut **tx)
            .await?;
            sqlx::query("DELETE FROM audio_duplicate_reviews WHERE duplicate_meeting_id = ?")
                .bind(&replacement_id)
                .execute(&mut **tx)
                .await?;
            sqlx::query(
                "UPDATE audio_duplicate_reviews SET canonical_meeting_id = ? \
                 WHERE sha256 = ? AND duplicate_meeting_id <> ?",
            )
            .bind(&replacement_id)
            .bind(&sha256)
            .bind(&replacement_id)
            .execute(&mut **tx)
            .await?;
        } else {
            sqlx::query("DELETE FROM audio_identities WHERE sha256 = ?")
                .bind(&sha256)
                .execute(&mut **tx)
                .await?;
        }
    }

    sqlx::query("DELETE FROM audio_duplicate_reviews WHERE duplicate_meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM meeting_audio_identities WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn enqueue_missing_backfill(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let meeting_ids: Vec<String> = sqlx::query_scalar(
        "SELECT m.id FROM meetings m \
         WHERE m.folder_path IS NOT NULL AND length(trim(m.folder_path)) > 0 \
           AND NOT EXISTS ( \
             SELECT 1 FROM meeting_audio_identities mai WHERE mai.meeting_id = m.id \
           ) \
         ORDER BY m.created_at, m.id",
    )
    .fetch_all(pool)
    .await?;

    let mut created = 0usize;
    for meeting_id in meeting_ids {
        if crate::jobs::store::enqueue_unique(
            pool,
            crate::jobs::kind::AUDIO_IDENTITY_BACKFILL,
            Some(&meeting_id),
            &serde_json::json!({ "version": 1 }),
        )
        .await?
        .created
        {
            created += 1;
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "fe432cb211dac676fcd7d2f05033f82be9fd8325923e6e3a322758fee60e94cf";

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE meetings(
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                folder_path TEXT
            )",
            "CREATE TABLE audio_identities(
                sha256 TEXT PRIMARY KEY,
                canonical_meeting_id TEXT NOT NULL UNIQUE,
                byte_size INTEGER NOT NULL,
                duration_ms INTEGER,
                verified_at TEXT DEFAULT CURRENT_TIMESTAMP,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                denoise_applied INTEGER
            )",
            "CREATE TABLE meeting_audio_identities(
                meeting_id TEXT PRIMARY KEY,
                sha256 TEXT NOT NULL,
                role TEXT NOT NULL,
                denoise_applied INTEGER,
                detected_at TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(sha256, denoise_applied)
            )",
            "CREATE TABLE audio_duplicate_reviews(
                duplicate_meeting_id TEXT PRIMARY KEY,
                canonical_meeting_id TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                status TEXT NOT NULL,
                detected_at TEXT DEFAULT CURRENT_TIMESTAMP,
                resolved_at TEXT
            )",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO meetings(id,title,created_at,folder_path) VALUES
             ('m1','First','2026-01-01','/tmp/first'),
             ('m2','Second','2026-01-02','/tmp/second')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn duplicate_registration_preserves_both_meetings_for_review() {
        let pool = pool().await;
        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            register_import_identity(&mut tx, "m1", HASH, 7, Some(1_000), Some(true))
                .await
                .unwrap(),
            IdentityRegistration::Canonical
        );
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            register_import_identity(&mut tx, "m2", HASH, 7, Some(1_000), Some(false))
                .await
                .unwrap(),
            IdentityRegistration::DuplicateCandidate {
                canonical_meeting_id: "m1".to_string()
            }
        );
        tx.commit().await.unwrap();

        let existing = find_canonical_meeting(&pool, HASH).await.unwrap().unwrap();
        assert_eq!(existing.meeting_id, "m1");
        let denoised = find_processing_variant(&pool, HASH, true)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(denoised.meeting_id, "m1");
        let raw = find_processing_variant(&pool, HASH, false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw.meeting_id, "m2");
        let review: (String, String) = sqlx::query_as(
            "SELECT canonical_meeting_id, status FROM audio_duplicate_reviews \
             WHERE duplicate_meeting_id='m2'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(review, ("m1".to_string(), "pending".to_string()));
    }

    #[tokio::test]
    async fn deleting_canonical_promotes_oldest_duplicate() {
        let pool = pool().await;
        for (meeting_id, denoise_applied) in [("m1", false), ("m2", true)] {
            let mut tx = pool.begin().await.unwrap();
            register_import_identity(
                &mut tx,
                meeting_id,
                HASH,
                7,
                None,
                Some(denoise_applied),
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }

        let mut tx = pool.begin().await.unwrap();
        release_meeting_identity(&mut tx, "m1").await.unwrap();
        sqlx::query("DELETE FROM meetings WHERE id='m1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let existing = find_canonical_meeting(&pool, HASH).await.unwrap().unwrap();
        assert_eq!(existing.meeting_id, "m2");
        assert_eq!(existing.denoise_applied, Some(true));
        let role: String =
            sqlx::query_scalar("SELECT role FROM meeting_audio_identities WHERE meeting_id='m2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(role, "canonical");
    }

    #[tokio::test]
    async fn backfill_prefers_the_meeting_with_more_user_content() {
        let pool = pool().await;
        sqlx::query(
            "CREATE TABLE transcripts(
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                transcript TEXT,
                summary TEXT,
                action_items TEXT,
                key_points TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts(id, meeting_id, transcript, summary) \
             VALUES ('t1', 'm2', 'a much more complete transcript', 'edited summary')",
        )
        .execute(&pool)
        .await
        .unwrap();

        for meeting_id in ["m1", "m2"] {
            let mut tx = pool.begin().await.unwrap();
            register_backfilled_identity(&mut tx, meeting_id, HASH, 7, None)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }

        let existing = find_canonical_meeting(&pool, HASH).await.unwrap().unwrap();
        assert_eq!(existing.meeting_id, "m2");
        let pending_duplicate: String = sqlx::query_scalar(
            "SELECT duplicate_meeting_id FROM audio_duplicate_reviews WHERE sha256 = ?",
        )
        .bind(HASH)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(pending_duplicate, "m1");
    }
}
