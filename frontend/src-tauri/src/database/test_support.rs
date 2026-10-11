//! In-memory SQLite helpers for repository tests.

use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// A fresh in-memory database with every migration applied. One connection
/// with no idle timeout, because each in-memory connection is its own database.
pub async fn test_pool() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

pub async fn insert_meeting(pool: &SqlitePool, id: &str, title: &str, folder_path: Option<&str>) {
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(title)
    .bind(now)
    .bind(now)
    .bind(folder_path)
    .execute(pool)
    .await
    .unwrap();
}

pub async fn insert_transcript(pool: &SqlitePool, meeting_id: &str, text: &str) {
    sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES (?, ?, ?, ?)")
        .bind(format!("transcript-{}", uuid::Uuid::new_v4()))
        .bind(meeting_id)
        .bind(text)
        .bind("00:00:01")
        .execute(pool)
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migration_creates_organization_schema() {
        let pool = test_pool().await;
        insert_meeting(&pool, "meeting-1", "Standup", None).await;

        let (folder_id, deleted_at): (Option<String>, Option<String>) =
            sqlx::query_as("SELECT folder_id, deleted_at FROM meetings WHERE id = 'meeting-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(folder_id, None);
        assert_eq!(deleted_at, None);

        for table in ["folders", "tags", "meeting_tags"] {
            let count: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {}", table))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count.0, 0, "{} should start empty", table);
        }
    }

    #[tokio::test]
    async fn live_folder_names_are_unique_case_insensitively() {
        let pool = test_pool().await;
        let insert = |id: &'static str, name: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO folders (id, name, created_at, updated_at) VALUES (?, ?, 't', 't')",
                )
                .bind(id)
                .bind(name)
                .execute(&pool)
                .await
            }
        };

        insert("folder-a", "Work").await.unwrap();
        assert!(insert("folder-b", "work").await.is_err());

        sqlx::query("UPDATE folders SET deleted_at = 't' WHERE id = 'folder-a'")
            .execute(&pool)
            .await
            .unwrap();
        insert("folder-b", "work").await.unwrap();
    }
}
