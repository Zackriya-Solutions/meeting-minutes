//! Deterministic query planning for Russian-first, code-switched meeting archives.
//!
//! Speech transcripts frequently contain transliterated product names, inflections and
//! ASR misspellings.  FTS5 alone only understands exact tokens, while sending the raw,
//! typo-heavy sentence to an embedder can also reduce semantic recall.  This module
//! creates one shared query plan for lexical, fuzzy and semantic retrieval without
//! sending the user's archive or question to another service.

use std::collections::HashSet;

const MAX_CONFIRMED_TERMS: i64 = 500;
const MAX_ALIASES_PER_TERM: i64 = 16;

/// A group of spellings which represent one query concept.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryConcept {
    pub original: String,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan {
    /// Content-bearing concepts used by lexical and fuzzy retrieval.
    pub concepts: Vec<QueryConcept>,
    /// Corrected form used for semantic embedding. Stop words are retained because
    /// sentence encoders benefit from natural syntax.
    pub semantic_query: String,
    /// Whether an ASR/transliteration alias changed the semantic query.
    pub rewritten: bool,
}

impl QueryPlan {
    pub fn build(query: &str) -> Self {
        let raw_terms = tokenize(query);
        let mut rewritten = false;
        let mut semantic_terms: Vec<String> = raw_terms
            .iter()
            .map(|term| {
                if let Some((canonical, _)) = known_aliases(term) {
                    rewritten |= canonical != term;
                    canonical.to_string()
                } else {
                    term.clone()
                }
            })
            .collect();

        // Evaluation questions rarely reuse the user's exact wording in a meeting.
        // Add a small deterministic vocabulary of observable signals so both semantic
        // and lexical retrieval can find evidence such as blockers or lack of follow-
        // through for a question phrased as "кто самый неэффективный".
        for term in &raw_terms {
            if let Some(related) = related_evidence_terms(term) {
                rewritten = true;
                for value in related {
                    if !semantic_terms.iter().any(|term| term == value) {
                        semantic_terms.push((*value).to_string());
                    }
                }
            }
        }

        let mut seen = HashSet::new();
        let mut concepts = Vec::new();
        for term in raw_terms {
            if is_stopword(&term) || term.chars().count() < 3 {
                continue;
            }
            let (original, mut variants) = match known_aliases(&term) {
                Some((canonical, aliases)) => (
                    canonical.to_string(),
                    aliases.iter().map(|value| (*value).to_string()).collect(),
                ),
                None => (term.clone(), vec![term.clone()]),
            };
            if let Some(transliterated) = transliteration_variant(&term) {
                variants.push(transliterated);
            }
            if let Some(related) = related_evidence_terms(&term) {
                variants.extend(related.iter().map(|value| (*value).to_string()));
            }
            variants.sort();
            variants.dedup();
            if seen.insert(original.clone()) {
                concepts.push(QueryConcept { original, variants });
            }
        }

        Self {
            concepts,
            semantic_query: semantic_terms.join(" "),
            rewritten,
        }
    }

    /// Safe FTS tokens. Aliases are included to bridge Latin/Cyrillic product names,
    /// but bounded so a long natural-language question cannot create a huge MATCH AST.
    pub fn expanded_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for concept in &self.concepts {
            for variant in &concept.variants {
                if !terms.contains(variant) {
                    terms.push(variant.clone());
                }
                if terms.len() >= 32 {
                    return terms;
                }
            }
        }
        terms
    }

    pub fn primary_terms(&self) -> Vec<String> {
        self.concepts
            .iter()
            .map(|concept| concept.original.clone())
            .take(16)
            .collect()
    }

    /// Add only user-confirmed terminology aliases. This connects the existing local
    /// learning loop to retrieval: corrections improve future search without training
    /// a model or trusting unreviewed model guesses.
    pub async fn enrich_from_confirmed_terminology(
        &mut self,
        pool: &sqlx::SqlitePool,
    ) -> Result<(), sqlx::Error> {
        let rows = confirmed_terminology_rows(pool).await?;

        let mut glossary: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (canonical, alias) in rows {
            let canonical_terms = tokenize(&canonical);
            if canonical_terms.is_empty() {
                continue;
            }
            let variants = glossary.entry(canonical).or_default();
            variants.extend(canonical_terms);
            if let Some(alias) = alias {
                variants.extend(tokenize(&alias));
            }
            variants.sort();
            variants.dedup();
        }

        for concept in &mut self.concepts {
            for (canonical, variants) in &glossary {
                let matches_confirmed_term = concept.variants.iter().any(|query_variant| {
                    variants
                        .iter()
                        .any(|known| token_similarity(query_variant, known) >= 0.88)
                });
                if matches_confirmed_term {
                    for variant in variants {
                        if !concept.variants.contains(variant) {
                            concept.variants.push(variant.clone());
                            self.rewritten = true;
                        }
                    }
                    if !self.semantic_query.contains(canonical.as_str()) {
                        self.semantic_query.push(' ');
                        self.semantic_query.push_str(canonical);
                    }
                }
            }
            concept.variants.sort();
            concept.variants.dedup();
        }
        Ok(())
    }
}

