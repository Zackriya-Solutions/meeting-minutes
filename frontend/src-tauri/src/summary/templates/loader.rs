use super::defaults;
use super::types::Template;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use once_cell::sync::Lazy;
use std::sync::RwLock;

// Global storage for the bundled templates directory path
static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// Set the bundled templates directory path (called once at app startup)
pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

// Test-only override for the custom templates directory, so template writes can
// be exercised against a temp dir instead of the real user profile.
#[cfg(test)]
static CUSTOM_TEMPLATES_DIR_OVERRIDE: Lazy<RwLock<Option<PathBuf>>> =
    Lazy::new(|| RwLock::new(None));

/// Get the user's custom templates directory path
///
/// Returns the platform-specific application data directory for custom templates:
/// - macOS: ~/Library/Application Support/Meetily/templates/
/// - Windows: %APPDATA%\Meetily\templates\
/// - Linux: ~/.config/Meetily/templates/
pub fn get_custom_templates_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Ok(override_dir) = CUSTOM_TEMPLATES_DIR_OVERRIDE.read() {
        if let Some(path) = override_dir.as_ref() {
            return Some(path.clone());
        }
    }

    let mut path = dirs::data_dir()?;
    path.push("Meetily");
    path.push("templates");
    Some(path)
}

/// Where a template id resolves from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateSource {
    /// Embedded in the binary.
    Builtin,
    /// Shipped in the app resources directory.
    Bundled,
    /// Written by the user into the app data directory.
    Custom,
}

/// Resolves where a template id comes from, using the same precedence as
/// [`get_template`]: custom shadows bundled, which shadows builtin.
pub fn template_source(template_id: &str) -> Option<TemplateSource> {
    if load_custom_template(template_id).is_some() {
        Some(TemplateSource::Custom)
    } else if load_bundled_template(template_id).is_some() {
        Some(TemplateSource::Bundled)
    } else if defaults::get_builtin_template(template_id).is_some() {
        Some(TemplateSource::Builtin)
    } else {
        None
    }
}

/// True when a template id is already claimed by a builtin or bundled template.
///
/// Saving a custom template under such an id would silently shadow the original,
/// so [`save_custom_template`] refuses it.
pub fn is_reserved_template_id(template_id: &str) -> bool {
    defaults::get_builtin_template(template_id).is_some()
        || load_bundled_template(template_id).is_some()
}

/// Rejects ids that would escape the templates directory or produce an
/// unusable filename.
pub fn validate_template_id(template_id: &str) -> Result<(), String> {
    if template_id.is_empty() {
        return Err("Template id cannot be empty".to_string());
    }

    if template_id.len() > 64 {
        return Err("Template id cannot be longer than 64 characters".to_string());
    }

    if !template_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "Invalid template id '{}'. Use lowercase letters, digits, '_' and '-' only",
            template_id
        ));
    }

    Ok(())
}

/// Writes a custom template to the user's templates directory.
///
/// The JSON is validated first, and the id is refused if it belongs to a builtin
/// or bundled template.
pub fn save_custom_template(template_id: &str, json_content: &str) -> Result<Template, String> {
    validate_template_id(template_id)?;

    if is_reserved_template_id(template_id) {
        return Err(format!(
            "'{}' is a built-in template id. Duplicate it under a different id instead of overwriting it.",
            template_id
        ));
    }

    let template = validate_and_parse_template(json_content)?;

    let dir = get_custom_templates_dir()
        .ok_or_else(|| "Could not locate the application data directory".to_string())?;

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create templates directory {:?}: {}", dir, e))?;

    let path = dir.join(format!("{}.json", template_id));
    let pretty = serde_json::to_string_pretty(&template)
        .map_err(|e| format!("Failed to serialize template: {}", e))?;

    std::fs::write(&path, pretty)
        .map_err(|e| format!("Failed to write template {:?}: {}", path, e))?;

    info!("Saved custom template '{}' to {:?}", template_id, path);
    Ok(template)
}

