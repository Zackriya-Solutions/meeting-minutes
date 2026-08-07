use crate::summary::templates::{self, Template};
use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tracing::info;

/// Template metadata for UI display
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Template identifier (e.g., "daily_standup", "standard_meeting")
    pub id: String,

    /// Display name for the template
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,

    /// True if the app ships this template, so deleting it resets rather than removes
    pub builtin: bool,
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
        .map(|(id, name, description)| TemplateInfo {
            builtin: templates::is_builtin(&id),
            id,
            name,
            description,
        })
        .collect();

    info!("Found {} available templates", template_infos.len());

    Ok(template_infos)
}

/// Gets the full body of a template, for the editor and for previews
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup")
///
/// # Returns
/// The template with every section field the editor needs
#[tauri::command]
pub async fn api_get_template_details<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<Template, String> {
    info!("api_get_template_details called for template_id: {}", template_id);

    templates::get_template(&template_id)
}

/// Creates or overwrites a user template
///
/// Writes to the user's custom templates directory, which takes precedence over
/// the templates shipped with the app. Saving under a shipped id therefore
/// overrides it; `api_delete_template` undoes that.
///
/// # Arguments
/// * `template_id` - Existing id to overwrite, or `None` to create a new template
/// * `template` - Template body; validated before anything is written
///
/// # Returns
/// The id the template was saved under
#[tauri::command]
pub async fn api_save_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: Option<String>,
    template: Template,
) -> Result<String, String> {
    info!("api_save_template called for template_id: {:?}", template_id);

    templates::save_template(template_id.as_deref(), &template)
}

/// Deletes a user template
///
/// For a shipped template this resets it to the bundled version; for a
/// user-created one it removes it entirely.
#[tauri::command]
pub async fn api_delete_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    info!("api_delete_template called for template_id: {}", template_id);

    templates::delete_template(&template_id)
}
