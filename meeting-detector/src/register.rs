//! Finalize a recording: transcode the WAV to `audio.mp4`, write the meeting
//! folder's `metadata.json` + empty `transcripts.json`, and register the meeting
//! in the main app's SQLite DB so it appears in Memento's meeting list.
//!
//! The registered meeting has audio but no transcript; the user transcribes it
//! from Memento's "Enhance"/Retranscribe button, which reads `audio.mp4`.

use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};

/// Everything needed to finalize + register one recording. All fields are `Send`
/// so this can run on a worker thread off the UI event loop.
pub struct Finalize {
    pub meeting_id: String,
    pub folder: PathBuf,
    pub wav_path: PathBuf,
    pub title: String,
    pub device_name: String,
    pub duration_secs: f64,
    pub started: DateTime<Utc>,
}

pub struct FinalizeOutcome {
    /// Whether the DB row was written (false => user must Import manually).
    pub registered: bool,
}

pub fn finalize(input: Finalize) -> Result<FinalizeOutcome> {
    let mp4 = input.folder.join("audio.mp4");
    transcode_to_mp4(&input.wav_path, &mp4).context("transcoding recording to mp4")?;
    // The mp4 is the canonical artifact; drop the intermediate WAV.
    if let Err(e) = std::fs::remove_file(&input.wav_path) {
        log::warn!("could not remove intermediate WAV: {e}");
    }

    write_metadata(&input)?;
    write_empty_transcripts(&input.folder, input.started)?;

    let registered = match register_in_db(&input.meeting_id, &input.folder, &input.title) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("could not register meeting in Memento DB (import it manually instead): {e}");
            false
        }
    };

    Ok(FinalizeOutcome { registered })
}

/// Transcode WAV -> AAC-LC mp4 (mono, 48 kHz, +faststart), matching the main
/// app's `encode.rs` so playback and retranscription behave identically.
fn transcode_to_mp4(wav: &Path, mp4: &Path) -> Result<()> {
    let ffmpeg = crate::paths::find_ffmpeg()
        .ok_or_else(|| anyhow!("ffmpeg not found (needed to encode the recording)"))?;

    let output = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(wav)
        .args([
            "-ac", "1",
            "-ar", "48000",
            "-c:a", "aac",
            "-b:a", "192k",
            "-profile:a", "aac_low",
            "-movflags", "+faststart",
            "-f", "mp4",
        ])
        .arg(mp4)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("running ffmpeg at {}", ffmpeg.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "ffmpeg failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn write_metadata(input: &Finalize) -> Result<()> {
    let meta = serde_json::json!({
        "version": "1.0",
        "meeting_id": input.meeting_id,
        "meeting_name": input.title,
        "created_at": input.started.to_rfc3339(),
        "completed_at": Utc::now().to_rfc3339(),
        "duration_seconds": input.duration_secs.round() as i64,
        "devices": { "microphone": input.device_name, "system_audio": null },
        "audio_file": "audio.mp4",
        "transcript_file": "transcripts.json",
        "sample_rate": 48000,
        "status": "completed",
        "source": "memento-detector",
    });
    std::fs::write(
        input.folder.join("metadata.json"),
        serde_json::to_vec_pretty(&meta)?,
    )
    .context("writing metadata.json")?;
    Ok(())
}

fn write_empty_transcripts(folder: &Path, started: DateTime<Utc>) -> Result<()> {
    let transcripts = serde_json::json!({
        "version": "1.0",
        "segments": [],
        "last_updated": started.to_rfc3339(),
        "total_segments": 0,
    });
    std::fs::write(
        folder.join("transcripts.json"),
        serde_json::to_vec_pretty(&transcripts)?,
    )
    .context("writing transcripts.json")?;
    Ok(())
}

/// Insert a row into the main app's `meetings` table.
///
/// Opens the DB directly (WAL is already the persistent journal mode; we only add
/// a busy_timeout and keep the write to a single, immediately-committed statement
/// so we never contend with the running app for long). Uses SQLite's own
/// `datetime('now')` for timestamps, which the app's sqlx layer decodes cleanly.
fn register_in_db(meeting_id: &str, folder: &Path, title: &str) -> Result<()> {
    let db = crate::paths::database_path().ok_or_else(|| anyhow!("could not resolve DB path"))?;
    if !db.exists() {
        return Err(anyhow!(
            "Memento database not found at {} (run the main app once first)",
            db.display()
        ));
    }

    let conn = rusqlite::Connection::open(&db).context("opening Memento database")?;
    conn.busy_timeout(Duration::from_secs(10))?;
    conn.execute(
        "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) \
         VALUES (?1, ?2, datetime('now'), datetime('now'), ?3)",
        rusqlite::params![meeting_id, title, folder.to_string_lossy()],
    )
    .context("inserting meeting row")?;

    log::info!("registered meeting '{title}' ({meeting_id}) in Memento");
    Ok(())
}
