/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// Vietnamese meeting notes template
pub const VIETNAMESE_MEETING: &str = include_str!("../../../templates/vietnamese_meeting.json");

/// Vietnamese daily standup template
pub const VIETNAMESE_STANDUP: &str = include_str!("../../../templates/vietnamese_standup.json");

/// Vietnamese interview template
pub const VIETNAMESE_INTERVIEW: &str =
    include_str!("../../../templates/vietnamese_interview.json");

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("daily_standup", DAILY_STANDUP),
        ("standard_meeting", STANDARD_MEETING),
        ("vietnamese_meeting", VIETNAMESE_MEETING),
        ("vietnamese_standup", VIETNAMESE_STANDUP),
        ("vietnamese_interview", VIETNAMESE_INTERVIEW),
    ]
}

/// Get a built-in template by identifier
///
/// # Arguments
/// * `id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// The template JSON content if found, None otherwise
pub fn get_builtin_template(id: &str) -> Option<&'static str> {
    match id {
        "daily_standup" => Some(DAILY_STANDUP),
        "standard_meeting" => Some(STANDARD_MEETING),
        "vietnamese_meeting" => Some(VIETNAMESE_MEETING),
        "vietnamese_standup" => Some(VIETNAMESE_STANDUP),
        "vietnamese_interview" => Some(VIETNAMESE_INTERVIEW),
        _ => None,
    }
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    vec![
        "daily_standup",
        "standard_meeting",
        "vietnamese_meeting",
        "vietnamese_standup",
        "vietnamese_interview",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_templates_valid_json() {
        for (id, content) in get_builtin_templates() {
            let result = serde_json::from_str::<serde_json::Value>(content);
            assert!(
                result.is_ok(),
                "Built-in template '{}' contains invalid JSON: {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("daily_standup").is_some());
        assert!(get_builtin_template("standard_meeting").is_some());
        assert!(get_builtin_template("vietnamese_meeting").is_some());
        assert!(get_builtin_template("vietnamese_standup").is_some());
        assert!(get_builtin_template("vietnamese_interview").is_some());
        assert!(get_builtin_template("nonexistent").is_none());
    }

    #[test]
    fn test_list_builtin_template_ids() {
        let ids = list_builtin_template_ids();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains(&"vietnamese_meeting"));
        assert!(ids.contains(&"vietnamese_standup"));
        assert!(ids.contains(&"vietnamese_interview"));
    }
}
