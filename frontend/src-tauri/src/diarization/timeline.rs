use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq)]
pub struct DiarizationWindow {
    pub start_time: f64,
    pub end_time: f64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerTimelineSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub speaker_ids: Vec<String>,
    pub confidence: f32,
    pub overlap: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlignedTranscriptSegment {
    pub transcript_id: String,
    pub speaker_ids: Vec<String>,
    pub confidence: f32,
}

pub struct RollingDiarizationBuffer {
    sample_rate: u32,
    window_samples: usize,
    stride_samples: usize,
    buffer: VecDeque<f32>,
    buffer_start_sample: u64,
    next_window_start_sample: u64,
    initialized: bool,
}

impl RollingDiarizationBuffer {
    pub fn new(sample_rate: u32, window_seconds: f64, stride_seconds: f64) -> Self {
        let window_samples = (sample_rate as f64 * window_seconds).round() as usize;
        let stride_samples = (sample_rate as f64 * stride_seconds).round() as usize;

        Self {
            sample_rate,
            window_samples: window_samples.max(1),
            stride_samples: stride_samples.max(1),
            buffer: VecDeque::new(),
            buffer_start_sample: 0,
            next_window_start_sample: 0,
            initialized: false,
        }
    }

    pub fn push_samples(&mut self, samples: &[f32]) -> Vec<DiarizationWindow> {
        self.initialized = true;
        self.buffer.extend(samples.iter().copied());
        self.collect_complete_windows()
    }

    pub fn push_samples_at(&mut self, start_time: f64, samples: &[f32]) -> Vec<DiarizationWindow> {
        let start_sample = (start_time.max(0.0) * self.sample_rate as f64).round() as u64;

        if !self.initialized {
            self.buffer_start_sample = start_sample;
            self.next_window_start_sample = start_sample;
            self.initialized = true;
        }

        let buffer_end_sample = self.buffer_start_sample + self.buffer.len() as u64;
        if start_sample > buffer_end_sample {
            if self.buffer.is_empty() {
                self.buffer_start_sample = start_sample;
                self.next_window_start_sample = self.next_window_start_sample.max(start_sample);
            } else {
                let gap_samples = (start_sample - buffer_end_sample) as usize;
                self.buffer.extend(std::iter::repeat(0.0).take(gap_samples));
            }
        }

        let refreshed_buffer_end_sample = self.buffer_start_sample + self.buffer.len() as u64;
        let overlap_samples = refreshed_buffer_end_sample.saturating_sub(start_sample) as usize;
        if overlap_samples < samples.len() {
            self.buffer
                .extend(samples[overlap_samples..].iter().copied());
        }

        self.collect_complete_windows()
    }

    fn collect_complete_windows(&mut self) -> Vec<DiarizationWindow> {
        let mut windows = Vec::new();
        while self.has_complete_next_window() {
            let start_offset = (self.next_window_start_sample - self.buffer_start_sample) as usize;
            let samples = self
                .buffer
                .iter()
                .skip(start_offset)
                .take(self.window_samples)
                .copied()
                .collect::<Vec<_>>();
            let start_time = self.next_window_start_sample as f64 / self.sample_rate as f64;
            let end_time = (self.next_window_start_sample + self.window_samples as u64) as f64
                / self.sample_rate as f64;

            windows.push(DiarizationWindow {
                start_time,
                end_time,
                samples,
            });

            self.next_window_start_sample += self.stride_samples as u64;
            self.trim_consumed_prefix();
        }

        windows
    }

    fn has_complete_next_window(&self) -> bool {
        let buffer_end_sample = self.buffer_start_sample + self.buffer.len() as u64;
        buffer_end_sample >= self.next_window_start_sample + self.window_samples as u64
    }