async fn confirmed_terminology_rows(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    sqlx::query_as(
        "WITH selected_terms AS ( \
             SELECT id, canonical, support_count, last_seen_at \
             FROM terminology_terms WHERE status='confirmed' \
             ORDER BY support_count DESC, last_seen_at DESC, id LIMIT ? \
         ), ranked_aliases AS ( \
             SELECT term_id, alias, ROW_NUMBER() OVER ( \
                 PARTITION BY term_id \
                 ORDER BY support_count DESC, created_at DESC, id \
             ) AS alias_rank \
             FROM terminology_aliases \
             WHERE status='confirmed' AND term_id IN (SELECT id FROM selected_terms) \
         ) \
         SELECT st.canonical, ra.alias FROM selected_terms st \
         LEFT JOIN ranked_aliases ra \
           ON ra.term_id=st.id AND ra.alias_rank <= ? \
         ORDER BY st.support_count DESC, st.last_seen_at DESC, st.id, ra.alias_rank",
    )
    .bind(MAX_CONFIRMED_TERMS)
    .bind(MAX_ALIASES_PER_TERM)
    .fetch_all(pool)
    .await
}

pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('ё', "е")
        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .map(|term| term.trim_matches(['-', '_']).to_string())
        .filter(|term| !term.is_empty())
        .collect()
}

/// Score how completely the document represents the query concepts. Exact/prefix
/// matches win; edit-distance similarity is only allowed for longer words so short
/// Russian words do not create noisy fallback hits.
pub fn concept_coverage(concepts: &[QueryConcept], text: &str) -> (f64, Vec<String>) {
    if concepts.is_empty() {
        return (0.0, Vec::new());
    }
    let document_terms = tokenize(text);
    let mut total = 0.0;
    let mut matched = Vec::new();
    for concept in concepts {
        let best = concept
            .variants
            .iter()
            .flat_map(|variant| {
                document_terms
                    .iter()
                    .map(move |document| token_similarity(variant, document))
            })
            .fold(0.0_f64, f64::max);
        if best >= 0.72 {
            total += best;
            matched.push(concept.original.clone());
        }
    }
    (total / concepts.len() as f64, matched)
}

fn token_similarity(left: &str, right: &str) -> f64 {
    if left == right {
        return 1.0;
    }
    let left_len = left.chars().count();
    let right_len = right.chars().count();
    if left_len >= 5 && right_len >= 5 && (left.starts_with(right) || right.starts_with(left)) {
        return 0.9;
    }
    if left_len < 5 || right_len < 5 || left_len.abs_diff(right_len) > 3 {
        return 0.0;
    }
    let levenshtein = strsim::normalized_levenshtein(left, right);
    let jaro = strsim::jaro_winkler(left, right);
    levenshtein.max(jaro)
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "а" | "без"
            | "бы"
            | "был"
            | "была"
            | "были"
            | "было"
            | "в"
            | "вам"
            | "вас"
            | "весь"
            | "во"
            | "вот"
            | "все"
            | "всего"
            | "вы"
            | "где"
            | "да"
            | "для"
            | "до"
            | "его"
            | "ее"
            | "если"
            | "есть"
            | "еще"
            | "же"
            | "за"
            | "здесь"
            | "и"
            | "из"
            | "или"
            | "им"
            | "их"
            | "к"
            | "как"
            | "какой"
            | "какая"
            | "какие"
            | "когда"
            | "который"
            | "ли"
            | "мне"
            | "может"
            | "мы"
            | "на"
            | "над"
            | "не"
            | "него"
            | "нет"
            | "но"
            | "ну"
            | "о"
            | "об"
            | "он"
            | "она"
            | "они"
            | "от"
            | "по"
            | "под"
            | "при"
            | "про"
            | "с"
            | "сейчас"
            | "себя"
            | "так"
            | "там"
            | "то"
            | "тоже"
            | "только"
            | "тут"
            | "ты"
            | "у"
            | "уже"
            | "что"
            | "чтобы"
            | "это"
            | "этот"
            | "я"
            | "about"
            | "and"
            | "are"
            | "for"
            | "from"
            | "how"
            | "in"
            | "is"
            | "of"
            | "on"
            | "the"
            | "to"
            | "was"
            | "were"
            | "what"
            | "with"
    )
}

