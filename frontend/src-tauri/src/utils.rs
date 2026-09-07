pub fn format_timestamp(seconds: f64) -> String {
    let total_seconds = seconds as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

pub(crate) fn log_snippet(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut snippet = String::new();

    for _ in 0..max_chars {
        let Some(character) = chars.next() else {
            return snippet;
        };
        snippet.push(if character.is_control() { ' ' } else { character });
    }

    if chars.next().is_some() {
        snippet.push('…');
    }

    snippet
}

pub(crate) fn url_origin_for_log(raw: &str) -> String {
    let Ok(url) = url::Url::parse(raw) else {
        return "<invalid-url>".to_string();
    };
    let Some(host) = url.host_str() else {
        return "<invalid-url>".to_string();
    };

    match url.port() {
        Some(port) => format!("{}://{}:{}", url.scheme(), host, port),
        None => format!("{}://{}", url.scheme(), host),
    }
}

#[cfg(test)]
mod tests {
    use super::{log_snippet, url_origin_for_log};

    #[test]
    fn logging_helpers_bound_content_and_strip_url_secrets() {
        assert_eq!(log_snippet("short", 10), "short");
        assert_eq!(log_snippet("résumé", 3), "rés…");
        assert_eq!(log_snippet("a\r\nb\u{7f}", 5), "a  b ");
        assert_eq!(log_snippet("nonempty", 0), "…");
        assert_eq!(
            url_origin_for_log("https://user:secret@example.com:8443/path?token=secret#fragment"),
            "https://example.com:8443"
        );
        assert_eq!(url_origin_for_log("not a URL"), "<invalid-url>");
    }
}

/// Opens macOS System Settings to a specific privacy preference pane
#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn open_system_settings(preference_pane: String) -> Result<(), String> {
    use std::process::Command;

    // Construct the URL for System Settings
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{}", preference_pane);

    // Use the 'open' command on macOS to open the URL
    Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open system settings: {}", e))?;

    Ok(())
} 