/// Deletes a custom template. Builtin and bundled templates cannot be deleted.
pub fn delete_custom_template(template_id: &str) -> Result<(), String> {
    validate_template_id(template_id)?;

    let dir = get_custom_templates_dir()
        .ok_or_else(|| "Could not locate the application data directory".to_string())?;
    let path = dir.join(format!("{}.json", template_id));

    if !path.exists() {
        return Err(format!(
            "'{}' is not a custom template, so it cannot be deleted",
            template_id
        ));
    }

    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete template {:?}: {}", path, e))?;

    info!("Deleted custom template '{}'", template_id);
    Ok(())
}

/// Suggests an unused id derived from `base`, e.g. `standard_meeting_copy`.
pub fn suggest_available_id(base: &str) -> String {
    let normalized: String = base
        .chars()
        .map(|c| {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
                lower
            } else {
                '_'
            }
        })
        .collect();
    let normalized = normalized.trim_matches('_').to_string();
    let stem = if normalized.is_empty() {
        "template".to_string()
    } else {
        normalized
    };

    let existing = list_template_ids();
    let first = format!("{}_copy", stem);
    if !existing.contains(&first) {
        return first;
    }

    for n in 2..1000 {
        let candidate = format!("{}_copy_{}", stem, n);
        if !existing.contains(&candidate) {
            return candidate;
        }
    }

    format!("{}_copy_{}", stem, existing.len() + 1)
}

/// Load a template from the bundled resources directory
///
/// # Arguments
/// * `template_id` - Template identifier (without .json extension)
///
/// # Returns
/// The template JSON content if found, None otherwise
fn load_bundled_template(template_id: &str) -> Option<String> {
    let bundled_dir = BUNDLED_TEMPLATES_DIR.read().ok()?.clone()?;
    let template_path = bundled_dir.join(format!("{}.json", template_id));

    debug!("Checking for bundled template at: {:?}", template_path);

    match std::fs::read_to_string(&template_path) {
        Ok(content) => {
            info!("Loaded bundled template '{}' from {:?}", template_id, template_path);
            Some(content)
        }
        Err(e) => {
            debug!("No bundled template '{}' found: {}", template_id, e);
            None
        }
    }
}

/// Load a template from the user's custom templates directory
///
/// # Arguments
/// * `template_id` - Template identifier (without .json extension)
///
/// # Returns
/// The template JSON content if found, None otherwise
fn load_custom_template(template_id: &str) -> Option<String> {
    let custom_dir = get_custom_templates_dir()?;
    let template_path = custom_dir.join(format!("{}.json", template_id));

    debug!("Checking for custom template at: {:?}", template_path);

    match std::fs::read_to_string(&template_path) {
        Ok(content) => {
            info!("Loaded custom template '{}' from {:?}", template_id, template_path);
            Some(content)
        }
        Err(e) => {
            debug!("No custom template '{}' found: {}", template_id, e);
            None
        }
    }
}

/// Load and parse a template by identifier
///
/// This function implements a fallback strategy:
/// 1. Check user's custom templates directory
/// 2. Check bundled resources directory (app templates)
/// 3. Fall back to built-in embedded templates
/// 4. Return error if not found in any location
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// Parsed and validated Template struct
pub fn get_template(template_id: &str) -> Result<Template, String> {
    info!("Loading template: {}", template_id);

    // Try custom template first, then bundled, then built-in
    let json_content = if let Some(custom_content) = load_custom_template(template_id) {
        debug!("Using custom template for '{}'", template_id);
        custom_content
    } else if let Some(bundled_content) = load_bundled_template(template_id) {
        debug!("Using bundled template for '{}'", template_id);
        bundled_content
    } else if let Some(builtin_content) = defaults::get_builtin_template(template_id) {
        debug!("Using built-in template for '{}'", template_id);
        builtin_content.to_string()
    } else {
        return Err(format!(
            "Template '{}' not found. Available templates: {}",
            template_id,
            list_template_ids().join(", ")
        ));
    };

    // Parse and validate
    validate_and_parse_template(&json_content)
}

