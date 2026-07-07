//! Versioned prompts (PLAN.md §9): prompts live as files under `src-tauri/prompts/` and
//! are embedded at build time. Changing a prompt = a NEW file (e.g. `extract_v2.md`),
//! never an in-place edit — so a prompt version is always reproducible.

/// The entity/action extraction prompt (Phase 3).
pub fn extract_v1() -> &'static str {
    include_str!("../../prompts/extract_v1.md")
}

/// The RAG grounded-answer prompt (Phase 4).
pub fn rag_answer_v1() -> &'static str {
    include_str!("../../prompts/rag_answer_v1.md")
}

/// Sentinel the RAG model returns when the answer is not in the provided context
/// (PLAN.md Phase 4 low-confidence guard). Must match `rag_answer_v1.md` exactly.
pub const RAG_NOT_FOUND: &str = "в записях не найдено";

/// Fill `{{key}}` placeholders in a template. Simple, deterministic, and dependency-free
/// (no templating engine needed for these prompts).
pub fn fill(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_all_placeholders() {
        let t = "Вопрос: {{question}}\nКонтекст: {{context}}";
        let filled = fill(t, &[("question", "бюджет?"), ("context", "[1] ...")]);
        assert_eq!(filled, "Вопрос: бюджет?\nКонтекст: [1] ...");
        assert!(!filled.contains("{{"));
    }

    #[test]
    fn prompts_embedded_and_nonempty() {
        assert!(extract_v1().contains("action_items"));
        assert!(rag_answer_v1().contains(RAG_NOT_FOUND));
    }
}
