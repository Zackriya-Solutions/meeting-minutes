//! Deliberately small allowlists. Unrecognized apps never trigger recording.
pub fn desktop_app(bundle: &str) -> Option<&'static str> {
    [
        ("us.zoom.xos", "Zoom"),
        ("com.microsoft.teams2", "Microsoft Teams"),
        ("com.microsoft.teams", "Microsoft Teams"),
        ("com.tinyspeck.slackmacgap", "Slack"),
        ("com.apple.facetime", "FaceTime"),
        ("com.cisco.webexmeetingsapp", "Webex"),
    ]
    .into_iter()
    .find(|(id, _)| bundle == *id || bundle.starts_with(&format!("{id}.")))
    .map(|(_, label)| label)
}

pub fn browser(bundle: &str) -> Option<&'static str> {
    [
        ("com.google.Chrome", "com.google.Chrome"),
        ("com.microsoft.edgemac", "com.microsoft.edgemac"),
        ("com.brave.Browser", "com.brave.Browser"),
        ("company.thebrowser.Browser", "company.thebrowser.Browser"),
        ("com.apple.Safari", "com.apple.Safari"),
        ("com.apple.WebKit", "com.apple.Safari"),
    ]
    .into_iter()
    .find(|(id, _)| bundle == *id || bundle.starts_with(&format!("{id}.")))
    .map(|(_, id)| id)
}

pub fn conference_url(url: &str) -> bool {
    let Some(rest) = url.trim().strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    match host {
        "meet.google.com" => {
            let parts: Vec<_> = path.split('-').collect();
            parts.len() == 3
                && parts.iter().zip([3, 4, 3]).all(|(part, len)| {
                    part.len() == len && part.bytes().all(|b| b.is_ascii_lowercase())
                })
        }
        "teams.microsoft.com" | "teams.cloud.microsoft" | "teams.live.com" => {
            path.starts_with("l/meetup-join/") || path.starts_with("meet/")
        }
        _ if host == "zoom.us" || host.ends_with(".zoom.us") => {
            path.starts_with("j/") || path.starts_with("wc/")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_apps_and_helpers_but_not_unrelated_capture() {
        assert_eq!(
            desktop_app("com.microsoft.teams2.helper"),
            Some("Microsoft Teams")
        );
        assert_eq!(desktop_app("com.meetily.ai"), None);
        assert_eq!(desktop_app("com.example.dictation"), None);
        assert_eq!(desktop_app("com.microsoft.teams2fake"), None);
        assert_eq!(
            browser("com.google.Chrome.helper"),
            Some("com.google.Chrome")
        );
    }
    #[test]
    fn recognizes_conference_links_without_lookalike_domains() {
        for url in [
            "https://meet.google.com/abc-defg-hij?authuser=0",
            "https://us02web.zoom.us/j/123",
            "https://teams.microsoft.com/l/meetup-join/abc",
        ] {
            assert!(conference_url(url), "{url}");
        }
        for url in [
            "https://meet.google.com/",
            "https://meet.google.com/landing",
            "https://zoom.us/",
            "https://zoom.us.evil.test/j/123",
            "https://evil.test/meet.google.com/abc-defg-hij",
            "http://meet.google.com/abc-defg-hij",
            "https://teams.microsoft.com/v2/",
        ] {
            assert!(!conference_url(url), "{url}");
        }
    }
}
