// Meeting domain prompt management.
//
// A "meeting domain" is a plain-text vocabulary hint file used to bias
// Whisper transcription via `whisper_full_params.initial_prompt`. Each
// domain is one `.txt` file named after the domain (e.g. `tekni.txt`).
//
// Files are looked up in two locations:
// 1. Primary: `<app_data_dir>/domains/` — matches the convention used by
//    `whisper_engine::commands::set_models_directory`.
// 2. Secondary: `~/.meetily/domains/` — convenient manual edit location
//    (also supported via a symlink to the primary directory).
//
// Whisper's `initial_prompt` is capped around 224 tokens; we truncate
// prompts at ~1000 characters as a conservative char-based approximation
// and log a warning when truncation kicks in.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Runtime};

/// Approximate cap for `initial_prompt`. Whisper itself enforces ~224 tokens;
/// 1000 chars is a safe overestimate for Latin scripts including Scandinavian.
pub const MAX_PROMPT_CHARS: usize = 1000;

const DOMAINS_SUBDIR: &str = "domains";
const SECONDARY_HOME_SUBDIR: &str = ".meetily/domains";

/// Primary domains directory (under Tauri's app_data_dir). Set during app
/// startup via [`set_domain_directory`], mirroring the models directory pattern.
static DOMAIN_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Initialize the primary domain directory and create it if missing.
///
/// Call this from the Tauri `setup` hook, similar to
/// `whisper_engine::commands::set_models_directory`.
pub fn set_domain_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            log::error!("Failed to resolve app_data_dir for meeting domains: {}", e);
            return;
        }
    };

    let domains_dir = app_data_dir.join(DOMAINS_SUBDIR);

    if !domains_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&domains_dir) {
            log::error!(
                "Failed to create meeting domain directory at {}: {}",
                domains_dir.display(),
                e
            );
            return;
        }
    }

    log::info!("Meeting domain directory set to: {}", domains_dir.display());

    if let Ok(mut guard) = DOMAIN_DIR.lock() {
        *guard = Some(domains_dir);
    }
}

/// Primary domain directory, if it has been initialized.
pub fn primary_dir() -> Option<PathBuf> {
    DOMAIN_DIR.lock().ok().and_then(|g| g.clone())
}

/// Secondary domain directory under the user's home (`~/.meetily/domains/`).
/// Returned only when the path actually exists, so callers can scan it safely.
pub fn secondary_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(SECONDARY_HOME_SUBDIR);
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// List available domain names (filenames without `.txt`) from both
/// directories. Names from [`primary_dir`] take precedence on conflict.
pub fn list_domains() -> Result<Vec<String>> {
    let mut names: BTreeSet<String> = BTreeSet::new();

    for dir in [secondary_dir(), primary_dir()].into_iter().flatten() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(it) => it,
            Err(e) => {
                log::warn!("Could not read domain directory {}: {}", dir.display(), e);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("txt") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if !stem.is_empty() {
                    names.insert(stem.to_string());
                }
            }
        }
    }

    Ok(names.into_iter().collect())
}

/// Validate that a domain name is safe to use as a filename. Rejects names
/// that contain path separators, parent traversal, NUL bytes, or that start
/// with a dot (hidden files). Returns the trimmed, validated name.
pub fn validate_domain_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("domain name is empty");
    }
    if trimmed.len() > 64 {
        anyhow::bail!("domain name is too long (max 64 chars)");
    }
    if trimmed.starts_with('.') {
        anyhow::bail!("domain name must not start with '.'");
    }
    for ch in trimmed.chars() {
        match ch {
            '/' | '\\' | '\0' => anyhow::bail!("domain name contains invalid character: {:?}", ch),
            c if c.is_control() => anyhow::bail!("domain name contains control character"),
            _ => {}
        }
    }
    if trimmed == ".." || trimmed.contains("..") {
        anyhow::bail!("domain name must not contain '..'");
    }
    Ok(trimmed.to_string())
}

fn primary_dir_required() -> Result<PathBuf> {
    primary_dir().ok_or_else(|| anyhow::anyhow!("meeting domain directory not initialized"))
}

/// Read the raw content of a domain file (no truncation, for editing in UI).
/// Returns `Ok(None)` if the file does not exist.
pub fn get_domain_content(name: &str) -> Result<Option<String>> {
    let name = validate_domain_name(name)?;
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = primary_dir() {
        candidates.push(p.join(format!("{name}.txt")));
    }
    if let Some(p) = secondary_dir() {
        candidates.push(p.join(format!("{name}.txt")));
    }
    let Some(path) = candidates.into_iter().find(|p| p.is_file()) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading domain file {}", path.display()))?;
    Ok(Some(text))
}

/// Write (create or overwrite) a domain file in the primary directory.
pub fn save_domain(name: &str, content: &str) -> Result<()> {
    let name = validate_domain_name(name)?;
    let dir = primary_dir_required()?;
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating domain dir {}", dir.display()))?;
    let path = dir.join(format!("{name}.txt"));
    std::fs::write(&path, content)
        .with_context(|| format!("writing domain file {}", path.display()))?;
    log::info!("Saved meeting domain '{}' to {}", name, path.display());
    Ok(())
}

