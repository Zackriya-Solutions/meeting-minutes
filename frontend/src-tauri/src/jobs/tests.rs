//! Phase 0 job-queue tests. Exercise the store + single-job execution logic
//! deterministically (no infinite runner loop) against an in-memory SQLite DB.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use super::runner::{run_one_for_test, RunnerConfig};
use super::{
    enqueue_missing_diarization, enqueue_missing_transcript_refinement, enqueue_speaker_naming,
    kind, store, JobContext, JobHandler,
};

/// In-memory pool pinned to a single connection so all queries share one DB.
async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory db");
    // Just the jobs table (mirrors migration 20260706000000).
    sqlx::query(
        "CREATE TABLE jobs (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            meeting_id TEXT,
            payload TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL DEFAULT 'queued',
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            run_after TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn ctx(pool: &SqlitePool) -> JobContext {
    JobContext {
        pool: pool.clone(),
        notify: Arc::new(tokio::sync::Notify::new()),
    }
}

async fn status_of(pool: &SqlitePool, id: i64) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM jobs WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Handler that always fails.
struct AlwaysFail;
#[async_trait]
impl JobHandler for AlwaysFail {
    fn kind(&self) -> &'static str {
        "always_fail"
    }
    async fn run(
        &self,
        _: &JobContext,
        _: Option<&str>,
        _: &serde_json::Value,
    ) -> anyhow::Result<()> {
        anyhow::bail!("boom")
    }
}

/// Handler that succeeds after N failures (counts invocations).
struct FailThenSucceed {
    fail_times: usize,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl JobHandler for FailThenSucceed {
    fn kind(&self) -> &'static str {
        "flaky"
    }
    async fn run(
        &self,
        _: &JobContext,
        _: Option<&str>,
        _: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if n < self.fail_times {
            anyhow::bail!("transient failure #{n}");
        }
        Ok(())
    }
}

