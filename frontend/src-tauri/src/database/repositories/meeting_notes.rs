use crate::api::{SearchMatchSource, TranscriptSearchResult};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::search_snippet::match_context;
use anyhow::{Context, Result};
use chrono::Utc;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::Path;

const NOTES_MARKDOWN_FILE: &str = "notes.md";

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

/// Writes the saved note beside the recording. Replaces any existing file.
pub(crate) fn write_notes_markdown(folder: &Path, markdown: &str) -> Result<()> {
    let notes_path = folder.join(NOTES_MARKDOWN_FILE);
    let temp_path = folder.join(format!(".notes.md.{}.tmp", uuid::Uuid::new_v4()));
    let mut contents = markdown.to_string();
    if !contents.ends_with('\n') {
        contents.push('\n');
    }

    std::fs::write(&temp_path, contents)
        .with_context(|| format!("Failed to write {}", temp_path.display()))?;
    std::fs::rename(&temp_path, &notes_path).with_context(|| {
        format!(
            "Failed to replace {} with {}",
            notes_path.display(),
            temp_path.display()
        )
    })?;

    Ok(())
}

/// Removes notes.md when the saved note is blank. A missing file is already clear.
fn remove_notes_markdown(folder: &Path) -> Result<()> {
    let notes_path = folder.join(NOTES_MARKDOWN_FILE);
    match std::fs::remove_file(&notes_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to remove {}", notes_path.display()))
        }
    }
}

fn sync_notes_markdown(folder: &Path, markdown: &str) -> Result<()> {
    if markdown.trim().is_empty() {
        remove_notes_markdown(folder)
    } else {
        write_notes_markdown(folder, markdown)
    }
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

    /// Writes notes.md into the recording folder. A missing folder or a file error
    /// is logged and does not change the database save that already succeeded.
    /// Blank markdown removes the file, matching the deleted database row.
    pub async fn write_saved_notes_markdown(pool: &SqlitePool, meeting_id: &str, markdown: &str) {
        let meeting = match MeetingsRepository::get_meeting_metadata(pool, meeting_id).await {
            Ok(Some(meeting)) => meeting,
            Ok(None) => {
                warn!("Meeting not found while writing notes.md: {}", meeting_id);
                return;
            }
            Err(e) => {
                warn!(
                    "Failed to read meeting folder for notes.md (meeting_id={}): {}",
                    meeting_id, e
                );
                return;
            }
        };

        let Some(folder_path) = meeting.folder_path.filter(|path| !path.trim().is_empty()) else {
            info!("No recording folder for meeting {}; skipped notes.md", meeting_id);
            return;
        };

        if let Err(e) = sync_notes_markdown(Path::new(&folder_path), markdown) {
            warn!("Failed to update notes.md for meeting_id={}: {}", meeting_id, e);
        }
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

    #[test]
    fn notes_markdown_is_written_with_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        write_notes_markdown(dir.path(), "Ship Friday").unwrap();

        let raw = std::fs::read_to_string(dir.path().join("notes.md")).unwrap();
        assert_eq!(raw, "Ship Friday\n");
        assert!(dir.path().read_dir().unwrap().all(|entry| {
            !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp")
        }));
    }

    #[test]
    fn a_later_save_replaces_notes_markdown() {
        let dir = tempfile::tempdir().unwrap();
        write_notes_markdown(dir.path(), "first").unwrap();
        write_notes_markdown(dir.path(), "second").unwrap();

        let raw = std::fs::read_to_string(dir.path().join("notes.md")).unwrap();
        assert_eq!(raw, "second\n");
        assert!(dir.path().read_dir().unwrap().all(|entry| {
            !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp")
        }));
    }

    #[test]
    fn blank_markdown_removes_notes_markdown() {
        let dir = tempfile::tempdir().unwrap();
        write_notes_markdown(dir.path(), "Ship Friday").unwrap();
        sync_notes_markdown(dir.path(), "  \n ").unwrap();
        assert!(!dir.path().join("notes.md").exists());

        sync_notes_markdown(dir.path(), "   ").unwrap();
        assert!(!dir.path().join("notes.md").exists());
    }
}