/// Stable aliases for Memento's own identity are useful for the product-history use
/// case and cover the most common Russian ASR spellings seen in imported meetings.
/// Other code-switched words are handled by transliteration + fuzzy matching below.
fn known_aliases(term: &str) -> Option<(&'static str, &'static [&'static str])> {
    const MEMENTO: &[&str] = &["memento", "мементо", "мемента", "мементоу"];
    const MEETILY: &[&str] = &[
        "meetily",
        "митили",
        "мители",
        "метили",
        "митилы",
        "митилли",
        "meetly",
    ];
    if MEMENTO.contains(&term) {
        Some(("memento", MEMENTO))
    } else if MEETILY.contains(&term) {
        Some(("meetily", MEETILY))
    } else {
        None
    }
}

/// Related, observable evidence for common analytical questions. These are not
/// treated as facts about a person; they only widen retrieval so the answer model can
/// compare cited fragments and state the limits of an indirect conclusion.
fn related_evidence_terms(term: &str) -> Option<&'static [&'static str]> {
    const INEFFECTIVE: &[&str] = &[
        "пассивный",
        "безрезультатный",
        "задержка",
        "просрочка",
        "блокер",
        "проблема",
        "сорван",
        "не выполнено",
        "не решено",
    ];
    const EFFECTIVE: &[&str] = &[
        "результат",
        "решение",
        "договоренность",
        "выполнено",
        "завершено",
        "прогресс",
        "инициатива",
        "вовлеченность",
    ];
    const PASSIVE: &[&str] = &[
        "молчал",
        "не участвовал",
        "не ответил",
        "не высказался",
        "без инициативы",
        "мало говорил",
    ];
    const OVERLOADED: &[&str] = &[
        "нагрузка",
        "занят",
        "дедлайн",
        "не успеваю",
        "много задач",
        "переработка",
        "перегрузка",
    ];

    match term {
        "неэффективный" | "неэффективная" | "неэффективные" | "неэффективность" => {
            Some(INEFFECTIVE)
        }
        "эффективный" | "эффективная" | "эффективные" | "эффективность" => {
            Some(EFFECTIVE)
        }
        "пассивный" | "пассивная" | "пассивные" | "пассивность" => {
            Some(PASSIVE)
        }
        "перегруженный" | "перегруженная" | "перегруженные" | "перегруженность" => {
            Some(OVERLOADED)
        }
        _ => None,
    }
}

