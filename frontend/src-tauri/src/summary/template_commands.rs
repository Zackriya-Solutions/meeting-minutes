use crate::summary::templates::{self, TemplateSection, TemplateSource};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Runtime};
use tracing::{info, warn};

/// Emitted after a custom template is saved or deleted so open template pickers
/// can refresh without an app restart.
const TEMPLATES_CHANGED_EVENT: &str = "templates-changed";

/// Template metadata for UI display
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Template identifier (e.g., "daily_standup", "standard_meeting")
    pub id: String,

    /// Display name for the template
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,

    /// Where the template resolves from: builtin, bundled or custom
    pub source: TemplateSource,

    /// Whether the template editor may modify or delete it
    pub editable: bool,
}

/// Detailed template structure for preview/debugging
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateDetails {
    /// Template identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// Description
    pub description: String,

    /// List of section titles in order
    pub sections: Vec<String>,
}

/// Lists all available templates
///
/// Returns templates from both built-in (embedded) and custom (user data directory) sources.
/// Templates are automatically discovered - no code changes needed to add new templates.
///
/// # Returns
/// Vector of TemplateInfo with id, name, and description for each template
#[tauri::command]
pub async fn api_list_templates<R: Runtime>(
    _app: tauri::AppHandle<R>,
) -> Result<Vec<TemplateInfo>, String> {
    info!("api_list_templates called");

    let templates = templates::list_templates();

    let template_infos: Vec<TemplateInfo> = templates
        .into_iter()
        .map(|(id, name, description)| {
            let source = templates::template_source(&id).unwrap_or(TemplateSource::Builtin);
            TemplateInfo {
                id,
                name,
                description,
                source,
                editable: source == TemplateSource::Custom,
            }
        })
        .collect();

    info!("Found {} available templates", template_infos.len());

    Ok(template_infos)
}

/// Gets detailed information about a specific template
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup")
///
/// # Returns
/// TemplateDetails with full template structure
#[tauri::command]
pub async fn api_get_template_details<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateDetails, String> {
    info!("api_get_template_details called for template_id: {}", template_id);

    let template = templates::get_template(&template_id)?;

    let section_titles: Vec<String> = template
        .sections
        .iter()
        .map(|section| section.title.clone())
        .collect();

    let details = TemplateDetails {
        id: template_id,
        name: template.name,
        description: template.description,
        sections: section_titles,
    };

    info!("Retrieved template details for '{}'", details.name);

    Ok(details)
}

/// Validates a custom template JSON string
///
/// Useful for template editor UI or validation before saving custom templates
///
/// # Arguments
/// * `template_json` - Raw JSON string of the template
///
/// # Returns
/// Ok(template_name) if valid, Err(error_message) if invalid
#[tauri::command]
pub async fn api_validate_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_json: String,
) -> Result<String, String> {
    info!("api_validate_template called");

    match templates::validate_and_parse_template(&template_json) {
        Ok(template) => {
            info!("Template '{}' validated successfully", template.name);
            Ok(template.name)
        }
        Err(e) => {
            warn!("Template validation failed: {}", e);
            Err(e)
        }
    }
}

/// Full template definition for the editor.
///
/// `api_get_template_details` deliberately returns section *titles* only, which
/// is enough for a preview but not for editing; this returns the whole thing.
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateWithSource {
    pub id: String,
    pub name: String,
    pub description: String,
    pub sections: Vec<TemplateSection>,
    pub source: TemplateSource,
    /// False for builtin and bundled templates, which must be duplicated first.
    pub editable: bool,
    /// A free id to duplicate this template under, e.g. `standard_meeting_copy`.
    pub suggested_copy_id: String,
}

/// Gets a template's full definition plus where it came from.
#[tauri::command]
pub async fn api_get_template_source<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateWithSource, String> {
    info!("api_get_template_source called for '{}'", template_id);

    let template = templates::get_template(&template_id)?;
    let source = templates::template_source(&template_id)
        .ok_or_else(|| format!("Template '{}' not found", template_id))?;

    Ok(TemplateWithSource {
        suggested_copy_id: templates::suggest_available_id(&template_id),
        id: template_id,
        name: template.name,
        description: template.description,
        sections: template.sections,
        source,
        editable: source == TemplateSource::Custom,
    })
}

/// Saves a custom template to the user's templates directory.
///
/// Rejects ids that belong to a builtin or bundled template: a same-named custom
/// file would silently shadow the original, which is confusing when it happened
/// by accident from the editor.
#[tauri::command]
pub async fn api_save_custom_template<R: Runtime>(
    app: tauri::AppHandle<R>,
    template_id: String,
    template_json: String,
) -> Result<TemplateInfo, String> {
    info!("api_save_custom_template called for '{}'", template_id);

    let template = templates::save_custom_template(&template_id, &template_json).inspect_err(
        |e| warn!("Failed to save template '{}': {}", template_id, e),
    )?;

    if let Err(e) = app.emit(TEMPLATES_CHANGED_EVENT, &template_id) {
        warn!("Failed to emit {}: {}", TEMPLATES_CHANGED_EVENT, e);
    }

    Ok(TemplateInfo {
        id: template_id,
        name: template.name,
        description: template.description,
        source: TemplateSource::Custom,
        editable: true,
    })
}

/// Deletes a custom template. Builtin and bundled templates cannot be deleted.
#[tauri::command]
pub async fn api_delete_custom_template<R: Runtime>(
    app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    info!("api_delete_custom_template called for '{}'", template_id);

    templates::delete_custom_template(&template_id).inspect_err(|e| {
        warn!("Failed to delete template '{}': {}", template_id, e)
    })?;

    if let Err(e) = app.emit(TEMPLATES_CHANGED_EVENT, &template_id) {
        warn!("Failed to emit {}: {}", TEMPLATES_CHANGED_EVENT, e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_templates() {
        // This test requires the templates to be embedded/available
        // In a real test environment, you might want to mock the templates module

        // For now, just verify the function compiles and runs
        // You can expand this with more specific assertions
    }

    #[tokio::test]
    async fn test_validate_template_valid() {
        let valid_json = r#"
        {
            "name": "Test Template",
            "description": "A test template",
            "sections": [
                {
                    "title": "Summary",
                    "instruction": "Provide a summary",
                    "format": "paragraph"
                }
            ]
        }"#;

        // Mock app handle would be needed for actual testing
        // For now, test the validation logic directly
        let result = templates::validate_and_parse_template(valid_json);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_template_invalid() {
        let invalid_json = "invalid json";

        let result = templates::validate_and_parse_template(invalid_json);
        assert!(result.is_err());
    }
}
