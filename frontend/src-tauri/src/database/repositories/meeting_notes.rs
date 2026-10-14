use crate::api::{SearchMatchSource, TranscriptSearchResult};
use crate::database::repositories::search_snippet::match_context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// A meeting's user-written note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingNotes {
    pub markdown: String,
    /// The editor document: a JSON array of BlockNote blocks.
    pub notes_json: Option<String>,
}

pub struct MeetingNotesRepository;

/// True when `json` is a JSON array whose items are all objects.
pub fn is_block_array(json: &str) -> bool {
    serde_json::from_str::<Vec<serde_json::Map<String, serde_json::Value>>>(json).is_ok()
}

impl MeetingNotesRepository {
    pub async fn get(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingNotes>, sqlx::Error> {
        let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT notes_markdown, notes_json FROM meeting_notes WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|(markdown, notes_json)| MeetingNotes {
            markdown: markdown.unwrap_or_default(),
            notes_json,
        }))
    }

    /// Saves the note for a meeting. A note with no text is removed.
    /// Returns `false` when the meeting does not exist, except that an empty note writes
    /// nothing and returns `true` even for a meeting that does not exist.
    pub async fn upsert(
        pool: &SqlitePool,
        meeting_id: &str,
        markdown: &str,
        notes_json: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        if markdown.trim().is_empty() {
            sqlx::query("DELETE FROM meeting_notes WHERE meeting_id = ?")
                .bind(meeting_id)
                .execute(pool)
                .await?;
            return Ok(true);
        }

        let now = Utc::now();
        let result = sqlx::query(
            "INSERT INTO meeting_notes (meeting_id, notes_markdown, notes_json, created_at, updated_at)
             SELECT ?, ?, ?, ?, ?
             WHERE EXISTS (SELECT 1 FROM meetings WHERE id = ?)
             ON CONFLICT(meeting_id) DO UPDATE SET
                 notes_markdown = excluded.notes_markdown,
                 notes_json = excluded.notes_json,
                 updated_at = excluded.updated_at",
        )
        .bind(meeting_id)
        .bind(markdown)
        .bind(notes_json)
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Finds notes containing `query`, ignoring case.
    pub async fn search(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, sqlx::Error> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let pattern = format!("%{}%", query.to_lowercase());
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, n.notes_markdown, n.updated_at
             FROM meeting_notes n
             JOIN meetings m ON m.id = n.meeting_id
             WHERE LOWER(n.notes_markdown) LIKE ?",
        )
        .bind(&pattern)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, title, markdown, updated_at)| TranscriptSearchResult {
                id,
                title,
                match_context: match_context(&markdown, query),
                timestamp: updated_at,
                source: SearchMatchSource::Notes,
            })
            .collect())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    /// An in-memory database with the app's real schema applied.
    pub(crate) async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    pub(crate) async fn seed_meeting(pool: &SqlitePool, id: &str, title: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind(Utc::now())
            .bind(Utc::now())
            .execute(pool)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::{seed_meeting, test_pool};
    use std::time::Duration;

    const BLOCKS: &str = r#"[{"id":"a","type":"paragraph","content":[{"type":"text","text":"Ship Friday"}]}]"#;

    #[tokio::test]
    async fn get_returns_none_without_a_note() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        assert!(MeetingNotesRepository::upsert(&pool, "m1", "Ship Friday", Some(BLOCKS))
            .await
            .unwrap());
        assert_eq!(
            MeetingNotesRepository::get(&pool, "m1").await.unwrap(),
            Some(MeetingNotes {
                markdown: "Ship Friday".into(),
                notes_json: Some(BLOCKS.into()),
            })
        );
    }

    #[tokio::test]
    async fn second_upsert_updates_content_and_keeps_created_at() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        MeetingNotesRepository::upsert(&pool, "m1", "first", Some(BLOCKS)).await.unwrap();
        let (created_before, updated_before): (String, String) =
            sqlx::query_as("SELECT created_at, updated_at FROM meeting_notes WHERE meeting_id = 'm1'")
                .fetch_one(&pool)
                .await
                .unwrap();

        tokio::time::sleep(Duration::from_millis(5)).await;
        MeetingNotesRepository::upsert(&pool, "m1", "second", Some(BLOCKS)).await.unwrap();

        let (created_after, updated_after, markdown): (String, String, String) = sqlx::query_as(
            "SELECT created_at, updated_at, notes_markdown FROM meeting_notes WHERE meeting_id = 'm1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(created_after, created_before);
        assert_ne!(updated_after, updated_before);
        assert_eq!(markdown, "second");
    }

    #[tokio::test]
    async fn empty_markdown_removes_the_row() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        MeetingNotesRepository::upsert(&pool, "m1", "Ship Friday", Some(BLOCKS)).await.unwrap();
        assert!(MeetingNotesRepository::upsert(&pool, "m1", "  \n ", None).await.unwrap());
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_notes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn upsert_for_a_missing_meeting_returns_false() {
        let pool = test_pool().await;
        assert!(!MeetingNotesRepository::upsert(&pool, "ghost", "text", Some(BLOCKS))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_its_note() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        MeetingNotesRepository::upsert(&pool, "m1", "Ship Friday", Some(BLOCKS)).await.unwrap();
        sqlx::query("DELETE FROM meetings WHERE id = 'm1'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(MeetingNotesRepository::get(&pool, "m1").await.unwrap(), None);
    }

    #[test]
    fn block_array_accepts_only_arrays_of_objects() {
        assert!(is_block_array(BLOCKS));
        assert!(is_block_array("[]"));
        assert!(!is_block_array(r#"{"blocks":[]}"#));
        assert!(!is_block_array(r#"["text"]"#));
        assert!(!is_block_array("not json"));
    }

    #[tokio::test]
    async fn search_finds_a_note_ignoring_case() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        seed_meeting(&pool, "m2", "Retro").await;
        MeetingNotesRepository::upsert(&pool, "m1", "Budget review moved to Friday", Some(BLOCKS))
            .await
            .unwrap();
        MeetingNotesRepository::upsert(&pool, "m2", "Nothing relevant", Some(BLOCKS))
            .await
            .unwrap();

        let results = MeetingNotesRepository::search(&pool, "BUDGET").await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
        assert_eq!(results[0].title, "Planning");
        assert_eq!(results[0].source, SearchMatchSource::Notes);
        assert!(results[0].match_context.contains("Budget review"));
    }

    #[tokio::test]
    async fn search_with_a_blank_query_returns_nothing() {
        let pool = test_pool().await;
        seed_meeting(&pool, "m1", "Planning").await;
        MeetingNotesRepository::upsert(&pool, "m1", "Budget review", Some(BLOCKS))
            .await
            .unwrap();
        assert!(MeetingNotesRepository::search(&pool, "   ").await.unwrap().is_empty());
    }
}
