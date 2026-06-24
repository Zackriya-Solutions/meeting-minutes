//! Best-effort launcher for the Python ML sidecar. On startup we check whether
//! the sidecar already answers on its health endpoint; if not, we try to spawn
//! it from a known location. Entirely non-fatal — translation/search simply
//! return errors (handled gracefully in the UI) when the sidecar is absent.

use super::config::config;
use std::path::PathBuf;
use std::process::Command;

/// Locate the `sidecar/` directory (dev layouts + an env override).
fn find_sidecar_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MEET_SIDECAR_DIR") {
        let p = PathBuf::from(dir);
        if p.join("run.sh").exists() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("sidecar"));
        candidates.push(cwd.join("../sidecar"));
        candidates.push(cwd.join("../../sidecar"));
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..6 {
            if let Some(dir) = &d {
                candidates.push(dir.join("sidecar"));
                d = dir.parent().map(|p| p.to_path_buf());
            }
        }
    }
    candidates.into_iter().find(|p| p.join("run.sh").exists())
}

/// Spawn the sidecar if it is not already healthy. Runs in a background thread
/// so it never blocks app startup.
pub fn ensure_started() {
    std::thread::spawn(|| {
        let cfg = config();
        let health = format!("{}/health", cfg.sidecar_url.trim_end_matches('/'));

        // Already up?
        if reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(800))
            .build()
            .ok()
            .and_then(|c| c.get(&health).send().ok())
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            log::info!("📝 meeting-log: sidecar already running at {}", cfg.sidecar_url);
            return;
        }

        let Some(dir) = find_sidecar_dir() else {
            log::warn!(
                "📝 meeting-log: sidecar dir not found; run `sidecar/run.sh` manually to enable translation/search"
            );
            return;
        };

        log::info!("📝 meeting-log: starting sidecar from {:?}", dir);
        let run_sh = dir.join("run.sh");
        let spawn = Command::new("bash")
            .arg(run_sh)
            .current_dir(&dir)
            .env("OLLAMA_URL", &cfg.ollama_url)
            .spawn();
        match spawn {
            Ok(_) => log::info!("📝 meeting-log: sidecar launch requested"),
            Err(e) => log::warn!("📝 meeting-log: failed to launch sidecar: {e}"),
        }
    });
}
