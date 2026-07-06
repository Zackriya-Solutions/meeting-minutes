//! Per-segment audio **source attribution** — deciding which capture stream
//! (the local microphone vs. the system/loopback output) dominated a given
//! transcript segment. This is **not speaker diarization**: it never identifies
//! *who* spoke, only *which device* the audio came from, and it never writes
//! `transcripts.speaker`.
//!
//! # How it works
//! Attribution is computed **live during recording**. As the pipeline mixes the
//! two streams it appends the *pre-normalization* mic energy and the raw system
//! energy for each 100 ms frame to a [`SourceAttributionTimeline`]. When VAD
//! emits a speech segment, [`SourceAttributionTimeline::classify_segment`]
//! compares the accumulated per-stream energy over that segment's time span.
//!
//! # Why historical segments cannot be backfilled
//! Attribution requires the **separate** mic and system energy, which only
//! exists while the two streams are live. Only the *mixed* audio is persisted to
//! disk (see `pipeline.rs`), so segments produced before this feature, imported
//! from an external file, or re-transcribed from the saved mix have no per-stream
//! signal to attribute. Recovering source from a finished mix is source
//! separation / diarization, which is deliberately out of scope. Those segments
//! carry `audio_source = NULL` and render with **no** source label (see the
//! frontend `transcriptSource.ts` and the migration notes).
//!
//! # Single-stream recordings
//! - **Microphone only** (no meeting audio playing): system energy stays near
//!   the silence floor (`ENERGY_FLOOR`), so segments classify as `Microphone` → "Me".
//! - **System only** (user silent / muted): mic energy stays near the floor, so
//!   segments classify as `System` → "Speaker".
//! - **Both below the floor** (silence): `Unknown`, which also displays as the
//!   safe "Speaker" fallback.
//!
//! # Presentation vs. stored attribution
//! [`display_label`] intentionally collapses the four raw states into a **binary**
//! "Me" / "Speaker" label (see its docs). The raw `audio_source` and
//! `source_confidence` are preserved end-to-end in the DB and API for any
//! downstream consumer that wants the full four-state signal.
//!
//! # Thresholds
//! The constants below are heuristics tuned for this pipeline's energy profile
//! (mic normalized to −23 LUFS at capture, system left raw — attribution uses the
//! *pre-normalization* mic so the comparison is like-for-like). They are
//! documented here rather than exposed as configuration because the app has no
//! calibration/telemetry surface to tune them against; adding knobs nobody can
//! meaningfully set would be premature (YAGNI). Revisit if such a surface exists.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Granularity of energy analysis. 100 ms ≈ syllable scale: fine enough to catch
/// turn-taking *within* a single VAD segment, coarse enough that per-frame energy
/// is statistically stable.
const ATTRIBUTION_FRAME_MS: f64 = 100.0;
/// Mean-square energy (≈ a −80 dBFS / 1e-4 RMS signal) below which a stream is
/// treated as silent. Also guards the divisions and `log10` in [`classify_energy`].
const ENERGY_FLOOR: f64 = 1.0e-8;
/// Fraction of total segment energy a single stream must hold to be declared the
/// sole source. 0.72 (≈ +4 dB over the other stream) requires *clear* dominance,
/// not a slim majority, so crosstalk is not confidently mislabeled as one source.
const DOMINANCE_RATIO: f64 = 0.72;
/// Additional guard alongside [`DOMINANCE_RATIO`]: the dominant stream must also
/// exceed the other by ≥ 4 dB (~2.5× power). Blocks a confident single-source
/// call when a loud-but-close second source is present.
const MIN_DB_MARGIN: f64 = 4.0;
/// Lower edge of the "both streams meaningfully active" band. Derived from
/// [`DOMINANCE_RATIO`] so the Mixed band and the dominance thresholds are exact
/// complements — there is no unclassifiable gap between them.
const MIXED_RATIO_LOW: f64 = 1.0 - DOMINANCE_RATIO;
/// Upper edge of the Mixed band (symmetric with [`MIXED_RATIO_LOW`]).
const MIXED_RATIO_HIGH: f64 = DOMINANCE_RATIO;
/// Retain at least this much timeline behind the most recently classified segment
/// so late-arriving or overlapping VAD segments still find their frames before
/// pruning removes them.
const PRUNE_SAFETY_MARGIN_MS: f64 = 3000.0;
/// Hard cap on how far back frames are kept, to bound memory on very long
/// recordings (30 minutes of 100 ms frames ≈ 18k frames).
const MAX_TIMELINE_RETENTION_MS: f64 = 30.0 * 60.0 * 1000.0;
/// For a segment to be called Mixed via *alternation* (each source dominating in
/// turn), each source must dominate for at least this many milliseconds…
const MIN_ALTERNATING_SOURCE_MS: f64 = 200.0;
/// …and for at least this fraction of the segment's covered time. Together these
/// require genuine back-and-forth, not a single brief blip from the other stream.
const MIN_ALTERNATING_SOURCE_RATIO: f64 = 0.20;
/// Confidence below which the attribution is too weak to surface as a distinct
/// label; [`display_label`] falls back to the safe "Speaker". Mirrored on the
/// frontend as `DISPLAY_CONFIDENCE_THRESHOLD` in `transcriptSource.ts`.
const MIN_DISPLAY_CONFIDENCE: f32 = 0.55;

