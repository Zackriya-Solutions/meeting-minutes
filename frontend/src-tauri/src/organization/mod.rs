//! Meeting folders, tags, and Trash.

use log::warn;
use tauri::{AppHandle, Emitter, Runtime};

pub mod commands;
pub mod purge;
pub mod scheduler;

/// Tell the frontend that meetings, folders, tags, or Trash changed so it can refetch.
pub(crate) fn notify_meetings_changed<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.emit("meetings-changed", ()) {
        warn!("Failed to emit meetings-changed: {}", e);
    }
}
