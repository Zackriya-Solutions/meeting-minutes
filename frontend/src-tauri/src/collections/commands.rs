//! Tauri commands for collections, saved searches, series suggestions, and backfill
//! (PLAN.md Phase 5). Thin DB wrappers over the schema in migrations 20260706000000/2.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::collections::{
    auto_assign_meeting, derive_series_match_rule, normalize_title, suggest_series, MeetingRef,
    MIN_SERIES_SIZE,
};
use crate::database::repositories::setting::redact_secret;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CollectionRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub meeting_count: i64,
    pub auto_add: bool,
    pub match_rule: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CollectionMeetingRow {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub occurred_at: Option<String>,
    pub folder_path: Option<String>,
    pub in_collection: bool,
}

fn normalize_collection_name(name: String) -> Result<String, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Collection name cannot be empty".into());
    }
    if name.chars().count() > 120 {
        return Err("Collection name cannot be longer than 120 characters".into());
    }
    Ok(name)
}

fn normalize_collection_kind(kind: Option<String>) -> Result<String, String> {
    let kind = kind.unwrap_or_else(|| "manual".into());
    match kind.as_str() {
        "manual" | "series" => Ok(kind),
        _ => Err("Collection kind must be either 'manual' or 'series'".into()),
    }
}

#[tauri::command]
pub async fn create_collection(
    state: tauri::State<'_, AppState>,
    name: String,
    kind: Option<String>,
) -> Result<i64, String> {
    let pool = state.db_manager.pool();
    let name = normalize_collection_name(name)?;
    let kind = normalize_collection_kind(kind)?;
    sqlx::query_scalar::<_, i64>("INSERT INTO collections(name, kind) VALUES(?, ?) RETURNING id")
        .bind(name)
        .bind(kind)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_collection(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
    name: String,
) -> Result<(), String> {
    let name = normalize_collection_name(name)?;
    let result = sqlx::query("UPDATE collections SET name = ? WHERE id = ?")
        .bind(name)
        .bind(collection_id)
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    if result.rows_affected() == 0 {
        return Err("Collection not found".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_collection(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> Result<(), String> {
    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM meeting_collections WHERE collection_id = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM collection_auto_exclusions WHERE collection_id = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let result = sqlx::query("DELETE FROM collections WHERE id = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if result.rows_affected() == 0 {
        return Err("Collection not found".into());
    }
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_meeting_to_collection(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    collection_id: i64,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    sqlx::query("INSERT OR IGNORE INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)")
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
    let rows: Vec<(i64, String, String, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT c.id, c.name, c.kind, COUNT(mc.meeting_id), c.auto_add, c.match_rule \
         FROM collections c \
         LEFT JOIN meeting_collections mc ON mc.collection_id = c.id \
         GROUP BY c.id, c.name, c.kind, c.auto_add, c.match_rule \
         ORDER BY lower(c.name), c.id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(id, name, kind, meeting_count, auto_add, match_rule)| CollectionRow {
                id,
                name,
                kind,
                meeting_count,
                auto_add: auto_add != 0,
                match_rule,
            },
        )
        .collect())
}

#[tauri::command]
pub async fn list_collection_candidates(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> Result<Vec<CollectionMeetingRow>, String> {
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collections WHERE id = ?)")
        .bind(collection_id)
        .fetch_one(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())?;
    if exists == 0 {
        return Err("Collection not found".into());
    }

    let rows: Vec<(String, String, String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT m.id, m.title, m.created_at, m.occurred_at, m.folder_path, \
         EXISTS(SELECT 1 FROM meeting_collections mc \
                WHERE mc.meeting_id = m.id AND mc.collection_id = ?) \
         FROM meetings m ORDER BY m.created_at DESC, m.id",
    )
    .bind(collection_id)
    .fetch_all(state.db_manager.pool())
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(id, title, created_at, occurred_at, folder_path, in_collection)| {
                CollectionMeetingRow {
                    id,
                    title,
                    created_at,
                    occurred_at,
                    folder_path,
                    in_collection: in_collection != 0,
                }
            },
        )
        .collect())
}

#[tauri::command]
pub async fn set_collection_meetings(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
    meeting_ids: Vec<String>,
) -> Result<(), String> {
    let mut unique_ids = Vec::with_capacity(meeting_ids.len());
    let mut seen = HashSet::with_capacity(meeting_ids.len());
    for id in meeting_ids {
        if seen.insert(id.clone()) {
            unique_ids.push(id);
        }
    }

    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|e| e.to_string())?;
    let collection_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collections WHERE id = ?)")
            .bind(collection_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    if collection_exists == 0 {
        return Err("Collection not found".into());
    }
    let collection_kind: String = sqlx::query_scalar("SELECT kind FROM collections WHERE id = ?")
        .bind(collection_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let previous_ids: HashSet<String> =
        sqlx::query_scalar("SELECT meeting_id FROM meeting_collections WHERE collection_id = ?")
            .bind(collection_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();

    for meeting_id in &unique_ids {
        let meeting_exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meetings WHERE id = ?)")
                .bind(meeting_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        if meeting_exists == 0 {
            return Err(format!("Meeting not found: {meeting_id}"));
        }
    }

    sqlx::query("DELETE FROM meeting_collections WHERE collection_id = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for meeting_id in unique_ids {
        sqlx::query("INSERT INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)")
            .bind(&meeting_id)
            .bind(collection_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        if collection_kind == "series" {
            sqlx::query(
                "DELETE FROM collection_auto_exclusions
                 WHERE collection_id = ? AND meeting_id = ?",
            )
            .bind(collection_id)
            .bind(&meeting_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    if collection_kind == "series" {
        for removed_id in previous_ids.difference(&seen) {
            sqlx::query(
                "INSERT OR IGNORE INTO collection_auto_exclusions(collection_id, meeting_id)
                 VALUES(?, ?)",
            )
            .bind(collection_id)
            .bind(removed_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().await.map_err(|e| e.to_string())
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
    let filters = input
        .filters
        .unwrap_or_else(|| serde_json::json!({}))
        .to_string();
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

#[tauri::command]
pub async fn accept_series_suggestion(
    state: tauri::State<'_, AppState>,
    suggested_name: String,
    meeting_ids: Vec<String>,
) -> Result<i64, String> {
    let name = normalize_collection_name(suggested_name)?;
    let unique_ids: Vec<String> = meeting_ids
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if unique_ids.len() < MIN_SERIES_SIZE {
        return Err(format!(
            "A series requires at least {MIN_SERIES_SIZE} meetings"
        ));
    }

    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|e| e.to_string())?;
    let mut series_titles = Vec::with_capacity(unique_ids.len());
    for meeting_id in &unique_ids {
        let title: Option<String> = sqlx::query_scalar("SELECT title FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        let Some(title) = title else {
            return Err(format!("Meeting not found: {meeting_id}"));
        };
        series_titles.push(title);
    }

    let match_rule = derive_series_match_rule(series_titles.iter().map(String::as_str))
        .unwrap_or_else(|| normalize_title(&name));
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections(name, kind, auto_add, match_rule)
             VALUES(?, 'series', 1, ?) RETURNING id",
    )
    .bind(name)
    .bind(match_rule)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    for meeting_id in unique_ids {
        sqlx::query("INSERT INTO meeting_collections(meeting_id, collection_id) VALUES(?, ?)")
            .bind(meeting_id)
            .bind(collection_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(collection_id)
}

#[derive(Debug, Serialize)]
pub struct SeriesAutoAddResult {
    pub enabled: bool,
    pub match_rule: Option<String>,
    pub added_count: usize,
}

#[tauri::command]
pub async fn set_series_auto_add(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
    enabled: bool,
) -> Result<SeriesAutoAddResult, String> {
    let pool = state.db_manager.pool();
    let collection: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT kind, match_rule FROM collections WHERE id = ?")
            .bind(collection_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some((kind, stored_rule)) = collection else {
        return Err("Collection not found".into());
    };
    if kind != "series" {
        return Err("Auto-add is available only for recurring series".into());
    }

    let match_rule = if enabled && stored_rule.as_deref().unwrap_or("").trim().is_empty() {
        let titles: Vec<String> = sqlx::query_scalar(
            "SELECT m.title FROM meetings m
             JOIN meeting_collections mc ON mc.meeting_id = m.id
             WHERE mc.collection_id = ? ORDER BY m.created_at DESC",
        )
        .bind(collection_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        derive_series_match_rule(titles.iter().map(String::as_str))
            .ok_or("Could not derive a stable matching rule from this series")?
    } else {
        stored_rule.unwrap_or_default()
    };

    sqlx::query("UPDATE collections SET auto_add = ?, match_rule = ? WHERE id = ?")
        .bind(enabled as i64)
        .bind((!match_rule.is_empty()).then_some(match_rule.clone()))
        .bind(collection_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut added_count = 0;
    if enabled {
        let meetings: Vec<(String, String)> = sqlx::query_as("SELECT id, title FROM meetings")
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
        for (meeting_id, title) in meetings {
            let assigned = auto_assign_meeting(pool, &meeting_id, &title)
                .await
                .map_err(|e| e.to_string())?;
            if assigned.contains(&collection_id) {
                added_count += 1;
            }
        }
    }

    Ok(SeriesAutoAddResult {
        enabled,
        match_rule: (!match_rule.is_empty()).then_some(match_rule),
        added_count,
    })
}

#[tauri::command]
pub async fn convert_collection_to_series(
    state: tauri::State<'_, AppState>,
    collection_id: i64,
) -> Result<SeriesAutoAddResult, String> {
    let pool = state.db_manager.pool();
    let collection: Option<String> =
        sqlx::query_scalar("SELECT kind FROM collections WHERE id = ?")
            .bind(collection_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some(kind) = collection else {
        return Err("Collection not found".into());
    };
    if kind == "series" {
        return Err("Collection is already a recurring series".into());
    }

    let titles: Vec<String> = sqlx::query_scalar(
        "SELECT m.title FROM meetings m
         JOIN meeting_collections mc ON mc.meeting_id = m.id
         WHERE mc.collection_id = ? ORDER BY m.created_at DESC",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    if titles.len() < MIN_SERIES_SIZE {
        return Err(format!(
            "A recurring series requires at least {MIN_SERIES_SIZE} meetings"
        ));
    }
    let match_rule = derive_series_match_rule(titles.iter().map(String::as_str))
        .ok_or("Could not derive a stable matching rule from these meetings")?;

    sqlx::query(
        "UPDATE collections SET kind = 'series', auto_add = 1, match_rule = ? WHERE id = ?",
    )
    .bind(&match_rule)
    .bind(collection_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let meetings: Vec<(String, String)> = sqlx::query_as("SELECT id, title FROM meetings")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut added_count = 0;
    for (meeting_id, title) in meetings {
        let assigned = auto_assign_meeting(pool, &meeting_id, &title)
            .await
            .map_err(|e| e.to_string())?;
        if assigned.contains(&collection_id) {
            added_count += 1;
        }
    }

    Ok(SeriesAutoAddResult {
        enabled: true,
        match_rule: Some(match_rule),
        added_count,
    })
}

/// Propose series from the meeting archive (PLAN.md Phase 5 auto-series).
#[tauri::command]
pub async fn suggest_meeting_series(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SeriesSuggestionOut>, String> {
    let pool = state.db_manager.pool();
    // Once a suggestion has been accepted, do not propose the same meetings
    // again on the next launch. Manual collections do not suppress discovery.
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT m.id, m.title, \
                COALESCE(m.occurred_at, datetime(m.created_at, 'localtime')) \
         FROM meetings m \
         WHERE NOT EXISTS ( \
           SELECT 1 FROM meeting_collections mc \
           JOIN collections c ON c.id = mc.collection_id \
           WHERE mc.meeting_id = m.id AND c.kind = 'series' \
         )",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let meetings: Vec<MeetingRef> = rows
        .into_iter()
        .filter_map(|(id, title, created_at)| {
            // created_at may be a full datetime; take the leading YYYY-MM-DD.
            let date =
                chrono::NaiveDate::parse_from_str(created_at.get(0..10).unwrap_or(""), "%Y-%m-%d")
                    .ok()?;
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
    crate::jobs::store::enqueue_unique(
        pool,
        crate::jobs::kind::BACKFILL,
        None,
        &serde_json::json!({ "reason": "manual" }),
    )
    .await
    .map(|outcome| outcome.id)
    .map_err(|e| e.to_string())
}

fn is_secret_setting(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.ends_with(".api_key")
        || key.ends_with(".auth_key")
        || key.ends_with(".password")
        || key.ends_with(".token")
        || key.ends_with(".client_secret")
        || key.ends_with(".authorization_key")
        || key.ends_with(".secret")
        || key.ends_with(".credential")
}

/// Read app settings for the renderer. Secret values never cross the Tauri IPC boundary:
/// configured secrets are represented by a non-secret sentinel so existing UI presence
/// checks keep working without receiving credentials.
#[tauri::command]
pub async fn get_app_settings(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let pool = state.db_manager.pool();
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM app_settings_kv")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(key, value)| {
            if is_secret_setting(&key) {
                let public_value = redact_secret(Some(value)).unwrap_or_default();
                (key, public_value)
            } else {
                (key, value)
            }
        })
        .collect())
}

/// Set a privacy/config value (PLAN.md Phase 5/§8). Enforced at the LLM provider layer
/// via `crate::llm::PrivacyConfig::load`.
#[tauri::command]
pub async fn set_app_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("invalid app setting key".to_string());
    }
    if value.len() > 65_536 {
        return Err("app setting value is too large".to_string());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name_is_trimmed_and_bounded() {
        assert_eq!(
            normalize_collection_name("  Product  ".into()).unwrap(),
            "Product"
        );
        assert!(normalize_collection_name("   ".into()).is_err());
        assert!(normalize_collection_name("x".repeat(121)).is_err());
    }

    #[test]
    fn only_supported_collection_kinds_are_accepted() {
        assert_eq!(normalize_collection_kind(None).unwrap(), "manual");
        assert_eq!(
            normalize_collection_kind(Some("series".into())).unwrap(),
            "series"
        );
        assert!(normalize_collection_kind(Some("smart".into())).is_err());
    }

    #[test]
    fn secret_keys_are_classified_without_hiding_public_config() {
        assert!(is_secret_setting("deepseek.api_key"));
        assert!(is_secret_setting("gigachat.auth_key"));
        assert!(is_secret_setting("gigachat.password"));
        assert!(is_secret_setting("future_provider.client_secret"));
        assert!(!is_secret_setting("gigachat.model"));
        assert!(!is_secret_setting("privacy.local_only"));
    }
}
