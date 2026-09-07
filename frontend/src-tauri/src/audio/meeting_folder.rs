use log::{info, warn};
use std::path::{Path, PathBuf};

use super::recording_preferences::get_default_recordings_folder;

/// Build the list of directories where meeting folders are allowed to live.
pub fn recordings_roots(save_folder: PathBuf) -> Vec<PathBuf> {
    let default_folder = get_default_recordings_folder();
    if save_folder == default_folder {
        vec![save_folder]
    } else {
        vec![save_folder, default_folder]
    }
}

/// Delete a meeting folder if it exists and is inside an allowed recordings root.
pub fn delete_meeting_folder_if_allowed(
    folder_path: &str,
    allowed_roots: &[PathBuf],
) -> Result<(), String> {
    let trimmed = folder_path.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let path = Path::new(trimmed);
    if !path.exists() {
        info!("Meeting folder already absent, skipping delete: {}", trimmed);
        return Ok(());
    }

    if !path.is_dir() {
        warn!("Meeting folder path is not a directory: {}", trimmed);
        return Ok(());
    }

    let canonical_folder = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve meeting folder '{}': {}", trimmed, e))?;

    if !is_within_allowed_roots(&canonical_folder, allowed_roots) {
        warn!(
            "Refusing to delete meeting folder outside recordings directories: {}",
            canonical_folder.display()
        );
        return Ok(());
    }

    std::fs::remove_dir_all(&canonical_folder)
        .map_err(|e| format!("Failed to delete meeting folder '{}': {}", trimmed, e))?;

    info!("Deleted meeting folder: {}", canonical_folder.display());
    Ok(())
}

fn is_within_allowed_roots(folder: &Path, allowed_roots: &[PathBuf]) -> bool {
    allowed_roots
        .iter()
        .any(|root| is_path_within_root(folder, root))
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    let root_path = canonicalize_if_exists(root);
    path.starts_with(&root_path)
}

fn canonicalize_if_exists(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("meetily-{name}-{nanos}"))
    }

    #[test]
    fn deletes_folder_inside_allowed_root() {
        let root = unique_temp_dir("delete-allowed");
        let meeting = root.join("Meeting_2026-01-01_12-00");
        fs::create_dir_all(&meeting).expect("create meeting dir");
        fs::write(meeting.join("metadata.json"), "{}").expect("write metadata");

        delete_meeting_folder_if_allowed(
            meeting.to_string_lossy().as_ref(),
            &[root.clone()],
        )
        .expect("delete should succeed");

        assert!(!meeting.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_folder_outside_allowed_root() {
        let allowed_root = unique_temp_dir("allowed-root");
        let outside_root = unique_temp_dir("outside-root");
        let meeting = outside_root.join("Meeting_outside");
        fs::create_dir_all(&meeting).expect("create meeting dir");

        delete_meeting_folder_if_allowed(
            meeting.to_string_lossy().as_ref(),
            &[allowed_root.clone()],
        )
        .expect("should not error when refusing");

        assert!(meeting.exists());

        let _ = fs::remove_dir_all(meeting);
        let _ = fs::remove_dir_all(outside_root);
        let _ = fs::remove_dir_all(allowed_root);
    }
}