fn transliteration_variant(term: &str) -> Option<String> {
    if term.chars().all(|c| c.is_ascii_alphabetic()) {
        let mut value = term.to_string();
        for (latin, cyrillic) in [
            ("shch", "щ"),
            ("sch", "щ"),
            ("zh", "ж"),
            ("kh", "х"),
            ("ts", "ц"),
            ("ch", "ч"),
            ("sh", "ш"),
            ("yu", "ю"),
            ("ya", "я"),
            ("yo", "е"),
            ("ye", "е"),
        ] {
            value = value.replace(latin, cyrillic);
        }
        for (latin, cyrillic) in [
            ("a", "а"),
            ("b", "б"),
            ("c", "к"),
            ("d", "д"),
            ("e", "е"),
            ("f", "ф"),
            ("g", "г"),
            ("h", "х"),
            ("i", "и"),
            ("j", "й"),
            ("k", "к"),
            ("l", "л"),
            ("m", "м"),
            ("n", "н"),
            ("o", "о"),
            ("p", "п"),
            ("q", "к"),
            ("r", "р"),
            ("s", "с"),
            ("t", "т"),
            ("u", "у"),
            ("v", "в"),
            ("w", "в"),
            ("x", "кс"),
            ("y", "и"),
            ("z", "з"),
        ] {
            value = value.replace(latin, cyrillic);
        }
        (value != term).then_some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_typo_heavy_product_history_query() {
        let plan = QueryPlan::build(
            "собери историю о понимании как митили стало мементо и какеи направления развития и проблемы",
        );
        assert!(plan.rewritten);
        assert!(plan.semantic_query.contains("meetily стало memento"));
        assert!(!plan.primary_terms().contains(&"как".to_string()));
        // Content-bearing request verbs are not silently removed just to tune this
        // fixture. Relevance thresholds, not a query-specific stopword, handle them.
        assert!(plan.primary_terms().contains(&"собери".to_string()));
        let expanded = plan.expanded_terms();
        assert!(expanded.contains(&"meetily".to_string()));
        assert!(expanded.contains(&"митили".to_string()));
        assert!(expanded.contains(&"мементо".to_string()));
    }

    #[test]
    fn fuzzy_coverage_tolerates_asr_misspellings_but_not_unrelated_words() {
        let plan = QueryPlan::build("направления развития и проблемы");
        let (good, matched) = concept_coverage(
            &plan.concepts,
            "Обсудили направления дальнейшего развития продукта и основные проблеммы.",
        );
        let (bad, _) = concept_coverage(&plan.concepts, "Поговорили о бюджете и отпуске.");
        assert!(good > 0.8, "coverage={good}, matched={matched:?}");
        assert_eq!(bad, 0.0);
    }

    #[test]
    fn transliteration_supports_common_code_switching() {
        assert_eq!(
            transliteration_variant("pipeline").as_deref(),
            Some("пипелине")
        );
        let plan = QueryPlan::build("pipeline");
        assert!(plan.expanded_terms().iter().any(|term| term == "пипелине"));
    }

    #[test]
    fn evaluative_query_expands_to_observable_evidence() {
        let plan = QueryPlan::build("кто в команде самый неэффективный");
        let expanded = plan.expanded_terms();

        assert!(plan.rewritten);
        assert!(expanded.iter().any(|term| term == "блокер"));
        assert!(expanded.iter().any(|term| term == "просрочка"));
        assert!(plan.semantic_query.contains("пассивный"));
        assert!(plan.semantic_query.contains("не решено"));
    }

    #[tokio::test]
    async fn confirmed_terminology_improves_future_queries() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE terminology_terms(id INTEGER PRIMARY KEY, canonical TEXT, status TEXT, support_count INTEGER, last_seen_at TEXT)")
            .execute(&pool).await.unwrap();
        sqlx::query("CREATE TABLE terminology_aliases(id INTEGER PRIMARY KEY, term_id INTEGER, alias TEXT, status TEXT, support_count INTEGER, created_at TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO terminology_terms VALUES(1,'crowded','confirmed',99,'2026-01-02'),(2,'pipeline','confirmed',4,'2026-01-01')")
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..600 {
            sqlx::query("INSERT INTO terminology_aliases(term_id,alias,status,support_count,created_at) VALUES(1,?,'confirmed',1,'2026-01-01')")
                .bind(format!("crowded-{index}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO terminology_aliases(term_id,alias,status,support_count,created_at) VALUES(2,'пайплайн','confirmed',2,'2026-01-01')")
            .execute(&pool)
            .await
            .unwrap();

        let rows = confirmed_terminology_rows(&pool).await.unwrap();
        assert_eq!(
            rows.iter()
                .filter(|(canonical, _)| canonical == "crowded")
                .count(),
            MAX_ALIASES_PER_TERM as usize
        );
        assert!(rows
            .iter()
            .any(|(canonical, alias)| canonical == "pipeline"
                && alias.as_deref() == Some("пайплайн")));

        let mut plan = QueryPlan::build("что решили по пайплайну");
        plan.enrich_from_confirmed_terminology(&pool).await.unwrap();
        assert!(plan.expanded_terms().iter().any(|term| term == "pipeline"));
        assert!(plan.rewritten);
    }
}
