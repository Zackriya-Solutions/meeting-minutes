use super::defaults;
use super::types::Template;
use log::{debug, info, warn};
use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

const HIDDEN_TEMPLATES_FILE: &str = ".hidden.json";

// Global storage for the bundled templates directory path
static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

thread_local! {
    static CUSTOM_TEMPLATES_DIR_OVERRIDE: RefCell<Option<PathBuf>> = RefCell::new(None);
}

/// Set the bundled templates directory path (called once at app startup)
pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

/// Get the user's custom templates directory path
///
/// Returns the platform-specific application data directory for custom templates:
/// - macOS: ~/Library/Application Support/Meetily/templates/
/// - Windows: %APPDATA%\Meetily\templates\
/// - Linux: ~/.config/Meetily/templates/
fn get_custom_templates_dir() -> Option<PathBuf> {
    let override_dir = CUSTOM_TEMPLATES_DIR_OVERRIDE.with(|slot| slot.borrow().clone());
    if override_dir.is_some() {
        return override_dir;
    }

    let mut path = dirs::data_dir()?;
    path.push("Meetily");
    path.push("templates");
    Some(path)
}

/// A template shown in the picker and the settings list.
#[derive(Debug, Clone)]
pub struct ListedTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    /// `builtin`, `custom`, or `edited`.
    pub kind: String,
}

fn validate_template_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid template id '{id}'"))
    }
}

fn ensure_custom_dir() -> Result<PathBuf, String> {
    let dir = get_custom_templates_dir().ok_or("Could not resolve the templates directory")?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create templates directory: {e}"))?;
    Ok(dir)
}

fn atomic_write(path: &Path, contents: &str) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid template path".to_string())?;
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, contents).map_err(|e| format!("Failed to write template: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Failed to save template: {e}"));
    }
    Ok(())
}

fn custom_template_file(id: &str) -> Option<PathBuf> {
    let dir = get_custom_templates_dir()?;
    let path = dir.join(format!("{id}.json"));
    path.is_file().then_some(path)
}

fn bundled_template_file(id: &str) -> Option<PathBuf> {
    let bundled_dir = BUNDLED_TEMPLATES_DIR.read().ok()?.clone()?;
    let path = bundled_dir.join(format!("{id}.json"));
    path.is_file().then_some(path)
}

fn is_shipped_template(id: &str) -> bool {
    defaults::get_builtin_template(id).is_some() || bundled_template_file(id).is_some()
}

/// `edited` when a user copy overrides a shipped template, `custom` for a user-only
/// template, and `builtin` for an untouched shipped template.
pub fn template_kind(id: &str) -> String {
    let custom = custom_template_file(id).is_some();
    if is_shipped_template(id) && custom {
        "edited".to_string()
    } else if custom {
        "custom".to_string()
    } else {
        "builtin".to_string()
    }
}

fn hidden_file_path(dir: &Path) -> PathBuf {
    dir.join(HIDDEN_TEMPLATES_FILE)
}

fn read_hidden_ids() -> Vec<String> {
    let Some(dir) = get_custom_templates_dir() else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(hidden_file_path(&dir)) else {
        return Vec::new();
    };
    let parsed: Vec<String> = serde_json::from_str(&content).unwrap_or_default();
    let mut ids = Vec::new();
    for id in parsed {
        if validate_template_id(&id).is_ok() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn write_hidden_ids(ids: &[String]) -> Result<(), String> {
    let dir = ensure_custom_dir()?;
    let path = hidden_file_path(&dir);
    if ids.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to restore templates: {e}"))?;
        }
        return Ok(());
    }
    let json = serde_json::to_string_pretty(ids)
        .map_err(|e| format!("Failed to save hidden templates: {e}"))?;
    atomic_write(&path, &format!("{json}\n"))
}

fn hide_template(id: &str) -> Result<(), String> {
    let mut ids = read_hidden_ids();
    if !ids.iter().any(|hidden| hidden == id) {
        ids.push(id.to_string());
        ids.sort();
        write_hidden_ids(&ids)?;
    }
    Ok(())
}

fn unhide_template(id: &str) -> Result<(), String> {
    let mut ids = read_hidden_ids();
    let before = ids.len();
    ids.retain(|hidden| hidden != id);
    if ids.len() != before {
        write_hidden_ids(&ids)?;
    }
    Ok(())
}

fn remove_custom_file(id: &str) -> Result<(), String> {
    if let Some(path) = custom_template_file(id) {
        std::fs::remove_file(path).map_err(|e| format!("Failed to delete template: {e}"))?;
    }
    Ok(())
}

