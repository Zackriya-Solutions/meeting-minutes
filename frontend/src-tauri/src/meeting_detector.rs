// Auto-detect meetings and toggle recording without user input.
//
// Signals watched:
//   1. Process launches — Zoom, Teams, Slack, Discord, Webex, GoToMeeting desktop apps
//   2. Browser tab URLs — Google Meet, Zoom-web, Teams-web, Whereby (Chrome/Safari/Arc/Edge)
//
// State machine:
//   - "auto-started" flag tracks whether WE fired the record. If the user
//     manually stops recording, the flag clears so we don't try to stop again.
//   - We only auto-stop if we auto-started; a manually-started recording is
//     never touched by the detector.
//
// Polling interval: 10s (light on CPU, catches meeting starts within 10s).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Runtime};

use crate::tray;

static AUTO_STARTED: AtomicBool = AtomicBool::new(false);

/// Apps whose presence in the process list is a "call-only" signal — these
/// apps are launched to make/join a call, not left idling. Zoom is EXCLUDED
/// from this list even though it's the most common — Zoom often idles with
/// its home screen open, so we check its window title instead (see below).
const CALL_ONLY_PROCESSES: &[&str] = &[
    "Webex",             // Webex
    "GoToMeeting",       // GoTo Meeting
    "BlueJeans",         // BlueJeans
    "GoogleMeet",        // Google Meet standalone app (if installed)
];

/// Substring in Zoom's window title that means we're actually in a meeting
/// (as opposed to sitting on the home screen). Zoom uses "Zoom Meeting" for
/// active meetings, and "Zoom" / "Zoom - Free Account" etc. when idle.
const ZOOM_IN_MEETING_TITLE_SUBSTRING: &str = "Meeting";

/// Substrings that mark a browser tab URL as a meeting-in-progress.
const MEETING_URL_PATTERNS: &[&str] = &[
    "meet.google.com/",             // Google Meet (path segment after the / is the meeting id)
    "zoom.us/j/",                   // Zoom join links
    "zoom.us/wc/",                  // Zoom web-client meeting
    "app.zoom.us/wc/",              // Zoom web-client alt host
    "teams.microsoft.com/l/meetup-join", // Teams meetup-join
    "teams.live.com/meet",          // Teams personal
    "teams.microsoft.com/v2/",      // Teams v2 web client
    "whereby.com/",                 // Whereby room URLs
    "app.gather.town/app/",         // Gather Town
    "around.co/r/",                 // Around
];

/// Browsers we can query for open tabs via AppleScript.
const BROWSERS: &[&str] = &[
    "Google Chrome",
    "Google Chrome Canary",
    "Brave Browser",
    "Microsoft Edge",
    "Safari",
    "Arc",
    "Wavebox",
    "Vivaldi",
    "Orion",
    "SigmaOS",
];

fn matching_meeting_process() -> Option<String> {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    // 1) Call-only apps: presence = meeting.
    let processes = sys.processes();
    for proc in processes.values() {
        let name = proc.name().to_string_lossy();
        for target in CALL_ONLY_PROCESSES {
            if name == *target {
                return Some(name.to_string());
            }
        }
    }

    // 2) Zoom: only fires if its main window title contains "Meeting" (idle
    // Zoom shows "Zoom" or "Zoom - Free Account", active meeting shows
    // "Zoom Meeting"). Skips the false-positive of idle Zoom.
    let zoom_running = processes.values().any(|p| p.name().to_string_lossy() == "zoom.us");
    if zoom_running && zoom_window_indicates_meeting() {
        return Some("Zoom (in meeting)".into());
    }

    None
}

/// Ask System Events for Zoom's window title and check if it contains "Meeting".
fn zoom_window_indicates_meeting() -> bool {
    let script = r#"
    tell application "System Events"
        try
            tell process "zoom.us"
                set winTitles to name of every window
                repeat with t in winTitles
                    if t contains "Meeting" then return "yes"
                end repeat
            end tell
        end try
        return "no"
    end tell
    "#;
    match Command::new("osascript").arg("-e").arg(script).output() {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim() == "yes"
        }
        _ => false,
    }
}

fn get_tab_urls(browser: &str) -> Vec<String> {
    // Only run the AppleScript if the browser is actually running (fast path).
    let is_running = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"System Events\" to (name of processes) contains \"{}\"",
            browser
        ))
        .output()
        .ok()
        .and_then(|o| Some(String::from_utf8_lossy(&o.stdout).trim() == "true"))
        .unwrap_or(false);

    if !is_running {
        return Vec::new();
    }

    let script = if browser == "Safari" {
        // Safari's tab addressability is slightly different from Chromium-based browsers
        format!(
            r#"tell application "{}"
                set urlList to {{}}
                try
                    repeat with w in windows
                        repeat with t in tabs of w
                            try
                                set end of urlList to URL of t
                            end try
                        end repeat
                    end repeat
                end try
                return urlList
            end tell"#,
            browser
        )
    } else {
        format!(
            r#"tell application "{}"
                set urlList to {{}}
                try
                    repeat with w in windows
                        repeat with t in tabs of w
                            try
                                set end of urlList to URL of t
                            end try
                        end repeat
                    end repeat
                end try
                return urlList
            end tell"#,
            browser
        )
    };

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .split(", ")
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn matching_meeting_url() -> Option<String> {
    for browser in BROWSERS {
        let urls = get_tab_urls(browser);
        for url in urls {
            for pat in MEETING_URL_PATTERNS {
                if url.contains(pat) {
                    return Some(url.clone());
                }
            }
        }
    }
    None
}

/// Spawn the background watcher. Runs for the lifetime of the app.
pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        log::info!("[auto-detect] meeting watcher started (interval=10s)");

        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            let proc_hit = matching_meeting_process();
            let url_hit = matching_meeting_url();
            let meeting_active = proc_hit.is_some() || url_hit.is_some();
            let currently_recording = crate::is_recording().await;
            let we_started = AUTO_STARTED.load(Ordering::Relaxed);

            match (meeting_active, currently_recording, we_started) {
                // Meeting detected, not recording → auto-start
                (true, false, _) => {
                    let reason = match (&proc_hit, &url_hit) {
                        (Some(p), _) => format!("process={}", p),
                        (_, Some(u)) => format!("url={}", u.chars().take(80).collect::<String>()),
                        _ => "(unknown)".into(),
                    };
                    log::info!("[auto-detect] meeting detected ({}) → auto-starting recording", reason);
                    tray::toggle_recording_handler(&app);
                    AUTO_STARTED.store(true, Ordering::Relaxed);
                }

                // No meeting active, we started this recording → auto-stop
                (false, true, true) => {
                    log::info!("[auto-detect] no meeting active → stopping our auto-recording");
                    tray::toggle_recording_handler(&app);
                    AUTO_STARTED.store(false, Ordering::Relaxed);
                }

                // Recording stopped (probably by user) → clear our flag
                (_, false, true) => {
                    AUTO_STARTED.store(false, Ordering::Relaxed);
                }

                _ => {}
            }
        }
    });
}
