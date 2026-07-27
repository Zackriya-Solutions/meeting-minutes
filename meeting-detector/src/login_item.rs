//! "Start at login" support via a per-user LaunchAgent.
//!
//! Writes `~/Library/LaunchAgents/com.meetily.detector.plist` pointing at the
//! currently-running executable and (un)loads it with `launchctl`. Best used with
//! the bundled `.app` so the launched binary keeps its bundle identity.

use std::path::PathBuf;

use anyhow::{anyhow, Result};

const LABEL: &str = "com.meetily.detector";

fn plist_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{LABEL}.plist"))
    })
}

/// Whether the login item is currently installed.
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// Install + load the LaunchAgent so the detector starts at login.
pub fn enable() -> Result<()> {
    let exe = std::env::current_exe()?;
    let plist = plist_path().ok_or_else(|| anyhow!("could not resolve home directory"))?;
    if let Some(dir) = plist.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        label = LABEL,
        exe = exe.display(),
    );

    std::fs::write(&plist, contents)?;
    // -w persists the (un)loaded state across logins. Ignore load errors (e.g.
    // already loaded); the plist presence is what `is_enabled` reports.
    let _ = std::process::Command::new("launchctl")
        .arg("load")
        .arg("-w")
        .arg(&plist)
        .status();
    log::info!("start-at-login enabled -> {}", plist.display());
    Ok(())
}

/// Unload + remove the LaunchAgent.
pub fn disable() -> Result<()> {
    let plist = plist_path().ok_or_else(|| anyhow!("could not resolve home directory"))?;
    let _ = std::process::Command::new("launchctl")
        .arg("unload")
        .arg("-w")
        .arg(&plist)
        .status();
    if plist.exists() {
        std::fs::remove_file(&plist)?;
    }
    log::info!("start-at-login disabled");
    Ok(())
}
