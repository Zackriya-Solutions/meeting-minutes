// diarization/session.rs
//
// Per-recording diarization state: embedding extractor + online clusterer.
// Created when a recording starts (if the feature is enabled and the model
// is present) and dropped when it ends.

use super::clustering::SpeakerClusterer;
use super::embedding::{EmbeddingError, EmbeddingExtractor};
use super::timeline::{RollingDiarizationBuffer, SpeakerTimeline, SpeakerTimelineSegment};
use std::path::Path;

/// Minimum samples needed for the fbank frontend to produce the 10 frames
/// required by EmbeddingExtractor::compute (25ms frame + 9 * 10ms shifts).
const MIN_SAMPLES_FOR_EMBEDDING: usize = 1_840;
const DEFAULT_DIARIZATION_WINDOW_SECONDS: f64 = 10.0;
const DEFAULT_DIARIZATION_STRIDE_SECONDS: f64 = 5.0;
const DIARIZATION_SAMPLE_RATE: u32 = 16_000;

fn has_enough_samples_for_embedding(samples_len: usize) -> bool {
    samples_len >= MIN_SAMPLES_FOR_EMBEDDING
}

fn select_live_speaker_label(
    timeline_label: Option<String>,
    direct_segment_label: Option<String>,
    last_label: Option<String>,
) -> Option<String> {
    direct_segment_label.or(timeline_label).or(last_label)
}

fn should_compute_direct_segment_label(samples_len: usize) -> bool {
    has_enough_samples_for_embedding(samples_len)
}

pub struct DiarizationSession {
    extractor: EmbeddingExtractor,
    clusterer: SpeakerClusterer,
    rolling_buffer: RollingDiarizationBuffer,
    speaker_timeline: SpeakerTimeline,
}

impl DiarizationSession {
    pub fn new(embedding_model_path: &Path) -> Result<Self, EmbeddingError> {
        Self::with_profiles(embedding_model_path, Vec::new())
    }

    /// Create a session pre-seeded with saved voice profiles (name, centroid)
    /// so returning speakers are labeled by name instead of "Speaker N".
    pub fn with_profiles(
        embedding_model_path: &Path,
        profiles: Vec<(String, Vec<f32>)>,
    ) -> Result<Self, EmbeddingError> {
        let mut clusterer = SpeakerClusterer::new();
        for (name, centroid) in profiles {
            clusterer.seed_profile(&name, centroid);
        }
        Ok(Self {
            extractor: EmbeddingExtractor::new(embedding_model_path)?,
            clusterer,
            rolling_buffer: RollingDiarizationBuffer::new(
                DIARIZATION_SAMPLE_RATE,
                DEFAULT_DIARIZATION_WINDOW_SECONDS,
                DEFAULT_DIARIZATION_STRIDE_SECONDS,
            ),
            speaker_timeline: SpeakerTimeline::new(),
        })
    }

    /// (label, centroid, segment count) snapshot for persisting this
    /// recording's speakers (written to speakers.json at recording end).
    pub fn centroid_snapshot(&self) -> Vec<(String, Vec<f32>, usize)> {
        self.clusterer
            .centroids()
            .map(|(label, centroid, count)| (label.to_string(), centroid.to_vec(), count))
            .collect()
    }

    /// Assign a speaker label to a 16kHz mono speech segment.
    /// Returns None only when no label can be produced (e.g. first segment
    /// is too short). Diarization failures must never break transcription —
    /// errors are logged and degrade to the previous label or None.
    pub fn label_segment(&mut self, samples_16k: &[f32]) -> Option<String> {
        if !has_enough_samples_for_embedding(samples_16k.len()) {
            return self.clusterer.last_label();
        }
        match self.extractor.compute(samples_16k) {
            Ok(embedding) => Some(self.clusterer.assign(&embedding)),
            Err(e) => {
                log::warn!(
                    "Diarization embedding failed, carrying previous label: {}",
                    e
                );
                self.clusterer.last_label()
            }
        }
    }

    /// Observe a 16kHz mono ASR chunk at its recording-relative timestamp,
    /// update the rolling diarization timeline when enough context exists,
    /// then align the ASR chunk back onto the best speaker label.
    pub fn label_segment_at(&mut self, start_time: f64, samples_16k: &[f32]) -> Option<String> {
        let duration = samples_16k.len() as f64 / DIARIZATION_SAMPLE_RATE as f64;
        let end_time = start_time + duration;

        for window in self.rolling_buffer.push_samples_at(start_time, samples_16k) {
            if !has_enough_samples_for_embedding(window.samples.len()) {
                continue;
            }

            match self.extractor.compute(&window.samples) {
                Ok(embedding) => {
                    let label = self.clusterer.assign(&embedding);
                    self.speaker_timeline
                        .push_window_segment(SpeakerTimelineSegment {
                            start_time: window.start_time,
                            end_time: window.end_time,
                            speaker_ids: vec![label],
                            confidence: 0.8,
                            overlap: false,
                        });
                }
                Err(e) => {
                    log::warn!("Diarization window embedding failed: {}", e);
                }
            }
        }

        let timeline_label = self
            .speaker_timeline
            .speaker_label_for_range(start_time, end_time);

        let direct_segment_label = if should_compute_direct_segment_label(samples_16k.len()) {
            self.label_segment(samples_16k)
        } else {
            None
        };

        select_live_speaker_label(
            timeline_label,
            direct_segment_label,
            self.clusterer.last_label(),
        )
    }

    pub fn timeline_snapshot(&self) -> Vec<SpeakerTimelineSegment> {
        self.speaker_timeline.segments().to_vec()
    }

    pub fn clusterer(&self) -> &SpeakerClusterer {
        &self.clusterer
    }

    pub fn clusterer_mut(&mut self) -> &mut SpeakerClusterer {
        &mut self.clusterer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_gate_matches_minimum_fbank_frames() {
        assert!(!has_enough_samples_for_embedding(
            MIN_SAMPLES_FOR_EMBEDDING - 1
        ));
        assert!(has_enough_samples_for_embedding(MIN_SAMPLES_FOR_EMBEDDING));
    }

    #[test]
    fn live_label_uses_warmup_label_before_timeline_exists() {
        assert_eq!(
            select_live_speaker_label(None, Some("Speaker 1".to_string()), None).as_deref(),
            Some("Speaker 1")
        );
    }

    #[test]
    fn live_label_uses_timeline_when_direct_segment_label_is_absent() {
        assert_eq!(
            select_live_speaker_label(
                Some("Speaker 2".to_string()),
                None,
                Some("Speaker 3".to_string())
            )
            .as_deref(),
            Some("Speaker 2")
        );
    }

    #[test]
    fn live_label_prefers_direct_segment_over_timeline_for_chunk_turns() {
        assert_eq!(
            select_live_speaker_label(
                Some("Speaker 2".to_string()),
                Some("Speaker 1".to_string()),
                Some("Speaker 2".to_string())
            )
            .as_deref(),
            Some("Speaker 1")
        );
    }

    #[test]
    fn live_label_computes_direct_label_when_timeline_misses_after_startup() {
        assert!(should_compute_direct_segment_label(
            MIN_SAMPLES_FOR_EMBEDDING
        ));
    }
}
