//! RAG over the archive (PLAN.md Phase 4).
//!
//! Retrieval reuses the hybrid engine (top 12 within scope). This module owns the
//! grounding logic: building the numbered `[N]` context, the low-confidence guard
//! (never fabricate), and citation enforcement (answers must cite ≥1 source, else they
//! are regenerated once). All of that is pure and unit-tested; the LLM answer call is
//! wired via `crate::llm::guarded_complete` in the chat command.

use crate::llm::prompts::RAG_NOT_FOUND;
use crate::llm::router;
use crate::search::hybrid::{HybridSearch, SearchFilters, SearchHit};

/// Number of chunks fed to the answer prompt (PLAN.md Phase 4: top 12).
pub const RAG_TOP_K: usize = 12;

/// Low-confidence floor on the top fused RRF score. Below this we return "not found"
/// rather than prompting the model (avoids grounding on noise). Tunable; the default
/// requires the best hit to land within ~top-10 of at least one branch.
pub const MIN_CONFIDENCE: f64 = 1.0 / (crate::search::hybrid::DEFAULT_RRF_K + 10.0);

/// A source the answer can cite. `index` is the 1-based `[N]` marker.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Citation {
    pub index: usize,
    pub chunk_id: i64,
    pub meeting_id: String,
    pub start_ms: i64,
}

/// Build the numbered context block + citation table from retrieved hits.
pub fn build_context(hits: &[SearchHit]) -> (String, Vec<Citation>) {
    let mut ctx = String::new();
    let mut citations = Vec::with_capacity(hits.len());
    for (i, h) in hits.iter().enumerate() {
        let n = i + 1;
        ctx.push_str(&format!("[{n}] ({} · чанк {}) {}\n", h.meeting_title, h.chunk_id, h.text));
        citations.push(Citation {
            index: n,
            chunk_id: h.chunk_id,
            meeting_id: h.meeting_id.clone(),
            start_ms: h.start_ms,
        });
    }
    (ctx, citations)
}

/// Extract the distinct `[N]` citation indices referenced in an answer, in order.
pub fn parse_citations(answer: &str) -> Vec<usize> {
    use once_cell::sync::Lazy;
    use regex::Regex;
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[(\d+)\]").unwrap());
    let mut seen = std::collections::HashSet::new();
    RE.captures_iter(answer)
        .filter_map(|c| c[1].parse::<usize>().ok())
        .filter(|n| seen.insert(*n))
        .collect()
}

/// Verdict for a generated answer.
#[derive(Debug, PartialEq)]
pub enum AnswerVerdict {
    /// Grounded answer with its resolved citations (only markers that map to a source).
    Found { citations: Vec<Citation> },
    /// Guard tripped (low confidence or the sentinel) — show "not found", never fabricate.
    NotFound,
    /// Answer had no valid citation — caller regenerates once, then warns.
    NeedsCitation,
}

/// Apply the grounding guards to a generated `answer`. `top_score` is the best fused RRF
/// score from retrieval; `available` are the citations from [`build_context`].
pub fn evaluate_answer(
    answer: &str,
    top_score: f64,
    available: &[Citation],
) -> AnswerVerdict {
    if available.is_empty() || top_score < MIN_CONFIDENCE {
        return AnswerVerdict::NotFound;
    }
    if answer.trim().eq_ignore_ascii_case(RAG_NOT_FOUND) || answer.contains(RAG_NOT_FOUND) {
        return AnswerVerdict::NotFound;
    }
    let cited = parse_citations(answer);
    let resolved: Vec<Citation> = cited
        .into_iter()
        .filter_map(|n| available.iter().find(|c| c.index == n).cloned())
        .collect();
    if resolved.is_empty() {
        AnswerVerdict::NeedsCitation
    } else {
        AnswerVerdict::Found { citations: resolved }
    }
}

/// User-facing "not found" message (distinct from the model sentinel `RAG_NOT_FOUND`).
pub const NOT_FOUND_MESSAGE: &str = "В ваших записях ответ не найден.";

/// Retrieval scope for a RAG question.
#[derive(Debug, Clone)]
pub enum RagScope {
    Archive,
    Collection(i64),
    Meeting(String),
}

impl RagScope {
    /// Map to (retrieval filters, router scope, a stable label for chat_sessions.scope).
    fn resolve(&self) -> (SearchFilters, router::Scope, String) {
        match self {
            RagScope::Archive => (SearchFilters::default(), router::Scope::Archive, "archive".to_string()),
            RagScope::Collection(id) => (
                SearchFilters { collection_ids: vec![*id], ..Default::default() },
                router::Scope::Collection,
                format!("collection:{id}"),
            ),
            RagScope::Meeting(mid) => (
                SearchFilters { meeting_ids: vec![mid.clone()], ..Default::default() },
                router::Scope::SingleMeeting,
                format!("meeting:{mid}"),
            ),
        }
    }

    pub fn label(&self) -> String {
        self.resolve().2
    }
}

/// A grounded answer for the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RagAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub found: bool,
    /// Set when the answer was accepted despite missing citations (shown as a warning).
    pub warning: Option<String>,
}

