use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

use super::timeline::SpeakerTimelineSegment;

const MIN_SEGMENT_OVERLAP_INTERSECTION_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiarizationSegment {
    pub speaker_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlapStatus {
    Detected,
    MarkedAmbiguous,
    Resolved,
    Failed,
    Skipped,
}

impl OverlapStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Detected => "Detected",
            Self::MarkedAmbiguous => "MarkedAmbiguous",
            Self::Resolved => "Resolved",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributionSource {
    NormalDiarization,
    OverlapDetectedAmbiguous,
    Level5Resolved,
    UserCorrected,
}

impl AttributionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NormalDiarization => "NormalDiarization",
            Self::OverlapDetectedAmbiguous => "OverlapDetectedAmbiguous",
            Self::Level5Resolved => "Level5Resolved",
            Self::UserCorrected => "UserCorrected",
        }
    }
}

impl Default for AttributionSource {
    fn default() -> Self {
        Self::NormalDiarization
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapRegion {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_ids: Vec<String>,
    pub confidence: f32,
    pub estimated_speaker_count: usize,
    pub status: OverlapStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerAttributedWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: Option<String>,
    pub candidate_speaker_ids: Vec<String>,
    pub confidence: Option<f32>,
    pub overlap_region_id: Option<String>,
    pub attribution_source: AttributionSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioWindow {
    pub start_ms: u64,
    pub end_ms: u64,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerProfile {
    pub speaker_id: String,
    pub display_name: Option<String>,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlapResolverError {
    Failed(String),
}

impl fmt::Display for OverlapResolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(message) => write!(f, "overlap resolution failed: {}", message),
        }
    }
}

impl std::error::Error for OverlapResolverError {}

pub trait OverlapResolver {
    fn resolve(
        &self,
        region: &OverlapRegion,
        audio_window: &AudioWindow,
        speaker_profiles: &[SpeakerProfile],
    ) -> Result<Vec<SpeakerAttributedWord>, OverlapResolverError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpOverlapResolver;

impl OverlapResolver for NoOpOverlapResolver {
    fn resolve(
        &self,
        _region: &OverlapRegion,
        _audio_window: &AudioWindow,
        _speaker_profiles: &[SpeakerProfile],
    ) -> Result<Vec<SpeakerAttributedWord>, OverlapResolverError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlapProcessingMode {
    Live,
    PostMeetingRefinement,
}

impl OverlapProcessingMode {
    pub fn allows_heavy_work(&self) -> bool {
        matches!(self, Self::PostMeetingRefinement)
    }
}

#[derive(Debug, Clone)]
pub struct OverlapResolutionPolicy {
    pub enabled: bool,
    pub min_duration_ms: u64,
    pub confidence_threshold: f32,
    pub max_speakers: usize,
    pub mode: OverlapProcessingMode,
}

impl Default for OverlapResolutionPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            min_duration_ms: 500,
            confidence_threshold: 0.60,
            max_speakers: 2,
            mode: OverlapProcessingMode::PostMeetingRefinement,
        }
    }
}

impl OverlapResolutionPolicy {
    pub fn should_run(&self, region: &OverlapRegion) -> bool {
        self.enabled
            && region.end_ms.saturating_sub(region.start_ms) >= self.min_duration_ms
            && region.estimated_speaker_count <= self.max_speakers
            && region.confidence >= self.confidence_threshold
            && self.mode.allows_heavy_work()
    }
}

#[derive(Debug, Clone)]
pub struct OverlapDetectorConfig {
    pub min_duration_ms: u64,
    pub min_confidence: f32,
    pub merge_gap_ms: u64,
    pub max_speakers: usize,
}

impl Default for OverlapDetectorConfig {
    fn default() -> Self {
        Self {
            min_duration_ms: 500,
            min_confidence: 0.60,
            merge_gap_ms: 300,
            max_speakers: 2,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverlapDetector {
    config: OverlapDetectorConfig,
}

impl OverlapDetector {
    pub fn new(config: OverlapDetectorConfig) -> Self {
        Self { config }
    }

    pub fn detect(&self, segments: &[DiarizationSegment]) -> Vec<OverlapRegion> {
        let mut regions = Vec::new();

        for (left_index, left) in segments.iter().enumerate() {
            for right in segments.iter().skip(left_index + 1) {
                if left.speaker_id == right.speaker_id {
                    continue;
                }

                let start_ms = left.start_ms.max(right.start_ms);
                let end_ms = left.end_ms.min(right.end_ms);
                if end_ms <= start_ms {
                    continue;
                }

                let duration_ms = end_ms - start_ms;
                if duration_ms < self.config.min_duration_ms {
                    continue;
                }

                let confidence = left.confidence.min(right.confidence);
                if confidence < self.config.min_confidence {
                    continue;
                }

                regions.push(self.build_region(
                    start_ms,
                    end_ms,
                    [left.speaker_id.clone(), right.speaker_id.clone()],
                    confidence,
                ));
            }
        }

        regions.sort_by_key(|region| (region.start_ms, region.end_ms));
        self.merge_nearby_regions(regions)
    }

    fn merge_nearby_regions(&self, regions: Vec<OverlapRegion>) -> Vec<OverlapRegion> {
        let mut merged: Vec<OverlapRegion> = Vec::new();

        for region in regions {
            if let Some(last) = merged.last_mut() {
                let gap_ms = region.start_ms.saturating_sub(last.end_ms);
                if region.start_ms <= last.end_ms || gap_ms <= self.config.merge_gap_ms {
                    let last_duration = last.end_ms.saturating_sub(last.start_ms) as f32;
                    let region_duration = region.end_ms.saturating_sub(region.start_ms) as f32;
                    let total_duration = last_duration + region_duration;

                    last.end_ms = last.end_ms.max(region.end_ms);
                    last.speaker_ids = merge_speaker_ids(&last.speaker_ids, &region.speaker_ids);
                    last.estimated_speaker_count =
                        last.speaker_ids.len().min(self.config.max_speakers);
                    if total_duration > 0.0 {
                        last.confidence = ((last.confidence * last_duration)
                            + (region.confidence * region_duration))
                            / total_duration;
                    } else {
                        last.confidence = last.confidence.min(region.confidence);
                    }
                    last.id = overlap_region_id(last.start_ms, last.end_ms);
                    continue;
                }
            }

            merged.push(region);
        }

        merged
    }

    fn build_region<I>(
        &self,
        start_ms: u64,
        end_ms: u64,
        speaker_ids: I,
        confidence: f32,
    ) -> OverlapRegion
    where
        I: IntoIterator<Item = String>,
    {
        let speaker_ids = unique_sorted_speaker_ids(speaker_ids);
        OverlapRegion {
            id: overlap_region_id(start_ms, end_ms),
            start_ms,
            end_ms,
            estimated_speaker_count: speaker_ids.len().min(self.config.max_speakers),
            speaker_ids,
            confidence,
            status: OverlapStatus::Detected,
        }
    }
}

fn merge_speaker_ids(left: &[String], right: &[String]) -> Vec<String> {
    unique_sorted_speaker_ids(left.iter().chain(right.iter()).cloned())
}

fn unique_sorted_speaker_ids<I>(speaker_ids: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    speaker_ids
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn overlap_region_id(start_ms: u64, end_ms: u64) -> String {
    format!("overlap-{}-{}", start_ms, end_ms)
}

pub fn mark_words_ambiguous_for_overlaps(
    words: &mut [SpeakerAttributedWord],
    regions: &mut [OverlapRegion],
) -> usize {
    let mut marked = 0;

    for word in words {
        let Some(region) = regions.iter_mut().find(|region| {
            timestamp_ranges_intersect(word.start_ms, word.end_ms, region.start_ms, region.end_ms)
        }) else {
            continue;
        };

        word.overlap_region_id = Some(region.id.clone());
        word.attribution_source = AttributionSource::OverlapDetectedAmbiguous;
        word.candidate_speaker_ids = region.speaker_ids.clone();
        if region.speaker_ids.len() > 1 {
            word.speaker_id = None;
        }
        region.status = OverlapStatus::MarkedAmbiguous;
        marked += 1;
    }

    marked
}

pub fn replace_ambiguous_words_for_region(
    words: &mut Vec<SpeakerAttributedWord>,
    region: &mut OverlapRegion,
    mut resolved_words: Vec<SpeakerAttributedWord>,
) -> bool {
    if resolved_words.is_empty() {
        region.status = OverlapStatus::Skipped;
        return false;
    }

    let mut replaced_any = false;
    let mut inserted_resolved = false;
    for word in &mut resolved_words {
        word.overlap_region_id = Some(region.id.clone());
        word.attribution_source = AttributionSource::Level5Resolved;
        if word.candidate_speaker_ids.is_empty() {
            word.candidate_speaker_ids = region.speaker_ids.clone();
        }
    }
    resolved_words.sort_by_key(|word| (word.start_ms, word.end_ms));

    let original_words = std::mem::take(words);
    for word in original_words {
        if word.overlap_region_id.as_deref() == Some(region.id.as_str()) {
            replaced_any = true;
            if !inserted_resolved {
                words.extend(resolved_words.clone());
                inserted_resolved = true;
            }
        } else {
            words.push(word);
        }
    }

    if replaced_any {
        region.status = OverlapStatus::Resolved;
    } else {
        region.status = OverlapStatus::Skipped;
    }

    replaced_any
}

pub fn resolve_overlap_region_with_policy<R: OverlapResolver>(
    words: &mut Vec<SpeakerAttributedWord>,
    region: &mut OverlapRegion,
    audio_window: &AudioWindow,
    speaker_profiles: &[SpeakerProfile],
    resolver: &R,
    policy: &OverlapResolutionPolicy,
) -> Result<bool, OverlapResolverError> {
    if !policy.should_run(region) {
        region.status = OverlapStatus::Skipped;
        return Ok(false);
    }

    match resolver.resolve(region, audio_window, speaker_profiles) {
        Ok(resolved_words) => Ok(replace_ambiguous_words_for_region(
            words,
            region,
            resolved_words,
        )),
        Err(error) => {
            region.status = OverlapStatus::Failed;
            Err(error)
        }
    }
}

pub fn detect_overlap_regions_from_timeline(
    timeline_segments: &[SpeakerTimelineSegment],
    detector: &OverlapDetector,
) -> Vec<OverlapRegion> {
    let diarization_segments = timeline_segments
        .iter()
        .flat_map(|segment| {
            let start_ms = seconds_to_ms(segment.start_time);
            let end_ms = seconds_to_ms(segment.end_time);
            segment
                .speaker_ids
                .iter()
                .map(move |speaker_id| DiarizationSegment {
                    speaker_id: speaker_id.clone(),
                    start_ms,
                    end_ms,
                    confidence: segment.confidence,
                })
        })
        .collect::<Vec<_>>();

    detector.detect(&diarization_segments)
}

pub fn find_overlap_region_for_range(
    regions: &[OverlapRegion],
    start_ms: u64,
    end_ms: u64,
) -> Option<&OverlapRegion> {
    let segment_duration_ms = end_ms.saturating_sub(start_ms);
    if segment_duration_ms == 0 {
        return None;
    }

    let required_intersection_ms = MIN_SEGMENT_OVERLAP_INTERSECTION_MS.min(segment_duration_ms);

    regions.iter().find(|region| {
        timestamp_range_intersection_ms(start_ms, end_ms, region.start_ms, region.end_ms)
            >= required_intersection_ms
    })
}

fn timestamp_ranges_intersect(
    left_start_ms: u64,
    left_end_ms: u64,
    right_start_ms: u64,
    right_end_ms: u64,
) -> bool {
    left_start_ms < right_end_ms && left_end_ms > right_start_ms
}

fn timestamp_range_intersection_ms(
    left_start_ms: u64,
    left_end_ms: u64,
    right_start_ms: u64,
    right_end_ms: u64,
) -> u64 {
    let start_ms = left_start_ms.max(right_start_ms);
    let end_ms = left_end_ms.min(right_end_ms);
    end_ms.saturating_sub(start_ms)
}

fn seconds_to_ms(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1000.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diarization::timeline::SpeakerTimelineSegment;

    fn segment(speaker_id: &str, start_ms: u64, end_ms: u64) -> DiarizationSegment {
        DiarizationSegment {
            speaker_id: speaker_id.to_string(),
            start_ms,
            end_ms,
            confidence: 0.9,
        }
    }

    #[test]
    fn detects_no_overlap_for_sequential_speaker_turns() {
        let detector = OverlapDetector::default();
        let regions = detector.detect(&vec![
            segment("Speaker 1", 0, 1_000),
            segment("Speaker 2", 1_000, 2_000),
        ]);

        assert!(regions.is_empty());
    }

    #[test]
    fn detects_simple_two_speaker_overlap() {
        let detector = OverlapDetector::default();
        let regions = detector.detect(&vec![
            segment("Speaker 1", 0, 2_000),
            segment("Speaker 2", 1_000, 3_000),
        ]);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_ms, 1_000);
        assert_eq!(regions[0].end_ms, 2_000);
        assert_eq!(
            regions[0].speaker_ids,
            vec!["Speaker 1".to_string(), "Speaker 2".to_string()]
        );
        assert_eq!(regions[0].estimated_speaker_count, 2);
        assert_eq!(regions[0].status, OverlapStatus::Detected);
    }

    #[test]
    fn ignores_same_speaker_overlap() {
        let detector = OverlapDetector::default();
        let regions = detector.detect(&vec![
            segment("Speaker 1", 0, 2_000),
            segment("Speaker 1", 1_000, 3_000),
        ]);

        assert!(regions.is_empty());
    }

    #[test]
    fn ignores_short_overlap() {
        let detector = OverlapDetector::default();
        let regions = detector.detect(&vec![
            segment("Speaker 1", 0, 1_000),
            segment("Speaker 2", 700, 1_500),
        ]);

        assert!(regions.is_empty());
    }

    #[test]
    fn merges_nearby_overlap_regions() {
        let detector = OverlapDetector::default();
        let regions = detector.detect(&vec![
            segment("Speaker 1", 0, 1_000),
            segment("Speaker 2", 400, 1_000),
            segment("Speaker 1", 1_200, 2_200),
            segment("Speaker 2", 1_200, 2_200),
        ]);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_ms, 400);
        assert_eq!(regions[0].end_ms, 2_200);
        assert_eq!(
            regions[0].speaker_ids,
            vec!["Speaker 1".to_string(), "Speaker 2".to_string()]
        );
    }

    fn attributed_word(
        text: &str,
        start_ms: u64,
        end_ms: u64,
        speaker_id: Option<&str>,
    ) -> SpeakerAttributedWord {
        SpeakerAttributedWord {
            text: text.to_string(),
            start_ms,
            end_ms,
            speaker_id: speaker_id.map(str::to_string),
            candidate_speaker_ids: Vec::new(),
            confidence: Some(0.9),
            overlap_region_id: None,
            attribution_source: AttributionSource::NormalDiarization,
        }
    }

    fn overlap_region() -> OverlapRegion {
        OverlapRegion {
            id: "overlap-1000-1500".to_string(),
            start_ms: 1_000,
            end_ms: 1_500,
            speaker_ids: vec!["Speaker 1".to_string(), "Speaker 2".to_string()],
            confidence: 0.85,
            estimated_speaker_count: 2,
            status: OverlapStatus::Detected,
        }
    }

    #[test]
    fn detects_overlap_from_multi_speaker_timeline_segment() {
        let timeline = vec![SpeakerTimelineSegment {
            start_time: 10.2,
            end_time: 12.8,
            speaker_ids: vec!["Speaker 1".to_string(), "Speaker 2".to_string()],
            confidence: 0.85,
            overlap: true,
        }];

        let regions = detect_overlap_regions_from_timeline(&timeline, &OverlapDetector::default());

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].start_ms, 10_200);
        assert_eq!(regions[0].end_ms, 12_800);
        assert_eq!(regions[0].estimated_speaker_count, 2);
    }

