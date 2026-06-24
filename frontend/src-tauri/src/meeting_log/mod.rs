//! Meeting-log feature set (personal fork):
//! - file output: live transcript append, per-session summary, daily log
//! - Thai STT glossary (consumed by the whisper engine)
//! - translation (TH→EN) via the Python sidecar
//! - local memory search (sqlite-vec + FTS5) via the Python sidecar
//!
//! All paths/models are read from `config` (env + `.env` + defaults). Nothing
//! here reaches outside 127.0.0.1.

pub mod commands;
pub mod config;
pub mod memory;
pub mod models;
pub mod notes;
pub mod session;
pub mod sidecar;
pub mod summary;
pub mod translate;

pub use config::config;

/// Begin a meeting-log session (called when recording starts). Non-fatal.
pub fn begin(meeting_name: &str) {
    match session::start_session(meeting_name) {
        Ok(path) => log::info!("📝 meeting-log: transcript → {:?}", path),
        Err(e) => log::warn!("📝 meeting-log: failed to start session: {e}"),
    }
}

/// Append a finalized transcript segment to the live file. Non-fatal.
pub fn record_final(text: &str, speaker: Option<&str>) {
    session::append_final_segment(text, speaker);
}

/// Finalize the session (summary + daily log + indexing). Called at stop.
/// Returns a human-readable status for logging; never panics.
pub async fn end() -> Option<summary::FinalizeResult> {
    let session = session::take_session()?;
    match summary::finalize(session).await {
        Ok(res) => {
            log::info!(
                "📝 meeting-log: summary → {} | daily → {}",
                res.summary_path,
                res.daily_log_path
            );
            Some(res)
        }
        Err(e) => {
            log::warn!("📝 meeting-log: finalize failed: {e}");
            None
        }
    }
}
