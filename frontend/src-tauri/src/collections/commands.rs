//! Tauri commands for collections, saved searches, series suggestions, and backfill
//! (PLAN.md Phase 5). Thin DB wrappers over the schema in migrations 20260706000000/2.

use serde::{Deserialize, Serialize};

use crate::collections::{suggest_series, MeetingRef, MIN_SERIES_SIZE};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CollectionRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
}

#[tauri::command]
pub async fn create_collection(
    state: tauri::State<'_, AppState>,
    name: String,
    kind: Option<String>,
) -> Result<i64, String> {
    let pool = state.db_manager.pool();
    let kind = kind.unwrap_or_else(|| "manual".into());
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO collections(name, kind) VALUES(?, ?) RETURNING id",
    )
    .bind(name)
    .bind(kind)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_meeting_to_collection(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    collection_id: i64,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    sqlx::query(
        "INSERT OR IGNORE INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)",
    )
    .bind(meeting_id)
    .bind(collection_id)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_collections(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<CollectionRow>, String> {
    let pool = state.db_manager.pool();
    let rows: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, kind FROM collections ORDER BY name")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, name, kind)| CollectionRow { id, name, kind })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct SaveSearchInput {
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub filters: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn save_search(
    state: tauri::State<'_, AppState>,
    input: SaveSearchInput,
) -> Result<i64, String> {
    let pool = state.db_manager.pool();
    let filters = input.filters.unwrap_or_else(|| serde_json::json!({})).to_string();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO saved_searches(name, query, filters) VALUES(?, ?, ?) RETURNING id",
    )
    .bind(input.name)
    .bind(input.query)
    .bind(filters)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct SeriesSuggestionOut {
    pub suggested_name: String,
    pub meeting_ids: Vec<String>,
    pub cadence: String,
}

/// Propose series from the meeting archive (PLAN.md Phase 5 auto-series).
#[tauri::command]
pub async fn suggest_meeting_series(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SeriesSuggestionOut>, String> {
    let pool = state.db_manager.pool();
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, title, created_at FROM meetings")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

    let meetings: Vec<MeetingRef> = rows
        .into_iter()
        .filter_map(|(id, title, created_at)| {
            // created_at may be a full datetime; take the leading YYYY-MM-DD.
            let date = chrono::NaiveDate::parse_from_str(created_at.get(0..10).unwrap_or(""), "%Y-%m-%d").ok()?;
            Some(MeetingRef { id, title, date })
        })
        .collect();

    Ok(suggest_series(&meetings, MIN_SERIES_SIZE)
        .into_iter()
        .map(|s| SeriesSuggestionOut {
            suggested_name: s.suggested_name,
            meeting_ids: s.meeting_ids,
            cadence: format!("{:?}", s.cadence),
        })
        .collect())
}

/// Kick off a backfill over the whole archive (PLAN.md Phase 5). Enqueues one `backfill`
/// job; the handler fans out `chunk_embed` for un-chunked meetings. Non-blocking.
#[tauri::command]
pub async fn run_backfill(state: tauri::State<'_, AppState>) -> Result<i64, String> {
    let pool = state.db_manager.pool();
    crate::jobs::enqueue_raw(pool, crate::jobs::kind::BACKFILL, None, &serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())
}

/// Set a privacy/config value (PLAN.md Phase 5/§8). Enforced at the LLM provider layer
/// via `crate::llm::PrivacyConfig::load`.
#[tauri::command]
pub async fn set_app_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    sqlx::query(
        "INSERT INTO app_settings_kv(key, value, updated_at) VALUES(?, ?, datetime('now')) \
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=datetime('now')",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}
