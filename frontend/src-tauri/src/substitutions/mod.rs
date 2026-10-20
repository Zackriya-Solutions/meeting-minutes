//! User-defined transcription substitutions ("vocabulary corrections").
//!
//! Two effects:
//! 1. `apply()` rewrites finished transcript text using ordered rules
//!    (word-boundary aware, optional case sensitivity) before the text is
//!    returned to callers — UI, storage, summaries, and any downstream
//!    connector all receive corrected text.
//! 2. `initial_prompt()` builds a glossary string that is fed to Whisper's
//!    `initial_prompt` so vocabulary terms are recognized correctly in the
//!    first place.
//!
//! Rules live in a JSON file next to the custom templates directory:
//! - macOS:   ~/Library/Application Support/Meetily/substitutions.json
//! - Linux:   ~/.local/share/Meetily/substitutions.json
//! - Windows: %APPDATA%\Meetily\substitutions.json
//!
//! The file is hot-reloaded: edits (by hand or via the Settings UI) take
//! effect on the next transcription without restarting the app.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single user-defined substitution rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstitutionRule {
    /// Text to look for (matched literally; regex metacharacters are escaped).
    pub find: String,
    /// Text to replace it with.
    pub replace: String,
    /// Match whole words only (default true). When false, substrings match.
    #[serde(default = "default_true")]
    pub whole_word: bool,
    /// Case-sensitive match (default false — "tuttle" matches "Tuttle").
    #[serde(default)]
    pub case_sensitive: bool,
    /// Rule switch. Disabled rules are kept in the file but not applied.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// On-disk JSON shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubstitutionFile {
    version: u32,
    rules: Vec<SubstitutionRule>,
}

/// A compiled rule (regex prebuilt for speed).
struct CompiledRule {
    regex: Regex,
    replace: String,
}

// ---------------------------------------------------------------------------
// Pure engine (unit-testable, no filesystem)
// ---------------------------------------------------------------------------

/// Compile rules into regexes. Returns only the enabled rules, in order.
fn compile_rules(rules: &[SubstitutionRule]) -> Vec<CompiledRule> {
    rules
        .iter()
        .filter(|r| r.enabled && !r.find.trim().is_empty())
        .filter_map(|r| {
            let mut pattern = String::new();
            if !r.case_sensitive {
                pattern.push_str("(?i)");
            }
            let lit = r.find.trim();
            let escaped = regex::escape(lit);
            if r.whole_word {
                // Word boundaries only make sense adjacent to word characters.
                // A token like "c++" ends in '+', where `\b` can never match
                // (boundary needs a word char on one side), so only add `\b`
                // where the token edge is itself a word character.
                let is_word = |c: char| c.is_alphanumeric() || c == '_';
                if lit.chars().next().map(is_word).unwrap_or(false) {
                    pattern.push_str(r#"\b"#);
                }
                pattern.push_str(&escaped);
                if lit.chars().last().map(is_word).unwrap_or(false) {
                    pattern.push_str(r#"\b"#);
                }
            } else {
                pattern.push_str(&escaped);
            }
            match Regex::new(&pattern) {
                Ok(regex) => Some(CompiledRule {
                    regex,
                    replace: r.replace.clone(),
                }),
                Err(e) => {
                    log::warn!("Invalid substitution rule '{}': {}", r.find, e);
                    None
                }
            }
        })
        .collect()
}

/// Apply compiled rules to text, in order.
fn apply_rules(text: &str, compiled: &[CompiledRule]) -> String {
    let mut out = text.to_string();
    for rule in compiled {
        if rule.regex.is_match(&out) {
            out = rule.regex.replace_all(&out, rule.replace.as_str()).to_string();
        }
    }
    out
}

/// Build the Whisper `initial_prompt` glossary from enabled rules.
/// Uses the distinct replacement values (the spellings we WANT), capped to
/// keep the prompt within Whisper's effective context (~224 tokens).
fn build_initial_prompt(rules: &[SubstitutionRule]) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    let mut terms: Vec<String> = rules
        .iter()
        .filter(|r| r.enabled)
        .map(|r| r.replace.trim().to_string())
        .filter(|t| !t.is_empty() && t.chars().any(|c| c.is_alphabetic()))
        .filter(|t| seen.insert(t.to_lowercase()))
        .collect();
    terms.sort();
    if terms.is_empty() {
        return None;
    }
    let mut prompt = String::from("Glossary of correct spellings for this meeting: ");
    for term in &terms {
        let candidate = if prompt.ends_with(": ") {
            format!("{}{}", prompt, term)
        } else {
            format!("{}, {}", prompt, term)
        };
        if candidate.len() > 400 {
            break;
        }
        prompt = candidate;
    }
    if prompt.ends_with(": ") {
        return None; // nothing fit
    }
    prompt.push('.');
    Some(prompt)
}

// ---------------------------------------------------------------------------
// File-backed state (hot-reloaded)
// ---------------------------------------------------------------------------

struct EngineState {
    mtime: Option<SystemTime>,
    compiled: Vec<CompiledRule>,
    prompt: Option<String>,
    rules: Vec<SubstitutionRule>,
}

static STATE: Lazy<Mutex<EngineState>> = Lazy::new(|| {
    Mutex::new(EngineState {
        mtime: None,
        compiled: Vec::new(),
        prompt: None,
        rules: Vec::new(),
    })
});

/// Location of the substitutions JSON file.
pub fn config_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("Meetily");
    path.push("substitutions.json");
    path
}

