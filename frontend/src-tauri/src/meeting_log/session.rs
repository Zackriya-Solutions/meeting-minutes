//! Live meeting-log session: creates the per-day folder, appends each final
//! transcript segment to disk immediately (crash-safe), and holds the segments
//! in memory so they can be summarized + indexed when the session ends.

use super::config::config;
use chrono::{DateTime, Local};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// A single finalized transcript line.
#[derive(Debug, Clone)]
pub struct LoggedSegment {
    /// Content timestamp, e.g. "14:30:12" (colons OK — this is file content).
    pub clock: String,
    pub speaker: Option<String>,
    pub text: String,
}

/// State for the currently-recording session.
#[derive(Debug, Clone)]
pub struct SessionLog {
    pub meeting_name: String,
    /// e.g. ".../meet-log/2026-06-24"
    pub date_folder: PathBuf,
    /// Colon-safe session time, e.g. "14-30-05".
    pub session_time: String,
    /// Human header time, e.g. "2026-06-24 14:30:05".
    pub started_human: String,
    /// ISO date, e.g. "2026-06-24".
    pub date_iso: String,
    pub transcript_path: PathBuf,
    pub segments: Vec<LoggedSegment>,
}

static SESSION: Mutex<Option<SessionLog>> = Mutex::new(None);

/// Begin a new session: create folders + transcript file with a header.
/// Returns the transcript path. Errors are non-fatal to recording — callers
/// should log and continue if this fails.
pub fn start_session(meeting_name: &str) -> Result<PathBuf, String> {
    let cfg = config();
    let now: DateTime<Local> = Local::now();

    let date_dir_name = now.format(&cfg.date_format).to_string();
    let session_time = now.format(&cfg.time_format).to_string();
    let date_iso = now.format("%Y-%m-%d").to_string();
    let started_human = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let date_folder = cfg.log_root.join(&date_dir_name);
    fs::create_dir_all(&date_folder)
        .map_err(|e| format!("create date folder {:?}: {e}", date_folder))?;
    // Ensure the vector index dir exists too (cheap, idempotent).
    if let Some(idx_dir) = cfg.vector_db_path.parent() {
        let _ = fs::create_dir_all(idx_dir);
    }

    let transcript_name = cfg.transcript_pattern.replace("{time}", &session_time);
    let transcript_path = date_folder.join(transcript_name);

    let header = format!("# Transcript — {}\n\n", started_human);
    fs::write(&transcript_path, header)
        .map_err(|e| format!("write transcript header {:?}: {e}", transcript_path))?;

    let session = SessionLog {
        meeting_name: meeting_name.to_string(),
        date_folder,
        session_time,
        started_human,
        date_iso,
        transcript_path: transcript_path.clone(),
        segments: Vec::new(),
    };

    let mut guard = SESSION.lock().map_err(|e| e.to_string())?;
    *guard = Some(session);
    Ok(transcript_path)
}

/// Append one finalized segment to the transcript file and flush immediately.
/// `speaker` is optional (diarization may fill it later). No-op if no active
/// session.
pub fn append_final_segment(text: &str, speaker: Option<&str>) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let clock = Local::now().format("%H:%M:%S").to_string();

    let mut guard = match SESSION.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let Some(session) = guard.as_mut() else {
        return;
    };

    let line = match speaker {
        Some(s) if !s.is_empty() => format!("[{}] {}: {}\n", clock, s, text),
        _ => format!("[{}] {}\n", clock, text),
    };

    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session.transcript_path)
    {
        Ok(mut f) => {
            if let Err(e) = f.write_all(line.as_bytes()).and_then(|_| f.flush()) {
                log::warn!("meeting_log: failed to append transcript line: {e}");
            }
        }
        Err(e) => log::warn!("meeting_log: failed to open transcript file: {e}"),
    }

    session.segments.push(LoggedSegment {
        clock,
        speaker: speaker.map(|s| s.to_string()),
        text: text.to_string(),
    });
}

/// Take and clear the active session (called at stop). Returns owned data so
/// async finalization can run without holding the lock.
pub fn take_session() -> Option<SessionLog> {
    SESSION.lock().ok().and_then(|mut g| g.take())
}

/// Is there an active meeting-log session?
pub fn has_active_session() -> bool {
    SESSION
        .lock()
        .ok()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

impl SessionLog {
    /// Full transcript as plain text lines (no markdown header), for the LLM.
    pub fn transcript_plain(&self) -> String {
        self.segments
            .iter()
            .map(|s| match &s.speaker {
                Some(sp) if !sp.is_empty() => format!("[{}] {}: {}", s.clock, sp, s.text),
                _ => format!("[{}] {}", s.clock, s.text),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
