//! RAG over the archive (PLAN.md Phase 4).
//!
//! Retrieval reuses the hybrid engine (top 12 within scope). This module owns the
//! grounding logic: building the numbered `[N]` context, the low-confidence guard
//! (never fabricate), and citation enforcement (answers must cite ≥1 source, else they
//! are regenerated once). All of that is pure and unit-tested; the LLM answer call is
//! wired via `crate::llm::guarded_complete` in the chat command.

use crate::llm::prompts::RAG_NOT_FOUND;
use crate::llm::router;
use crate::search::hybrid::{
    diversify_by_meeting, HybridSearch, SearchDiagnostics, SearchFilters, SearchHit,
};

/// Number of chunks fed to the answer prompt (PLAN.md Phase 4: top 12).
pub const RAG_TOP_K: usize = 12;
const RAG_CANDIDATE_K: usize = RAG_TOP_K * 5;

fn rag_system_prompt() -> String {
    format!(
        "Отвечай только на основе фрагментов и цитируй источники как [N]. \
         Если подтверждена лишь часть вопроса, ответь на подтверждённую часть и явно \
         перечисли, чего не хватает. Только если фрагменты не отвечают ни на одну \
         существенную часть, верни ровно: «{RAG_NOT_FOUND}»."
    )
}

/// Evidence thresholds after lexical/fuzzy/semantic scoring. RRF is retained only as
/// a ranking tie-breaker; it must never be treated as semantic confidence.
pub const MIN_CONFIDENCE: f64 = 0.25;
pub const STRONG_CONFIDENCE: f64 = 0.38;

pub fn retrieval_is_sufficient(hits: &[SearchHit]) -> bool {
    let Some(first) = hits.first() else {
        return false;
    };
    if first.score >= STRONG_CONFIDENCE {
        return true;
    }
    hits.iter()
        .filter(|hit| hit.score >= MIN_CONFIDENCE)
        .count()
        >= 2
}