    #[test]
    fn marks_intersecting_words_as_overlap_ambiguous() {
        let mut words = vec![
            attributed_word("before", 0, 900, Some("Speaker 1")),
            attributed_word("mixed", 1_100, 1_250, Some("Speaker 1")),
        ];
        let mut regions = vec![overlap_region()];

        let marked = mark_words_ambiguous_for_overlaps(&mut words, &mut regions);

        assert_eq!(marked, 1);
        assert_eq!(words[0].speaker_id.as_deref(), Some("Speaker 1"));
        assert_eq!(
            words[0].attribution_source,
            AttributionSource::NormalDiarization
        );
        assert_eq!(words[1].speaker_id, None);
        assert_eq!(
            words[1].candidate_speaker_ids,
            vec!["Speaker 1".to_string(), "Speaker 2".to_string()]
        );
        assert_eq!(
            words[1].overlap_region_id.as_deref(),
            Some("overlap-1000-1500")
        );
        assert_eq!(
            words[1].attribution_source,
            AttributionSource::OverlapDetectedAmbiguous
        );
        assert_eq!(regions[0].status, OverlapStatus::MarkedAmbiguous);
    }

    #[test]
    fn segment_overlap_lookup_ignores_tiny_edge_intersections() {
        let regions = vec![OverlapRegion {
            id: "overlap-20050-25050".to_string(),
            start_ms: 20_050,
            end_ms: 25_050,
            speaker_ids: vec!["Speaker 1".to_string(), "Speaker 2".to_string()],
            confidence: 0.8,
            estimated_speaker_count: 2,
            status: OverlapStatus::Detected,
        }];

        assert!(find_overlap_region_for_range(&regions, 24_810, 41_320).is_none());
        assert!(find_overlap_region_for_range(&regions, 24_300, 25_400).is_some());
    }

