//! User-managed terminology corrections.
//!
//! A terminology entry maps a commonly mis-transcribed word or phrase to its correct
//! spelling (for example "post gres" -> "Postgres"). Entries are stored as JSON in the
//! app data directory and applied deterministically to generated summaries, independent
//! of the transcription engine or the LLM.

use log::{info, warn};
use regex::{NoExpand, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{Manager, Runtime};

const TERMINOLOGY_FILE_NAME: &str = "terminology.json";

/// A single correction: occurrences of `wrong` are replaced with `correct`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminologyEntry {
    /// The incorrect word or phrase as it appears in transcripts or summaries.
    pub wrong: String,
    /// The correct spelling to substitute.
    pub correct: String,
    /// When false (the default), `wrong` matches regardless of letter case.
    #[serde(default)]
    pub match_case: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TerminologyFile {
    #[serde(default)]
    entries: Vec<TerminologyEntry>,
}

fn terminology_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(TERMINOLOGY_FILE_NAME)
}

fn app_data_dir<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))
}

/// Trims entries, drops incomplete or no-op entries, and removes duplicates.
///
/// Duplicates are detected on `wrong` (case-insensitively unless `match_case` is set);
/// the last occurrence wins so that newer edits replace older ones.
pub fn normalize_entries(entries: Vec<TerminologyEntry>) -> Vec<TerminologyEntry> {
    let mut normalized: Vec<TerminologyEntry> = Vec::new();

    for entry in entries {
        let wrong = entry.wrong.trim().to_string();
        let correct = entry.correct.trim().to_string();
        if wrong.is_empty() || correct.is_empty() || wrong == correct {
            continue;
        }

        let candidate = TerminologyEntry { wrong, correct, match_case: entry.match_case };
        normalized.retain(|existing| !same_key(existing, &candidate));
        normalized.push(candidate);
    }

    normalized
}

fn same_key(a: &TerminologyEntry, b: &TerminologyEntry) -> bool {
    if a.match_case || b.match_case {
        a.wrong == b.wrong && a.match_case == b.match_case
    } else {
        a.wrong.to_lowercase() == b.wrong.to_lowercase()
    }
}

/// Loads terminology entries from the app data directory.
///
/// A missing file yields an empty list; an unreadable file is logged and treated as empty
/// so summary generation is never blocked by a bad terminology file.
pub fn load_entries(app_data_dir: &Path) -> Vec<TerminologyEntry> {
    let path = terminology_path(app_data_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            warn!("Failed to read terminology file {:?}: {}", path, e);
            return Vec::new();
        }
    };

    match serde_json::from_str::<TerminologyFile>(&content) {
        Ok(file) => normalize_entries(file.entries),
        Err(e) => {
            warn!("Failed to parse terminology file {:?}: {}", path, e);
            Vec::new()
        }
    }
}

/// Normalizes and persists terminology entries, returning what was saved.
///
/// Writes to a temporary file first and renames it so a crash cannot leave a partial file.
pub fn save_entries(
    app_data_dir: &Path,
    entries: Vec<TerminologyEntry>,
) -> Result<Vec<TerminologyEntry>, String> {
    let entries = normalize_entries(entries);
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    let json = serde_json::to_string_pretty(&TerminologyFile { entries: entries.clone() })
        .map_err(|e| format!("Failed to serialize terminology: {}", e))?;

    let path = terminology_path(app_data_dir);
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json)
        .map_err(|e| format!("Failed to write terminology file: {}", e))?;
    std::fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to replace terminology file: {}", e))?;

    info!("Saved {} terminology entries to {:?}", entries.len(), path);
    Ok(entries)
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn build_pattern(entry: &TerminologyEntry) -> Option<Regex> {
    let escaped = regex::escape(&entry.wrong);
    // Only anchor on word boundaries where the phrase itself starts or ends with a word
    // character; "\b" next to punctuation would otherwise prevent any match.
    let starts_with_word = entry.wrong.chars().next().is_some_and(is_word_char);
    let ends_with_word = entry.wrong.chars().last().is_some_and(is_word_char);
    let pattern = format!(
        "{}{}{}",
        if starts_with_word { r"\b" } else { "" },
        escaped,
        if ends_with_word { r"\b" } else { "" },
    );

    match RegexBuilder::new(&pattern).case_insensitive(!entry.match_case).build() {
        Ok(regex) => Some(regex),
        Err(e) => {
            warn!("Skipping terminology entry '{}': {}", entry.wrong, e);
            None
        }
    }
}

/// Applies terminology corrections to `text`, matching whole words and phrases only.
///
/// Longer phrases are applied first so that a phrase entry is not pre-empted by a shorter
/// entry contained within it. Returns the corrected text and the number of replacements.
pub fn apply_corrections(text: &str, entries: &[TerminologyEntry]) -> (String, usize) {
    let mut ordered: Vec<&TerminologyEntry> = entries.iter().collect();
    ordered.sort_by_key(|entry| std::cmp::Reverse(entry.wrong.chars().count()));

    let mut corrected = text.to_string();
    let mut replacements = 0;

    for entry in ordered {
        let Some(regex) = build_pattern(entry) else { continue };
        let changed = regex
            .find_iter(&corrected)
            .filter(|m| m.as_str() != entry.correct)
            .count();
        if changed == 0 {
            continue;
        }
        corrected = regex.replace_all(&corrected, NoExpand(&entry.correct)).into_owned();
        replacements += changed;
    }

    (corrected, replacements)
}

