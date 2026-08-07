use super::defaults;
use super::types::Template;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use once_cell::sync::Lazy;
use std::sync::RwLock;

// Global storage for the bundled templates directory path
static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

// ponytail: override for the custom templates dir, so the save/delete tests don't
// write into the real user data directory. Nothing in the app sets it.
static CUSTOM_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// Set the bundled templates directory path (called once at app startup)
pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

/// Override where custom templates are read from and written to
#[cfg(test)]
pub fn set_custom_templates_dir(path: PathBuf) {
    info!("Custom templates directory set to: {:?}", path);
    if let Ok(mut dir) = CUSTOM_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

/// Get the user's custom templates directory path
///
/// Returns the platform-specific application data directory for custom templates:
/// - macOS: ~/Library/Application Support/Conversationaly/templates/
/// - Windows: %APPDATA%\Conversationaly\templates\
/// - Linux: ~/.config/Conversationaly/templates/
fn get_custom_templates_dir() -> Option<PathBuf> {
    if let Some(path) = CUSTOM_TEMPLATES_DIR.read().ok().and_then(|d| d.clone()) {
        return Some(path);
    }
    let mut path = dirs::data_dir()?;
    path.push("Conversationaly");
    path.push("templates");
    Some(path)
}

/// Reject ids that would escape the templates directory or produce odd filenames.
///
/// Ids arrive over IPC from the frontend and are interpolated straight into a
/// filename, so this is the trust boundary for every read, write and delete.
fn validate_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    if ok {
        Ok(())
    } else {
        Err(format!(
            "Invalid template id '{}': use letters, digits, '_' and '-' only",
            id
        ))
    }
}

/// Turn a display name into a filesystem-safe id
fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    if slug.is_empty() {
        "template".to_string()
    } else {
        slug
    }
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
    validate_id(template_id)?;

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
fn validate_and_parse_template(json_content: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse template JSON: {}", e))?;

    template.validate()?;

    Ok(template)
}

/// True if `template_id` is backed by a shipped template (embedded or bundled)
///
/// Deleting such a template restores the shipped version rather than removing it,
/// which is what lets the UI label the action "Reset" instead of "Delete".
pub fn is_builtin(template_id: &str) -> bool {
    defaults::get_builtin_template(template_id).is_some()
        || load_bundled_template(template_id).is_some()
}

/// Save a template into the user's custom templates directory
///
/// # Arguments
/// * `template_id` - Existing id to overwrite, or `None` to create a new template
///   (the id is then slugified from the template name, suffixed on collision)
/// * `template` - The template to write; validated before anything touches disk
///
/// # Returns
/// The id the template was written under
pub fn save_template(template_id: Option<&str>, template: &Template) -> Result<String, String> {
    template.validate()?;

    let dir = get_custom_templates_dir()
        .ok_or_else(|| "Could not resolve the application data directory".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create templates directory: {}", e))?;

    let id = match template_id {
        Some(id) => {
            validate_id(id)?;
            id.to_string()
        }
        None => {
            let base = slugify(&template.name);
            let taken = list_template_ids();
            let mut candidate = base.clone();
            let mut suffix = 2;
            while taken.contains(&candidate) {
                candidate = format!("{}_{}", base, suffix);
                suffix += 1;
            }
            candidate
        }
    };

    let json = serde_json::to_string_pretty(template)
        .map_err(|e| format!("Failed to serialize template: {}", e))?;

    let path = dir.join(format!("{}.json", id));
    std::fs::write(&path, json).map_err(|e| format!("Failed to write template: {}", e))?;

    info!("Saved template '{}' to {:?}", id, path);
    Ok(id)
}

/// Delete the custom copy of a template
///
/// For a shipped id this resets it to the version bundled with the app; for a
/// user-created id it removes the template entirely.
pub fn delete_template(template_id: &str) -> Result<(), String> {
    validate_id(template_id)?;

    let dir = get_custom_templates_dir()
        .ok_or_else(|| "Could not resolve the application data directory".to_string())?;
    let path = dir.join(format!("{}.json", template_id));

    match std::fs::remove_file(&path) {
        Ok(()) => {
            info!("Deleted custom template '{}'", template_id);
            Ok(())
        }
        // No custom copy: a shipped template is already at its default, anything
        // else never existed.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if is_builtin(template_id) {
                Ok(())
            } else {
                Err(format!("Template '{}' not found", template_id))
            }
        }
        Err(e) => Err(format!("Failed to delete template: {}", e)),
    }
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

    /// Covers the whole save/delete contract in one pass: overriding a shipped
    /// template, resetting it by deletion, slug generation with collisions, and
    /// the id guard. Runs against a temp dir so it never touches user data.
    #[test]
    fn test_save_override_and_delete_restores_shipped_template() {
        let temp = tempfile::tempdir().expect("temp dir");
        set_custom_templates_dir(temp.path().to_path_buf());

        let shipped = get_template("standard_meeting").expect("shipped template loads");
        assert_eq!(shipped.name, "Standard Meeting Notes");

        // A custom copy shadows the shipped one
        let mut edited = shipped.clone();
        edited.name = "Overridden".to_string();
        let id = save_template(Some("standard_meeting"), &edited).expect("save override");
        assert_eq!(id, "standard_meeting");
        assert_eq!(get_template("standard_meeting").unwrap().name, "Overridden");

        // Deleting the custom copy resets it
        delete_template("standard_meeting").expect("reset");
        assert_eq!(
            get_template("standard_meeting").unwrap().name,
            "Standard Meeting Notes"
        );

        // Creating slugifies the name and suffixes collisions
        let mut created = shipped.clone();
        created.name = "My Notes!".to_string();
        assert_eq!(save_template(None, &created).unwrap(), "my_notes");
        assert_eq!(save_template(None, &created).unwrap(), "my_notes_2");

        // A user-created template is really gone, and deleting it twice errors
        delete_template("my_notes").expect("delete custom");
        assert!(delete_template("my_notes").is_err());

        // Ids may not escape the templates directory
        assert!(save_template(Some("../evil"), &created).is_err());
        assert!(delete_template("../evil").is_err());
        assert!(get_template("../evil").is_err());

        // Invalid templates never reach disk
        let mut broken = shipped.clone();
        broken.sections.clear();
        assert!(save_template(None, &broken).is_err());
    }
}
