//! Tauri command surface for archive search (PLAN.md Phase 1 task 5).

use serde::{Deserialize, Serialize};

use crate::search::hybrid::{HybridSearch, SearchFilters, SearchHit};
use crate::search::rag::{self, Citation, RagScope};
use crate::state::AppState;

/// Filter payload from the frontend filter panel. All fields optional.
#[derive(Debug, Default, Deserialize)]
pub struct SearchFiltersInput {
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub speaker_ids: Vec<i64>,
    #[serde(default)]
    pub collection_ids: Vec<i64>,
}

impl From<SearchFiltersInput> for SearchFilters {
    fn from(i: SearchFiltersInput) -> Self {
        SearchFilters {
            date_from: i.date_from,
            date_to: i.date_to,
            speaker_ids: i.speaker_ids,
            collection_ids: i.collection_ids,
            meeting_ids: Vec::new(),
        }
    }
}

/// Search the meeting archive. Returns fused chunk-level hits with meeting metadata and
/// `start_ms` for jump-to-timestamp.
///
/// NOTE: the vector branch is enabled once the Phase 1 embedder lands — this command
/// then embeds `query` and passes it to [`HybridSearch::search`]. Until then it runs
/// FTS-only, which is fully functional (RRF over a single branch preserves BM25 order).
#[tauri::command]
pub async fn search_meetings(
    state: tauri::State<'_, AppState>,
    query: String,
    filters: Option<SearchFiltersInput>,
    limit: Option<usize>,
) -> Result<Vec<SearchHit>, String> {
    let pool = state.db_manager.pool();
    let filters: SearchFilters = filters.unwrap_or_default().into();

    // Embed the query for the vector branch (None → FTS-only when no model is loaded).
    let query_embedding = crate::pipeline::embedder::embed_query(query.clone())
        .await
        .and_then(|r| r.ok());
    HybridSearch::search(pool, &query, query_embedding.as_deref(), &filters, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct RagAskInput {
    pub query: String,
    /// "archive" | "collection" | "meeting" (default archive).
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub collection_id: Option<i64>,
    #[serde(default)]
    pub meeting_id: Option<String>,
    /// Continue an existing chat session; omit to start a new one.
    #[serde(default)]
    pub session_id: Option<i64>,
}

fn default_scope() -> String {
    "archive".to_string()
}

#[derive(Debug, Serialize)]
pub struct RagAskResponse {
    pub session_id: i64,
    pub answer: String,
    pub citations: Vec<Citation>,
    pub found: bool,
    pub warning: Option<String>,
}

/// Ask a question over the archive with citations (PLAN.md Phase 4). Retrieves within
/// scope, generates a grounded answer via the routed LLM (GigaChat/DeepSeek), enforces
/// citations, and persists the turn to chat history.
#[tauri::command]
pub async fn rag_ask(
    state: tauri::State<'_, AppState>,
    input: RagAskInput,
) -> Result<RagAskResponse, String> {
    let pool = state.db_manager.pool();

    let scope = match input.scope.as_str() {
        "collection" => RagScope::Collection(
            input.collection_id.ok_or("collection_id required for collection scope")?,
        ),
        "meeting" => RagScope::Meeting(
            input.meeting_id.clone().ok_or("meeting_id required for meeting scope")?,
        ),
        _ => RagScope::Archive,
    };

    // Resolve or create the chat session for this scope.
    let session_id = match input.session_id {
        Some(id) => id,
        None => sqlx::query_scalar::<_, i64>("INSERT INTO chat_sessions(scope) VALUES(?) RETURNING id")
            .bind(scope.label())
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?,
    };

    // Last 6 turns (oldest-first) as conversation history.
    let mut history: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM chat_messages WHERE session_id = ? ORDER BY id DESC LIMIT 6",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    history.reverse();

    let answer = rag::ask(pool, &input.query, &scope, &history).await?;

    // Persist the turn (user question + assistant answer with citations JSON).
    sqlx::query("INSERT INTO chat_messages(session_id, role, content) VALUES(?, 'user', ?)")
        .bind(session_id)
        .bind(&input.query)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let citations_json = serde_json::to_string(&answer.citations).unwrap_or_else(|_| "[]".to_string());
    sqlx::query("INSERT INTO chat_messages(session_id, role, content, citations) VALUES(?, 'assistant', ?, ?)")
        .bind(session_id)
        .bind(&answer.answer)
        .bind(citations_json)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let _ = sqlx::query("UPDATE chat_sessions SET updated_at = datetime('now') WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await;

    Ok(RagAskResponse {
        session_id,
        answer: answer.answer,
        citations: answer.citations,
        found: answer.found,
        warning: answer.warning,
    })
}