fn consider_template_id(ids: &mut Vec<String>, id: String, hidden: &[String]) {
    if hidden.iter().any(|hidden_id| hidden_id == &id)
        || ids.contains(&id)
        || validate_template_id(&id).is_err()
    {
        return;
    }
    ids.push(id);
}

fn slug_from_name(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !slug.is_empty() && !previous_separator {
            slug.push('_');
            previous_separator = true;
        }
        if slug.len() >= 64 {
            break;
        }
    }
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "template".to_string()
    } else {
        slug.to_string()
    }
}

fn all_template_ids() -> Vec<String> {
    let mut ids = list_template_ids();
    for id in read_hidden_ids() {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn allocate_template_id(name: &str) -> Result<String, String> {
    let base = slug_from_name(name);
    let taken = all_template_ids();
    if !taken.iter().any(|id| id == &base) {
        return Ok(base);
    }
    for suffix_number in 2..1000 {
        let suffix = format!("_{suffix_number}");
        let candidate = if base.len() + suffix.len() > 64 {
            format!("{}{suffix}", &base[..64 - suffix.len()])
        } else {
            format!("{base}{suffix}")
        };
        if validate_template_id(&candidate).is_ok() && !taken.iter().any(|id| id == &candidate) {
            return Ok(candidate);
        }
    }
    Err("Could not allocate a template id".to_string())
}

/// Save a template into the user templates directory.
///
/// An empty id allocates a new id from the template name. An existing id keeps
/// that id, including when the display name changes. Saving a shipped template
/// stores an override and makes the template visible again if it was hidden.
pub fn save_template(id: Option<&str>, template: &Template) -> Result<String, String> {
    template.validate()?;
    let id = match id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(id) => {
            validate_template_id(id)?;
            id.to_string()
        }
        None => allocate_template_id(&template.name)?,
    };

    let dir = ensure_custom_dir()?;
    let json = serde_json::to_string_pretty(template)
        .map_err(|e| format!("Failed to serialize template: {e}"))?;
    atomic_write(&dir.join(format!("{id}.json")), &format!("{json}\n"))?;
    unhide_template(&id)?;
    info!("Saved template '{id}'");
    Ok(id)
}

/// Delete a user template, or hide a shipped template and drop its override.
pub fn delete_template(id: &str) -> Result<(), String> {
    validate_template_id(id)?;
    if is_shipped_template(id) {
        remove_custom_file(id)?;
        hide_template(id)?;
        info!("Hid shipped template '{id}'");
        return Ok(());
    }
    if custom_template_file(id).is_none() {
        return Err(format!("Template '{id}' not found"));
    }
    remove_custom_file(id)?;
    info!("Deleted custom template '{id}'");
    Ok(())
}

/// Remove a user override so a shipped template returns to its original content.
pub fn reset_template(id: &str) -> Result<(), String> {
    validate_template_id(id)?;
    if !is_shipped_template(id) {
        return Err("Only a built-in template can be reset".to_string());
    }
    remove_custom_file(id)?;
    info!("Reset template '{id}' to the shipped version");
    Ok(())
}

/// Make every hidden shipped template visible again.
pub fn restore_hidden_templates() -> Result<Vec<String>, String> {
    let hidden = read_hidden_ids();
    write_hidden_ids(&[])?;
    info!("Restored {} hidden templates", hidden.len());
    Ok(hidden)
}

/// How many shipped templates are currently hidden.
pub fn hidden_template_count() -> usize {
    read_hidden_ids().len()
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
            info!(
                "Loaded bundled template '{}' from {:?}",
                template_id, template_path
            );
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
            info!(
                "Loaded custom template '{}' from {:?}",
                template_id, template_path
            );
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
fn collect_json_ids(dir: &Path, ids: &mut Vec<String>, hidden: &[String]) {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                if let Some(filename) = file_name.to_str() {
                    if filename.starts_with('.') || !filename.ends_with(".json") {
                        continue;
                    }
                    let id = filename.trim_end_matches(".json").to_string();
                    consider_template_id(ids, id, hidden);
                }
            }
        }
        Err(e) => {
            warn!("Failed to read templates directory {:?}: {}", dir, e);
        }
    }
}

pub fn list_template_ids() -> Vec<String> {
    let hidden = read_hidden_ids();
    let mut ids = Vec::new();

    for id in defaults::list_builtin_template_ids() {
        consider_template_id(&mut ids, id.to_string(), &hidden);
    }

    if let Ok(bundled_dir_lock) = BUNDLED_TEMPLATES_DIR.read() {
        if let Some(bundled_dir) = bundled_dir_lock.as_ref() {
            if bundled_dir.exists() {
                collect_json_ids(bundled_dir, &mut ids, &hidden);
            }
        }
    }

    if let Some(custom_dir) = get_custom_templates_dir() {
        if custom_dir.exists() {
            collect_json_ids(&custom_dir, &mut ids, &hidden);
        }
    }

    ids.sort();
    ids
}

