use log::Metadata;

const PULSETALK_TARGET_PREFIX: &str = "pulsetalk::";
const DICTATION_MODULE_PREFIX: &str = "app_lib::dictation";

/// Keep the on-disk support log intentionally narrower than development stdout.
/// This prevents inherited meeting, profile, and HTTP content from becoming a
/// second persistent data store while retaining the full dictation lifecycle.
pub(crate) fn should_persist(metadata: &Metadata<'_>) -> bool {
    let target = metadata.target();
    target.starts_with(PULSETALK_TARGET_PREFIX)
        || target == DICTATION_MODULE_PREFIX
        || target.starts_with("app_lib::dictation::")
}

#[cfg(test)]
mod tests {
    use super::should_persist;
    use log::{Level, Metadata};

    fn metadata(target: &'static str, level: Level) -> Metadata<'static> {
        Metadata::builder().target(target).level(level).build()
    }

    #[test]
    fn keeps_pulsetalk_lifecycle_and_dictation_records() {
        assert!(should_persist(&metadata(
            "pulsetalk::lifecycle",
            Level::Info
        )));
        assert!(should_persist(&metadata(
            "app_lib::dictation::coordinator",
            Level::Error
        )));
    }

    #[test]
    fn rejects_inherited_content_bearing_modules_even_on_error() {
        assert!(!should_persist(&metadata(
            "app_lib::api::api",
            Level::Error
        )));
        assert!(!should_persist(&metadata(
            "app_lib::notifications::manager",
            Level::Warn
        )));
        assert!(!should_persist(&metadata(
            "app_lib::summary::service",
            Level::Error
        )));
    }
}
