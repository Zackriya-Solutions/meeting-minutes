//! Configuration for the meeting-log feature set (file output, translation,
//! summary, memory search). Values are read from environment variables, with a
//! `.env` file loaded as a fallback, and finally baked-in defaults so the
//! packaged app works out of the box.
//!
//! All paths are read from config — nothing here is hardcoded at the call site.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolved configuration, loaded once on first access.
#[derive(Debug, Clone)]
pub struct MeetingLogConfig {
    pub log_root: PathBuf,
    /// chrono strftime format for the per-day folder name (e.g. "%Y-%m-%d").
    pub date_format: String,
    /// chrono strftime format for the session time component (e.g. "%H-%M-%S").
    pub time_format: String,
    pub transcript_pattern: String,
    pub summary_pattern: String,
    pub daily_log: String,

    pub stt_model: String,
    /// Model for →English translation (translategemma is EN-only).
    pub translate_model: String,
    /// Model for →Thai translation (needs a Thai-capable multilingual model).
    pub translate_model_th: String,
    /// Default target language for the translate button ("th" or "en").
    pub translate_default_target: String,
    pub summary_model: String,
    /// Used automatically if `summary_model` is not installed in Ollama.
    pub summary_model_fallback: String,
    pub embed_model: String,

    pub ollama_url: String,
    pub sidecar_url: String,

    pub vector_db_path: PathBuf,
    pub glossary: Vec<String>,

    /// Root for Quick Note daily archive markdown files.
    pub note_log_root: PathBuf,
    /// "on_launch" enables archive + carry-forward at app start.
    pub note_rollover: String,
}

static CONFIG: Lazy<MeetingLogConfig> = Lazy::new(MeetingLogConfig::load);

/// Access the process-wide meeting-log configuration.
pub fn config() -> &'static MeetingLogConfig {
    &CONFIG
}

impl MeetingLogConfig {
    fn load() -> Self {
        let env = load_env_layers();

        let get = |key: &str, default: &str| -> String {
            std::env::var(key)
                .ok()
                .or_else(|| env.get(key).cloned())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };

        let log_root = get("MEET_LOG_ROOT", "/Users/AR697030/Desktop/personal/meet-log");
        let date_format = to_chrono_format(&get("MEET_LOG_DATE_FORMAT", "yyyy-MM-dd"));
        let time_format = to_chrono_format(&get("MEET_TIME_FORMAT", "HH-mm-ss"));
        let vector_db_path = get(
            "VECTOR_DB_PATH",
            &format!("{}/.index/meetings.db", log_root),
        );
        let glossary = get(
            "STT_GLOSSARY",
            "Kafka,ACL,gRPC,consumer lag,Vault Core,ThoughtMachine,Redis,partition,deploy,UAT,broker",
        )
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

        MeetingLogConfig {
            log_root: PathBuf::from(log_root),
            date_format,
            time_format,
            transcript_pattern: get("MEET_TRANSCRIPT_PATTERN", "{time}_transcript.md"),
            summary_pattern: get("MEET_SUMMARY_PATTERN", "{time}_summary.md"),
            daily_log: get("MEET_DAILY_LOG", "summary.md"),
            stt_model: get("STT_MODEL", "large-v3"),
            translate_model: get("TRANSLATE_MODEL", "translategemma:4b"),
            translate_model_th: get("TRANSLATE_MODEL_TH", "qwen3.5:9b"),
            translate_default_target: get("TRANSLATE_DEFAULT_TARGET", "th"),
            summary_model: get("SUMMARY_MODEL", "qwen3.5:9b"),
            summary_model_fallback: get("SUMMARY_MODEL_FALLBACK", "qwen2.5:14b"),
            embed_model: get("EMBED_MODEL", "bge-m3"),
            ollama_url: get("OLLAMA_URL", "http://127.0.0.1:11434"),
            sidecar_url: get("MEET_SIDECAR_URL", "http://127.0.0.1:8178"),
            vector_db_path: PathBuf::from(vector_db_path),
            glossary,
            note_log_root: PathBuf::from(get(
                "NOTE_LOG_ROOT",
                "/Users/AR697030/Desktop/personal/note-log",
            )),
            note_rollover: get("NOTE_ROLLOVER", "on_launch"),
        }
    }
}

/// Search likely `.env` locations and merge them (first found wins per key).
/// We do NOT override real environment variables — those take precedence.
fn load_env_layers() -> HashMap<String, String> {
    let mut merged = HashMap::new();

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
        candidates.push(cwd.join("../.env"));
        candidates.push(cwd.join("../../.env"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(".env"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        candidates.push(home.join("Library/Application Support/Meetily/.env"));
    }

    for path in candidates {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            for (k, v) in parse_env(&contents) {
                merged.entry(k).or_insert(v);
            }
        }
    }
    merged
}

/// Minimal `.env` parser: `KEY=value`, ignores blank/`#` lines and inline
/// `# comments` after the value. Quotes are stripped.
fn parse_env(contents: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        // Strip an inline comment that is not inside quotes.
        let mut val = rest.trim().to_string();
        if !(val.starts_with('"') || val.starts_with('\'')) {
            if let Some(idx) = val.find(" #") {
                val = val[..idx].trim().to_string();
            }
        }
        let val = val
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();
        if !key.is_empty() {
            out.push((key, val));
        }
    }
    out
}

/// Translate the Java-style date tokens used in the spec/.env
/// (yyyy, MM, dd, HH, mm, ss) into chrono strftime specifiers. If the input
/// already looks like a strftime string (contains '%'), it is returned as-is.
fn to_chrono_format(fmt: &str) -> String {
    if fmt.contains('%') {
        return fmt.to_string();
    }
    fmt.replace("yyyy", "%Y")
        .replace("yy", "%y")
        .replace("MM", "%m")
        .replace("dd", "%d")
        .replace("HH", "%H")
        .replace("mm", "%M")
        .replace("ss", "%S")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_java_tokens() {
        assert_eq!(to_chrono_format("yyyy-MM-dd"), "%Y-%m-%d");
        assert_eq!(to_chrono_format("HH-mm-ss"), "%H-%M-%S");
        assert_eq!(to_chrono_format("%Y/%m"), "%Y/%m");
    }

    #[test]
    fn parses_env_with_inline_comment() {
        let parsed = parse_env("MEET_LOG_ROOT=/tmp/x   # the root\nFOO=bar\n# comment\n");
        assert_eq!(parsed[0], ("MEET_LOG_ROOT".into(), "/tmp/x".into()));
        assert_eq!(parsed[1], ("FOO".into(), "bar".into()));
    }
}