/// Delete a domain file from the primary directory. Files in the secondary
/// (`~/.meetily/domains/`) directory are left alone — those are user-managed.
pub fn delete_domain(name: &str) -> Result<()> {
    let name = validate_domain_name(name)?;
    let dir = primary_dir_required()?;
    let path = dir.join(format!("{name}.txt"));
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path)
        .with_context(|| format!("deleting domain file {}", path.display()))?;
    log::info!("Deleted meeting domain '{}' ({})", name, path.display());
    Ok(())
}

/// Load and prepare the prompt text for `domain`. Returns:
/// - `Ok(None)` if no file exists or the file is empty/whitespace-only.
/// - `Ok(Some(text))` truncated to [`MAX_PROMPT_CHARS`] with a warning logged
///   when the original was longer.
///
/// Resolution order: primary directory first, then secondary.
pub fn load_prompt(domain: &str) -> Result<Option<String>> {
    let domain = domain.trim();
    if domain.is_empty() {
        return Ok(None);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = primary_dir() {
        candidates.push(p.join(format!("{domain}.txt")));
    }
    if let Some(p) = secondary_dir() {
        candidates.push(p.join(format!("{domain}.txt")));
    }

    let Some(path) = candidates.into_iter().find(|p| p.is_file()) else {
        log::debug!("No prompt file found for meeting domain '{}'", domain);
        return Ok(None);
    };

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading prompt file {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let char_count = trimmed.chars().count();
    let prompt = if char_count > MAX_PROMPT_CHARS {
        log::warn!(
            "Meeting domain '{}' prompt is {} chars (>{} cap); truncating",
            domain,
            char_count,
            MAX_PROMPT_CHARS
        );
        trimmed.chars().take(MAX_PROMPT_CHARS).collect::<String>()
    } else {
        trimmed.to_string()
    };

    log::info!(
        "Loaded meeting domain '{}' prompt ({} chars) from {}",
        domain,
        prompt.chars().count(),
        path.display()
    );

    Ok(Some(prompt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex as StdMutex;
    use tempfile::TempDir;

    // Tests share the static DOMAIN_DIR, so they must run serially.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn set_primary_for_test(dir: PathBuf) {
        *DOMAIN_DIR.lock().unwrap() = Some(dir);
    }

    fn clear_primary_for_test() {
        *DOMAIN_DIR.lock().unwrap() = None;
    }

    #[test]
    fn load_prompt_returns_none_for_empty_name() {
        let _guard = TEST_LOCK.lock().unwrap();
        let result = load_prompt("").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_prompt_returns_none_when_file_missing() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        set_primary_for_test(tmp.path().to_path_buf());
        let result = load_prompt("nonexistent").unwrap();
        assert!(result.is_none());
        clear_primary_for_test();
    }

    #[test]
    fn load_prompt_truncates_long_content() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let long_text = "a".repeat(MAX_PROMPT_CHARS + 500);
        fs::write(tmp.path().join("big.txt"), &long_text).unwrap();
        set_primary_for_test(tmp.path().to_path_buf());

        let result = load_prompt("big").unwrap().unwrap();
        assert_eq!(result.chars().count(), MAX_PROMPT_CHARS);
        clear_primary_for_test();
    }

    #[test]
    fn list_domains_includes_txt_files_only() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("tekni.txt"), "Tekni Gruppen").unwrap();
        fs::write(tmp.path().join("localfood.txt"), "Local Food").unwrap();
        fs::write(tmp.path().join("ignore.md"), "ignored").unwrap();
        set_primary_for_test(tmp.path().to_path_buf());

        let domains = list_domains().unwrap();
        assert!(domains.contains(&"tekni".to_string()));
        assert!(domains.contains(&"localfood".to_string()));
        assert!(!domains.iter().any(|d| d == "ignore"));
        clear_primary_for_test();
    }

    #[test]
    fn validate_domain_name_rejects_unsafe_inputs() {
        assert!(validate_domain_name("").is_err());
        assert!(validate_domain_name("   ").is_err());
        assert!(validate_domain_name(".hidden").is_err());
        assert!(validate_domain_name("../etc").is_err());
        assert!(validate_domain_name("foo/bar").is_err());
        assert!(validate_domain_name("foo\\bar").is_err());
        assert!(validate_domain_name("foo\0bar").is_err());
        assert!(validate_domain_name(&"x".repeat(65)).is_err());
        assert_eq!(validate_domain_name("  tekni  ").unwrap(), "tekni");
        assert!(validate_domain_name("local-food_v2").is_ok());
    }

    #[test]
    fn save_and_delete_domain_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        set_primary_for_test(tmp.path().to_path_buf());

        save_domain("acme", "Acme Corp, Wile E. Coyote").unwrap();
        let content = get_domain_content("acme").unwrap().unwrap();
        assert!(content.contains("Acme Corp"));

        let domains = list_domains().unwrap();
        assert!(domains.contains(&"acme".to_string()));

        delete_domain("acme").unwrap();
        assert!(get_domain_content("acme").unwrap().is_none());

        clear_primary_for_test();
    }

    #[test]
    fn load_prompt_returns_none_for_whitespace_only() {
        let _guard = TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("blank.txt"), "   \n\t  \n").unwrap();
        set_primary_for_test(tmp.path().to_path_buf());

        let result = load_prompt("blank").unwrap();
        assert!(result.is_none());
        clear_primary_for_test();
    }
}
