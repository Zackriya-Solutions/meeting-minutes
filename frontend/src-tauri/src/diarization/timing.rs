//! Map between the transcript clock of a live recording and positions in its decoded audio file.
//!
//! Transcript times count mixed pipeline samples. The decoder (symphonia) returns every AAC frame,
//! including encoder priming and end padding, because it ignores the MP4 edit list. A file encoded
//! once therefore leads the clock by the 1024-sample priming. Older recordings were joined from
//! 30 s AAC checkpoints without re-encoding, so every checkpoint adds 1024 priming and 768 padding
//! samples at 48 kHz.
use crate::audio::incremental_saver::{AUDIO_LAYOUT_FIELD, AUDIO_LAYOUT_SINGLE_STREAM};
use serde_json::Value;
use std::path::Path;

/// Encoder priming of one AAC stream, in samples.
pub const AAC_PRIMING_SAMPLES: usize = 1024;
/// Audio samples in one full legacy recording checkpoint (30 s at 48 kHz).
pub const CHECKPOINT_SAMPLES_48K: usize = 1_440_000;
/// Decoded samples of one full legacy recording checkpoint: 1408 AAC frames of 1024.
pub const CHECKPOINT_FRAMES_48K: usize = 1_441_792;
const AAC_FRAME: usize = 1024;
const RATE_48K: f64 = 48_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeMap {
    /// Transcript rows were produced from the decoded file itself (retranscribed or imported).
    Identity,
    /// One AAC stream: the file leads the transcript clock by a constant.
    Offset { seconds: f64 },
    /// Legacy recording joined from 30 s AAC checkpoints without re-encoding.
    Checkpoints,
    /// Layout not recognised: times are used as they are and rows are never cut.
    Unknown,
}

impl TimeMap {
    /// Position in the decoded file of transcript time `clock_s`.
    pub fn file_s(&self, clock_s: f64) -> f64 {
        match *self {
            TimeMap::Identity | TimeMap::Unknown => clock_s,
            TimeMap::Offset { seconds } => clock_s + seconds,
            TimeMap::Checkpoints => {
                let n = (clock_s.max(0.0) * RATE_48K).round();
                let k = (n / CHECKPOINT_SAMPLES_48K as f64).floor();
                let extra = (CHECKPOINT_FRAMES_48K - CHECKPOINT_SAMPLES_48K) as f64;
                (n + AAC_PRIMING_SAMPLES as f64 + k * extra) / RATE_48K
            }
        }
    }

    /// Transcript time of decoded-file position `file_s`. Priming and padding map to the nearest
    /// edge of their checkpoint's audio.
    pub fn clock_s(&self, file_s: f64) -> f64 {
        match *self {
            TimeMap::Identity | TimeMap::Unknown => file_s,
            TimeMap::Offset { seconds } => (file_s - seconds).max(0.0),
            TimeMap::Checkpoints => {
                let f = (file_s.max(0.0) * RATE_48K).round();
                let k = (f / CHECKPOINT_FRAMES_48K as f64).floor();
                let within = (f - k * CHECKPOINT_FRAMES_48K as f64 - AAC_PRIMING_SAMPLES as f64)
                    .clamp(0.0, CHECKPOINT_SAMPLES_48K as f64);
                (k * CHECKPOINT_SAMPLES_48K as f64 + within) / RATE_48K
            }
        }
    }

    /// Rows are cut and re-transcribed only when the mapping is known.
    pub fn allows_splitting(&self) -> bool {
        !matches!(self, TimeMap::Unknown)
    }
}

/// The meeting folder's metadata.json, if it can be read.
pub fn read_metadata(folder: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(folder.join("metadata.json")).ok()?).ok()
}

/// How transcript times relate to the decoded audio. `native_rate` and `native_frames` describe
/// the decoded file before resampling.
pub fn recording_time_map(metadata: Option<&Value>, native_rate: u32, native_frames: usize) -> TimeMap {
    if let Some(m) = metadata {
        let source = m.get("source").and_then(Value::as_str);
        if m.get("retranscribed_at").is_some() || matches!(source, Some("retranscription") | Some("import")) {
            return TimeMap::Identity;
        }
    }
    recording_layout(metadata, native_rate, native_frames)
}

/// How the decoded file relates to the recording clock, whatever its transcript was timed by: the
/// map a live transcript of this file would need. An imported file has no recording clock.
pub fn recording_layout(metadata: Option<&Value>, native_rate: u32, native_frames: usize) -> TimeMap {
    if let Some(m) = metadata {
        if m.get("source").and_then(Value::as_str) == Some("import") {
            return TimeMap::Identity;
        }
        if m.get(AUDIO_LAYOUT_FIELD).and_then(Value::as_str) == Some(AUDIO_LAYOUT_SINGLE_STREAM) {
            return TimeMap::Offset { seconds: AAC_PRIMING_SAMPLES as f64 / native_rate.max(1) as f64 };
        }
    }
    if native_rate == 48_000 && fits_checkpoint_layout(native_frames) {
        TimeMap::Checkpoints
    } else {
        TimeMap::Unknown
    }
}