/// Which capture stream a transcript segment was attributed to. Serialized as
/// snake_case (`"microphone"`, `"system"`, `"mixed"`, `"unknown"`) on the wire
/// and stored as that same TEXT in the `transcripts.audio_source` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptAudioSource {
    Microphone,
    System,
    Mixed,
    Unknown,
}

impl TranscriptAudioSource {
    /// The canonical lowercase wire/DB string for this source — the canonical
    /// Rust-side mapping. The DB `CHECK` constraint (see the migration) and the
    /// frontend `transcriptSource.ts` hold parallel copies of these literals and
    /// must be updated together whenever a variant is added or renamed.
    pub fn as_wire(self) -> &'static str {
        match self {
            TranscriptAudioSource::Microphone => "microphone",
            TranscriptAudioSource::System => "system",
            TranscriptAudioSource::Mixed => "mixed",
            TranscriptAudioSource::Unknown => "unknown",
        }
    }

    /// Parse an untrusted wire/DB string (from frontend IPC or an imported file)
    /// into a source, tolerating surrounding whitespace and any casing. Returns
    /// `None` for anything outside the known set so the persistence boundary can
    /// reject invalid input instead of storing it.
    pub fn from_wire(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "microphone" => Some(TranscriptAudioSource::Microphone),
            "system" => Some(TranscriptAudioSource::System),
            "mixed" => Some(TranscriptAudioSource::Mixed),
            "unknown" => Some(TranscriptAudioSource::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SourceAttribution {
    pub audio_source: TranscriptAudioSource,
    pub source_confidence: f32,
}

#[derive(Debug, Clone)]
struct AttributionFrame {
    start_ms: f64,
    end_ms: f64,
    mic_energy: f64,
    system_energy: f64,
}

#[derive(Debug, Default)]
pub struct SourceAttributionTimeline {
    frames: VecDeque<AttributionFrame>,
    cursor_ms: f64,
}

impl SourceAttributionTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append_window(&mut self, mic_window: &[f32], system_window: &[f32], sample_rate: u32) {
        let frame_samples = ((sample_rate as f64 * ATTRIBUTION_FRAME_MS) / 1000.0)
            .round()
            .max(1.0) as usize;
        let len = mic_window.len().max(system_window.len());

        if len == 0 || sample_rate == 0 {
            return;
        }

        let mut offset = 0;
        while offset < len {
            let end = (offset + frame_samples).min(len);
            let start_ms = self.cursor_ms + (offset as f64 / sample_rate as f64 * 1000.0);
            let end_ms = self.cursor_ms + (end as f64 / sample_rate as f64 * 1000.0);

            self.frames.push_back(AttributionFrame {
                start_ms,
                end_ms,
                mic_energy: mean_square(slice_or_empty(mic_window, offset, end)),
                system_energy: mean_square(slice_or_empty(system_window, offset, end)),
            });

            offset = end;
        }

        self.cursor_ms += len as f64 / sample_rate as f64 * 1000.0;
    }

    pub fn classify_segment(&self, start_ms: f64, end_ms: f64) -> SourceAttribution {
        if end_ms <= start_ms {
            return unknown();
        }

        let mut mic_energy = 0.0;
        let mut system_energy = 0.0;
        let mut mic_dominant_ms = 0.0;
        let mut system_dominant_ms = 0.0;
        let mut covered_ms = 0.0;

        for frame in &self.frames {
            let overlap_start = start_ms.max(frame.start_ms);
            let overlap_end = end_ms.min(frame.end_ms);
            let overlap_ms = overlap_end - overlap_start;

            if overlap_ms > 0.0 {
                mic_energy += frame.mic_energy * overlap_ms;
                system_energy += frame.system_energy * overlap_ms;
                covered_ms += overlap_ms;

                match classify_energy(frame.mic_energy, frame.system_energy).audio_source {
                    TranscriptAudioSource::Microphone => mic_dominant_ms += overlap_ms,
                    TranscriptAudioSource::System => system_dominant_ms += overlap_ms,
                    TranscriptAudioSource::Mixed | TranscriptAudioSource::Unknown => {}
                }
            }
        }

        if covered_ms <= 0.0 {
            return unknown();
        }

        if has_alternating_sources(mic_dominant_ms, system_dominant_ms, covered_ms) {
            return SourceAttribution {
                audio_source: TranscriptAudioSource::Mixed,
                source_confidence: ((mic_dominant_ms + system_dominant_ms) / covered_ms) as f32,
            };
        }

        classify_energy(mic_energy / covered_ms, system_energy / covered_ms)
    }

    pub fn prune_before(&mut self, cutoff_ms: f64) {
        let safe_cutoff = (cutoff_ms - PRUNE_SAFETY_MARGIN_MS).max(0.0);

        while self
            .frames
            .front()
            .is_some_and(|frame| frame.end_ms < safe_cutoff)
        {
            self.frames.pop_front();
        }
    }

    pub fn prune_stale(&mut self) {
        if self.cursor_ms > MAX_TIMELINE_RETENTION_MS {
            self.prune_before(self.cursor_ms - MAX_TIMELINE_RETENTION_MS);
        }
    }
}

/// Map a raw attribution to the **binary presentation label** shown to users.
///
/// This is deliberately lossy: only a confident `Microphone` becomes "Me"; every
/// other case — `System`, `Mixed`, `Unknown`, or any source below
/// `MIN_DISPLAY_CONFIDENCE` — becomes "Speaker". The design goal is to never
/// assert a false "Me" (attributing the remote party's words to the user is worse
/// than the conservative default), so "Speaker" doubles as the safe fallback.
///
/// This collapses `System`/`Mixed`/`Unknown`/low-confidence into one visible
/// label **on purpose**. Callers that need to distinguish those states must read
/// the raw `audio_source` / `source_confidence` fields (preserved in the DB and
/// API), e.g. the UI badge tooltip — do not infer the underlying state from this
/// string.
pub fn display_label(source: TranscriptAudioSource, confidence: f32) -> &'static str {
    if confidence < MIN_DISPLAY_CONFIDENCE {
        return "Speaker";
    }

    match source {
        TranscriptAudioSource::Microphone => "Me",
        TranscriptAudioSource::System => "Speaker",
        TranscriptAudioSource::Mixed | TranscriptAudioSource::Unknown => "Speaker",
    }
}

