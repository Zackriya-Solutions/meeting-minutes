//! Local user-authored preparation and scratchpad notes for a standup.
//!
//! These notes intentionally never enter transcript evidence, generated standup records, or a
//! series digest. They are private context controlled by the user.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

const MAX_NOTE_CHARS: usize = 4_000;
const ALLOWED_KINDS: [&str; 3] = ["planned_update", "parking_lot", "private_note"];
const ALLOWED_STATUSES: [&str; 3] = ["open", "done", "archived"];

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StandupPrivateNote {
    pub id: i64,
    pub meeting_id: String,
    pub kind: String,
    pub text: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStandupPrivateNoteInput {
    pub meeting_id: String,
    pub kind: String,
    pub text: String,
}

fn normalize_note(kind: String, text: String) -> Result<(String, String), String> {
    let kind = kind.trim().to_lowercase();
    if !ALLOWED_KINDS.contains(&kind.as_str()) {
        return Err("Unsupported standup note kind".to_string());
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Standup note cannot be empty".to_string());
    }
    if text.chars().count() > MAX_NOTE_CHARS {
        return Err(format!(
            "Standup note cannot exceed {MAX_NOTE_CHARS} characters"
        ));
    }
    Ok((kind, text))
}

async fn list_notes(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<Vec<StandupPrivateNote>, sqlx::Error> {
    sqlx::query_as::<_, StandupPrivateNote>(
        "SELECT id, meeting_id, kind, text, status, created_at, updated_at \
         FROM standup_private_notes WHERE meeting_id = ? AND status != 'archived' \
         ORDER BY CASE status WHEN 'open' THEN 0 ELSE 1 END, \
                  CASE kind WHEN 'planned_update' THEN 0 WHEN 'parking_lot' THEN 1 ELSE 2 END, id",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
}

#[tauri::command]
pub async fn list_standup_private_notes(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<StandupPrivateNote>, String> {
    list_notes(state.db_manager.pool(), meeting_id.trim())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_standup_private_note(
    state: tauri::State<'_, AppState>,
    input: CreateStandupPrivateNoteInput,
) -> Result<StandupPrivateNote, String> {
    create_note(state.db_manager.pool(), input).await
}

async fn create_note(
    pool: &SqlitePool,
    input: CreateStandupPrivateNoteInput,
) -> Result<StandupPrivateNote, String> {
    let meeting_id = input.meeting_id.trim();
    if meeting_id.is_empty() {
        return Err("Meeting id is required".to_string());
    }
    let (kind, text) = normalize_note(input.kind, input.text)?;
    sqlx::query_as::<_, StandupPrivateNote>(
        "INSERT INTO standup_private_notes(meeting_id, kind, text) \
         SELECT ?, ?, ? WHERE EXISTS (SELECT 1 FROM meetings WHERE id = ?) \
         RETURNING id, meeting_id, kind, text, status, created_at, updated_at",
    )
    .bind(meeting_id)
    .bind(kind)
    .bind(text)
    .bind(meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "Meeting was not found".to_string())
}

#[tauri::command]
pub async fn set_standup_private_note_status(
    state: tauri::State<'_, AppState>,
    note_id: i64,
    status: String,
) -> Result<(), String> {
    let status = status.trim().to_lowercase();
    if !ALLOWED_STATUSES.contains(&status.as_str()) {
        return Err("Unsupported standup note status".to_string());
    }
    let result = sqlx::query(
        "UPDATE standup_private_notes SET status = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(status)
    .bind(note_id)
    .execute(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err("Standup note was not found".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings(id) VALUES('m1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE standup_private_notes(\
             id INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL,\
             status TEXT NOT NULL DEFAULT 'open', created_at TEXT NOT NULL DEFAULT (datetime('now')),\
             updated_at TEXT NOT NULL DEFAULT (datetime('now')))",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[test]
    fn note_content_is_bounded_and_kinds_are_allowlisted() {
        assert!(normalize_note("private_note".into(), "  my note  ".into()).is_ok());
        assert!(normalize_note("transcript".into(), "do not inject".into()).is_err());
        assert!(normalize_note("planned_update".into(), " ".into()).is_err());
        assert!(normalize_note("planned_update".into(), "x".repeat(4_001)).is_err());
    }

    #[tokio::test]
    async fn private_notes_remain_separate_and_archived_notes_are_hidden() {
        let pool = pool().await;
        for (kind, text, status) in [
            ("private_note", "only for me", "open"),
            ("planned_update", "share this", "done"),
            ("parking_lot", "old topic", "archived"),
        ] {
            sqlx::query(
                "INSERT INTO standup_private_notes(meeting_id, kind, text, status) VALUES('m1', ?, ?, ?)",
            )
            .bind(kind)
            .bind(text)
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        }
        let notes = list_notes(&pool, "m1").await.unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].text, "only for me");
        assert!(notes.iter().all(|note| note.text != "old topic"));
    }

    #[tokio::test]
    async fn private_note_creation_requires_an_existing_meeting() {
        let pool = pool().await;
        let input = |meeting_id: &str| CreateStandupPrivateNoteInput {
            meeting_id: meeting_id.to_string(),
            kind: "private_note".to_string(),
            text: "safe local note".to_string(),
        };

        assert!(create_note(&pool, input("m1")).await.is_ok());
        assert_eq!(
            create_note(&pool, input("missing")).await.unwrap_err(),
            "Meeting was not found"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM standup_private_notes")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
