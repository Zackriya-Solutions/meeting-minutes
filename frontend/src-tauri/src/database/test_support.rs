//! Shared fixtures for database tests.
use crate::api::TranscriptSegment;
use crate::database::repositories::speaker::{MeetingSpeaker, NewSpeaker, SpeakerWrite, SpeakersRepository};
use crate::database::repositories::transcript::TranscriptsRepository;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::collections::HashMap;

pub struct SeedRow {
    pub id: &'static str,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub speaker: Option<&'static str>,
    pub text: &'static str,
}

/// In-memory database with every real migration applied.
/// A single connection keeps the in-memory database alive for the pool's lifetime.
pub async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

pub async fn seed_meeting(pool: &SqlitePool, meeting_id: &str, rows: &[SeedRow]) {
    let now = chrono::Utc::now();
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, NULL)")
        .bind(meeting_id)
        .bind("Test meeting")
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert meeting");
    let mut conn = pool.acquire().await.expect("acquire connection");
    for row in rows {
        let segment = TranscriptSegment {
            id: row.id.to_string(),
            text: row.text.to_string(),
            timestamp: "2026-09-27T10:00:00Z".to_string(),
            audio_start_time: row.start,
            audio_end_time: row.end,
            duration: match (row.start, row.end) {
                (Some(s), Some(e)) => Some(e - s),
                _ => None,
            },
            speaker: row.speaker.map(str::to_string),
        };
        TranscriptsRepository::insert_row(&mut conn, row.id, meeting_id, &segment)
            .await
            .expect("insert transcript");
    }
}

/// Replaces the meeting's speakers.
pub async fn write_speakers(pool: &SqlitePool, meeting_id: &str, speakers: Vec<NewSpeaker>) {
    let mut conn = pool.acquire().await.expect("acquire connection");
    SpeakersRepository::replace_for_meeting(&mut conn, meeting_id, &SpeakerWrite { speakers, ..Default::default() })
        .await
        .expect("write speakers");
}

/// Creates the meeting without rows and writes its speakers.
pub async fn seed_speakers(pool: &SqlitePool, meeting_id: &str, speakers: Vec<NewSpeaker>) {
    seed_meeting(pool, meeting_id, &[]).await;
    write_speakers(pool, meeting_id, speakers).await;
}

/// The stored speaker with this key.
pub async fn stored_speaker(pool: &SqlitePool, meeting_id: &str, key: &str) -> MeetingSpeaker {
    SpeakersRepository::list(pool, meeting_id)
        .await
        .expect("list speakers")
        .into_iter()
        .find(|s| s.speaker_key == key)
        .expect("speaker exists")
}

/// The meeting's stored speakers by key.
pub async fn speakers_by_key(pool: &SqlitePool, meeting_id: &str) -> HashMap<String, MeetingSpeaker> {
    SpeakersRepository::list(pool, meeting_id)
        .await
        .expect("list speakers")
        .into_iter()
        .map(|s| (s.speaker_key.clone(), s))
        .collect()
}

pub async fn people_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM people").fetch_one(pool).await.expect("count people")
}

pub async fn seed_person(pool: &SqlitePool, id: &str, name: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO people (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(name)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert person");
}