pub fn unknown() -> SourceAttribution {
    SourceAttribution {
        audio_source: TranscriptAudioSource::Unknown,
        source_confidence: 0.0,
    }
}

fn classify_energy(mic_energy: f64, system_energy: f64) -> SourceAttribution {
    let total = mic_energy + system_energy;

    if total < ENERGY_FLOOR {
        return unknown();
    }

    let mic_ratio = mic_energy / total;
    let system_ratio = system_energy / total;
    let db_margin = 10.0 * ((mic_energy + ENERGY_FLOOR) / (system_energy + ENERGY_FLOOR)).log10();

    if mic_ratio >= DOMINANCE_RATIO && db_margin >= MIN_DB_MARGIN {
        return SourceAttribution {
            audio_source: TranscriptAudioSource::Microphone,
            source_confidence: mic_ratio as f32,
        };
    }

    if system_ratio >= DOMINANCE_RATIO && -db_margin >= MIN_DB_MARGIN {
        return SourceAttribution {
            audio_source: TranscriptAudioSource::System,
            source_confidence: system_ratio as f32,
        };
    }

    if mic_ratio >= MIXED_RATIO_LOW && mic_ratio <= MIXED_RATIO_HIGH {
        return SourceAttribution {
            audio_source: TranscriptAudioSource::Mixed,
            source_confidence: (1.0 - (mic_ratio - 0.5).abs() * 2.0) as f32,
        };
    }

    unknown()
}