/// List visible templates with their metadata and origin.
pub fn list_templates() -> Vec<ListedTemplate> {
    let mut templates = Vec::new();

    for id in list_template_ids() {
        match get_template(&id) {
            Ok(template) => {
                templates.push(ListedTemplate {
                    id: id.clone(),
                    name: template.name,
                    description: template.description,
                    kind: template_kind(&id),
                });
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

    fn sample_template(name: &str) -> Template {
        Template {
            name: name.to_string(),
            description: "A custom template".to_string(),
            sections: vec![super::super::types::TemplateSection {
                title: "Summary".to_string(),
                instruction: "Summarize the meeting".to_string(),
                format: "paragraph".to_string(),
                item_format: None,
                example_item_format: None,
            }],
        }
    }

    struct CustomDirGuard {
        dir: PathBuf,
    }

    impl Drop for CustomDirGuard {
        fn drop(&mut self) {
            CUSTOM_TEMPLATES_DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn use_temp_templates() -> CustomDirGuard {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("meetily-templates-{nanos}"));
        std::fs::create_dir_all(&dir).expect("temp templates dir");
        CUSTOM_TEMPLATES_DIR_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(dir.clone()));
        CustomDirGuard { dir }
    }

    #[test]
    fn test_save_custom_template_lists_as_custom() {
        let _guard = use_temp_templates();
        let id = save_template(None, &sample_template("Client Call")).expect("save");
        assert_eq!(id, "client_call");

        let listed = list_templates();
        let saved = listed
            .iter()
            .find(|template| template.id == id)
            .expect("listed");
        assert_eq!(saved.kind, "custom");
        assert_eq!(saved.name, "Client Call");
        assert_eq!(get_template(&id).unwrap().description, "A custom template");
    }

    #[test]
    fn test_save_override_reset_and_keep_id_when_renamed() {
        let _guard = use_temp_templates();
        let mut template = sample_template("Renamed Standup");
        let id = save_template(Some("daily_standup"), &template).expect("override");
        assert_eq!(id, "daily_standup");
        assert_eq!(template_kind("daily_standup"), "edited");
        assert_eq!(
            get_template("daily_standup").unwrap().name,
            "Renamed Standup"
        );

        template.name = "Standup Again".to_string();
        let id = save_template(Some("daily_standup"), &template).expect("rename");
        assert_eq!(id, "daily_standup");

        reset_template("daily_standup").expect("reset");
        assert_eq!(template_kind("daily_standup"), "builtin");
        assert_eq!(get_template("daily_standup").unwrap().name, "Daily Standup");
    }

    #[test]
    fn test_delete_custom_template() {
        let _guard = use_temp_templates();
        let id = save_template(None, &sample_template("One Off")).expect("save");
        delete_template(&id).expect("delete");
        assert!(!list_template_ids().contains(&id));
        assert!(get_template(&id).is_err());
    }

    #[test]
    fn test_delete_builtin_hides_and_restore_brings_it_back() {
        let _guard = use_temp_templates();
        save_template(Some("daily_standup"), &sample_template("Edited Standup")).expect("override");
        delete_template("daily_standup").expect("hide");

        assert!(!list_template_ids().contains(&"daily_standup".to_string()));
        assert_eq!(hidden_template_count(), 1);
        assert!(custom_template_file("daily_standup").is_none());

        let restored = restore_hidden_templates().expect("restore");
        assert_eq!(restored, vec!["daily_standup".to_string()]);
        assert_eq!(hidden_template_count(), 0);
        assert!(list_template_ids().contains(&"daily_standup".to_string()));
        assert_eq!(get_template("daily_standup").unwrap().name, "Daily Standup");
        assert_eq!(template_kind("daily_standup"), "builtin");
    }

    #[test]
    fn test_duplicate_name_gets_a_distinct_id() {
        let _guard = use_temp_templates();
        let first = save_template(None, &sample_template("Client Call")).expect("first");
        let second = save_template(None, &sample_template("Client Call")).expect("second");
        assert_eq!(first, "client_call");
        assert_eq!(second, "client_call_2");
    }

    #[test]
    fn test_save_rejects_invalid_template_without_writing() {
        let guard = use_temp_templates();
        let mut template = sample_template("Broken");
        template.name.clear();
        assert!(save_template(None, &template).is_err());
        assert!(std::fs::read_dir(&guard.dir)
            .unwrap()
            .flatten()
            .next()
            .is_none());
    }
}
