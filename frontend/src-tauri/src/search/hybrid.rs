//! Hybrid search fusion (PLAN.md Phase 1, branch fusion).
//!
//! Lexical, alias-expanded, fuzzy and vector rankings are combined with Reciprocal
//! Rank Fusion: `score(d) = Σ_i 1/(k + rank_i(d))`, k=60.
//! RRF needs only ranks (not comparable scores), so it fuses lexical and semantic
//! candidates cleanly and degrades when a branch is unavailable (e.g. when sqlite-vec
//! is unavailable — see [`crate::vector`]). Final confidence is computed separately
//! from lexical coverage and cosine distance; an RRF rank is not a confidence score.
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
    /// Independent evidence signals. Unlike RRF rank, these remain meaningful when
    /// one retrieval branch is temporarily unavailable.
    pub lexical_score: f64,
    pub semantic_score: f64,
    pub match_sources: Vec<String>,
    /// Query terms present for client-side highlighting.
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SearchDiagnostics {
    pub query_rewritten: bool,
    pub lexical_hits: usize,
    pub fuzzy_hits: usize,
    pub semantic_hits: usize,
    pub transcript_fallback_hits: usize,
    pub semantic_available: bool,
    pub candidates_considered: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub diagnostics: SearchDiagnostics,
}

