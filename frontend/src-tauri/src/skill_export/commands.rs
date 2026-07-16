use regex::Regex;
use std::path::{Path, PathBuf};

fn sanitize_skill_name(skill_name: &str) -> Result<String, String> {
    let mut normalized = skill_name.trim().to_lowercase();

    let re_spaces = Regex::new(r"\s+").map_err(|e| format!("Regex error: {}", e))?;
    normalized = re_spaces.replace_all(&normalized, "-").to_string();

    let re_invalid = Regex::new(r"[^a-z0-9_-]").map_err(|e| format!("Regex error: {}", e))?;
    normalized = re_invalid.replace_all(&normalized, "").to_string();

    let re_hyphen_runs = Regex::new(r"-+").map_err(|e| format!("Regex error: {}", e))?;
    normalized = re_hyphen_runs.replace_all(&normalized, "-").to_string();

    normalized = normalized.trim_matches('-').to_string();

    if normalized.is_empty() {
        return Err("Skill name cannot be empty after sanitization".to_string());
    }

    Ok(normalized)
}

fn extract_description(markdown: &str) -> Option<String> {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            break;
        }
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("---") {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn ensure_frontmatter(markdown: &str, sanitized_skill_name: &str) -> String {
    let trimmed = markdown.trim();
    if trimmed.starts_with("---") {
        return markdown.to_string();
    }

    let description = extract_description(trimmed)
        .unwrap_or_else(|| format!("Generated skill for {}", sanitized_skill_name));

    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}",
        sanitized_skill_name, description, trimmed
    )
}

fn export_skill_at_base_path(
    base_path: &Path,
    skill_name: &str,
    markdown: &str,
    overwrite: bool,
) -> Result<String, String> {
    let sanitized_skill_name = sanitize_skill_name(skill_name)?;
    let skill_dir = base_path.join(&sanitized_skill_name);
    let skill_file = skill_dir.join("SKILL.md");

    if skill_file.exists() && !overwrite {
        return Err(format!(
            "SKILL_ALREADY_EXISTS:{}",
            skill_file.display()
        ));
    }

    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create skill directory: {}", e))?;

    let content = ensure_frontmatter(markdown, &sanitized_skill_name);

    std::fs::write(&skill_file, content).map_err(|e| format!("Failed to write SKILL.md: {}", e))?;

    Ok(skill_file.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_skill(skill_name: String, markdown: String, overwrite: Option<bool>) -> Result<String, String> {
    let home_dir = dirs::home_dir().ok_or_else(|| "Failed to resolve home directory".to_string())?;
    let base_path = home_dir.join(".hermes").join("skills");

    export_skill_at_base_path(&base_path, &skill_name, &markdown, overwrite.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_skill_name() {
        let name = sanitize_skill_name(" Deploy   Staging Server! ").unwrap();
        assert_eq!(name, "deploy-staging-server");
    }

    #[test]
    fn rejects_empty_sanitized_name() {
        let result = sanitize_skill_name("!!!");
        assert!(result.is_err());
    }

    #[test]
    fn returns_conflict_without_overwrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path();

        let first = export_skill_at_base_path(base_path, "demo-skill", "# Demo", false);
        assert!(first.is_ok());

        let second = export_skill_at_base_path(base_path, "demo-skill", "# Demo 2", false);
        assert!(second.is_err());
        assert!(second.unwrap_err().starts_with("SKILL_ALREADY_EXISTS:"));
    }

    #[test]
    fn overwrites_when_enabled() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path();

        let first = export_skill_at_base_path(base_path, "demo-skill", "# Demo", false).unwrap();
        let second = export_skill_at_base_path(base_path, "demo-skill", "# Updated", true).unwrap();

        assert_eq!(first, second);

        let content = std::fs::read_to_string(PathBuf::from(second)).unwrap();
        assert!(content.contains("# Updated"));
    }

    #[test]
    fn adds_frontmatter_when_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let base_path = temp_dir.path();

        let path = export_skill_at_base_path(
            base_path,
            "test-skill",
            "## When To Use\n- during handoffs",
            false,
        )
        .unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with("---\nname: test-skill\n"));
        assert!(content.contains("description:"));
    }
}
