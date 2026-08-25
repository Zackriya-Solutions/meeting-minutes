// PR-33: Orphan checkpoint detection and recovery.
// Scans `<app_data>/meetings/*/` for leftover `.checkpoints/audio_chunk_*.mp4`
// directories left behind by crashed recording sessions. The frontend listens for
// the `orphan-checkpoints-detected` event and prompts the user to recover or discard.

use log::{info, warn};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct OrphanCheckpoint {
    pub meeting_folder: String,
    pub display_name: String,
    pub chunk_count: u32,
    pub estimated_duration_seconds: f64,
    pub last_modified_ms: i64,
}

pub fn scan_orphan_checkpoints(app_data_dir: &Path) -> Vec<OrphanCheckpoint> {
    let meetings_root = app_data_dir.join("meetings");
    if !meetings_root.exists() { return Vec::new(); }

    let mut results = Vec::new();
    let entries = match std::fs::read_dir(&meetings_root) {
        Ok(e) => e,
        Err(e) => { warn!("meetings read failed: {}", e); return results; }
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let meeting_folder = entry.path();
        if !meeting_folder.is_dir() { continue; }
        let checkpoints_dir = meeting_folder.join(".checkpoints");
        if !checkpoints_dir.is_dir() { continue; }

        let mut mp4_count: u32 = 0;
        let mut latest_modified: Option<std::time::SystemTime> = None;
        if let Ok(read) = std::fs::read_dir(&checkpoints_dir) {
            for fe in read.filter_map(|f| f.ok()) {
                let p = fe.path();
                if p.extension().and_then(|s| s.to_str()) == Some("mp4") {
                    mp4_count += 1;
                    if let Ok(meta) = fe.metadata() {
                        if let Ok(modified) = meta.modified() {
                            latest_modified = Some(match latest_modified {
                                Some(prev) if prev > modified => prev,
                                _ => modified,
                            });
                        }
                    }
                }
            }
        }

        if mp4_count == 0 { continue; }

        let last_modified_ms = latest_modified
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        results.push(OrphanCheckpoint {
            meeting_folder: meeting_folder.to_string_lossy().to_string(),
            display_name: meeting_folder.file_name().and_then(|s| s.to_str()).unwrap_or("unknown").to_string(),
            chunk_count: mp4_count,
            estimated_duration_seconds: (mp4_count as f64) * 30.0,
            last_modified_ms,
        });
    }

    results.sort_by(|a, b| b.last_modified_ms.cmp(&a.last_modified_ms));
    results
}

pub fn discard_orphan_checkpoint(meeting_folder: &Path) -> Result<(), String> {
    let cp = meeting_folder.join(".checkpoints");
    if !cp.exists() { return Ok(()); }
    std::fs::remove_dir_all(&cp).map_err(|e| format!("discard failed: {}", e))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_chunk(dir: &Path, idx: u32) {
        fs::write(dir.join(format!("audio_chunk_{:03}.mp4", idx)), b"x").unwrap();
    }

    #[test]
    fn scan_returns_empty_when_meetings_dir_missing() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_empty");
        let _ = fs::remove_dir_all(&tmp);
        assert!(scan_orphan_checkpoints(&tmp).is_empty());
    }

    #[test]
    fn scan_finds_meeting_with_checkpoints() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_find");
        let _ = fs::remove_dir_all(&tmp);
        let cp = tmp.join("meetings").join("Test_Meeting").join(".checkpoints");
        fs::create_dir_all(&cp).unwrap();
        write_chunk(&cp, 0);
        write_chunk(&cp, 1);
        let r = scan_orphan_checkpoints(&tmp);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].display_name, "Test_Meeting");
        assert_eq!(r[0].chunk_count, 2);
        assert_eq!(r[0].estimated_duration_seconds, 60.0);
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn scan_skips_meetings_without_checkpoints() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_skip");
        let _ = fs::remove_dir_all(&tmp);
        let m1 = tmp.join("meetings").join("Has");
        fs::create_dir_all(m1.join(".checkpoints")).unwrap();
        write_chunk(&m1.join(".checkpoints"), 0);
        fs::create_dir_all(tmp.join("meetings").join("No")).unwrap();
        let r = scan_orphan_checkpoints(&tmp);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].display_name, "Has");
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn scan_sorts_by_mtime_descending() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_sort");
        let _ = fs::remove_dir_all(&tmp);
        let m1 = tmp.join("meetings").join("Older");
        let cp1 = m1.join(".checkpoints");
        fs::create_dir_all(&cp1).unwrap();
        write_chunk(&cp1, 0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let m2 = tmp.join("meetings").join("Newer");
        let cp2 = m2.join(".checkpoints");
        fs::create_dir_all(&cp2).unwrap();
        write_chunk(&cp2, 0);
        let r = scan_orphan_checkpoints(&tmp);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].display_name, "Newer");
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn discard_removes_checkpoints_dir() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_discard");
        let _ = fs::remove_dir_all(&tmp);
        let meeting = tmp.join("M");
        let cp = meeting.join(".checkpoints");
        fs::create_dir_all(&cp).unwrap();
        write_chunk(&cp, 0);
        assert!(cp.exists());
        discard_orphan_checkpoint(&meeting).unwrap();
        assert!(!cp.exists());
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn discard_is_noop_when_dir_missing() {
        let tmp = std::env::temp_dir().join("meetily_orphan_test_discard_noop");
        let _ = fs::remove_dir_all(&tmp);
        let meeting = tmp.join("No_CP");
        fs::create_dir_all(&meeting).unwrap();
        assert!(discard_orphan_checkpoint(&meeting).is_ok());
        fs::remove_dir_all(&tmp).unwrap();
    }
}