/// Per-branch retrieval depth before fusion (PLAN.md Phase 1: top 20 each).
const BRANCH_LIMIT: i64 = 20;
const FUZZY_PREFIX_LIMIT: i64 = 96;
const MAX_FUZZY_CANDIDATES: usize = 512;

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
        let plan = crate::search::query::QueryPlan::build(query_text);
        Ok(
            Self::search_planned(pool, &plan, query_embedding, filters, limit)
                .await?
                .hits,
        )
    }

    /// Search with a shared query plan. Callers should embed `plan.semantic_query`,
    /// not the raw typo-heavy input, and pass that vector here.
    pub async fn search_planned(
        pool: &sqlx::SqlitePool,
        plan: &crate::search::query::QueryPlan,
        query_embedding: Option<&[f32]>,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<SearchResults, sqlx::Error> {
        use std::collections::{HashMap, HashSet};

        let primary_terms = plan.primary_terms();
        let expanded_terms = plan.expanded_terms();

        // Resolve scope before either branch ranks candidates.
        let allowed = allowed_chunk_ids(pool, filters).await?;

        // Two lexical rankings: canonical content terms and alias-expanded terms.
        let fts_ids = fts_branch(pool, &primary_terms, BRANCH_LIMIT, allowed.as_ref()).await?;
        let expanded_fts_ids = if expanded_terms == primary_terms {
            Vec::new()
        } else {
            fts_branch(pool, &expanded_terms, BRANCH_LIMIT, allowed.as_ref()).await?
        };
        let prefix_fts_ids =
            fts_prefix_branch(pool, &expanded_terms, FUZZY_PREFIX_LIMIT, allowed.as_ref()).await?;

        // Branch B: vector KNN (best-effort; empty if unavailable or not requested).
        let vec_rows: Vec<(i64, f64)> = match query_embedding {
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
                    Ok(rows) => rows,
                    Err(e) => {
                        log::warn!("vector branch unavailable, using FTS-only: {e}");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };

        // Fuzzy/title scoring is deliberately bounded. Prefix FTS supplies likely
        // inflection/ASR candidates; exact and semantic top-K are always retained; a
        // small deterministic sample covers cases where those branches are empty.
        let required_candidates: HashSet<i64> = fts_ids
            .iter()
            .chain(expanded_fts_ids.iter())
            .chain(prefix_fts_ids.iter())
            .chain(vec_rows.iter().map(|row| &row.0))
            .copied()
            .collect();
        let fuzzy_candidate_ids =
            bounded_fuzzy_candidate_ids(allowed.as_ref(), &required_candidates);
        let all_rows = load_rows_by_ids(pool, &fuzzy_candidate_ids).await?;
        let mut lexical_by_id: HashMap<i64, (f64, f64, Vec<String>)> = HashMap::new();
        let mut fuzzy_ranked: Vec<(i64, f64)> = all_rows
            .iter()
            .filter_map(|row| {
                let (body, matched) =
                    crate::search::query::concept_coverage(&plan.concepts, &row.text);
                let (title, _) =
                    crate::search::query::concept_coverage(&plan.concepts, &row.meeting_title);
                lexical_by_id.insert(row.id, (body, title, matched));
                let fuzzy = body.max(title * 0.9);
                (fuzzy >= fuzzy_candidate_floor(plan.concepts.len())).then_some((row.id, fuzzy))
            })
            .collect();
        fuzzy_ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        fuzzy_ranked.truncate(BRANCH_LIMIT as usize);
        let fuzzy_ids: Vec<i64> = fuzzy_ranked.iter().map(|row| row.0).collect();
        let vec_ids: Vec<i64> = vec_rows.iter().map(|row| row.0).collect();

        let rankings = vec![
            fts_ids.clone(),
            expanded_fts_ids.clone(),
            prefix_fts_ids,
            fuzzy_ids.clone(),
            vec_ids.clone(),
        ];
        let fused = reciprocal_rank_fusion(&rankings, DEFAULT_RRF_K);
        let fused_by_id: HashMap<i64, f64> = fused.iter().copied().collect();
        let lexical_ids: HashSet<i64> = fts_ids
            .iter()
            .chain(expanded_fts_ids.iter())
            .copied()
            .collect();
        let fuzzy_set: HashSet<i64> = fuzzy_ids.iter().copied().collect();
        let semantic_by_id: HashMap<i64, f64> = vec_rows
            .iter()
            .map(|(id, distance)| (*id, (1.0 - distance).clamp(0.0, 1.0)))
            .collect();

        let mut hits: Vec<SearchHit> = all_rows
            .into_iter()
            .filter(|row| fused_by_id.contains_key(&row.id))
            .map(|row| {
                let (coverage, title_coverage, matched_terms) =
                    lexical_by_id.remove(&row.id).unwrap_or_default();
                let semantic_score = semantic_by_id.get(&row.id).copied().unwrap_or(0.0);
                let exact_lexical = lexical_ids.contains(&row.id);
                // Exact FTS proves that at least one token matched, not that the
                // fragment answers the whole question. Confidence therefore remains
                // proportional to concept coverage; exactness is retained as a rank
                // branch/source instead of granting every OR-match a fixed floor.
                let lexical_score = coverage.max(title_coverage * 0.9);
                let mut match_sources = Vec::new();
                if exact_lexical {
                    match_sources.push("keyword".to_string());
                }
                if fuzzy_set.contains(&row.id) && !exact_lexical {
                    match_sources.push("fuzzy".to_string());
                }
                if semantic_by_id.contains_key(&row.id) {
                    match_sources.push("semantic".to_string());
                }
                if title_coverage >= fuzzy_candidate_floor(plan.concepts.len()) {
                    match_sources.push("title".to_string());
                }
                let agreement_bonus = 0.035 * match_sources.len().saturating_sub(1) as f64;
                let rrf_tiebreak = fused_by_id.get(&row.id).copied().unwrap_or(0.0) / 100.0;
                let score =
                    (lexical_score.max(semantic_score) + agreement_bonus + rrf_tiebreak).min(1.0);
                SearchHit {
                    chunk_id: row.id,
                    meeting_id: row.meeting_id,
                    meeting_title: row.meeting_title,
                    start_ms: row.start_ms,
                    text: row.text,
                    score,
                    lexical_score,
                    semantic_score,
                    match_sources,
                    matched_terms,
                }
            })
            .collect();

        // Meetings become searchable immediately after transcription. If their chunk
        // job has not run yet, use transcript segments as temporary retrieval units.
        let transcript_hits = transcript_fallback_hits(pool, plan, filters).await?;
        let transcript_fallback_count = transcript_hits.len();
        hits.extend(transcript_hits);
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.chunk_id.cmp(&b.chunk_id))
        });
        let candidates_considered = hits.len();
        hits.truncate(limit);

        Ok(SearchResults {
            hits,
            diagnostics: SearchDiagnostics {
                query_rewritten: plan.rewritten,
                lexical_hits: lexical_ids.len(),
                fuzzy_hits: fuzzy_ids.len(),
                semantic_hits: vec_ids.len(),
                transcript_fallback_hits: transcript_fallback_count,
                semantic_available: query_embedding.is_some() && !vec_ids.is_empty(),
                candidates_considered,
            },
        })
    }
}