fn has_alternating_sources(mic_dominant_ms: f64, system_dominant_ms: f64, covered_ms: f64) -> bool {
    if covered_ms <= 0.0 {
        return false;
    }

    mic_dominant_ms >= MIN_ALTERNATING_SOURCE_MS
        && system_dominant_ms >= MIN_ALTERNATING_SOURCE_MS
        && mic_dominant_ms / covered_ms >= MIN_ALTERNATING_SOURCE_RATIO
        && system_dominant_ms / covered_ms >= MIN_ALTERNATING_SOURCE_RATIO
}

fn mean_square(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    samples
        .iter()
        .map(|sample| {
            let value = *sample as f64;
            value * value
        })
        .sum::<f64>()
        / samples.len() as f64
}

fn slice_or_empty(samples: &[f32], start: usize, end: usize) -> &[f32] {
    if start >= samples.len() {
        &[]
    } else {
        &samples[start..end.min(samples.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repeated(value: f32, len: usize) -> Vec<f32> {
        vec![value; len]
    }

    #[test]
    fn classifies_microphone_dominant_segment() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.8, 4800), &repeated(0.05, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Microphone);
        assert!(result.source_confidence >= 0.72);
    }

    #[test]
    fn classifies_system_dominant_segment() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.05, 4800), &repeated(0.8, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::System);
        assert!(result.source_confidence >= 0.72);
    }

    #[test]
    fn classifies_balanced_active_sources_as_mixed() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.5, 4800), &repeated(0.5, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Mixed);
    }

    #[test]
    fn ambiguous_active_ratios_are_mixed_not_unknown() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.83666, 4800), &repeated(0.54772, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Mixed);
        assert!(result.source_confidence >= 0.55);
    }

    #[test]
    fn ambiguous_active_ratios_are_mixed_not_unknown_on_system_side() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.54772, 4800), &repeated(0.83666, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Mixed);
        assert!(result.source_confidence >= 0.55);
    }

    #[test]
    fn classifies_silence_as_unknown() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.0, 4800), &repeated(0.0, 4800), 48_000);

        let result = timeline.classify_segment(0.0, 100.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Unknown);
        assert_eq!(result.source_confidence, 0.0);
    }

    #[test]
    fn weights_partial_frame_overlap() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.7, 4800), &repeated(0.05, 4800), 48_000);
        timeline.append_window(&repeated(0.05, 4800), &repeated(0.7, 4800), 48_000);

        let result = timeline.classify_segment(25.0, 75.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Microphone);
    }

    #[test]
    fn alternating_dominant_sources_are_mixed() {
        let mut timeline = SourceAttributionTimeline::new();
        timeline.append_window(&repeated(0.7, 9600), &repeated(0.05, 9600), 48_000);
        timeline.append_window(&repeated(0.05, 9600), &repeated(0.7, 9600), 48_000);

        let result = timeline.classify_segment(0.0, 400.0);

        assert_eq!(result.audio_source, TranscriptAudioSource::Mixed);
    }

    #[test]
    fn display_label_hides_low_confidence() {
        assert_eq!(
            display_label(TranscriptAudioSource::Microphone, 0.54),
            "Speaker"
        );
        assert_eq!(display_label(TranscriptAudioSource::Microphone, 0.90), "Me");
        assert_eq!(
            display_label(TranscriptAudioSource::System, 0.90),
            "Speaker"
        );
        assert_eq!(display_label(TranscriptAudioSource::Mixed, 0.90), "Speaker");
        assert_eq!(display_label(TranscriptAudioSource::Unknown, 0.0), "Speaker");
    }

    #[test]
    fn wire_mapping_round_trips_and_rejects_unknown_strings() {
        for source in [
            TranscriptAudioSource::Microphone,
            TranscriptAudioSource::System,
            TranscriptAudioSource::Mixed,
            TranscriptAudioSource::Unknown,
        ] {
            assert_eq!(TranscriptAudioSource::from_wire(source.as_wire()), Some(source));
        }

        assert_eq!(
            TranscriptAudioSource::from_wire("  Microphone "),
            Some(TranscriptAudioSource::Microphone)
        );
        assert_eq!(TranscriptAudioSource::from_wire("speaker"), None);
        assert_eq!(TranscriptAudioSource::from_wire(""), None);
    }
}
