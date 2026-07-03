use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const ATTRIBUTION_FRAME_MS: f64 = 100.0;
const ENERGY_FLOOR: f64 = 1.0e-8;
const DOMINANCE_RATIO: f64 = 0.72;
const MIN_DB_MARGIN: f64 = 4.0;
const MIXED_RATIO_LOW: f64 = 1.0 - DOMINANCE_RATIO;
const MIXED_RATIO_HIGH: f64 = DOMINANCE_RATIO;
const PRUNE_SAFETY_MARGIN_MS: f64 = 3000.0;
const MAX_TIMELINE_RETENTION_MS: f64 = 30.0 * 60.0 * 1000.0;
const MIN_ALTERNATING_SOURCE_MS: f64 = 200.0;
const MIN_ALTERNATING_SOURCE_RATIO: f64 = 0.20;
const MIN_DISPLAY_CONFIDENCE: f32 = 0.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptAudioSource {
    Microphone,
    System,
    Mixed,
    Unknown,
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
}