/// Validate and parse template JSON
///
/// # Arguments
/// * `json_content` - Raw JSON string
///
/// # Returns
/// Parsed and validated Template struct
pub fn validate_and_parse_template(json_content: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse template JSON: {}", e))?;

    template.validate()?;

    Ok(template)
}

/// List all available template identifiers
///
/// Returns a combined list of:
/// - Built-in template IDs
/// - Bundled template IDs (from app resources)
/// - Custom template IDs (from user's data directory)
pub fn list_template_ids() -> Vec<String> {
    let mut ids: Vec<String> = defaults::list_builtin_template_ids()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    // Add bundled templates if directory is set
    if let Ok(bundled_dir_lock) = BUNDLED_TEMPLATES_DIR.read() {
        if let Some(bundled_dir) = bundled_dir_lock.as_ref() {
            if bundled_dir.exists() {
                match std::fs::read_dir(bundled_dir) {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            if let Some(filename) = entry.file_name().to_str() {
                                if filename.ends_with(".json") {
                                    let id = filename.trim_end_matches(".json").to_string();
                                    if !ids.contains(&id) {
                                        ids.push(id);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to read bundled templates directory: {}", e);
                    }
                }
            }
        }
    }

    // Add custom templates if directory exists
    if let Some(custom_dir) = get_custom_templates_dir() {
        if custom_dir.exists() {
            match std::fs::read_dir(&custom_dir) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if let Some(filename) = entry.file_name().to_str() {
                            if filename.ends_with(".json") {
                                let id = filename.trim_end_matches(".json").to_string();
                                if !ids.contains(&id) {
                                    ids.push(id);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read custom templates directory: {}", e);
                }
            }
        }
    }

    ids.sort();
    ids
}

/// List all available templates with their metadata
///
/// Returns a list of (id, name, description) tuples
pub fn list_templates() -> Vec<(String, String, String)> {
    let mut templates = Vec::new();

    for id in list_template_ids() {
        match get_template(&id) {
            Ok(template) => {
                templates.push((id, template.name, template.description));
            }
            Err(e) => {
                warn!("Failed to load template '{}': {}", id, e);
            }
        }
    }

    templates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_template() {
        let template = get_template("daily_standup");
        assert!(template.is_ok());

        let template = template.unwrap();
        assert_eq!(template.name, "Daily Standup");
        assert!(!template.sections.is_empty());
    }

    #[test]
    fn test_get_nonexistent_template() {
        let result = get_template("nonexistent_template");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_template_ids() {
        let ids = list_template_ids();
        assert!(ids.contains(&"daily_standup".to_string()));
        assert!(ids.contains(&"standard_meeting".to_string()));
    }

    #[test]
    fn test_validate_invalid_json() {
        let result = validate_and_parse_template("invalid json");
        assert!(result.is_err());
    }

    const VALID_TEMPLATE_JSON: &str = r#"{
        "name": "Weekly Sync",
        "description": "A weekly team sync",
        "sections": [
            { "title": "Özet", "instruction": "Kısa bir özet yaz", "format": "paragraph" }
        ]
    }"#;

    // The directory override is process-global, so tests that use it must not
    // run concurrently with one another.
    static OVERRIDE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Points the custom templates directory at a fresh temp dir for the
    /// duration of the guard, and restores it afterwards.
    struct TempCustomDir {
        path: PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TempCustomDir {
        fn new(label: &str) -> Self {
            // A poisoned lock only means an earlier test panicked; the override
            // is restored by Drop either way, so recovering is safe here.
            let lock = OVERRIDE_GUARD.lock().unwrap_or_else(|e| e.into_inner());

            let path = std::env::temp_dir().join(format!("meetily-template-tests-{}", label));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("temp dir should be creatable");

            *CUSTOM_TEMPLATES_DIR_OVERRIDE
                .write()
                .expect("override lock") = Some(path.clone());

            Self { path, _lock: lock }
        }
    }

    impl Drop for TempCustomDir {
        fn drop(&mut self) {
            *CUSTOM_TEMPLATES_DIR_OVERRIDE
                .write()
                .expect("override lock") = None;
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_validate_template_id_rules() {
        assert!(validate_template_id("weekly_sync").is_ok());
        assert!(validate_template_id("weekly-sync-2").is_ok());

        assert!(validate_template_id("").is_err());
        assert!(validate_template_id("Weekly Sync").is_err(), "spaces rejected");
        assert!(validate_template_id("WeeklySync").is_err(), "uppercase rejected");
        assert!(validate_template_id("../escape").is_err(), "traversal rejected");
        assert!(validate_template_id("a/b").is_err(), "separator rejected");
        assert!(validate_template_id(&"a".repeat(65)).is_err(), "over-long rejected");
    }

    #[test]
    fn test_builtin_ids_are_reserved() {
        assert!(is_reserved_template_id("standard_meeting"));
        assert!(is_reserved_template_id("daily_standup"));
        assert!(!is_reserved_template_id("some_template_nobody_has"));
    }

    #[test]
    fn test_custom_template_lifecycle() {
        let _guard = TempCustomDir::new("lifecycle");

        // Save
        let saved = save_custom_template("weekly_sync", VALID_TEMPLATE_JSON)
            .expect("saving a fresh id should succeed");
        assert_eq!(saved.name, "Weekly Sync");

        // Read back through the normal resolution path
        let loaded = get_template("weekly_sync").expect("saved template should load");
        assert_eq!(loaded.sections.len(), 1);
        assert_eq!(loaded.sections[0].title, "Özet");
        assert_eq!(template_source("weekly_sync"), Some(TemplateSource::Custom));
        assert!(list_template_ids().contains(&"weekly_sync".to_string()));

        // Overwriting the same custom id is allowed
        save_custom_template("weekly_sync", VALID_TEMPLATE_JSON)
            .expect("overwriting one's own custom template should succeed");

        // Delete
        delete_custom_template("weekly_sync").expect("deleting a custom template should succeed");
        assert!(get_template("weekly_sync").is_err());
        assert_eq!(template_source("weekly_sync"), None);
    }

    #[test]
    fn test_save_rejects_builtin_id() {
        let _guard = TempCustomDir::new("reserved");

        let error = save_custom_template("standard_meeting", VALID_TEMPLATE_JSON)
            .expect_err("built-in ids must be refused");
        assert!(
            error.contains("standard_meeting"),
            "error should name the id: {error}"
        );

        // The builtin must still resolve to the builtin.
        assert_eq!(
            template_source("standard_meeting"),
            Some(TemplateSource::Builtin)
        );
    }

    #[test]
    fn test_save_rejects_invalid_json_and_bad_ids() {
        let _guard = TempCustomDir::new("invalid");

        assert!(save_custom_template("weekly_sync", "not json").is_err());
        assert!(
            save_custom_template("weekly_sync", r#"{"name":"","description":"d","sections":[]}"#)
                .is_err(),
            "schema validation must run before writing"
        );
        assert!(save_custom_template("../escape", VALID_TEMPLATE_JSON).is_err());

        // Nothing should have been written.
        assert!(get_template("weekly_sync").is_err());
    }

    #[test]
    fn test_delete_rejects_non_custom_templates() {
        let _guard = TempCustomDir::new("delete-builtin");

        let error = delete_custom_template("standard_meeting")
            .expect_err("built-in templates cannot be deleted");
        assert!(error.contains("standard_meeting"), "got: {error}");
    }

    #[test]
    fn test_suggest_available_id_avoids_collisions() {
        assert_eq!(suggest_available_id("standard_meeting"), "standard_meeting_copy");
        assert_eq!(suggest_available_id("Weekly Sync!"), "weekly_sync_copy");
        assert_eq!(suggest_available_id(""), "template_copy");
    }
}
