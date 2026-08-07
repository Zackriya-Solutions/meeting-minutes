//! One-shot rename of the pre-Conversationaly data directories.
//!
//! `productName` and `identifier` feed the OS app-data path, so the rebrand
//! would otherwise boot every existing install against an empty library and
//! re-download every model. Renaming the directories once at startup is
//! cheaper than teaching each consumer two paths.

use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

const OLD_IDENTIFIER: &str = "com.meetily.ai";

/// Move the old data/config directories to their new names. Must run before
/// anything reads them — call it first in `setup()`.
pub fn migrate_legacy_data_dirs<R: Runtime>(app: &AppHandle<R>) {
    // Database, models, preferences, onboarding state.
    if let Ok(new) = app.path().app_data_dir() {
        if let Some(parent) = new.parent() {
            rename(&parent.join(OLD_IDENTIFIER), &new);
        }
    }
    // Custom summary templates (and the models fallback path).
    if let Some(dir) = dirs::data_dir() {
        rename(&dir.join("Meetily"), &dir.join("Conversationaly"));
    }
    // notifications.json
    if let Some(dir) = dirs::config_dir() {
        rename(&dir.join("meetily"), &dir.join("conversationaly"));
    }
    // ponytail: ~/Movies/meetily-recordings is deliberately left in place —
    // recording_preferences.json stores its absolute path and the user browses
    // that folder. Only the default for fresh installs changed.
}

fn rename(old: &Path, new: &Path) {
    if !old.exists() || new.exists() {
        return;
    }
    match std::fs::rename(old, new) {
        Ok(()) => log::info!("Migrated {} -> {}", old.display(), new.display()),
        Err(e) => log::error!(
            "Could not migrate {} -> {}: {e}. Previous data stays at the old path.",
            old.display(),
            new.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::rename;

    #[test]
    fn renames_once_and_never_clobbers() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("old");
        let new = tmp.path().join("new");

        // Nothing to do when the old directory was never created.
        rename(&old, &new);
        assert!(!new.exists());

        std::fs::create_dir(&old).unwrap();
        std::fs::write(old.join("db"), b"meetings").unwrap();
        rename(&old, &new);
        assert!(!old.exists());
        assert_eq!(std::fs::read(new.join("db")).unwrap(), b"meetings");

        // A second run must not overwrite data written since the migration.
        std::fs::create_dir(&old).unwrap();
        rename(&old, &new);
        assert!(old.exists());
        assert_eq!(std::fs::read(new.join("db")).unwrap(), b"meetings");
    }
}