/// A source the answer can cite. `index` is the 1-based `[N]` marker.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
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
        ctx.push_str(&format!(
            "[{n}] ({} · чанк {}) {}\n",
            h.meeting_title, h.chunk_id, h.text
        ));
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
pub fn evaluate_answer(answer: &str, top_score: f64, available: &[Citation]) -> AnswerVerdict {
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
        AnswerVerdict::Found {
            citations: resolved,
        }
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
            RagScope::Archive => (
                SearchFilters::default(),
                router::Scope::Archive,
                "archive".to_string(),
            ),
            RagScope::Collection(id) => (
                SearchFilters {
                    collection_ids: vec![*id],
                    ..Default::default()
                },
                router::Scope::Collection,
                format!("collection:{id}"),
            ),
            RagScope::Meeting(mid) => (
                SearchFilters {
                    meeting_ids: vec![mid.clone()],
                    ..Default::default()
                },
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
    pub diagnostics: RetrievalDiagnostics,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct RetrievalDiagnostics {
    /// `no_index` | `index_incomplete` | `no_relevant_evidence` |
    /// `answer_not_found` | `answer_ungrounded` | `ok`.
    pub reason: String,
    pub indexable_meetings: i64,
    pub indexed_meetings: i64,
    pub best_score: f64,
    pub query_rewritten: bool,
    pub semantic_available: bool,
    pub lexical_hits: usize,
    pub fuzzy_hits: usize,
    pub semantic_hits: usize,
    pub transcript_fallback_hits: usize,
    pub candidates_considered: usize,
}

impl RetrievalDiagnostics {
    fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.to_string();
        self
    }

    fn from_search(
        search: &SearchDiagnostics,
        indexable_meetings: i64,
        indexed_meetings: i64,
        best_score: f64,
        sufficient: bool,
    ) -> Self {
        let reason = if sufficient {
            "ok"
        } else if indexable_meetings == 0 {
            "no_index"
        } else if indexed_meetings < indexable_meetings {
            "index_incomplete"
        } else {
            "no_relevant_evidence"
        };
        Self {
            reason: reason.to_string(),
            indexable_meetings,
            indexed_meetings,
            best_score,
            query_rewritten: search.query_rewritten,
            semantic_available: search.semantic_available,
            lexical_hits: search.lexical_hits,
            fuzzy_hits: search.fuzzy_hits,
            semantic_hits: search.semantic_hits,
            transcript_fallback_hits: search.transcript_fallback_hits,
            candidates_considered: search.candidates_considered,
        }
    }
}

/// Single-meeting chat: answer over the full transcript + saved summary in context
/// (no retrieval, no "not found" grounding gate). Used for `RagScope::Meeting`, where
/// the whole meeting fits in the model window and the user is chatting *about this
/// meeting* rather than searching the archive.
async fn ask_single_meeting(
    pool: &sqlx::SqlitePool,
    query: &str,
    meeting_id: &str,
    history: &[(String, String)],
) -> Result<RagAnswer, String> {
    use crate::llm::router::Scope;
    use crate::llm::{complete_routed, Purpose};

    // Full transcript for the meeting, ordered by time.
    let rows = sqlx::query_as::<_, (String, Option<f64>, Option<String>)>(
        "SELECT transcript, audio_start_time, speaker FROM transcripts \
         WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("failed to load transcript: {e}"))?;

    let mut transcript = String::new();
    for (text, start, speaker) in rows {
        let ts = start
            .map(|s| {
                let s = s.max(0.0) as u64;
                format!("[{:02}:{:02}] ", s / 60, s % 60)
            })
            .unwrap_or_default();
        let who = speaker
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| format!("{s}: "))
            .unwrap_or_default();
        transcript.push_str(&format!("{ts}{who}{}\n", text.trim()));
    }

    // Guard the context window on very long meetings (keep the beginning).
    const MAX_TRANSCRIPT_CHARS: usize = 60_000;
    if transcript.chars().count() > MAX_TRANSCRIPT_CHARS {
        transcript = transcript.chars().take(MAX_TRANSCRIPT_CHARS).collect();
        transcript.push_str("\n[…транскрипт сокращён…]");
    }

    // Saved summary (best-effort; optional).
    let summary_block = match crate::database::repositories::summary::SummaryProcessesRepository::get_summary_data_for_meeting(pool, meeting_id).await {
        Ok(Some(process)) => process
            .result
            .or(process.result_backup)
            .map(|s| format!("\n\nСаммари встречи:\n{s}"))
            .unwrap_or_default(),
        _ => String::new(),
    };

    // Last-6-turns history (kept consistent with the retrieval path).
    let mut history_block = String::new();
    if !history.is_empty() {
        history_block.push_str("\n\nИстория диалога:\n");
        let start = history.len().saturating_sub(6);
        for (role, content) in &history[start..] {
            history_block.push_str(&format!("{role}: {content}\n"));
        }
    }

    let system = "Ты — ассистент по встрече. Отвечай на вопросы пользователя, опираясь на \
        транскрипт встречи и её саммари ниже. Отвечай на русском, кратко и по делу. Если в \
        транскрипте и саммари действительно нет нужной информации, честно скажи об этом.";
    let user = format!(
        "Транскрипт встречи:\n{transcript}{summary_block}{history_block}\n\nВопрос: {query}"
    );

    let raw = complete_routed(
        pool,
        Purpose::Chat,
        Scope::SingleMeeting,
        query.len(),
        system,
        &user,
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(RagAnswer {
        answer: raw,
        citations: vec![],
        found: true,
        warning: None,
        diagnostics: RetrievalDiagnostics {
            reason: "ok".to_string(),
            ..Default::default()
        },
    })
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

    // Single-meeting chat answers over the FULL transcript + saved summary in the
    // model context (the whole meeting fits) rather than retrieval — so questions
    // like "how many people were in the meeting" are answerable and there is no
    // "not found" grounding gate. Archive/collection scopes keep RAG retrieval.
    if let RagScope::Meeting(meeting_id) = scope {
        return ask_single_meeting(pool, query, meeting_id, history).await;
    }

    let (filters, router_scope, _label) = scope.resolve();

    let mut plan = crate::search::query::QueryPlan::build(query);
    if let Err(error) = plan.enrich_from_confirmed_terminology(pool).await {
        log::warn!("could not load confirmed terminology for RAG retrieval: {error}");
    }
    let mut search = {
        let _model_index_guard = crate::pipeline::embedder::model_index_read_guard().await;
        // Embed the deterministic corrected form. The original question is still sent
        // to the answer model; normalization only improves local retrieval.
        let query_embedding = crate::pipeline::embedder::embed_query(plan.semantic_query.clone())
            .await
            .and_then(|r| r.ok());
        HybridSearch::search_planned(
            pool,
            &plan,
            query_embedding.as_deref(),
            &filters,
            RAG_CANDIDATE_K,
        )
        .await
        .map_err(|e| format!("retrieval failed: {e}"))?
    };
    search.hits = diversify_by_meeting(std::mem::take(&mut search.hits), RAG_TOP_K);
    let (indexable_meetings, indexed_meetings) = index_health(pool, &filters)
        .await
        .map_err(|error| format!("index health failed: {error}"))?;
    let sufficient = retrieval_is_sufficient(&search.hits);
    let top_score = search.hits.first().map(|hit| hit.score).unwrap_or(0.0);
    let diagnostics = RetrievalDiagnostics::from_search(
        &search.diagnostics,
        indexable_meetings,
        indexed_meetings,
        top_score,
        sufficient,
    );
    if !sufficient {
        // Empty or low-confidence retrieval → don't call the LLM at all.
        return Ok(RagAnswer {
            answer: NOT_FOUND_MESSAGE.to_string(),
            citations: vec![],
            found: false,
            warning: None,
            diagnostics,
        });
    }
    let (context, citations) = build_context(&search.hits);

    // Last-6-turns history (PLAN.md Phase 4 context management) prepended to the prompt.
    let mut history_block = String::new();
    if !history.is_empty() {
        history_block.push_str("\n\nИстория диалога:\n");
        let start = history.len().saturating_sub(6);
        for (role, content) in &history[start..] {
            history_block.push_str(&format!("{role}: {content}\n"));
        }
    }
    let user = prompts::fill(
        prompts::rag_answer_v2(),
        &[("question", query), ("context", &context)],
    ) + &history_block;
    let system = rag_system_prompt();
    let qchars = query.len();

    let raw = complete_routed(pool, Purpose::Chat, router_scope, qchars, &system, &user)
        .await
        .map_err(|e| e.to_string())?;

    match evaluate_answer(&raw, top_score, &citations) {
        AnswerVerdict::NotFound => Ok(RagAnswer {
            answer: NOT_FOUND_MESSAGE.to_string(),
            citations: vec![],
            found: false,
            warning: None,
            diagnostics: diagnostics.with_reason("answer_not_found"),
        }),
        AnswerVerdict::Found { citations: cited } => Ok(RagAnswer {
            answer: raw,
            citations: cited,
            found: true,
            warning: None,
            diagnostics,
        }),
        AnswerVerdict::NeedsCitation => {
            // Reject + regenerate once (PLAN.md Phase 4), then accept with a warning.
            let raw2 = complete_routed(pool, Purpose::Chat, router_scope, qchars, &system, &user)
                .await
                .map_err(|e| e.to_string())?;
            match evaluate_answer(&raw2, top_score, &citations) {
                AnswerVerdict::Found { citations: cited } => Ok(RagAnswer {
                    answer: raw2,
                    citations: cited,
                    found: true,
                    warning: None,
                    diagnostics,
                }),
                AnswerVerdict::NotFound => Ok(RagAnswer {
                    answer: NOT_FOUND_MESSAGE.to_string(),
                    citations: vec![],
                    found: false,
                    warning: None,
                    diagnostics: diagnostics.with_reason("answer_not_found"),
                }),
                AnswerVerdict::NeedsCitation => {
                    // A fluent answer without a resolvable source is not a successful
                    // knowledge-base answer. Fail closed after the single retry.
                    let mut diagnostics = diagnostics;
                    diagnostics.reason = "answer_ungrounded".to_string();
                    Ok(RagAnswer {
                        answer: NOT_FOUND_MESSAGE.to_string(),
                        citations: vec![],
                        found: false,
                        warning: Some(
                            "Модель не смогла подтвердить ответ ссылками на записи.".to_string(),
                        ),
                        diagnostics,
                    })
                }
            }
        }
    }
}

async fn index_health(
    pool: &sqlx::SqlitePool,
    filters: &SearchFilters,
) -> Result<(i64, i64), sqlx::Error> {
    let mut predicate = String::from(
        "m.indexing_allowed=1 AND EXISTS (SELECT 1 FROM transcripts t \
         WHERE t.meeting_id=m.id AND length(trim(t.transcript)) > 0)",
    );
    if filters.date_from.is_some() {
        predicate.push_str(" AND m.created_at >= ?");
    }
    if filters.date_to.is_some() {
        predicate.push_str(" AND m.created_at <= ?");
    }
    if !filters.meeting_ids.is_empty() {
        predicate.push_str(&format!(
            " AND m.id IN ({})",
            vec!["?"; filters.meeting_ids.len()].join(",")
        ));
    }
    if !filters.collection_ids.is_empty() {
        let ids = filters
            .collection_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        predicate.push_str(&format!(
            " AND m.id IN (SELECT meeting_id FROM meeting_collections WHERE collection_id IN ({ids}))"
        ));
    }
    if !filters.speaker_ids.is_empty() {
        let speakers = filters
            .speaker_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        predicate.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM transcripts st WHERE st.meeting_id=m.id \
             AND st.speaker_id IN ({speakers}))"
        ));
    }

    async fn count_with_bindings(
        pool: &sqlx::SqlitePool,
        sql: &str,
        filters: &SearchFilters,
    ) -> Result<i64, sqlx::Error> {
        let mut query = sqlx::query_scalar::<_, i64>(sql);
        if let Some(from) = &filters.date_from {
            query = query.bind(from);
        }
        if let Some(to) = &filters.date_to {
            query = query.bind(to);
        }
        for meeting_id in &filters.meeting_ids {
            query = query.bind(meeting_id);
        }
        query.fetch_one(pool).await
    }

    let indexable = count_with_bindings(
        pool,
        &format!("SELECT COUNT(*) FROM meetings m WHERE {predicate}"),
        filters,
    )
    .await?;
    let indexed_scope = if filters.speaker_ids.is_empty() {
        "AND EXISTS (SELECT 1 FROM chunks c WHERE c.meeting_id=m.id)".to_string()
    } else {
        let speakers = filters
            .speaker_ids
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "AND EXISTS (SELECT 1 FROM chunks c WHERE c.meeting_id=m.id \
             AND EXISTS (SELECT 1 FROM transcripts st WHERE st.meeting_id=c.meeting_id \
             AND st.speaker_id IN ({speakers}) \
             AND CAST(st.audio_start_time * 1000 AS INTEGER) BETWEEN c.start_ms AND c.end_ms))"
        )
    };
    let indexed = count_with_bindings(
        pool,
        &format!("SELECT COUNT(*) FROM meetings m WHERE {predicate} {indexed_scope}"),
        filters,
    )
    .await?;
    Ok((indexable, indexed))
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
            score: 0.60,
            lexical_score: 0.60,
            semantic_score: 0.0,
            match_sources: vec!["keyword".into()],
            matched_terms: vec![],
        }
    }

    #[test]
    fn answer_model_not_found_has_a_distinct_diagnostic_reason() {
        let diagnostics = RetrievalDiagnostics {
            reason: "ok".into(),
            ..Default::default()
        }
        .with_reason("answer_not_found");
        assert_eq!(diagnostics.reason, "answer_not_found");
    }

    #[test]
    fn system_prompt_supports_partial_grounded_answers() {
        let prompt = rag_system_prompt();
        assert!(prompt.contains("подтверждённую часть"));
        assert!(prompt.contains(RAG_NOT_FOUND));
    }

    #[test]
    fn context_numbers_sources_from_one() {
        let (ctx, cites) =
            build_context(&[hit(10, "Standup", "бюджет"), hit(20, "Review", "сроки")]);
        assert!(ctx.contains("[1] (Standup") && ctx.contains("[2] (Review"));
        assert_eq!(cites[0].index, 1);
        assert_eq!(cites[0].chunk_id, 10);
        assert_eq!(cites[1].start_ms, 20_000);
    }

    #[test]
    fn parse_citations_distinct_in_order() {
        assert_eq!(
            parse_citations("Решили X [2]. Также Y [1] и снова [2]."),
            vec![2, 1]
        );
        assert!(parse_citations("нет ссылок").is_empty());
    }

    #[test]
    fn low_confidence_and_sentinel_are_not_found() {
        let cites = vec![Citation {
            index: 1,
            chunk_id: 1,
            meeting_id: "m1".into(),
            start_ms: 0,
        }];
        // top score below floor -> NotFound even with sources present
        assert_eq!(
            evaluate_answer("ответ [1]", 0.0001, &cites),
            AnswerVerdict::NotFound
        );
        // sentinel -> NotFound
        assert_eq!(
            evaluate_answer(RAG_NOT_FOUND, 0.50, &cites),
            AnswerVerdict::NotFound
        );
        // no sources -> NotFound
        assert_eq!(
            evaluate_answer("ответ [1]", 0.50, &[]),
            AnswerVerdict::NotFound
        );
    }

    #[test]
    fn retrieval_gate_rejects_empty_and_low_confidence_hits() {
        assert!(!retrieval_is_sufficient(&[]));
        let mut low = hit(1, "Noise", "случайный фрагмент");
        low.score = MIN_CONFIDENCE / 2.0;
        low.lexical_score = MIN_CONFIDENCE / 2.0;
        assert!(!retrieval_is_sufficient(&[low]));
        assert!(retrieval_is_sufficient(&[hit(
            2,
            "Relevant",
            "проект альфа"
        )]));

        let mut one_word_from_long_question = hit(3, "Noise", "развитие бюджета");
        one_word_from_long_question.score = 0.125;
        one_word_from_long_question.lexical_score = 0.125;
        assert!(!retrieval_is_sufficient(&[one_word_from_long_question]));
    }

    #[tokio::test]
    async fn index_health_respects_speaker_scope_and_chunk_overlap() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, created_at TEXT, indexing_allowed INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE transcripts(meeting_id TEXT, transcript TEXT, audio_start_time REAL, speaker_id INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, end_ms INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO meetings VALUES('ready','2026-01-01',1),('pending','2026-01-02',1),('other','2026-01-03',1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts VALUES \
             ('ready','alpha',5.0,7),('pending','beta',3.0,7),('other','gamma',2.0,9)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO chunks VALUES(1,'ready',0,10000),(2,'other',0,10000)")
            .execute(&pool)
            .await
            .unwrap();

        let speaker_seven = SearchFilters {
            speaker_ids: vec![7],
            ..Default::default()
        };
        assert_eq!(index_health(&pool, &speaker_seven).await.unwrap(), (2, 1));

        let absent_speaker = SearchFilters {
            speaker_ids: vec![42],
            ..Default::default()
        };
        assert_eq!(index_health(&pool, &absent_speaker).await.unwrap(), (0, 0));
    }

    #[test]
    fn uncited_answer_needs_regeneration() {
        let cites = vec![Citation {
            index: 1,
            chunk_id: 1,
            meeting_id: "m1".into(),
            start_ms: 0,
        }];
        assert_eq!(
            evaluate_answer("ответ без ссылок", 0.50, &cites),
            AnswerVerdict::NeedsCitation
        );
    }

    #[test]
    fn valid_cited_answer_is_found_with_resolved_citations() {
        let cites = vec![
            Citation {
                index: 1,
                chunk_id: 11,
                meeting_id: "m1".into(),
                start_ms: 0,
            },
            Citation {
                index: 2,
                chunk_id: 22,
                meeting_id: "m2".into(),
                start_ms: 5000,
            },
        ];
        match evaluate_answer("Мы решили X [2].", 0.50, &cites) {
            AnswerVerdict::Found { citations } => {
                assert_eq!(citations.len(), 1);
                assert_eq!(citations[0].chunk_id, 22);
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }
}