/// Whole AAC frames, ending in a checkpoint that holds its priming plus at least one frame.
fn fits_checkpoint_layout(frames: usize) -> bool {
    if frames == 0 || frames % AAC_FRAME != 0 {
        return false;
    }
    let last = frames % CHECKPOINT_FRAMES_48K;
    last == 0 || last >= 2 * AAC_FRAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const EPS: f64 = 1e-6;

    #[test]
    fn checkpoint_map_matches_the_measured_layout() {
        // Offsets measured on files joined with `-c copy`: 1024 + k * 1792 samples at 48 kHz.
        let m = TimeMap::Checkpoints;
        assert!((m.file_s(0.0) - 1024.0 / 48_000.0).abs() < EPS);
        assert!((m.file_s(30.0) - 1_442_816.0 / 48_000.0).abs() < EPS);
        assert!((m.file_s(75.0) - 3_604_608.0 / 48_000.0).abs() < EPS);
    }

    #[test]
    fn clock_s_inverts_file_s() {
        for m in [TimeMap::Checkpoints, TimeMap::Offset { seconds: 1024.0 / 48_000.0 }, TimeMap::Identity] {
            for t in [0.0, 12.3, 29.99, 30.0, 31.5, 75.0, 3599.0] {
                assert!((m.clock_s(m.file_s(t)) - t).abs() < 1e-4, "{m:?} at {t}");
            }
        }
    }

    #[test]
    fn priming_and_padding_map_to_checkpoint_edges() {
        let m = TimeMap::Checkpoints;
        // Inside the second checkpoint's priming.
        assert!((m.clock_s((1_441_792.0 + 500.0) / 48_000.0) - 30.0).abs() < EPS);
        // Inside the first checkpoint's end padding.
        assert!((m.clock_s((1_441_024.0 + 300.0) / 48_000.0) - 30.0).abs() < EPS);
    }

    #[test]
    fn single_stream_offset_is_the_encoder_priming() {
        let m = recording_time_map(Some(&json!({ "audio_layout": "single_stream" })), 48_000, 4_321_280);
        assert_eq!(m, TimeMap::Offset { seconds: 1024.0 / 48_000.0 });
        assert!((m.file_s(10.0) - (10.0 + 1024.0 / 48_000.0)).abs() < EPS);
        assert!(m.allows_splitting());
    }

    #[test]
    fn layout_ignores_where_the_transcript_came_from() {
        let checkpoint_frames = 1_441_792 * 2 + 4096;
        let retranscribed = json!({ "retranscribed_at": "2026-10-05T19:28:56Z" });
        assert_eq!(recording_layout(Some(&retranscribed), 48_000, checkpoint_frames), TimeMap::Checkpoints);
        assert_eq!(recording_layout(Some(&json!({ "source": "retranscription" })), 48_000, checkpoint_frames), TimeMap::Checkpoints);
        assert_eq!(recording_layout(Some(&json!({ "source": "import" })), 48_000, checkpoint_frames), TimeMap::Identity);
        assert_eq!(
            recording_layout(Some(&json!({ "retranscribed_at": "x", "audio_layout": "single_stream" })), 48_000, 4_321_280),
            TimeMap::Offset { seconds: 1024.0 / 48_000.0 }
        );
        assert_eq!(recording_layout(None, 44_100, 1024 * 3000), TimeMap::Unknown);
    }

    #[test]
    fn metadata_selects_the_map() {
        let checkpoint_frames = 1_441_792 * 2 + 4096;
        assert_eq!(
            recording_time_map(Some(&json!({ "retranscribed_at": "2026-09-27T10:00:00Z", "audio_layout": "single_stream" })), 48_000, checkpoint_frames),
            TimeMap::Identity
        );
        assert_eq!(recording_time_map(Some(&json!({ "source": "import" })), 44_100, 12_345), TimeMap::Identity);
        assert_eq!(recording_time_map(Some(&json!({ "source": "retranscription" })), 48_000, checkpoint_frames), TimeMap::Identity);
        assert_eq!(recording_time_map(Some(&json!({ "status": "completed" })), 48_000, checkpoint_frames), TimeMap::Checkpoints);
        assert_eq!(recording_time_map(None, 48_000, checkpoint_frames), TimeMap::Checkpoints);
        assert_eq!(recording_time_map(None, 44_100, 1024 * 3000), TimeMap::Unknown);
        assert_eq!(recording_time_map(None, 48_000, 1_000_001), TimeMap::Unknown);
        assert!(TimeMap::Checkpoints.allows_splitting());
        assert!(TimeMap::Identity.allows_splitting());
        assert!(!TimeMap::Unknown.allows_splitting());
        assert!((TimeMap::Unknown.file_s(42.0) - 42.0).abs() < EPS);
    }
}
