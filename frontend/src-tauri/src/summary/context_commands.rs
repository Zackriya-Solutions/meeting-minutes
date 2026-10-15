use crate::database::repositories::summary_context::{
    SummaryContextRepository, SummaryContextScope, MAX_SUMMARY_CONTEXT_CHARS,
};
use crate::state::AppState;
use crate::summary::context::{load_inherited_contexts, InheritedContext};
use serde::Serialize;
use sqlx::SqlitePool;

/// A meeting's saved context together with the inherited levels that apply to it.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryContext {
    pub context: String,
    pub use_inherited: bool,
    pub inherited: Vec<InheritedContext>,
}

/// The text of one context level.
#[derive(Debug, PartialEq, Serialize)]
pub struct ContextText {
    pub context: String,
}

/// Rejects text longer than the per-level limit. Surrounding whitespace is not counted.
pub(crate) fn check_context_length(context: &str) -> Result<(), String> {
    let length = context.trim().chars().count();
    if length > MAX_SUMMARY_CONTEXT_CHARS {
        return Err(format!(
            "Summary context is too long ({} characters). The limit is {}.",
            length, MAX_SUMMARY_CONTEXT_CHARS
        ));
    }
    Ok(())
}

pub(crate) async fn get_meeting_context(
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<MeetingSummaryContext, String> {
    let row = SummaryContextRepository::get(pool, SummaryContextScope::Meeting, meeting_id)
        .await
        .map_err(|e| format!("Failed to load summary context: {}", e))?;
    let inherited = load_inherited_contexts(pool, meeting_id)
        .await
        .map_err(|e| format!("Failed to load inherited summary context: {}", e))?;
    let (context, use_inherited) =
        row.map_or((String::new(), true), |r| (r.context, r.use_inherited));
    Ok(MeetingSummaryContext {
        context,
        use_inherited,
        inherited,
    })
}

pub(crate) async fn save_meeting_context(
    pool: &SqlitePool,
    meeting_id: &str,
    context: &str,
    use_inherited: bool,
) -> Result<(), String> {
    check_context_length(context)?;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Failed to save summary context: {}", e))?;
    if exists.is_none() {
        return Err(format!("Meeting not found: {}", meeting_id));
    }
    SummaryContextRepository::save(
        pool,
        SummaryContextScope::Meeting,
        meeting_id,
        context,
        use_inherited,
    )
    .await
    .map_err(|e| format!("Failed to save summary context: {}", e))
}

pub(crate) async fn get_global_context(pool: &SqlitePool) -> Result<ContextText, String> {
    let row = SummaryContextRepository::get(pool, SummaryContextScope::Global, "")
        .await
        .map_err(|e| format!("Failed to load summary context: {}", e))?;
    Ok(ContextText {
        context: row.map(|r| r.context).unwrap_or_default(),
    })
}

pub(crate) async fn save_global_context(pool: &SqlitePool, context: &str) -> Result<(), String> {
    check_context_length(context)?;
    SummaryContextRepository::save(pool, SummaryContextScope::Global, "", context, true)
        .await
        .map_err(|e| format!("Failed to save summary context: {}", e))
}

/// Gets a meeting's saved summary context and the inherited levels that apply to it.
#[tauri::command]
pub async fn api_get_meeting_summary_context(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingSummaryContext, String> {
    get_meeting_context(state.db_manager.pool(), &meeting_id).await
}

/// Saves a meeting's summary context and whether it uses the inherited levels.
#[tauri::command]
pub async fn api_save_meeting_summary_context(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    context: String,
    use_inherited: bool,
) -> Result<(), String> {
    save_meeting_context(state.db_manager.pool(), &meeting_id, &context, use_inherited).await
}

/// Gets the summary context that applies to every meeting.
#[tauri::command]
pub async fn api_get_global_summary_context(
    state: tauri::State<'_, AppState>,
) -> Result<ContextText, String> {
    get_global_context(state.db_manager.pool()).await
}

/// Saves the summary context that applies to every meeting.
#[tauri::command]
pub async fn api_save_global_summary_context(
    state: tauri::State<'_, AppState>,
    context: String,
) -> Result<(), String> {
    save_global_context(state.db_manager.pool(), &context).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::repositories::summary_context::test_util::{insert_meeting, test_pool};

    #[tokio::test]
    async fn a_meeting_without_saved_context_reads_as_empty_and_inheriting() {
        let pool = test_pool().await;
        insert_meeting(&pool, "m1").await;
        assert_eq!(
            get_meeting_context(&pool, "m1").await.unwrap(),
            MeetingSummaryContext { context: String::new(), use_inherited: true, inherited: vec![] }
        );
    }

    #[tokio::test]
    async fn meeting_context_round_trips_with_the_inherited_global_level() {
        let pool = test_pool().await;
        insert_meeting(&pool, "m1").await;
        save_global_context(&pool, "I am Noah.").await.unwrap();
        save_meeting_context(&pool, "m1", " Renewal call. ", false).await.unwrap();

        let loaded = get_meeting_context(&pool, "m1").await.unwrap();
        assert_eq!(loaded.context, "Renewal call.");
        assert!(!loaded.use_inherited);
        assert_eq!(loaded.inherited.len(), 1);
        assert_eq!(loaded.inherited[0].scope, SummaryContextScope::Global);
        assert_eq!(loaded.inherited[0].context, "I am Noah.");
    }

    #[tokio::test]
    async fn saving_for_an_unknown_meeting_is_an_error_and_stores_nothing() {
        let pool = test_pool().await;
        let error = save_meeting_context(&pool, "missing", "text", true).await.unwrap_err();
        assert!(error.contains("Meeting not found"), "{error}");
        assert_eq!(
            SummaryContextRepository::get(&pool, SummaryContextScope::Meeting, "missing")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn global_context_round_trips_and_starts_empty() {
        let pool = test_pool().await;
        assert_eq!(get_global_context(&pool).await.unwrap(), ContextText { context: String::new() });
        save_global_context(&pool, "I am Noah.").await.unwrap();
        assert_eq!(
            get_global_context(&pool).await.unwrap(),
            ContextText { context: "I am Noah.".to_string() }
        );
    }

    #[tokio::test]
    async fn the_limit_counts_characters_not_bytes_and_ignores_surrounding_whitespace() {
        let pool = test_pool().await;
        insert_meeting(&pool, "m1").await;

        let at_limit = "é🎯".repeat(MAX_SUMMARY_CONTEXT_CHARS / 2);
        assert_eq!(at_limit.chars().count(), MAX_SUMMARY_CONTEXT_CHARS);
        save_meeting_context(&pool, "m1", &format!("  {at_limit}\n"), true).await.unwrap();
        save_global_context(&pool, &at_limit).await.unwrap();

        let over_limit = format!("{at_limit}x");
        let error = save_meeting_context(&pool, "m1", &over_limit, true).await.unwrap_err();
        assert!(error.contains("4000"), "{error}");
        assert!(save_global_context(&pool, &over_limit).await.is_err());
        // The rejected save left the earlier text in place.
        assert_eq!(get_meeting_context(&pool, "m1").await.unwrap().context, at_limit);
    }
}
