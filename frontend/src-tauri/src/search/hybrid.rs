//! Hybrid search fusion (PLAN.md Phase 1, branch fusion).
//!
//! Two independent rankings — FTS5 BM25 (branch A) and vector KNN (branch B) — are
//! combined with Reciprocal Rank Fusion: `score(d) = Σ_i 1/(k + rank_i(d))`, k=60.
//! RRF needs only ranks (not comparable scores), so it fuses lexical and semantic
//! results cleanly and degrades to a single branch when the other is empty (e.g. when
//! sqlite-vec is unavailable — see [`crate::vector`]).
//!
//! The fusion here is pure and unit-tested; the engine that produces the two rankings
//! (FTS query + `crate::vector::knn` over chunk embeddings) is assembled on top once
//! the Phase 1 embedder lands. Filters (date/speaker/collection) are applied as SQL
//! predicates on BOTH branches before ranking.

/// Standard RRF constant from the literature (Cormack et al.). Configurable so the
/// eval harness can tune it.
pub const DEFAULT_RRF_K: f64 = 60.0;

/// Filters applied to both branches before ranking (PLAN.md Phase 1 task 4).
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Inclusive ISO date lower bound (matches `meetings.created_at`).
    pub date_from: Option<String>,
    /// Inclusive ISO date upper bound.
    pub date_to: Option<String>,
    /// Restrict to segments attributed to these speakers (Phase 2 populates).
    pub speaker_ids: Vec<i64>,
    /// Restrict to meetings in these collections (Phase 5 populates).
    pub collection_ids: Vec<i64>,
    /// Restrict to specific meetings (used by RAG meeting-scope; TEXT UUIDs).
    pub meeting_ids: Vec<String>,
}

impl SearchFilters {
    pub fn is_empty(&self) -> bool {
        self.date_from.is_none()
            && self.date_to.is_none()
            && self.speaker_ids.is_empty()
            && self.collection_ids.is_empty()
            && self.meeting_ids.is_empty()
    }
}