/// Loads the stored entries and applies them to `text`.
///
/// Returns the text unchanged when no app data directory is available.
pub fn apply_stored_corrections(app_data_dir: Option<&PathBuf>, text: &str) -> String {
    let Some(dir) = app_data_dir else { return text.to_string() };
    let entries = load_entries(dir);
    if entries.is_empty() {
        return text.to_string();
    }

    let (corrected, replacements) = apply_corrections(text, &entries);
    if replacements > 0 {
        info!("Applied {} terminology corrections to summary", replacements);
    }
    corrected
}

/// Returns all stored terminology entries.
#[tauri::command]
pub async fn api_get_terminology<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<TerminologyEntry>, String> {
    Ok(load_entries(&app_data_dir(&app)?))
}

/// Replaces the stored terminology list and returns the normalized result.
#[tauri::command]
pub async fn api_save_terminology<R: Runtime>(
    app: tauri::AppHandle<R>,
    entries: Vec<TerminologyEntry>,
) -> Result<Vec<TerminologyEntry>, String> {
    save_entries(&app_data_dir(&app)?, entries)
}

/// Adds or updates a single entry, keyed on `wrong`, and returns the full list.
#[tauri::command]
pub async fn api_add_terminology_entry<R: Runtime>(
    app: tauri::AppHandle<R>,
    entry: TerminologyEntry,
) -> Result<Vec<TerminologyEntry>, String> {
    let dir = app_data_dir(&app)?;
    let mut entries = load_entries(&dir);
    entries.push(entry);
    save_entries(&dir, entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(wrong: &str, correct: &str) -> TerminologyEntry {
        TerminologyEntry { wrong: wrong.into(), correct: correct.into(), match_case: false }
    }

    #[test]
    fn replaces_phrase_case_insensitively() {
        let (text, count) =
            apply_corrections("The post gres report and Post Gres jobs", &[entry("post gres", "Postgres")]);
        assert_eq!(text, "The Postgres report and Postgres jobs");
        assert_eq!(count, 2);
    }

    #[test]
    fn matches_whole_words_only() {
        let (text, count) = apply_corrections("JSAN and JSANX", &[entry("JSAN", "JSON")]);
        assert_eq!(text, "JSON and JSANX");
        assert_eq!(count, 1);
    }

    #[test]
    fn does_not_touch_longer_words_that_start_with_the_phrase() {
        let (text, count) =
            apply_corrections("Data bases team", &[entry("Data b", "Data B")]);
        assert_eq!(text, "Data bases team");
        assert_eq!(count, 0);
    }

    #[test]
    fn already_correct_text_counts_no_replacements() {
        let (text, count) = apply_corrections("Data B is up", &[entry("Data b", "Data B")]);
        assert_eq!(text, "Data B is up");
        assert_eq!(count, 0);
    }

    #[test]
    fn match_case_only_replaces_exact_case() {
        let entries = [TerminologyEntry { wrong: "Bot".into(), correct: "BOT".into(), match_case: true }];
        let (text, count) = apply_corrections("Bot and bot", &entries);
        assert_eq!(text, "BOT and bot");
        assert_eq!(count, 1);
    }

    #[test]
    fn longer_phrases_apply_before_shorter_ones() {
        let entries = [entry("Acmee", "Acme"), entry("Acmee post gres", "Acme Postgres")];
        let (text, _) = apply_corrections("Acmee post gres platform", &entries);
        assert_eq!(text, "Acme Postgres platform");
    }

    #[test]
    fn replacement_text_is_literal() {
        let (text, _) = apply_corrections("cost is X", &[entry("X", "$1")]);
        assert_eq!(text, "cost is $1");
    }

    #[test]
    fn phrases_ending_in_punctuation_still_match() {
        let (text, count) = apply_corrections("Ask J.S.O.N. today", &[entry("J.S.O.N.", "JSON")]);
        assert_eq!(text, "Ask JSON today");
        assert_eq!(count, 1);
    }

    #[test]
    fn normalize_drops_empty_and_noop_entries_and_dedupes() {
        let normalized = normalize_entries(vec![
            entry("  JSAN ", " JSON "),
            entry("", "x"),
            entry("same", "same"),
            entry("jsan", "JSON format"),
        ]);
        assert_eq!(normalized, vec![entry("jsan", "JSON format")]);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let saved = save_entries(dir.path(), vec![entry("Acmee", "Acme")]).unwrap();
        assert_eq!(load_entries(dir.path()), saved);
    }

    #[test]
    fn missing_or_corrupt_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_entries(dir.path()).is_empty());
        std::fs::write(dir.path().join(TERMINOLOGY_FILE_NAME), "not json").unwrap();
        assert!(load_entries(dir.path()).is_empty());
    }
}