fn file_mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Reload from disk if the file changed since the last load.
fn ensure_loaded() {
    let path = config_path();
    let mtime = file_mtime(&path);
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if mtime == state.mtime {
        return; // unchanged (including both-None => missing file, nothing to do)
    }
    let rules = match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<SubstitutionFile>(&content) {
            Ok(file) => {
                log::info!(
                    "Loaded {} substitution rules from {:?}",
                    file.rules.len(),
                    path
                );
                file.rules
            }
            Err(e) => {
                log::error!("Failed to parse {:?}: {} (keeping previous rules)", path, e);
                return; // keep previous state, do not update mtime so we retry later
            }
        },
        Err(_) => Vec::new(), // missing file = no rules (first run)
    };
    state.compiled = compile_rules(&rules);
    state.prompt = build_initial_prompt(&rules);
    state.rules = rules;
    state.mtime = mtime;
}

/// Apply substitutions to finished transcript text.
pub fn apply(text: &str) -> String {
    ensure_loaded();
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    apply_rules(text, &state.compiled)
}

/// Glossary prompt for Whisper, if any enabled rules exist.
pub fn initial_prompt() -> Option<String> {
    ensure_loaded();
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.prompt.clone()
}

/// Current rules (for the Settings UI).
pub fn current_rules() -> Vec<SubstitutionRule> {
    ensure_loaded();
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.rules.clone()
}

/// Persist rules to disk (creates the parent directory on demand).
pub fn save_rules(rules: &[SubstitutionRule]) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {:?}: {}", parent, e))?;
    }
    let file = SubstitutionFile {
        version: 1,
        rules: rules.to_vec(),
    };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write {:?}: {}", path, e))?;
    // Force reload on next use so changes apply immediately.
    if let Ok(mut state) = STATE.lock() {
        state.mtime = None;
    }
    log::info!("Saved {} substitution rules to {:?}", rules.len(), path);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands (Settings UI surface)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn substitutions_list() -> Result<Vec<SubstitutionRule>, String> {
    Ok(current_rules())
}

#[tauri::command]
pub async fn substitutions_save(rules: Vec<SubstitutionRule>) -> Result<(), String> {
    save_rules(&rules)
}

#[tauri::command]
pub async fn substitutions_test(text: String) -> Result<String, String> {
    Ok(apply(&text))
}

#[tauri::command]
pub async fn substitutions_file_path() -> Result<String, String> {
    Ok(config_path().to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(find: &str, replace: &str) -> SubstitutionRule {
        SubstitutionRule {
            find: find.to_string(),
            replace: replace.to_string(),
            whole_word: true,
            case_sensitive: false,
            enabled: true,
        }
    }

    fn run(rules: &[SubstitutionRule], text: &str) -> String {
        let compiled = compile_rules(rules);
        apply_rules(text, &compiled)
    }

    #[test]
    fn basic_case_insensitive_replace() {
        let rules = vec![rule("newplains", "New Plains")];
        assert_eq!(run(&rules, "we use newplains for that"), "we use New Plains for that");
        assert_eq!(run(&rules, "We use NewPlains today"), "We use New Plains today");
    }

    #[test]
    fn whole_word_respects_boundaries() {
        let mut r = rule("tuttle", "Tuttle");
        r.whole_word = true;
        assert_eq!(run(&[r.clone()], "she lives in tuttle ok"), "she lives in Tuttle ok");
        assert_eq!(run(&[r], "tuttledom is not a place"), "tuttledom is not a place");
    }

    #[test]
    fn substring_mode_when_whole_word_off() {
        let mut r = rule("skags", "Skaggs");
        r.whole_word = false;
        assert_eq!(run(&[r], "skags and SKAGS"), "Skaggs and Skaggs");
    }

    #[test]
    fn case_sensitive_matches_exact_only() {
        let mut r = rule("May", "Can");
        r.case_sensitive = true;
        assert_eq!(run(&[r], "May I go, may I stay"), "Can I go, may I stay");
    }

    #[test]
    fn empty_or_disabled_rules_are_noops() {
        assert_eq!(run(&[], "unchanged text"), "unchanged text");
        let mut r = rule("a", "b");
        r.enabled = false;
        assert_eq!(run(&[r], "a a a"), "a a a");
        let blank = rule("   ", "x");
        assert_eq!(run(&[blank], "a a a"), "a a a");
    }

    #[test]
    fn regex_metacharacters_are_literal() {
        let rules = vec![rule("c++", "C++")];
        assert_eq!(run(&rules, "we write in c++ here"), "we write in C++ here");
    }

    #[test]
    fn rules_apply_in_order() {
        let rules = vec![rule("john", "John Smith"), rule("John Smith", "J. Smith")];
        assert_eq!(run(&rules, "john called"), "J. Smith called");
    }

    #[test]
    fn initial_prompt_dedupes_and_skips_nonalpha() {
        let rules = vec![
            rule("tuttle", "Tuttle"),
            rule("TUTTLE", "Tuttle"), // duplicate target
            rule("123", "123"),       // no alphabetic chars -> skipped
            SubstitutionRule {
                find: "x".into(),
                replace: "".into(), // empty -> skipped
                whole_word: true,
                case_sensitive: false,
                enabled: true,
            },
        ];
        let prompt = build_initial_prompt(&rules).unwrap();
        assert!(prompt.starts_with("Glossary"), "got: {}", prompt);
        assert!(prompt.contains("Tuttle"));
        assert_eq!(prompt.matches("Tuttle").count(), 1);
        assert!(!prompt.contains("123"));
    }

    #[test]
    fn initial_prompt_none_when_empty() {
        assert!(build_initial_prompt(&[]).is_none());
    }
}
