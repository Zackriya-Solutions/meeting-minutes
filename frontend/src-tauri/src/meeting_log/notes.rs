//! Quick Note: per-day checklist cards persisted in SQLite, with an on-launch
//! rollover that archives finished days to `NOTE_LOG_ROOT/<date>.md` and carries
//! pending (❌) cards forward to today.

use super::config::config;
use crate::state::AppState;
use chrono::Local;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::fs;
use std::io::Write;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct QuickNote {
    pub id: i64,
    pub date: String,
    pub text: String,
    pub done: bool,
    pub created_at: String,
    pub carried_from: Option<String>,
}

fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn now_iso() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn fetch_for_date(pool: &SqlitePool, date: &str) -> Result<Vec<QuickNote>, String> {
    sqlx::query_as::<_, QuickNote>(
        "SELECT id, date, text, done, created_at, carried_from
         FROM quick_notes WHERE date = ? ORDER BY id ASC",
    )
    .bind(date)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Cards for today (call after rollover).
#[tauri::command]
pub async fn quick_notes_today(state: tauri::State<'_, AppState>) -> Result<Vec<QuickNote>, String> {
    fetch_for_date(state.db_manager.pool(), &today()).await
}

#[tauri::command]
pub async fn quick_note_add(
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<QuickNote, String> {
    let pool = state.db_manager.pool();
    let id = sqlx::query(
        "INSERT INTO quick_notes (date, text, done, created_at) VALUES (?, ?, 0, ?)",
    )
    .bind(today())
    .bind(text.trim())
    .bind(now_iso())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .last_insert_rowid();

    sqlx::query_as::<_, QuickNote>(
        "SELECT id, date, text, done, created_at, carried_from FROM quick_notes WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn quick_note_toggle(
    state: tauri::State<'_, AppState>,
    id: i64,
    done: bool,
) -> Result<(), String> {
    sqlx::query("UPDATE quick_notes SET done = ? WHERE id = ?")
        .bind(done)
        .bind(id)
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn quick_note_update_text(
    state: tauri::State<'_, AppState>,
    id: i64,
    text: String,
) -> Result<(), String> {
    sqlx::query("UPDATE quick_notes SET text = ? WHERE id = ?")
        .bind(text.trim())
        .bind(id)
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn quick_note_delete(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    sqlx::query("DELETE FROM quick_notes WHERE id = ?")
        .bind(id)
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct RolloverResult {
    pub archived_days: Vec<String>,
    pub carried: i64,
}

/// On-launch rollover: for each un-archived day before today, write an archive
/// markdown file, carry pending cards forward to today, and mark the day
/// archived. Idempotent (archived guard). Never panics.
#[tauri::command]
pub async fn quick_notes_rollover(
    state: tauri::State<'_, AppState>,
) -> Result<RolloverResult, String> {
    let cfg = config();
    if cfg.note_rollover.trim().to_lowercase() != "on_launch" {
        return Ok(RolloverResult { archived_days: vec![], carried: 0 });
    }
    let pool = state.db_manager.pool();
    let today = today();

    // Distinct past days with un-archived cards.
    let days: Vec<String> = sqlx::query(
        "SELECT DISTINCT date FROM quick_notes WHERE archived = 0 AND date < ? ORDER BY date ASC",
    )
    .bind(&today)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|r| r.get::<String, _>("date"))
    .collect();

    let mut archived_days = Vec::new();
    let mut carried = 0i64;

    for day in days {
        let cards = fetch_for_date(pool, &day).await?;
        if cards.is_empty() {
            continue;
        }

        // (a) Archive the whole day to NOTE_LOG_ROOT/<date>.md
        fs::create_dir_all(&cfg.note_log_root)
            .map_err(|e| format!("create note-log dir: {e}"))?;
        let path = cfg.note_log_root.join(format!("{day}.md"));
        let mut body = format!("# Quick Note — {day}\n\n");
        for c in &cards {
            if c.done {
                body.push_str(&format!("[x] {} ✅\n", c.text.trim()));
            } else {
                body.push_str(&format!("[ ] {} ❌\n", c.text.trim()));
            }
        }
        fs::write(&path, body).map_err(|e| format!("write {:?}: {e}", path))?;

        // (b) Carry pending (❌) cards forward to today.
        for c in cards.iter().filter(|c| !c.done) {
            sqlx::query(
                "INSERT INTO quick_notes (date, text, done, created_at, carried_from)
                 VALUES (?, ?, 0, ?, ?)",
            )
            .bind(&today)
            .bind(c.text.trim())
            .bind(now_iso())
            .bind(&day)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            carried += 1;
        }

        // (c) Mark the old day archived.
        sqlx::query("UPDATE quick_notes SET archived = 1 WHERE date = ?")
            .bind(&day)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        archived_days.push(day);
    }

    log::info!(
        "📝 quick-note rollover: archived {} day(s), carried {} card(s)",
        archived_days.len(),
        carried
    );
    Ok(RolloverResult { archived_days, carried })
}