/// Fuse several ranked id lists (each ordered best-first, no duplicates within a list)
/// into one ranking by Reciprocal Rank Fusion. Returns `(id, score)` sorted by score
/// descending; ties broken by ascending id for determinism.
pub fn reciprocal_rank_fusion(rankings: &[Vec<i64>], k: f64) -> Vec<(i64, f64)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for ranking in rankings {
        for (idx, &id) in ranking.iter().enumerate() {
            let rank = (idx + 1) as f64; // 1-based rank
            *scores.entry(id).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut fused: Vec<(i64, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    fused
}

/// A fused search result (chunk-level). `start_ms` powers jump-to-timestamp.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub chunk_id: i64,
    pub meeting_id: String,
    pub meeting_title: String,
    pub start_ms: i64,
    pub text: String,
    pub score: f64,
    /// Query terms present for client-side highlighting.
    pub matched_terms: Vec<String>,
}

/// Per-branch retrieval depth before fusion (PLAN.md Phase 1: top 20 each).
const BRANCH_LIMIT: i64 = 20;

/// The hybrid search engine. Runs BM25 (branch A) and vector KNN (branch B), applies
/// filters to both, fuses with RRF, and loads the top results. Degrades to FTS-only
/// when no query embedding is supplied or sqlite-vec is unavailable.
pub struct HybridSearch;

impl HybridSearch {
    /// Execute a hybrid search. `query_embedding` is the (L2-normalized) embedding of
    /// `query_text` produced by the Phase 1 embedder; pass `None` for FTS-only.
    pub async fn search(
        pool: &sqlx::SqlitePool,
        query_text: &str,
        query_embedding: Option<&[f32]>,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchHit>, sqlx::Error> {
        let terms = query_terms(query_text);

        // Resolve scope before either branch ranks candidates.
        let allowed = allowed_chunk_ids(pool, filters).await?;

        // Branch A: FTS5 BM25 over chunks.
        let fts_ids = fts_branch(pool, &terms, BRANCH_LIMIT, allowed.as_ref()).await?;

        // Branch B: vector KNN (best-effort; empty if unavailable or not requested).
        let vec_ids: Vec<i64> = match query_embedding {
            Some(emb) => {
                let result = match &allowed {
                    Some(ids) => {
                        let mut ids: Vec<i64> = ids.iter().copied().collect();
                        ids.sort_unstable();
                        crate::vector::knn_filtered(pool, emb, &ids, BRANCH_LIMIT).await
                    }
                    None => crate::vector::knn(pool, emb, BRANCH_LIMIT).await,
                };
                match result {
                    Ok(rows) => rows.into_iter().map(|(id, _)| id).collect(),
                    Err(e) => {
                        log::warn!("vector branch unavailable, using FTS-only: {e}");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };

        let fused = reciprocal_rank_fusion(&[fts_ids, vec_ids], DEFAULT_RRF_K);
        let top: Vec<(i64, f64)> = fused.into_iter().take(limit).collect();
        load_hits(pool, &top, &terms).await
    }
}

/// Tokenize the user query into lowercase alphanumeric terms (used for the FTS MATCH
/// expression and for highlighting).
fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Branch A: BM25 over `chunks_fts`, returning chunk ids best-first. Terms are quoted
/// and OR-joined so arbitrary user input can't break FTS5 query syntax.
async fn fts_branch(
    pool: &sqlx::SqlitePool,
    terms: &[String],
    limit: i64,
    allowed: Option<&std::collections::HashSet<i64>>,
) -> Result<Vec<i64>, sqlx::Error> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let match_expr = terms
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");

    let Some(allowed) = allowed else {
        return sqlx::query_scalar::<_, i64>(
            "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY bm25(chunks_fts), rowid LIMIT ?",
        ).bind(match_expr).bind(limit).fetch_all(pool).await;
    };
    if allowed.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<i64> = allowed.iter().copied().collect();
    ids.sort_unstable();
    let mut candidates: Vec<(i64, f64)> = Vec::new();
    for batch in ids.chunks(400) {
        let placeholders = vec!["?"; batch.len()].join(",");
        let sql = format!(
            "SELECT rowid, bm25(chunks_fts) AS rank FROM chunks_fts \
             WHERE chunks_fts MATCH ? AND rowid IN ({placeholders}) \
             ORDER BY rank, rowid LIMIT ?"
        );
        let mut query = sqlx::query_as::<_, (i64, f64)>(&sql).bind(&match_expr);
        for id in batch {
            query = query.bind(*id);
        }
        candidates.extend(query.bind(limit).fetch_all(pool).await?);
    }
    candidates.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    candidates.dedup_by_key(|row| row.0);
    candidates.truncate(limit as usize);
    Ok(candidates.into_iter().map(|(id, _)| id).collect())
}

/// Compute the set of chunk ids permitted by privacy and optional filters. This always
/// returns a set: a sensitive memory with indexing disabled must never enter an unfiltered
/// FTS/vector branch.
async fn allowed_chunk_ids(
    pool: &sqlx::SqlitePool,
    filters: &SearchFilters,
) -> Result<Option<std::collections::HashSet<i64>>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT DISTINCT c.id FROM chunks c JOIN meetings m ON m.id = c.meeting_id \
         WHERE m.indexing_allowed = 1",
    );
    if filters.date_from.is_some() {
        sql.push_str(" AND m.created_at >= ?");
    }
    if filters.date_to.is_some() {
        sql.push_str(" AND m.created_at <= ?");
    }
    if !filters.meeting_ids.is_empty() {
        // TEXT ids → bound placeholders (never inlined).
        let placeholders = vec!["?"; filters.meeting_ids.len()].join(",");
        sql.push_str(&format!(" AND c.meeting_id IN ({placeholders})"));
    }
    if !filters.collection_ids.is_empty() {
        sql.push_str(&format!(
            " AND c.meeting_id IN (SELECT meeting_id FROM meeting_collections WHERE collection_id IN ({}))",
            int_list(&filters.collection_ids)
        ));
    }
    if !filters.speaker_ids.is_empty() {
        // A chunk qualifies if any segment attributed to one of these speakers falls
        // within the chunk's time span (segments store seconds; chunks store ms).
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM transcripts t WHERE t.meeting_id = c.meeting_id \
              AND t.speaker_id IN ({}) \
              AND CAST(t.audio_start_time * 1000 AS INTEGER) BETWEEN c.start_ms AND c.end_ms)",
            int_list(&filters.speaker_ids)
        ));
    }

    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    if let Some(from) = &filters.date_from {
        q = q.bind(from);
    }
    if let Some(to) = &filters.date_to {
        q = q.bind(to);
    }
    for mid in &filters.meeting_ids {
        q = q.bind(mid);
    }
    let ids: Vec<i64> = q.fetch_all(pool).await?;
    Ok(Some(ids.into_iter().collect()))
}

