//! Local memory search client. The heavy lifting (bge-m3 embeddings,
//! sqlite-vec, FTS5 + PyThaiNLP segmentation, hybrid ranking) lives in the
//! Python sidecar. Rust just chunks the session and calls the sidecar's
//! `/index` and `/search` endpoints. Everything stays on 127.0.0.1.

use super::config::config;
use super::session::SessionLog;
use super::summary::StructuredSummary;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct IndexChunk {
    chunk_id: String,
    text: String,
    kind: String, // "transcript" | "summary"
}

#[derive(Debug, Serialize)]
struct IndexRequest {
    db_path: String,
    embed_model: String,
    meeting_date: String,
    session_time: String,
    topics: Vec<String>,
    file_path: String,
    summary_path: String,
    chunks: Vec<IndexChunk>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResult {
    pub snippet: String,
    pub meeting_date: String,
    pub session_time: String,
    #[serde(default)]
    pub topics: Vec<String>,
    pub file_path: String,
    #[serde(default)]
    pub summary_path: String,
    #[serde(default)]
    pub score: f32,
}

#[derive(Debug, Serialize)]
struct SearchRequest {
    db_path: String,
    embed_model: String,
    query: String,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchResult>,
}

/// Split text into ~chunk_chars windows on line boundaries.
fn chunk_text(text: &str, chunk_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if current.len() + line.len() + 1 > chunk_chars && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Index a finished session into the local memory store via the sidecar.
pub async fn index_session(
    session: &SessionLog,
    summary: &StructuredSummary,
) -> Result<(), String> {
    let cfg = config();

    let mut chunks: Vec<IndexChunk> = Vec::new();
    for (i, c) in chunk_text(&session.transcript_plain(), 800).into_iter().enumerate() {
        chunks.push(IndexChunk {
            chunk_id: format!("{}_{}_t{}", session.date_iso, session.session_time, i),
            text: c,
            kind: "transcript".to_string(),
        });
    }
    // Summary as its own searchable chunk.
    let summary_blob = format!(
        "Overview: {}\nTopics: {}\nActions: {}",
        summary.overview.join("; "),
        summary.topics.join(", "),
        summary
            .action_items
            .iter()
            .map(|a| format!("{}: {}", a.assignee, a.task))
            .collect::<Vec<_>>()
            .join("; ")
    );
    chunks.push(IndexChunk {
        chunk_id: format!("{}_{}_summary", session.date_iso, session.session_time),
        text: summary_blob,
        kind: "summary".to_string(),
    });

    if chunks.is_empty() {
        return Ok(());
    }

    let summary_name = cfg.summary_pattern.replace("{time}", &session.session_time);
    let req = IndexRequest {
        db_path: cfg.vector_db_path.to_string_lossy().to_string(),
        embed_model: cfg.embed_model.clone(),
        meeting_date: session.date_iso.clone(),
        session_time: session.session_time.clone(),
        topics: summary.topics.clone(),
        file_path: session.transcript_path.to_string_lossy().to_string(),
        summary_path: session
            .date_folder
            .join(summary_name)
            .to_string_lossy()
            .to_string(),
        chunks,
    };

    let url = format!("{}/index", cfg.sidecar_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("index request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("sidecar /index returned {}", resp.status()));
    }
    Ok(())
}

/// Hybrid search the local memory store via the sidecar.
pub async fn search(query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let cfg = config();
    let req = SearchRequest {
        db_path: cfg.vector_db_path.to_string_lossy().to_string(),
        embed_model: cfg.embed_model.clone(),
        query: query.to_string(),
        limit,
    };
    let url = format!("{}/search", cfg.sidecar_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("search request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("sidecar /search returned {}", resp.status()));
    }
    let parsed: SearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("search decode failed: {e}"))?;
    Ok(parsed.results)
}
