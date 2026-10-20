/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// Dealership / vehicle sales call template
pub const DEALERSHIP_SALES_CALL: &str = include_str!("../../../templates/dealership_sales_call.json");

/// One-on-one meeting template
pub const ONE_ON_ONE: &str = include_str!("../../../templates/one_on_one.json");

/// Decision log template
pub const DECISION_LOG: &str = include_str!("../../../templates/decision_log.json");

/// Real-estate property walkthrough template
pub const PROPERTY_WALKTHROUGH: &str = include_str!("../../../templates/property_walkthrough.json");

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("daily_standup", DAILY_STANDUP),
        ("standard_meeting", STANDARD_MEETING),
        ("dealership_sales_call", DEALERSHIP_SALES_CALL),
        ("one_on_one", ONE_ON_ONE),
        ("decision_log", DECISION_LOG),
        ("property_walkthrough", PROPERTY_WALKTHROUGH),
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
        "dealership_sales_call" => Some(DEALERSHIP_SALES_CALL),
        "one_on_one" => Some(ONE_ON_ONE),
        "decision_log" => Some(DECISION_LOG),
        "property_walkthrough" => Some(PROPERTY_WALKTHROUGH),
        _ => None,
    }
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    vec![
        "daily_standup",
        "standard_meeting",
        "dealership_sales_call",
        "one_on_one",
        "decision_log",
        "property_walkthrough",
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
        assert!(get_builtin_template("nonexistent").is_none());
    }
}
