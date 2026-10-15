use crate::summary::templates::{self, Template, TemplateSection};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use tauri::Runtime;

/// Template metadata for UI display
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInfo {
    /// Template identifier (e.g., "daily_standup", "standard_meeting")
    pub id: String,

    /// Display name for the template
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,

    /// `builtin`, `custom`, or `edited`
    pub kind: String,
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

/// One section sent to and from the template editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSectionPayload {
    pub title: String,
    pub instruction: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_format: Option<String>,
}

/// Template fields the editor can change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePayload {
    pub name: String,
    pub description: String,
    pub sections: Vec<TemplateSectionPayload>,
}

/// A template plus the id and origin the editor needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateEditorState {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub sections: Vec<TemplateSectionPayload>,
}

fn trimmed_optional(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn template_from_payload(payload: TemplatePayload) -> Template {
    Template {
        name: payload.name.trim().to_string(),
        description: payload.description.trim().to_string(),
        sections: payload
            .sections
            .into_iter()
            .map(|section| TemplateSection {
                title: section.title.trim().to_string(),
                instruction: section.instruction.trim().to_string(),
                format: section.format.trim().to_string(),
                item_format: trimmed_optional(section.item_format),
                example_item_format: None,
            })
            .collect(),
    }
}

fn section_payload(section: &TemplateSection) -> TemplateSectionPayload {
    TemplateSectionPayload {
        title: section.title.clone(),
        instruction: section.instruction.clone(),
        format: section.format.clone(),
        item_format: section
            .item_format
            .clone()
            .or_else(|| section.example_item_format.clone()),
    }
}

fn editor_state(template_id: &str) -> Result<TemplateEditorState, String> {
    let template = templates::get_template(template_id)?;
    Ok(TemplateEditorState {
        id: template_id.to_string(),
        kind: templates::template_kind(template_id),
        name: template.name,
        description: template.description,
        sections: template.sections.iter().map(section_payload).collect(),
    })
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
        .map(|template| TemplateInfo {
            id: template.id,
            name: template.name,
            description: template.description,
            kind: template.kind,
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
    info!(
        "api_get_template_details called for template_id: {}",
        template_id
    );

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

/// Returns the full template, including section instructions, for the editor.
#[tauri::command]
pub async fn api_get_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateEditorState, String> {
    info!("api_get_template called for template_id: {}", template_id);
    editor_state(&template_id)
}

/// Saves a new template or overwrites an existing one in the user templates directory.
///
/// Pass an empty id to allocate a new id from the template name.
#[tauri::command]
pub async fn api_save_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    id: Option<String>,
    template: TemplatePayload,
) -> Result<TemplateEditorState, String> {
    info!("api_save_template called");
    let saved_id = templates::save_template(id.as_deref(), &template_from_payload(template))?;
    editor_state(&saved_id)
}

/// Deletes a custom template, or hides a shipped template.
#[tauri::command]
pub async fn api_delete_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    info!(
        "api_delete_template called for template_id: {}",
        template_id
    );
    templates::delete_template(&template_id)
}

/// Drops a user override and returns the shipped template.
#[tauri::command]
pub async fn api_reset_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateEditorState, String> {
    info!("api_reset_template called for template_id: {}", template_id);
    templates::reset_template(&template_id)?;
    editor_state(&template_id)
}

/// Restores every hidden shipped template and returns their ids.
#[tauri::command]
pub async fn api_restore_builtin_templates<R: Runtime>(
    _app: tauri::AppHandle<R>,
) -> Result<Vec<String>, String> {
    info!("api_restore_builtin_templates called");
    templates::restore_hidden_templates()
}

/// Returns how many shipped templates are hidden.
#[tauri::command]
pub async fn api_hidden_template_count<R: Runtime>(
    _app: tauri::AppHandle<R>,
) -> Result<usize, String> {
    Ok(templates::hidden_template_count())
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
