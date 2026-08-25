/// Prefix patterns — any app whose name starts with one of these is considered Teams.
/// Covers variants like "Microsoft Teams WebView", "Microsoft Teams (work or school)", etc.
const TEAMS_APP_PREFIXES: &[&str] = &["microsoft teams", "teams"];

/// Substrings that indicate a Teams audio source that is NOT a real call
/// (e.g. notification chimes). These are excluded even if the prefix matches.
const TEAMS_APP_EXCLUSIONS: &[&str] = &["notification center"];

/// Known MS Teams process names (Windows/Linux) — matched against running process list
#[cfg(not(target_os = "macos"))]
const TEAMS_PROCESS_NAMES: &[&str] = &["ms-teams.exe", "Teams.exe", "teams", "teams-for-linux"];

/// Returns true if the given app display name corresponds to MS Teams.
/// Uses prefix matching so variants like "Microsoft Teams WebView" are correctly
/// identified, while excluding notification-only sources like
/// "Microsoft Teams (Notification Center)".
pub fn is_teams_app_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    let matches_prefix = TEAMS_APP_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    let is_excluded = TEAMS_APP_EXCLUSIONS
        .iter()
        .any(|excl| lower.contains(excl));
    matches_prefix && !is_excluded
}

/// Returns true if any app in `apps` is MS Teams (macOS audio-based detection).
pub fn detect_teams_audio_active(apps: &[String]) -> bool {
    apps.iter().any(|app| is_teams_app_name(app))
}

/// Returns true if an MS Teams process is currently running (Windows/Linux).
#[cfg(not(target_os = "macos"))]
pub fn detect_teams_process_running() -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        TEAMS_PROCESS_NAMES
            .iter()
            .any(|known| name == known.to_lowercase())
    })
}

/// No-op stub for macOS (uses audio-based detection instead).
#[cfg(target_os = "macos")]
pub fn detect_teams_process_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teams_app_names_matching() {
        assert!(is_teams_app_name("Microsoft Teams"));
        assert!(is_teams_app_name("microsoft teams")); // case-insensitive
        assert!(is_teams_app_name("Teams"));
        assert!(is_teams_app_name("Microsoft Teams (work or school)"));
        assert!(is_teams_app_name("Microsoft Teams WebView"));
        assert!(!is_teams_app_name("Microsoft Teams (Notification Center)")); // notification sounds, not a call
        assert!(!is_teams_app_name("Spotify"));
        assert!(!is_teams_app_name("Zoom"));
        assert!(!is_teams_app_name("Google Chrome"));
        assert!(!is_teams_app_name(""));
    }

    #[test]
    fn test_detect_teams_audio_active() {
        let apps = vec!["Spotify".to_string(), "Microsoft Teams".to_string()];
        assert!(detect_teams_audio_active(&apps));

        let apps_no_teams: Vec<String> = vec!["Spotify".to_string(), "Safari".to_string()];
        assert!(!detect_teams_audio_active(&apps_no_teams));

        let empty: Vec<String> = vec![];
        assert!(!detect_teams_audio_active(&empty));
    }
}