/// Answer a question over the archive (PLAN.md Phase 4), wired the same way as `extract`:
/// retrieve (hybrid, scoped) → build `[N]` context → `complete_routed` (privacy-guarded,
/// GigaChat/DeepSeek via the router) → grounding guards (low-confidence, cite-or-regen).
/// Retrieval currently uses the FTS branch (no query embedding yet); the vector branch
/// turns on automatically once the embedder provides query vectors.
pub async fn ask(
    pool: &sqlx::SqlitePool,
    query: &str,
    scope: &RagScope,
    history: &[(String, String)],
) -> Result<RagAnswer, String> {
    use crate::llm::{complete_routed, prompts, Purpose};

    let (filters, router_scope, _label) = scope.resolve();

    // Embed the question for the vector branch (None → FTS-only when no model is loaded).
    let query_embedding = crate::pipeline::embedder::embed_query(query.to_string())
        .await
        .and_then(|r| r.ok());
    let hits = HybridSearch::search(pool, query, query_embedding.as_deref(), &filters, RAG_TOP_K)
        .await
        .map_err(|e| format!("retrieval failed: {e}"))?;
    if hits.is_empty() {
        // Nothing retrieved → don't call the LLM at all.
        return Ok(RagAnswer { answer: NOT_FOUND_MESSAGE.to_string(), citations: vec![], found: false, warning: None });
    }
    let top_score = hits.first().map(|h| h.score).unwrap_or(0.0);
    let (context, citations) = build_context(&hits);

    // Last-6-turns history (PLAN.md Phase 4 context management) prepended to the prompt.
    let mut history_block = String::new();
    if !history.is_empty() {
        history_block.push_str("\n\nИстория диалога:\n");
        let start = history.len().saturating_sub(6);
        for (role, content) in &history[start..] {
            history_block.push_str(&format!("{role}: {content}\n"));
        }
    }
    let user = prompts::fill(prompts::rag_answer_v1(), &[("question", query), ("context", &context)])
        + &history_block;
    let system = "Отвечай только на основе фрагментов. Цитируй источники как [N]. \
                  Если ответа нет — верни «в записях не найдено».";
    let qchars = query.len();

    let raw = complete_routed(pool, Purpose::Chat, router_scope, qchars, system, &user)
        .await
        .map_err(|e| e.to_string())?;

    match evaluate_answer(&raw, top_score, &citations) {
        AnswerVerdict::NotFound => {
            Ok(RagAnswer { answer: NOT_FOUND_MESSAGE.to_string(), citations: vec![], found: false, warning: None })
        }
        AnswerVerdict::Found { citations: cited } => {
            Ok(RagAnswer { answer: raw, citations: cited, found: true, warning: None })
        }
        AnswerVerdict::NeedsCitation => {
            // Reject + regenerate once (PLAN.md Phase 4), then accept with a warning.
            let raw2 = complete_routed(pool, Purpose::Chat, router_scope, qchars, system, &user)
                .await
                .map_err(|e| e.to_string())?;
            match evaluate_answer(&raw2, top_score, &citations) {
                AnswerVerdict::Found { citations: cited } => {
                    Ok(RagAnswer { answer: raw2, citations: cited, found: true, warning: None })
                }
                AnswerVerdict::NotFound => {
                    Ok(RagAnswer { answer: NOT_FOUND_MESSAGE.to_string(), citations: vec![], found: false, warning: None })
                }
                AnswerVerdict::NeedsCitation => Ok(RagAnswer {
                    answer: raw2,
                    citations: vec![],
                    found: true,
                    warning: Some("ответ без ссылок на источники".to_string()),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(chunk_id: i64, title: &str, text: &str) -> SearchHit {
        SearchHit {
            chunk_id,
            meeting_id: format!("m{chunk_id}"),
            meeting_title: title.into(),
            start_ms: chunk_id * 1000,
            text: text.into(),
            score: 0.05,
            matched_terms: vec![],
        }
    }

    #[test]
    fn context_numbers_sources_from_one() {
        let (ctx, cites) = build_context(&[hit(10, "Standup", "бюджет"), hit(20, "Review", "сроки")]);
        assert!(ctx.contains("[1] (Standup") && ctx.contains("[2] (Review"));
        assert_eq!(cites[0].index, 1);
        assert_eq!(cites[0].chunk_id, 10);
        assert_eq!(cites[1].start_ms, 20_000);
    }

    #[test]
    fn parse_citations_distinct_in_order() {
        assert_eq!(parse_citations("Решили X [2]. Также Y [1] и снова [2]."), vec![2, 1]);
        assert!(parse_citations("нет ссылок").is_empty());
    }

    #[test]
    fn low_confidence_and_sentinel_are_not_found() {
        let cites = vec![Citation { index: 1, chunk_id: 1, meeting_id: "m1".into(), start_ms: 0 }];
        // top score below floor -> NotFound even with sources present
        assert_eq!(evaluate_answer("ответ [1]", 0.0001, &cites), AnswerVerdict::NotFound);
        // sentinel -> NotFound
        assert_eq!(evaluate_answer(RAG_NOT_FOUND, 0.05, &cites), AnswerVerdict::NotFound);
        // no sources -> NotFound
        assert_eq!(evaluate_answer("ответ [1]", 0.05, &[]), AnswerVerdict::NotFound);
    }

    #[test]
    fn uncited_answer_needs_regeneration() {
        let cites = vec![Citation { index: 1, chunk_id: 1, meeting_id: "m1".into(), start_ms: 0 }];
        assert_eq!(evaluate_answer("ответ без ссылок", 0.05, &cites), AnswerVerdict::NeedsCitation);
    }

    #[test]
    fn valid_cited_answer_is_found_with_resolved_citations() {
        let cites = vec![
            Citation { index: 1, chunk_id: 11, meeting_id: "m1".into(), start_ms: 0 },
            Citation { index: 2, chunk_id: 22, meeting_id: "m2".into(), start_ms: 5000 },
        ];
        match evaluate_answer("Мы решили X [2].", 0.05, &cites) {
            AnswerVerdict::Found { citations } => {
                assert_eq!(citations.len(), 1);
                assert_eq!(citations[0].chunk_id, 22);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }
}