fn fuzzy_candidate_floor(concept_count: usize) -> f64 {
    if concept_count == 0 {
        1.0
    } else {
        (0.9 / concept_count as f64).clamp(0.10, 0.45)
    }
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

/// Prefix FTS is a cheap corpus-side prefilter for fuzzy scoring. It handles common
/// Russian inflections and trailing ASR mistakes without pulling the whole archive
/// into Rust. Edit-distance scoring is applied only after this bounded branch.
async fn fts_prefix_branch(
    pool: &sqlx::SqlitePool,
    terms: &[String],
    limit: i64,
    allowed: Option<&std::collections::HashSet<i64>>,
) -> Result<Vec<i64>, sqlx::Error> {
    let mut prefixes = Vec::new();
    for term in terms {
        if term.chars().count() < 5 {
            continue;
        }
        let prefix: String = term.chars().take(6).collect();
        if !prefixes.contains(&prefix) {
            prefixes.push(prefix);
        }
        if prefixes.len() == 24 {
            break;
        }
    }
    if prefixes.is_empty() {
        return Ok(Vec::new());
    }
    let match_expr = prefixes
        .iter()
        .map(|prefix| format!("\"{}\"*", prefix.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");

    let Some(allowed) = allowed else {
        return sqlx::query_scalar::<_, i64>(
            "SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ? \
             ORDER BY bm25(chunks_fts), rowid LIMIT ?",
        )
        .bind(match_expr)
        .bind(limit)
        .fetch_all(pool)
        .await;
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
    Ok(candidates.into_iter().map(|row| row.0).collect())
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
    ids.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Debug)]
struct ChunkRow {
    id: i64,
    meeting_id: String,
    meeting_title: String,
    start_ms: i64,
    text: String,
}

fn bounded_fuzzy_candidate_ids(
    allowed: Option<&std::collections::HashSet<i64>>,
    required: &std::collections::HashSet<i64>,
) -> std::collections::HashSet<i64> {
    let mut selected = required.clone();
    let Some(allowed) = allowed else {
        return selected;
    };
    selected.retain(|id| allowed.contains(id));
    if selected.len() >= MAX_FUZZY_CANDIDATES {
        return selected;
    }
    let mut remaining: Vec<i64> = allowed
        .iter()
        .filter(|id| !selected.contains(id))
        .copied()
        .collect();
    // Newer deterministic ids are the most useful fallback while prefix/vector
    // branches still preserve relevant older chunks.
    remaining.sort_unstable_by(|a, b| b.cmp(a));
    for id in remaining
        .into_iter()
        .take(MAX_FUZZY_CANDIDATES - selected.len())
    {
        selected.insert(id);
    }
    selected
}

async fn load_rows_by_ids(
    pool: &sqlx::SqlitePool,
    ids: &std::collections::HashSet<i64>,
) -> Result<Vec<ChunkRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<i64> = ids.iter().copied().collect();
    ids.sort_unstable();
    let mut output = Vec::with_capacity(ids.len());
    for batch in ids.chunks(400) {
        let sql = format!(
            "SELECT c.id, c.meeting_id, COALESCE(m.title, ''), c.start_ms, c.text \
             FROM chunks c JOIN meetings m ON m.id = c.meeting_id \
             WHERE c.id IN ({})",
            vec!["?"; batch.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, String, String, i64, String)>(&sql);
        for id in batch {
            query = query.bind(*id);
        }
        output.extend(query.fetch_all(pool).await?.into_iter().map(chunk_row));
    }
    Ok(output)
}

fn chunk_row(
    (id, meeting_id, meeting_title, start_ms, text): (i64, String, String, i64, String),
) -> ChunkRow {
    ChunkRow {
        id,
        meeting_id,
        meeting_title,
        start_ms,
        text,
    }
}

/// Search transcript rows only for indexable meetings which do not have chunks yet.
/// These hits use a negative transcript rowid as an internal, collision-free id;
/// citations navigate by meeting + timestamp and therefore remain fully functional.
async fn transcript_fallback_hits(
    pool: &sqlx::SqlitePool,
    plan: &crate::search::query::QueryPlan,
    filters: &SearchFilters,
) -> Result<Vec<SearchHit>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT t.rowid, t.meeting_id, COALESCE(m.title, ''), \
                CAST(COALESCE(t.audio_start_time, 0) * 1000 AS INTEGER), t.transcript \
         FROM transcripts t JOIN meetings m ON m.id=t.meeting_id \
         WHERE m.indexing_allowed=1 AND length(trim(t.transcript)) > 0 \
           AND NOT EXISTS (SELECT 1 FROM chunks c WHERE c.meeting_id=t.meeting_id)",
    );
    if filters.date_from.is_some() {
        sql.push_str(" AND m.created_at >= ?");
    }
    if filters.date_to.is_some() {
        sql.push_str(" AND m.created_at <= ?");
    }
    if !filters.meeting_ids.is_empty() {
        sql.push_str(&format!(
            " AND t.meeting_id IN ({})",
            vec!["?"; filters.meeting_ids.len()].join(",")
        ));
    }
    if !filters.collection_ids.is_empty() {
        sql.push_str(&format!(
            " AND t.meeting_id IN (SELECT meeting_id FROM meeting_collections WHERE collection_id IN ({}))",
            int_list(&filters.collection_ids)
        ));
    }
    if !filters.speaker_ids.is_empty() {
        sql.push_str(&format!(
            " AND t.speaker_id IN ({})",
            int_list(&filters.speaker_ids)
        ));
    }
    sql.push_str(" ORDER BY m.created_at DESC, t.audio_start_time LIMIT 2000");

    let mut query = sqlx::query_as::<_, (i64, String, String, i64, String)>(&sql);
    if let Some(from) = &filters.date_from {
        query = query.bind(from);
    }
    if let Some(to) = &filters.date_to {
        query = query.bind(to);
    }
    for meeting_id in &filters.meeting_ids {
        query = query.bind(meeting_id);
    }

    let mut hits: Vec<SearchHit> = query
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|(rowid, meeting_id, meeting_title, start_ms, text)| {
            let (body, matched_terms) =
                crate::search::query::concept_coverage(&plan.concepts, &text);
            let (title, _) = crate::search::query::concept_coverage(&plan.concepts, &meeting_title);
            let lexical_score = body.max(title * 0.9);
            (lexical_score >= fuzzy_candidate_floor(plan.concepts.len())).then_some(SearchHit {
                chunk_id: -rowid,
                meeting_id,
                meeting_title,
                start_ms,
                text,
                score: lexical_score,
                lexical_score,
                semantic_score: 0.0,
                match_sources: vec!["transcript_fallback".to_string()],
                matched_terms,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.chunk_id.cmp(&b.chunk_id))
    });
    hits.truncate(BRANCH_LIMIT as usize);
    Ok(hits)
}

/// Avoid letting one long meeting consume the whole RAG context. If the search only
/// found one meeting, keep all of its best fragments; otherwise cap each at three.
pub(crate) fn diversify_by_meeting(hits: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    use std::collections::{HashMap, HashSet};
    let meeting_count = hits
        .iter()
        .map(|hit| hit.meeting_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    let per_meeting = if meeting_count <= 1 { limit } else { 3 };
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut output = Vec::with_capacity(limit);
    for hit in hits {
        let count = counts.entry(hit.meeting_id.clone()).or_default();
        if *count >= per_meeting {
            continue;
        }
        *count += 1;
        output.push(hit);
        if output.len() == limit {
            break;
        }
    }
    output
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

    #[test]
    fn fuzzy_scoring_candidate_budget_is_bounded_and_preserves_ranked_hits() {
        let allowed: std::collections::HashSet<i64> = (1..=2_000).collect();
        let required: std::collections::HashSet<i64> = [1, 25, 900].into_iter().collect();
        let selected = bounded_fuzzy_candidate_ids(Some(&allowed), &required);
        assert_eq!(selected.len(), MAX_FUZZY_CANDIDATES);
        assert!(required.is_subset(&selected));
        assert!(selected.contains(&2_000));
    }

    #[tokio::test]
    async fn scoped_search_is_not_crowded_out_by_global_top_k() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, indexing_allowed INTEGER NOT NULL DEFAULT 1)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_start_time REAL, speaker_id INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE chunks_fts USING fts5(text)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings(id,title,created_at) VALUES('outside','Outside','2026-01-01'),('inside','Inside','2026-01-01')")
            .execute(&pool).await.unwrap();
        for id in 1..=25_i64 {
            sqlx::query("INSERT INTO chunks VALUES(?,'outside',0,'бюджет')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(?,'бюджет')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO chunks VALUES(99,'inside',0,'бюджет')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(99,'бюджет')")
            .execute(&pool)
            .await
            .unwrap();
        let filters = SearchFilters {
            meeting_ids: vec!["inside".into()],
            ..Default::default()
        };
        let hits = HybridSearch::search(&pool, "бюджет", None, &filters, 5)
            .await
            .unwrap();
        assert_eq!(
            hits.iter().map(|hit| hit.chunk_id).collect::<Vec<_>>(),
            vec![99]
        );
    }

    #[tokio::test]
    async fn ordinary_search_does_not_apply_rag_diversification() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, indexing_allowed INTEGER NOT NULL DEFAULT 1)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_start_time REAL, speaker_id INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE chunks_fts USING fts5(text)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings VALUES('deep','Deep dive','2026-01-01',1),('brief','Brief','2026-01-02',1)")
            .execute(&pool).await.unwrap();
        for id in 1..=5_i64 {
            sqlx::query("INSERT INTO chunks VALUES(?,'deep',?,'бюджет проекта')")
                .bind(id)
                .bind(id * 1000)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(?,'бюджет проекта')")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO chunks VALUES(6,'brief',0,'бюджет проекта')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(6,'бюджет проекта')")
            .execute(&pool)
            .await
            .unwrap();

        let hits = HybridSearch::search(&pool, "бюджет", None, &SearchFilters::default(), 50)
            .await
            .unwrap();
        assert_eq!(hits.len(), 6);
        assert_eq!(
            hits.iter().filter(|hit| hit.meeting_id == "deep").count(),
            5
        );
    }

    #[tokio::test]
    async fn typo_heavy_cross_language_query_finds_product_history() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, indexing_allowed INTEGER NOT NULL DEFAULT 1)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_start_time REAL, speaker_id INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE chunks_fts USING fts5(text)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings VALUES('history','История продукта','2026-01-01',1),('noise','Бюджет','2026-01-02',1)")
            .execute(&pool).await.unwrap();
        let relevant = "Сначала приложение называлось Meetily. Затем продукт переименовали в Memento и обсудили направления развития и основные проблемы.";
        sqlx::query("INSERT INTO chunks VALUES(1,'history',12000,?)")
            .bind(relevant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chunks_fts(rowid,text) VALUES(1,?)")
            .bind(relevant)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO chunks VALUES(2,'noise',0,'Обсудили проблемы бюджета и офиса')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO chunks_fts(rowid,text) VALUES(2,'Обсудили проблемы бюджета и офиса')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let question = "собери историю как митили стало мементо и какеи направления развития и проблемы есть с ним";
        let hits = HybridSearch::search(&pool, question, None, &SearchFilters::default(), 5)
            .await
            .unwrap();
        assert_eq!(hits.first().map(|hit| hit.chunk_id), Some(1));
        assert!(
            hits[0].score >= 0.42,
            "unexpected evidence score: {}",
            hits[0].score
        );
        assert!(hits[0].matched_terms.iter().any(|term| term == "meetily"));
        let one_word_noise = hits.iter().find(|hit| hit.chunk_id == 2).unwrap();
        assert!(
            !crate::search::rag::retrieval_is_sufficient(std::slice::from_ref(one_word_noise)),
            "one shared concept must not ground a long question: {one_word_noise:?}"
        );
    }

    #[tokio::test]
    async fn unchunked_meeting_is_searchable_through_transcript_fallback() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, indexing_allowed INTEGER NOT NULL DEFAULT 1)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_start_time REAL, speaker_id INTEGER)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE VIRTUAL TABLE chunks_fts USING fts5(text)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO meetings VALUES('fresh','Новая встреча','2026-01-01',1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO transcripts VALUES('fresh','Решили перенести релиз на пятницу',42.5,NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = HybridSearch::search_planned(
            &pool,
            &crate::search::query::QueryPlan::build("когда перенесли релиз"),
            None,
            &SearchFilters::default(),
            5,
        )
        .await
        .unwrap();
        assert_eq!(result.hits.len(), 1);
        assert!(result.hits[0].chunk_id < 0);
        assert_eq!(result.hits[0].start_ms, 42_500);
        assert_eq!(result.diagnostics.transcript_fallback_hits, 1);
    }

    #[tokio::test]
    async fn privacy_filter_excludes_non_indexed_memories_without_other_filters() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, indexing_allowed INTEGER NOT NULL)",
        )
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
