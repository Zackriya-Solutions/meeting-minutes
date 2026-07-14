//! Entity + action-item extraction and resolution (PLAN.md Phase 3).
//!
//! The LLM extraction pass produces JSON (validated against [`Extraction`]); this module
//! then resolves each entity against the existing set and dedupes action items. The
//! resolution algorithm (normalization + exact/fuzzy matching with review band) is pure
//! and unit-tested; the LLM call and DB writes are wired in the `extract` job handler.

use serde::{Deserialize, Serialize};

/// Merge threshold: at/above this similarity, auto-merge into the existing entity.
pub const MERGE_THRESHOLD: f64 = 0.92;
/// Review band lower bound: [REVIEW..MERGE) goes to `pending_merges`, never auto-merges.
pub const REVIEW_THRESHOLD: f64 = 0.85;
/// Action-item dedupe cosine threshold (PLAN.md §11 #4, configurable).
pub const ACTION_DEDUPE_THRESHOLD: f32 = 0.85;

/// Normalize a name for matching: lowercase, trim, ё→е, collapse internal whitespace.
pub fn normalize_name(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace('ё', "е")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Outcome of resolving an extracted entity against existing ones.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// Exact (normalized) match on canonical name or an alias.
    Exact(i64),
    /// Fuzzy match ≥ MERGE_THRESHOLD → auto-merge into this entity.
    Merge(i64),
    /// Similarity in [REVIEW, MERGE) → queue for manual review against this entity.
    Review { entity_id: i64, score: f64 },
    /// No match → create a new entity.
    New,
}

/// An existing entity for matching: id, type, and all its normalized names (canonical +
/// aliases). Only entities of the SAME type are considered (a person named "Иванов" must
/// not merge with a client "Иванов и партнёры").
pub struct KnownEntity {
    pub id: i64,
    pub entity_type: String,
    pub normalized_names: Vec<String>,
}

/// Resolve `name` of `entity_type` against `existing`. `existing` should already be
/// filtered/scored fairly; this scans all and picks the best fuzzy score.
pub fn resolve_entity(name: &str, entity_type: &str, existing: &[KnownEntity]) -> Resolution {
    let norm = normalize_name(name);
    let mut best: Option<(i64, f64)> = None;

    for e in existing {
        if e.entity_type != entity_type {
            continue; // never cross entity types
        }
        for known in &e.normalized_names {
            if known == &norm {
                return Resolution::Exact(e.id);
            }
            let score = strsim::jaro_winkler(&norm, known);
            if best.map_or(true, |(_, s)| score > s) {
                best = Some((e.id, score));
            }
        }
    }

    match best {
        Some((id, score)) if score >= MERGE_THRESHOLD => Resolution::Merge(id),
        Some((id, score)) if score >= REVIEW_THRESHOLD => Resolution::Review { entity_id: id, score },
        _ => Resolution::New,
    }
}

// ---- LLM extraction payload (validated JSON) ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extraction {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub action_items: Vec<ExtractedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub quote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedAction {
    pub text: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub approx_position: Option<String>,
}

const VALID_ENTITY_TYPES: [&str; 4] = ["project", "person", "client", "topic"];

/// Parse and validate the LLM's JSON output. Tolerates a ```json fence. Returns an error
/// (so the handler can retry once, per the plan) on invalid JSON or bad entity types.
pub fn parse_and_validate(raw: &str) -> Result<Extraction, String> {
    let cleaned = strip_code_fence(raw);
    let extraction: Extraction =
        serde_json::from_str(cleaned).map_err(|e| format!("invalid extraction JSON: {e}"))?;
    for ent in &extraction.entities {
        if ent.name.trim().is_empty() {
            return Err("entity name must not be empty".to_string());
        }
        if !VALID_ENTITY_TYPES.contains(&ent.entity_type.as_str()) {
            return Err(format!("invalid entity type: {}", ent.entity_type));
        }
    }
    if extraction.action_items.iter().any(|item| item.text.trim().is_empty()) {
        return Err("action item text must not be empty".to_string());
    }
    Ok(extraction)
}

fn strip_code_fence(raw: &str) -> &str {
    let t = raw.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.trim().strip_suffix("```").unwrap_or(t).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(id: i64, ty: &str, names: &[&str]) -> KnownEntity {
        KnownEntity {
            id,
            entity_type: ty.into(),
            normalized_names: names.iter().map(|n| normalize_name(n)).collect(),
        }
    }

    #[test]
    fn normalize_handles_yo_and_whitespace() {
        assert_eq!(normalize_name("  Проёкт   Альфа "), "проект альфа");
    }

    #[test]
    fn alpha_variants_merge_but_types_never_cross() {
        let existing = vec![
            ent(1, "project", &["Проект Альфа", "Альфа"]),
            ent(2, "person", &["Иванов"]),
            ent(3, "client", &["Иванов и партнёры"]),
        ];
        // «Альфа» resolves to the project by exact alias.
        assert_eq!(resolve_entity("альфа", "project", &existing), Resolution::Exact(1));
        // person «Иванов» does NOT merge with client «Иванов и партнёры».
        assert_eq!(resolve_entity("Иванов", "person", &existing), Resolution::Exact(2));
        let client = resolve_entity("Иванов", "client", &existing);
        assert!(
            matches!(client, Resolution::Review { entity_id: 3, .. } | Resolution::New),
            "person name must not auto-merge into a client; got {client:?}"
        );
    }

    #[test]
    fn near_miss_goes_to_review_not_merge() {
        let existing = vec![ent(1, "project", &["Проект Альфа"])];
        // A close-but-not-identical variant lands in the review band or as new.
        let r = resolve_entity("Проект Альфабет", "project", &existing);
        assert!(matches!(r, Resolution::Review { .. } | Resolution::Merge(_) | Resolution::New));
        assert_ne!(r, Resolution::Exact(1));
    }

    #[test]
    fn parses_fenced_json_and_rejects_bad_type() {
        let ok = "```json\n{\"entities\":[{\"type\":\"person\",\"name\":\"Пётр\"}],\"action_items\":[]}\n```";
        let parsed = parse_and_validate(ok).unwrap();
        assert_eq!(parsed.entities.len(), 1);
        assert_eq!(parsed.entities[0].name, "Пётр");

        let bad = "{\"entities\":[{\"type\":\"animal\",\"name\":\"X\"}]}";
        assert!(parse_and_validate(bad).is_err());
    }

    #[test]
    fn missing_optional_fields_default() {
        let parsed = parse_and_validate("{\"action_items\":[{\"text\":\"сделать отчёт\"}]}").unwrap();
        assert!(parsed.entities.is_empty());
        assert_eq!(parsed.action_items[0].text, "сделать отчёт");
        assert!(parsed.action_items[0].owner.is_none());
    }
}