    fn trim_consumed_prefix(&mut self) {
        if self.next_window_start_sample <= self.buffer_start_sample {
            return;
        }

        let samples_to_drop = (self.next_window_start_sample - self.buffer_start_sample) as usize;
        let samples_to_drop = samples_to_drop.min(self.buffer.len());
        self.buffer.drain(..samples_to_drop);
        self.buffer_start_sample += samples_to_drop as u64;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpeakerTimeline {
    segments: Vec<SpeakerTimelineSegment>,
}

impl SpeakerTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_window_segments(segments: Vec<SpeakerTimelineSegment>) -> Self {
        let mut timeline = Self { segments };
        timeline.merge_overlapping_windows();
        timeline
    }

    pub fn push_window_segment(&mut self, segment: SpeakerTimelineSegment) {
        self.segments.push(segment);
        self.merge_overlapping_windows();
    }

    pub fn segments(&self) -> &[SpeakerTimelineSegment] {
        &self.segments
    }

    pub fn align_transcript(
        &self,
        transcript_id: &str,
        start_time: f64,
        end_time: f64,
    ) -> Option<AlignedTranscriptSegment> {
        let mut overlap_by_speaker: BTreeMap<String, f64> = BTreeMap::new();
        let mut confidence_weighted_sum = 0.0f64;
        let mut total_overlap = 0.0f64;

        for segment in &self.segments {
            let overlap_start = start_time.max(segment.start_time);
            let overlap_end = end_time.min(segment.end_time);
            let overlap = (overlap_end - overlap_start).max(0.0);
            if overlap <= 0.0 {
                continue;
            }

            for speaker_id in &segment.speaker_ids {
                *overlap_by_speaker.entry(speaker_id.clone()).or_default() += overlap;
            }
            confidence_weighted_sum += segment.confidence as f64 * overlap;
            total_overlap += overlap;
        }

        if overlap_by_speaker.is_empty() {
            return None;
        }

        let max_overlap = overlap_by_speaker.values().copied().fold(0.0f64, f64::max);
        let speaker_ids = overlap_by_speaker
            .into_iter()
            .filter_map(|(speaker, overlap)| {
                if (overlap - max_overlap).abs() < f64::EPSILON {
                    Some(speaker)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Some(AlignedTranscriptSegment {
            transcript_id: transcript_id.to_string(),
            speaker_ids,
            confidence: if total_overlap > 0.0 {
                (confidence_weighted_sum / total_overlap) as f32
            } else {
                0.0
            },
        })
    }

    pub fn speaker_label_for_range(&self, start_time: f64, end_time: f64) -> Option<String> {
        self.align_transcript("", start_time, end_time)
            .map(|aligned| aligned.speaker_ids.join(" + "))
            .filter(|label| !label.is_empty())
    }

    pub fn merge_false_singletons(&mut self, max_singleton_duration_seconds: f64) {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for segment in &self.segments {
            if segment.speaker_ids.len() == 1 {
                *counts.entry(segment.speaker_ids[0].clone()).or_default() += 1;
            }
        }

        for index in 0..self.segments.len() {
            if self.segments[index].speaker_ids.len() != 1 {
                continue;
            }

            let speaker = self.segments[index].speaker_ids[0].clone();
            let is_singleton = counts.get(&speaker).copied().unwrap_or_default() == 1;
            let duration = self.segments[index].end_time - self.segments[index].start_time;
            if !is_singleton || duration > max_singleton_duration_seconds {
                continue;
            }

            if let Some(replacement) = self.nearest_stable_speaker(index, &counts) {
                self.segments[index].speaker_ids = vec![replacement];
                self.segments[index].overlap = false;
            }
        }

        self.coalesce_adjacent();
    }

    fn nearest_stable_speaker(
        &self,
        index: usize,
        counts: &BTreeMap<String, usize>,
    ) -> Option<String> {
        let current = &self.segments[index];
        let previous = self.segments[..index].iter().rev().find_map(|segment| {
            stable_single_speaker(segment, counts).map(|speaker| {
                (
                    speaker,
                    (current.start_time - segment.end_time).max(0.0),
                    true,
                )
            })
        });
        let next = self.segments[index + 1..].iter().find_map(|segment| {
            stable_single_speaker(segment, counts).map(|speaker| {
                (
                    speaker,
                    (segment.start_time - current.end_time).max(0.0),
                    false,
                )
            })
        });

        match (previous, next) {
            (Some(prev), Some(next)) => {
                if prev.1 <= next.1 {
                    Some(prev.0)
                } else {
                    Some(next.0)
                }
            }
            (Some(prev), None) => Some(prev.0),
            (None, Some(next)) => Some(next.0),
            (None, None) => None,
        }
    }

    fn merge_overlapping_windows(&mut self) {
        if self.segments.is_empty() {
            return;
        }

        let source = self.segments.clone();
        let mut boundaries = source
            .iter()
            .flat_map(|segment| [segment.start_time, segment.end_time])
            .collect::<Vec<_>>();
        boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        boundaries.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

        let mut merged = Vec::new();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if end <= start {
                continue;
            }

            let covering = source
                .iter()
                .filter(|segment| segment.start_time < end && segment.end_time > start)
                .collect::<Vec<_>>();
            if covering.is_empty() {
                continue;
            }

            let mut speakers = BTreeSet::new();
            let mut confidence_sum = 0.0f32;
            let mut source_overlap = false;
            for segment in &covering {
                for speaker in &segment.speaker_ids {
                    speakers.insert(speaker.clone());
                }
                confidence_sum += segment.confidence;
                source_overlap |= segment.overlap;
            }

            let speaker_ids = speakers.into_iter().collect::<Vec<_>>();
            merged.push(SpeakerTimelineSegment {
                start_time: start,
                end_time: end,
                overlap: source_overlap || speaker_ids.len() > 1,
                confidence: confidence_sum / covering.len() as f32,
                speaker_ids,
            });
        }

        self.segments = merged;
        self.coalesce_adjacent();
    }

    fn coalesce_adjacent(&mut self) {
        let mut coalesced: Vec<SpeakerTimelineSegment> = Vec::new();
        for segment in self.segments.drain(..) {
            if let Some(last) = coalesced.last_mut() {
                if last.speaker_ids == segment.speaker_ids
                    && last.overlap == segment.overlap
                    && (last.end_time - segment.start_time).abs() < f64::EPSILON
                {
                    let last_duration = last.end_time - last.start_time;
                    let segment_duration = segment.end_time - segment.start_time;
                    let total_duration = last_duration + segment_duration;
                    if total_duration > 0.0 {
                        last.confidence = ((last.confidence as f64 * last_duration
                            + segment.confidence as f64 * segment_duration)
                            / total_duration) as f32;
                    }
                    last.end_time = segment.end_time;
                    continue;
                }
            }

            coalesced.push(segment);
        }
        self.segments = coalesced;
    }
}

fn stable_single_speaker(
    segment: &SpeakerTimelineSegment,
    counts: &BTreeMap<String, usize>,
) -> Option<String> {
    if segment.speaker_ids.len() == 1
        && counts
            .get(&segment.speaker_ids[0])
            .copied()
            .unwrap_or_default()
            > 1
    {
        Some(segment.speaker_ids[0].clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, speaker: &str) -> SpeakerTimelineSegment {
        SpeakerTimelineSegment {
            start_time: start,
            end_time: end,
            speaker_ids: vec![speaker.to_string()],
            confidence: 0.9,
            overlap: false,
        }
    }

    #[test]
    fn buffers_ten_second_windows_with_five_second_stride() {
        let mut buffer = RollingDiarizationBuffer::new(16_000, 10.0, 5.0);

        assert!(buffer.push_samples(&vec![0.0; 16_000 * 9]).is_empty());

        let first = buffer.push_samples(&vec![0.0; 16_000]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].start_time, 0.0);
        assert_eq!(first[0].end_time, 10.0);
        assert_eq!(first[0].samples.len(), 160_000);

        let second = buffer.push_samples(&vec![0.0; 16_000 * 5]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].start_time, 5.0);
        assert_eq!(second[0].end_time, 15.0);
        assert_eq!(second[0].samples.len(), 160_000);
    }

    #[test]
    fn timed_buffer_anchors_first_window_to_first_audio_timestamp() {
        let mut buffer = RollingDiarizationBuffer::new(16_000, 10.0, 5.0);

        let windows = buffer.push_samples_at(12.0, &vec![0.0; 160_000]);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_time, 12.0);
        assert_eq!(windows[0].end_time, 22.0);
    }

    #[test]
    fn merges_overlapping_windows_into_deterministic_activity_segments() {
        let timeline = SpeakerTimeline::from_window_segments(vec![
            seg(0.0, 10.0, "Speaker 1"),
            seg(5.0, 15.0, "Speaker 2"),
        ]);

        assert_eq!(timeline.segments().len(), 3);
        assert_eq!(timeline.segments()[0].speaker_ids, vec!["Speaker 1"]);
        assert_eq!(
            timeline.segments()[1].speaker_ids,
            vec!["Speaker 1", "Speaker 2"]
        );
        assert!(timeline.segments()[1].overlap);
        assert_eq!(timeline.segments()[2].speaker_ids, vec!["Speaker 2"]);
    }

    #[test]
    fn merges_false_singleton_cluster_into_nearest_stable_speaker() {
        let mut timeline = SpeakerTimeline::from_window_segments(vec![
            seg(0.0, 5.0, "Speaker 1"),
            seg(5.0, 10.0, "Speaker 2"),
            seg(10.0, 15.0, "Speaker 1"),
        ]);

        timeline.merge_false_singletons(6.0);

        assert_eq!(timeline.segments().len(), 1);
        assert_eq!(timeline.segments()[0].speaker_ids, vec!["Speaker 1"]);
        assert_eq!(timeline.segments()[0].start_time, 0.0);
        assert_eq!(timeline.segments()[0].end_time, 15.0);
    }

    #[test]
    fn aligns_transcript_range_to_speaker_with_most_overlap() {
        let timeline = SpeakerTimeline::from_window_segments(vec![
            seg(0.0, 4.0, "Speaker 1"),
            seg(4.0, 9.0, "Speaker 2"),
        ]);

        let aligned = timeline
            .align_transcript("transcript-1", 3.5, 6.5)
            .expect("speaker should align");

        assert_eq!(aligned.transcript_id, "transcript-1");
        assert_eq!(aligned.speaker_ids, vec!["Speaker 2"]);
    }
}