    #[test]
    fn noop_overlap_resolver_returns_no_words() {
        let resolver = NoOpOverlapResolver;
        let region = overlap_region();
        let audio_window = AudioWindow {
            start_ms: region.start_ms,
            end_ms: region.end_ms,
            sample_rate: 16_000,
            samples: Vec::new(),
        };

        let resolved = resolver
            .resolve(&region, &audio_window, &[])
            .expect("noop resolver should not fail");

        assert!(resolved.is_empty());
    }

    #[test]
    fn policy_blocks_level5_resolution_during_live_processing() {
        let region = overlap_region();
        let enabled_policy = OverlapResolutionPolicy {
            enabled: true,
            mode: OverlapProcessingMode::Live,
            ..OverlapResolutionPolicy::default()
        };
        let post_meeting_policy = OverlapResolutionPolicy {
            mode: OverlapProcessingMode::PostMeetingRefinement,
            ..enabled_policy.clone()
        };

        assert!(!enabled_policy.should_run(&region));
        assert!(post_meeting_policy.should_run(&region));
    }

    #[test]
    fn resolved_words_replace_only_the_ambiguous_region() {
        let mut words = vec![
            attributed_word("before", 0, 900, Some("Speaker 1")),
            attributed_word("mixed", 1_100, 1_250, None),
            attributed_word("after", 1_700, 1_900, Some("Speaker 2")),
        ];
        words[1].overlap_region_id = Some("overlap-1000-1500".to_string());
        words[1].attribution_source = AttributionSource::OverlapDetectedAmbiguous;
        let mut region = overlap_region();
        let resolved = vec![
            attributed_word("hello", 1_050, 1_150, Some("Speaker 1")),
            attributed_word("there", 1_150, 1_350, Some("Speaker 2")),
        ];

        let replaced = replace_ambiguous_words_for_region(&mut words, &mut region, resolved);

        assert!(replaced);
        assert_eq!(region.status, OverlapStatus::Resolved);
        assert_eq!(
            words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>(),
            vec!["before", "hello", "there", "after"]
        );
        assert_eq!(
            words[1].attribution_source,
            AttributionSource::Level5Resolved
        );
        assert_eq!(
            words[2].attribution_source,
            AttributionSource::Level5Resolved
        );
    }
}