fn int_list(ids: &[i64]) -> String {
    ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

/// Load full result rows for the fused top ids, preserving fused order and score.
async fn load_hits(
    pool: &sqlx::SqlitePool,
    ranked: &[(i64, f64)],
    terms: &[String],
) -> Result<Vec<SearchHit>, sqlx::Error> {
    if ranked.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
    let sql = format!(
        "SELECT c.id, c.meeting_id, m.title, c.start_ms, c.text \
         FROM chunks c JOIN meetings m ON m.id = c.meeting_id WHERE c.id IN ({})",
        int_list(&ids)
    );
    let rows: Vec<(i64, String, String, i64, String)> =
        sqlx::query_as(&sql).fetch_all(pool).await?;

    // Index rows by id, then emit in fused order.
    let mut by_id: std::collections::HashMap<i64, (String, String, i64, String)> = rows
        .into_iter()
        .map(|(id, mid, title, start, text)| (id, (mid, title, start, text)))
        .collect();

    let hits = ranked
        .iter()
        .filter_map(|(id, score)| {
            by_id.remove(id).map(|(meeting_id, meeting_title, start_ms, text)| {
                let matched_terms = terms
                    .iter()
                    .filter(|t| text.to_lowercase().contains(t.as_str()))
                    .cloned()
                    .collect();
                SearchHit {
                    chunk_id: *id,
                    meeting_id,
                    meeting_title,
                    start_ms,
                    text,
                    score: *score,
                    matched_terms,
                }
            })
        })
        .collect();
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_ranking_preserves_order() {
        let fused = reciprocal_rank_fusion(&[vec![10, 20, 30]], DEFAULT_RRF_K);
        let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![10, 20, 30]);
    }

    #[test]
    fn agreement_across_branches_boosts_rank() {
        // id 20 is #2 in FTS but #1 in vector; id 10 is #1 in FTS only.
        // Appearing highly in BOTH should let 20 win overall.
        let fts = vec![10, 20, 30];
        let vector = vec![20, 40, 10];
        let fused = reciprocal_rank_fusion(&[fts, vector], DEFAULT_RRF_K);
        assert_eq!(fused[0].0, 20, "cross-branch agreement wins");
    }

    #[test]
    fn empty_branch_degrades_to_the_other() {
        let fts = vec![1, 2, 3];
        let vector: Vec<i64> = vec![];
        let fused = reciprocal_rank_fusion(&[fts, vector], DEFAULT_RRF_K);
        let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2, 3], "FTS-only fallback still works");
    }

    #[test]
    fn is_deterministic_on_ties() {
        // Two ids each appearing once at rank 1 in separate lists tie on score;
        // ascending-id tiebreak makes the order stable.
        let a = reciprocal_rank_fusion(&[vec![5], vec![3]], DEFAULT_RRF_K);
        let b = reciprocal_rank_fusion(&[vec![3], vec![5]], DEFAULT_RRF_K);
        assert_eq!(a, b);
        assert_eq!(a[0].0, 3, "lower id first on tie");
    }

    #[tokio::test]
    async fn scoped_search_is_not_crowded_out_by_global_top_k() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, indexing_allowed INTEGER NOT NULL DEFAULT 1)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE chunks_fts USING fts5(text)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings(id,title,created_at) VALUES('outside','Outside','2026-01-01'),('inside','Inside','2026-01-01')")
            .execute(&pool).await.unwrap();
        for id in 1..=25_i64 {
            sqlx::query("INSERT INTO chunks VALUES(?,'outside',0,'бюджет')")
                .bind(id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(?,'бюджет')")
                .bind(id).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO chunks VALUES(99,'inside',0,'бюджет')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(99,'бюджет')")
            .execute(&pool).await.unwrap();
        let filters = SearchFilters { meeting_ids: vec!["inside".into()], ..Default::default() };
        let hits = HybridSearch::search(&pool, "бюджет", None, &filters, 5).await.unwrap();
        assert_eq!(hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(), vec![99]);
    }

    #[tokio::test]
    async fn privacy_filter_excludes_non_indexed_memories_without_other_filters() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, indexing_allowed INTEGER NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings VALUES('public',1),('sensitive',0)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chunks VALUES(1,'public'),(2,'sensitive')")
            .execute(&pool)
            .await
            .unwrap();

        let allowed = allowed_chunk_ids(&pool, &SearchFilters::default())
            .await
            .unwrap()
            .unwrap();
        assert!(allowed.contains(&1));
        assert!(!allowed.contains(&2));
    }
}