#[tokio::test]
async fn enqueue_and_claim_is_exclusive() {
    let pool = test_pool().await;
    let id = store::enqueue(&pool, "flaky", Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();

    let eligible = store::fetch_eligible(&pool, 10).await.unwrap();
    assert_eq!(eligible.len(), 1);

    assert!(
        store::try_claim(&pool, id).await.unwrap(),
        "first claim wins"
    );
    assert!(
        !store::try_claim(&pool, id).await.unwrap(),
        "second claim loses"
    );
    assert_eq!(status_of(&pool, id).await, "running");
}

#[tokio::test]
async fn search_index_jobs_are_prioritized_over_optional_audio_work() {
    let pool = test_pool().await;
    store::enqueue(&pool, kind::DIARIZE, Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();
    store::enqueue(&pool, kind::EXTRACT, Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();
    store::enqueue(&pool, kind::BACKFILL, None, &serde_json::json!({}))
        .await
        .unwrap();
    store::enqueue(
        &pool,
        kind::EMBEDDING_REPAIR,
        Some("m1"),
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    let eligible = store::fetch_eligible(&pool, 10).await.unwrap();
    let kinds: Vec<&str> = eligible.iter().map(|job| job.kind.as_str()).collect();
    assert_eq!(
        kinds,
        vec![
            kind::BACKFILL,
            kind::EMBEDDING_REPAIR,
            kind::EXTRACT,
            kind::DIARIZE,
        ]
    );
}

/// Naming runs after every other post-meeting job: it is the most optional step and the
/// only one that waits on a model to read the whole transcript. Queuing it twice for one
/// meeting (a re-run of "Detect speakers") must not name the same voices twice.
#[tokio::test]
async fn speaker_naming_is_queued_once_per_meeting_and_runs_last() {
    let pool = test_pool().await;
    store::enqueue(&pool, kind::DIARIZE, Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();
    let first = enqueue_speaker_naming(&pool, "m1").await.unwrap();
    let second = enqueue_speaker_naming(&pool, "m1").await.unwrap();
    assert_eq!(first, second, "an active naming job is reused");

    let kinds: Vec<String> = store::fetch_eligible(&pool, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|job| job.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![kind::DIARIZE.to_string(), kind::NAME_SPEAKERS.to_string()]
    );
}

#[tokio::test]
async fn unique_enqueue_reuses_active_job_and_allows_later_retry() {
    let pool = test_pool().await;
    let first = store::enqueue_unique(&pool, kind::CHUNK_EMBED, Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();
    let duplicate = store::enqueue_unique(
        &pool,
        kind::CHUNK_EMBED,
        Some("m1"),
        &serde_json::json!({ "duplicate": true }),
    )
    .await
    .unwrap();
    assert!(first.created);
    assert!(!duplicate.created);
    assert_eq!(duplicate.id, first.id);

    assert!(store::try_claim(&pool, first.id).await.unwrap());
    let while_running =
        store::enqueue_unique(&pool, kind::CHUNK_EMBED, Some("m1"), &serde_json::json!({}))
            .await
            .unwrap();
    assert_eq!(while_running.id, first.id);
    assert!(!while_running.created);

    store::mark_done(&pool, first.id).await.unwrap();
    let retry = store::enqueue_unique(&pool, kind::CHUNK_EMBED, Some("m1"), &serde_json::json!({}))
        .await
        .unwrap();
    assert!(retry.created);
    assert_ne!(retry.id, first.id);
}

#[tokio::test]
async fn retries_then_fails_after_max_attempts() {
    let pool = test_pool().await;
    let cfg = RunnerConfig {
        base_backoff_seconds: 0, // no delay so requeued jobs stay eligible
        max_attempts: 3,
        ..RunnerConfig::default()
    };
    let handler = AlwaysFail;
    let ctx = ctx(&pool);
    let id = store::enqueue(&pool, "always_fail", None, &serde_json::json!({}))
        .await
        .unwrap();

    for attempt in 1..=3 {
        let row = {
            let mut e = store::fetch_eligible(&pool, 1).await.unwrap();
            assert!(
                !e.is_empty(),
                "job should be eligible before attempt {attempt}"
            );
            e.remove(0)
        };
        assert!(store::try_claim(&pool, row.id).await.unwrap());
        run_one_for_test(&pool, &handler, &ctx, &cfg, row).await;
    }

    assert_eq!(
        status_of(&pool, id).await,
        "failed",
        "gives up after 3 attempts"
    );
    let (attempts, err): (i64, Option<String>) =
        sqlx::query_as("SELECT attempts, last_error FROM jobs WHERE id=?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 3);
    assert!(err.unwrap().contains("boom"));
}

#[tokio::test]
async fn flaky_job_eventually_succeeds() {
    let pool = test_pool().await;
    let cfg = RunnerConfig {
        base_backoff_seconds: 0,
        max_attempts: 3,
        ..RunnerConfig::default()
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let handler = FailThenSucceed {
        fail_times: 2,
        calls: calls.clone(),
    };
    let ctx = ctx(&pool);
    let id = store::enqueue(&pool, "flaky", None, &serde_json::json!({}))
        .await
        .unwrap();

    let mut final_status = String::new();
    for _ in 0..5 {
        let mut e = store::fetch_eligible(&pool, 1).await.unwrap();
        if e.is_empty() {
            final_status = status_of(&pool, id).await;
            break;
        }
        let row = e.remove(0);
        assert!(store::try_claim(&pool, row.id).await.unwrap());
        run_one_for_test(&pool, &handler, &ctx, &cfg, row).await;
        final_status = status_of(&pool, id).await;
        if final_status == "done" {
            break;
        }
    }
    assert_eq!(final_status, "done");
    assert_eq!(calls.load(Ordering::SeqCst), 3, "2 failures + 1 success");
}

#[tokio::test]
async fn recover_running_requeues_interrupted_jobs() {
    let pool = test_pool().await;
    let id = store::enqueue(&pool, "flaky", None, &serde_json::json!({}))
        .await
        .unwrap();
    store::try_claim(&pool, id).await.unwrap(); // -> running (simulates in-flight)
    assert_eq!(status_of(&pool, id).await, "running");

    // Simulate app restart.
    let recovered = store::recover_running(&pool).await.unwrap();
    assert_eq!(recovered, 1);
    assert_eq!(
        status_of(&pool, id).await,
        "queued",
        "interrupted job is retryable"
    );
}

#[tokio::test]
async fn startup_cleanup_retires_only_legacy_archive_fanout() {
    let pool = test_pool().await;
    sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY, created_at TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    for (id, created_at) in [
        ("old-legitimate", "2026-07-20 10:00:00"),
        ("old-archive", "2026-07-20 10:00:00"),
        ("new-meeting", "2026-07-24 11:00:00"),
    ] {
        sqlx::query("INSERT INTO meetings(id, created_at) VALUES(?, ?)")
            .bind(id)
            .bind(created_at)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (id, kind_name, meeting_id, payload, created_at) in [
        (
            10_i64,
            kind::CHUNK_EMBED,
            Some("old-legitimate"),
            "{}",
            "2026-07-24 09:59:00",
        ),
        (
            20,
            kind::BACKFILL,
            None,
            r#"{"reason":"startup"}"#,
            "2026-07-24 10:00:00",
        ),
        (
            21,
            kind::CHUNK_EMBED,
            Some("old-archive"),
            "{}",
            "2026-07-24 10:00:01",
        ),
        (
            22,
            kind::EXTRACT,
            Some("old-archive"),
            "{}",
            "2026-07-24 10:00:02",
        ),
        (
            23,
            kind::CHUNK_EMBED,
            Some("new-meeting"),
            "{}",
            "2026-07-24 11:01:00",
        ),
    ] {
        sqlx::query(
            "INSERT INTO jobs(id, kind, meeting_id, payload, status, created_at, updated_at) \
             VALUES(?, ?, ?, ?, 'queued', ?, ?)",
        )
        .bind(id)
        .bind(kind_name)
        .bind(meeting_id)
        .bind(payload)
        .bind(created_at)
        .bind(created_at)
        .execute(&pool)
        .await
        .unwrap();
    }

    let retired = store::retire_legacy_startup_backfill_fanout(&pool)
        .await
        .unwrap();
    assert_eq!(retired, 3, "startup root plus its two archive children");
    assert_eq!(status_of(&pool, 10).await, "queued");
    assert_eq!(status_of(&pool, 20).await, "done");
    assert_eq!(status_of(&pool, 21).await, "done");
    assert_eq!(status_of(&pool, 22).await, "done");
    assert_eq!(status_of(&pool, 23).await, "queued");
}

#[tokio::test]
async fn chunk_embed_creates_chunks_and_chains_diarize_and_extract() {
    let pool = test_pool().await;
    // Tables the chunk_embed handler touches.
    sqlx::query(
        "CREATE TABLE meetings (id TEXT PRIMARY KEY, indexing_allowed INTEGER NOT NULL DEFAULT 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO meetings(id) VALUES('m1')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, audio_start_time REAL, audio_end_time REAL)")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TABLE chunks (id INTEGER PRIMARY KEY, meeting_id TEXT, first_segment_id TEXT, last_segment_id TEXT, start_ms INTEGER, end_ms INTEGER, text TEXT, token_count INTEGER, embedding_status TEXT DEFAULT 'pending')")
        .execute(&pool).await.unwrap();
    sqlx::query(
        "CREATE VIRTUAL TABLE chunks_fts USING fts5(text, content='chunks', content_rowid='id')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER chunks_fts_ai AFTER INSERT ON chunks BEGIN INSERT INTO chunks_fts(rowid,text) VALUES(new.id,new.text); END")
        .execute(&pool).await.unwrap();
    for (i, txt) in ["первый сегмент про бюджет", "второй сегмент про проект"]
        .iter()
        .enumerate()
    {
        sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,audio_start_time,audio_end_time) VALUES(?,?,?,?,?)")
            .bind(format!("t{i}")).bind("m1").bind(*txt).bind(i as f64).bind((i + 1) as f64)
            .execute(&pool).await.unwrap();
    }

    let ctx = ctx(&pool);
    let handler = super::handlers::ChunkEmbedHandler;
    handler
        .run(
            &ctx,
            Some("m1"),
            &serde_json::json!({ "run_analysis": true }),
        )
        .await
        .unwrap();

    // Chunks created and FTS-indexed.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE meeting_id='m1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(n >= 1, "expected at least one chunk");
    let fts_hits: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'бюджет'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fts_hits, 1, "chunk indexed for FTS");

    // Idempotent: re-running does not duplicate chunks.
    handler
        .run(
            &ctx,
            Some("m1"),
            &serde_json::json!({ "run_analysis": true }),
        )
        .await
        .unwrap();
    let n2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE meeting_id='m1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, n2, "re-run is idempotent");

    // Chain enqueued.
    let kinds: Vec<String> = sqlx::query_scalar("SELECT DISTINCT kind FROM jobs ORDER BY kind")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(kinds.contains(&kind::DIARIZE.to_string()));
    assert!(kinds.contains(&kind::EXTRACT.to_string()));

    // Untagged jobs from a previous build may also be legitimate post-meeting work.
    // The handler keeps its historical behavior; startup cleanup identifies archive
    // fan-out by queue ordering instead of treating every `{}` payload as stale.
    sqlx::query("INSERT INTO meetings(id) VALUES('m2')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO transcripts(id,meeting_id,transcript,audio_start_time,audio_end_time) \
         VALUES('t-m2','m2','архивный импорт',0,1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    handler
        .run(&ctx, Some("m2"), &serde_json::json!({}))
        .await
        .unwrap();
    let analysis_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE meeting_id='m2' AND kind IN ('diarize','extract')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        analysis_jobs, 2,
        "legacy post-meeting analysis is preserved"
    );
    let archived_chunks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE meeting_id='m2'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        archived_chunks > 0,
        "legacy post-meeting indexing is preserved"
    );
}

#[tokio::test]
async fn private_memory_skips_indexing_but_still_chains_analysis() {
    let pool = test_pool().await;
    sqlx::query(
        "CREATE TABLE meetings (id TEXT PRIMARY KEY, indexing_allowed INTEGER NOT NULL DEFAULT 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO meetings(id, indexing_allowed) VALUES('m1', 0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE chunks (id INTEGER PRIMARY KEY, meeting_id TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO chunks(id, meeting_id) VALUES(1, 'm1')")
        .execute(&pool)
        .await
        .unwrap();

    let ctx = ctx(&pool);
    let handler = super::handlers::ChunkEmbedHandler;
    handler
        .run(
            &ctx,
            Some("m1"),
            &serde_json::json!({ "run_analysis": true }),
        )
        .await
        .unwrap();

    let chunk_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE meeting_id='m1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chunk_count, 0, "private memory must not remain indexed");

    let kinds: Vec<String> = sqlx::query_scalar("SELECT DISTINCT kind FROM jobs ORDER BY kind")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        kinds,
        vec![kind::DIARIZE.to_string(), kind::EXTRACT.to_string()]
    );
}

#[tokio::test]
async fn private_memory_blocks_cloud_extraction_handler() {
    let pool = test_pool().await;
    sqlx::query(
        "CREATE TABLE meetings (id TEXT PRIMARY KEY, cloud_processing_allowed INTEGER NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO meetings(id, cloud_processing_allowed) VALUES('m1', 0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE transcripts (meeting_id TEXT, transcript TEXT, speaker TEXT, \
         audio_start_time REAL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO transcripts(meeting_id, transcript, audio_start_time) \
         VALUES('m1', 'private transcript', 0)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let handler = super::handlers::ExtractHandler;
    handler
        .run(
            &ctx(&pool),
            Some("m1"),
            &serde_json::json!({ "run_analysis": true }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn missing_diarization_backfill_queues_each_eligible_meeting_once() {
    let pool = test_pool().await;
    sqlx::query(
        "CREATE TABLE meetings (
            id TEXT PRIMARY KEY,
            folder_path TEXT,
            diarization_enabled INTEGER,
            occurred_at TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE transcripts (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL,
            speaker_id INTEGER
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO meetings(id, folder_path, diarization_enabled, created_at) VALUES
            ('eligible', '/recordings/eligible', NULL, datetime('now')),
            ('already-diarized', '/recordings/done', 1, datetime('now')),
            ('disabled', '/recordings/disabled', 0, datetime('now')),
            ('empty', '/recordings/empty', 1, datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO transcripts(id, meeting_id, speaker_id) VALUES
            ('t1', 'eligible', NULL),
            ('t2', 'already-diarized', 7),
            ('t3', 'disabled', NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(enqueue_missing_diarization(&pool).await.unwrap(), 1);
    let queued: (String, String) = sqlx::query_as(
        "SELECT meeting_id, json_extract(payload, '$.source')
         FROM jobs WHERE kind='diarize'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued.0, "eligible");
    assert_eq!(queued.1, "automatic_diarization_backfill_v1");

    sqlx::query("UPDATE jobs SET status='done'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(enqueue_missing_diarization(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn missing_transcript_repair_requires_completed_two_minute_recording() {
    let pool = test_pool().await;
    sqlx::query(
        "CREATE TABLE meetings (
            id TEXT PRIMARY KEY,
            folder_path TEXT,
            diarization_enabled INTEGER,
            occurred_at TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();

    let root = tempfile::tempdir().unwrap();
    let completed = root.path().join("completed");
    let too_short = root.path().join("too-short");
    let unfinished = root.path().join("unfinished");
    std::fs::create_dir_all(&completed).unwrap();
    std::fs::create_dir_all(&too_short).unwrap();
    std::fs::create_dir_all(&unfinished).unwrap();
    std::fs::write(
        completed.join("metadata.json"),
        r#"{"status":"completed","duration_seconds":120.5}"#,
    )
    .unwrap();
    std::fs::write(
        too_short.join("metadata.json"),
        r#"{"status":"completed","duration_seconds":119.9}"#,
    )
    .unwrap();
    std::fs::write(
        unfinished.join("metadata.json"),
        r#"{"status":"recording","duration_seconds":180.0}"#,
    )
    .unwrap();

    for (id, folder) in [
        ("completed", completed),
        ("too-short", too_short),
        ("unfinished", unfinished),
    ] {
        sqlx::query(
            "INSERT INTO meetings(id, folder_path, diarization_enabled, created_at)
             VALUES (?, ?, 1, datetime('now'))",
        )
        .bind(id)
        .bind(folder.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .unwrap();
    }

    assert_eq!(
        enqueue_missing_transcript_refinement(&pool).await.unwrap(),
        1
    );
    let queued: (String, String) = sqlx::query_as(
        "SELECT meeting_id, json_extract(payload, '$.source')
         FROM jobs WHERE kind='refine_missing_transcript'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued.0, "completed");
    assert_eq!(queued.1, "automatic_transcript_repair_v1");

    sqlx::query("UPDATE jobs SET status='done'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        enqueue_missing_transcript_refinement(&pool).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn backfill_skips_empty_meetings_and_deduplicates_active_work() {
    let pool = test_pool().await;
    sqlx::query("CREATE TABLE meetings (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE chunks (id INTEGER PRIMARY KEY, meeting_id TEXT, embedding_status TEXT)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE chunk_embeddings (chunk_id INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO meetings(id) VALUES('with-text'), ('empty')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO transcripts(id, meeting_id, transcript) \
         VALUES('t1', 'with-text', 'searchable words'), ('t2', 'empty', '   ')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO chunks(id, meeting_id, embedding_status) VALUES(1, 'indexed', 'done')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let handler = super::handlers::BackfillHandler;
    let context = ctx(&pool);
    handler
        .run(&context, None, &serde_json::json!({ "reason": "startup" }))
        .await
        .unwrap();
    let startup_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(startup_jobs, 0, "legacy startup backfill is a no-op");
    handler
        .run(&context, None, &serde_json::json!({}))
        .await
        .unwrap();
    handler
        .run(&context, None, &serde_json::json!({}))
        .await
        .unwrap();

    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT kind, meeting_id FROM jobs WHERE kind='chunk_embed' ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        vec![(kind::CHUNK_EMBED.to_string(), Some("with-text".into()))]
    );
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM jobs WHERE kind='chunk_embed' LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["run_analysis"], false);
    assert_eq!(payload["source"], "archive_backfill");
    let repaired_status: String =
        sqlx::query_scalar("SELECT embedding_status FROM chunks WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        repaired_status, "pending",
        "missing vector row is repairable"
    );
}
