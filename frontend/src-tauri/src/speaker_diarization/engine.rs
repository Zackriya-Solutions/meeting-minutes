use super::models::SpeakerModelPaths;
use super::types::{DiarizationTurn, SpeakerAudioSegment, SpeakerLabelUpdate};
use crate::audio::vad::SpeechSegment;
use anyhow::{anyhow, Result};
use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

const SAMPLE_RATE: i32 = 16_000;
const SHORT_TURN_SECONDS: f64 = 0.5;
const SPEAKER_MATCH_THRESHOLD: f32 = 0.6;
const MAX_REALTIME_SPEAKERS: usize = 10;

pub struct DiarizationEngine {
    diarizer: OfflineSpeakerDiarization,
    extractor: SpeakerEmbeddingExtractor,
}

impl DiarizationEngine {
    pub fn new(paths: &SpeakerModelPaths, num_speakers: Option<usize>) -> Result<Self> {
        if let Some(count) = num_speakers {
            if !(1..=10).contains(&count) {
                return Err(anyhow!("Speaker count must be between 1 and 10"));
            }
        }

        let segmentation_path = paths.segmentation.to_string_lossy().to_string();
        let embedding_path = paths.embedding.to_string_lossy().to_string();
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(segmentation_path),
                },
                num_threads: 1,
                debug: false,
                provider: Some("cpu".to_string()),
            },
            embedding: SpeakerEmbeddingExtractorConfig {
                model: Some(embedding_path.clone()),
                num_threads: 1,
                debug: false,
                provider: Some("cpu".to_string()),
            },
            clustering: FastClusteringConfig {
                num_clusters: num_speakers.map(|value| value as i32).unwrap_or(-1),
                threshold: 0.5,
            },
            min_duration_on: 0.3,
            min_duration_off: 0.5,
        };

        let diarizer = OfflineSpeakerDiarization::create(&config)
            .ok_or_else(|| anyhow!("Failed to initialize sherpa speaker diarization"))?;
        let extractor = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig {
            model: Some(embedding_path),
            num_threads: 1,
            debug: false,
            provider: Some("cpu".to_string()),
        })
        .ok_or_else(|| anyhow!("Failed to initialize speaker embedding extractor"))?;

        Ok(Self {
            diarizer,
            extractor,
        })
    }

    pub fn diarize(&self, samples: &[f32]) -> Result<Vec<DiarizationTurn>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let result = self
            .diarizer
            .process(samples)
            .ok_or_else(|| anyhow!("Speaker diarization returned no result"))?;
        let raw = result.sort_by_start_time();
        let mut first_seen = HashMap::<i32, String>::new();
        let mut next_speaker = 0usize;
        let turns = raw
            .into_iter()
            .filter(|segment| segment.end > segment.start)
            .map(|segment| {
                let speaker = first_seen
                    .entry(segment.speaker)
                    .or_insert_with(|| {
                        let name = format!("speaker_{next_speaker:02}");
                        next_speaker += 1;
                        name
                    })
                    .clone();
                DiarizationTurn {
                    start: segment.start as f64,
                    end: segment.end as f64,
                    speaker,
                }
            })
            .collect::<Vec<_>>();
        Ok(stabilize_turns(&turns))
    }

    pub fn embedding(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let stream = self.extractor.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE, samples);
        stream.input_finished();
        if !self.extractor.is_ready(&stream) {
            return None;
        }
        self.extractor
            .compute(&stream)
            .and_then(normalize_embedding)
    }
}

#[derive(Debug, Clone)]
struct SpeakerCentroid {
    name: String,
    embedding: Vec<f32>,
    weight: f32,
}

#[derive(Debug, Default)]
struct SpeakerTracker {
    centroids: Vec<SpeakerCentroid>,
}

impl SpeakerTracker {
    fn assign(&mut self, embedding: &[f32], weight: f32) -> String {
        let best = self
            .centroids
            .iter()
            .enumerate()
            .filter_map(|(index, centroid)| {
                cosine_similarity(&centroid.embedding, embedding).map(|score| (index, score))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

        let index = match best {
            Some((index, score))
                if score >= SPEAKER_MATCH_THRESHOLD
                    || self.centroids.len() >= MAX_REALTIME_SPEAKERS =>
            {
                index
            }
            _ => {
                let name = format!("speaker_{:02}", self.centroids.len());
                self.centroids.push(SpeakerCentroid {
                    name: name.clone(),
                    embedding: embedding.to_vec(),
                    weight: weight.max(0.1),
                });
                return name;
            }
        };

        let centroid = &mut self.centroids[index];
        let new_weight = weight.max(0.1);
        let total_weight = centroid.weight + new_weight;
        for (current, incoming) in centroid.embedding.iter_mut().zip(embedding.iter()) {
            *current = (*current * centroid.weight + *incoming * new_weight) / total_weight;
        }
        if let Some(normalized) = normalize_embedding(centroid.embedding.clone()) {
            centroid.embedding = normalized;
        }
        centroid.weight = total_weight;
        centroid.name.clone()
    }
}

pub struct RealtimeSpeakerSession {
    engine: DiarizationEngine,
    tracker: SpeakerTracker,
}

impl RealtimeSpeakerSession {
    pub fn new(paths: &SpeakerModelPaths) -> Result<Self> {
        Ok(Self {
            engine: DiarizationEngine::new(paths, None)?,
            tracker: SpeakerTracker::default(),
        })
    }

    pub fn process_chunk(
        &mut self,
        samples: &[f32],
        chunk_start_seconds: f64,
    ) -> Result<Vec<SpeakerAudioSegment>> {
        if samples.is_empty() {
            return Ok(Vec::new());
        }

        let duration = samples.len() as f64 / SAMPLE_RATE as f64;
        let turns = self.engine.diarize(samples).unwrap_or_default();
        let mut local_segments = audio_segments_for_range(samples, 0.0, duration, &turns);
        if local_segments.is_empty() {
            local_segments.push(SpeakerAudioSegment {
                samples: samples.to_vec(),
                start_seconds: 0.0,
                end_seconds: duration,
                speaker: None,
            });
        }

        let mut grouped = HashMap::<String, Vec<f32>>::new();
        let mut local_speaker_order = Vec::<String>::new();
        let mut weights = HashMap::<String, f32>::new();
        for segment in &local_segments {
            if let Some(speaker) = &segment.speaker {
                if !grouped.contains_key(speaker) {
                    local_speaker_order.push(speaker.clone());
                }
                grouped
                    .entry(speaker.clone())
                    .or_default()
                    .extend_from_slice(&segment.samples);
                *weights.entry(speaker.clone()).or_default() +=
                    (segment.end_seconds - segment.start_seconds) as f32;
            }
        }

        let mut local_to_global = HashMap::<String, String>::new();
        for local_speaker in &local_speaker_order {
            let Some(audio) = grouped.get(local_speaker) else {
                continue;
            };
            if let Some(embedding) = self.engine.embedding(audio) {
                let global = self
                    .tracker
                    .assign(&embedding, *weights.get(local_speaker).unwrap_or(&1.0));
                local_to_global.insert(local_speaker.clone(), global);
            }
        }

        let chunk_fallback = if local_speaker_order.is_empty() {
            if let Some(embedding) = self.engine.embedding(samples) {
                Some(self.tracker.assign(&embedding, duration as f32))
            } else {
                None
            }
        } else {
            None
        };

        for segment in &mut local_segments {
            let mapped = segment
                .speaker
                .as_ref()
                .and_then(|speaker| local_to_global.get(speaker))
                .cloned()
                .or_else(|| {
                    if segment.speaker.is_none() {
                        chunk_fallback.clone()
                    } else {
                        None
                    }
                });
            segment.speaker = mapped;
            segment.start_seconds += chunk_start_seconds;
            segment.end_seconds += chunk_start_seconds;
        }

        Ok(merge_audio_segments(local_segments))
    }
}

pub fn align_vad_with_turns(
    full_audio: &[f32],
    vad_segments: &[SpeechSegment],
    turns: &[DiarizationTurn],
) -> Vec<SpeakerAudioSegment> {
    let audio_duration = full_audio.len() as f64 / SAMPLE_RATE as f64;
    let mut result = Vec::new();
    for vad in vad_segments {
        let start = (vad.start_timestamp_ms / 1000.0).clamp(0.0, audio_duration);
        let end = (vad.end_timestamp_ms / 1000.0).clamp(start, audio_duration);
        if end <= start {
            continue;
        }
        let start_index = seconds_to_index(start, full_audio.len());
        let end_index = seconds_to_index(end, full_audio.len());
        if end_index <= start_index {
            continue;
        }
        result.extend(audio_segments_for_range(
            &full_audio[start_index..end_index],
            start,
            end,
            turns,
        ));
    }
    merge_audio_segments(result)
}

fn audio_segments_for_range(
    samples: &[f32],
    range_start: f64,
    range_end: f64,
    turns: &[DiarizationTurn],
) -> Vec<SpeakerAudioSegment> {
    if samples.is_empty() || range_end <= range_start {
        return Vec::new();
    }
    let relevant = turns
        .iter()
        .filter_map(|turn| {
            let start = turn.start.max(range_start);
            let end = turn.end.min(range_end);
            (end > start).then_some(DiarizationTurn {
                start,
                end,
                speaker: turn.speaker.clone(),
            })
        })
        .collect::<Vec<_>>();

    if relevant.is_empty() {
        return vec![SpeakerAudioSegment {
            samples: samples.to_vec(),
            start_seconds: range_start,
            end_seconds: range_end,
            speaker: None,
        }];
    }

    let mut boundaries = vec![range_start, range_end];
    for turn in &relevant {
        boundaries.push(turn.start);
        boundaries.push(turn.end);
    }
    boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    boundaries.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut intervals = Vec::<(f64, f64, Option<String>)>::new();
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        if end <= start {
            continue;
        }
        let speaker = relevant
            .iter()
            .filter(|turn| turn.start < end && turn.end > start)
            .max_by(|a, b| {
                (a.end - a.start)
                    .partial_cmp(&(b.end - b.start))
                    .unwrap_or(Ordering::Equal)
            })
            .map(|turn| turn.speaker.clone());
        intervals.push((start, end, speaker));
    }

    let total_duration = range_end - range_start;
    intervals
        .into_iter()
        .filter_map(|(start, end, speaker)| {
            let relative_start = ((start - range_start) / total_duration).clamp(0.0, 1.0);
            let relative_end = ((end - range_start) / total_duration).clamp(0.0, 1.0);
            let start_index = (relative_start * samples.len() as f64).round() as usize;
            let end_index = (relative_end * samples.len() as f64).round() as usize;
            (end_index > start_index && end_index <= samples.len()).then(|| SpeakerAudioSegment {
                samples: samples[start_index..end_index].to_vec(),
                start_seconds: start,
                end_seconds: end,
                speaker,
            })
        })
        .collect()
}

fn stabilize_turns(turns: &[DiarizationTurn]) -> Vec<DiarizationTurn> {
    if turns.is_empty() {
        return Vec::new();
    }
    let mut boundaries = Vec::with_capacity(turns.len() * 2);
    for turn in turns {
        boundaries.push(turn.start);
        boundaries.push(turn.end);
    }
    boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    boundaries.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut flattened = Vec::new();
    for pair in boundaries.windows(2) {
        let start = pair[0];
        let end = pair[1];
        let speaker = turns
            .iter()
            .filter(|turn| turn.start < end && turn.end > start)
            .max_by(|a, b| {
                (a.end - a.start)
                    .partial_cmp(&(b.end - b.start))
                    .unwrap_or(Ordering::Equal)
            })
            .map(|turn| turn.speaker.clone());
        if let Some(speaker) = speaker {
            flattened.push(DiarizationTurn {
                start,
                end,
                speaker,
            });
        }
    }

    for index in 0..flattened.len() {
        if flattened[index].end - flattened[index].start >= SHORT_TURN_SECONDS {
            continue;
        }
        let previous = index.checked_sub(1).and_then(|value| flattened.get(value));
        let next = flattened.get(index + 1);
        let replacement = match (previous, next) {
            (Some(previous), Some(next)) => {
                if previous.end - previous.start >= next.end - next.start {
                    Some(previous.speaker.clone())
                } else {
                    Some(next.speaker.clone())
                }
            }
            (Some(previous), None) => Some(previous.speaker.clone()),
            (None, Some(next)) => Some(next.speaker.clone()),
            _ => None,
        };
        if let Some(replacement) = replacement {
            flattened[index].speaker = replacement;
        }
    }
    merge_turns(flattened)
}

fn merge_turns(turns: Vec<DiarizationTurn>) -> Vec<DiarizationTurn> {
    let mut merged: Vec<DiarizationTurn> = Vec::new();
    for turn in turns {
        if let Some(previous) = merged.last_mut() {
            if previous.speaker == turn.speaker && turn.start - previous.end <= 0.05 {
                previous.end = previous.end.max(turn.end);
                continue;
            }
        }
        merged.push(turn);
    }
    merged
}

fn merge_audio_segments(segments: Vec<SpeakerAudioSegment>) -> Vec<SpeakerAudioSegment> {
    let mut merged: Vec<SpeakerAudioSegment> = Vec::new();
    for mut segment in segments {
        if let Some(previous) = merged.last_mut() {
            if previous.speaker == segment.speaker
                && (segment.start_seconds - previous.end_seconds).abs() <= 0.05
            {
                previous.samples.append(&mut segment.samples);
                previous.end_seconds = segment.end_seconds;
                continue;
            }
        }
        merged.push(segment);
    }
    merged
}

fn seconds_to_index(seconds: f64, max: usize) -> usize {
    ((seconds * SAMPLE_RATE as f64).round() as usize).min(max)
}

fn normalize_embedding(mut embedding: Vec<f32>) -> Option<Vec<f32>> {
    let norm = embedding
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if !norm.is_finite() || norm <= f32::EPSILON {
        return None;
    }
    for value in &mut embedding {
        *value /= norm;
    }
    Some(embedding)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    Some(
        a.iter()
            .zip(b.iter())
            .map(|(left, right)| left * right)
            .sum(),
    )
}

pub fn refine_speaker_labels(
    transcript_segments: &[(u64, f64, f64, Option<String>)],
    offline_turns: &[DiarizationTurn],
) -> Vec<SpeakerLabelUpdate> {
    let mut overlap_by_pair = HashMap::<(String, String), f64>::new();
    for (_, start, end, provisional) in transcript_segments {
        let Some(provisional) = provisional else {
            continue;
        };
        for turn in offline_turns {
            let overlap = (end.min(turn.end) - start.max(turn.start)).max(0.0);
            if overlap > 0.0 {
                *overlap_by_pair
                    .entry((provisional.clone(), turn.speaker.clone()))
                    .or_default() += overlap;
            }
        }
    }

    let mut pairs = overlap_by_pair.into_iter().collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    let mut used_provisional = HashSet::new();
    let mut used_offline = HashSet::new();
    let mut offline_to_provisional = HashMap::<String, String>::new();
    for ((provisional, offline), _) in pairs {
        if used_provisional.insert(provisional.clone()) && used_offline.insert(offline.clone()) {
            offline_to_provisional.insert(offline, provisional);
        }
    }

    let mut used_names = transcript_segments
        .iter()
        .filter_map(|segment| segment.3.clone())
        .collect::<HashSet<_>>();
    let mut offline_to_generated = HashMap::<String, String>::new();
    for turn in offline_turns {
        if !offline_to_provisional.contains_key(&turn.speaker)
            && !offline_to_generated.contains_key(&turn.speaker)
        {
            let mut index = 0usize;
            let name = loop {
                let candidate = format!("speaker_{index:02}");
                if used_names.insert(candidate.clone()) {
                    break candidate;
                }
                index += 1;
            };
            offline_to_generated.insert(turn.speaker.clone(), name);
        }
    }

    transcript_segments
        .iter()
        .map(|(sequence_id, start, end, provisional)| {
            let dominant_offline = offline_turns
                .iter()
                .filter_map(|turn| {
                    let overlap = (end.min(turn.end) - start.max(turn.start)).max(0.0);
                    (overlap > 0.0).then_some((turn, overlap))
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
                .map(|(turn, _)| &turn.speaker);
            let speaker = match (dominant_offline, provisional) {
                (Some(offline), Some(provisional)) => offline_to_provisional
                    .get(offline)
                    .cloned()
                    .or_else(|| Some(provisional.clone())),
                (Some(offline), None) => offline_to_provisional
                    .get(offline)
                    .or_else(|| offline_to_generated.get(offline))
                    .cloned(),
                (None, provisional) => provisional.clone(),
            };
            SpeakerLabelUpdate {
                sequence_id: *sequence_id,
                speaker,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stabilizes_overlap_and_short_turns() {
        let turns = vec![
            DiarizationTurn {
                start: 0.0,
                end: 2.0,
                speaker: "speaker_00".into(),
            },
            DiarizationTurn {
                start: 1.8,
                end: 2.1,
                speaker: "speaker_01".into(),
            },
            DiarizationTurn {
                start: 2.1,
                end: 4.0,
                speaker: "speaker_00".into(),
            },
        ];
        let stabilized = stabilize_turns(&turns);
        assert_eq!(stabilized.len(), 1);
        assert_eq!(stabilized[0].speaker, "speaker_00");
    }

    #[test]
    fn tracker_reuses_close_embedding() {
        let mut tracker = SpeakerTracker::default();
        let first = tracker.assign(&[1.0, 0.0], 1.0);
        let second = tracker.assign(&[0.99, 0.01], 1.0);
        assert_eq!(first, second);
    }

    #[test]
    fn tracker_caps_session_at_ten_speakers() {
        let mut tracker = SpeakerTracker::default();
        for index in 0..11 {
            let mut embedding = vec![0.0; 11];
            embedding[index] = 1.0;
            tracker.assign(&embedding, 1.0);
        }
        assert_eq!(tracker.centroids.len(), MAX_REALTIME_SPEAKERS);
    }

    #[test]
    fn tracker_updates_centroid_by_duration() {
        let mut tracker = SpeakerTracker::default();
        tracker.assign(&[1.0, 0.0], 3.0);
        tracker.assign(&[0.8, 0.6], 1.0);
        assert!(tracker.centroids[0].embedding[0] > 0.95);
        assert!(tracker.centroids[0].embedding[1] > 0.0);
        assert_eq!(tracker.centroids[0].weight, 4.0);
    }

    #[test]
    fn aligns_vad_to_speaker_turns_in_time_order() {
        let audio = vec![0.25; 4 * SAMPLE_RATE as usize];
        let vad = vec![SpeechSegment {
            samples: audio.clone(),
            start_timestamp_ms: 0.0,
            end_timestamp_ms: 4_000.0,
            confidence: 0.9,
        }];
        let turns = vec![
            DiarizationTurn {
                start: 0.0,
                end: 2.0,
                speaker: "speaker_00".into(),
            },
            DiarizationTurn {
                start: 2.0,
                end: 4.0,
                speaker: "speaker_01".into(),
            },
        ];
        let aligned = align_vad_with_turns(&audio, &vad, &turns);
        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0].speaker.as_deref(), Some("speaker_00"));
        assert_eq!(aligned[1].speaker.as_deref(), Some("speaker_01"));
        assert_eq!(
            aligned
                .iter()
                .map(|segment| segment.samples.len())
                .sum::<usize>(),
            audio.len()
        );
    }

    #[test]
    fn leaves_uncovered_audio_without_a_speaker() {
        let audio = vec![0.25; 3 * SAMPLE_RATE as usize];
        let turns = vec![DiarizationTurn {
            start: 1.0,
            end: 2.0,
            speaker: "speaker_00".into(),
        }];

        let aligned = audio_segments_for_range(&audio, 0.0, 3.0, &turns);

        assert_eq!(aligned.len(), 3);
        assert_eq!(aligned[0].speaker, None);
        assert_eq!(aligned[1].speaker.as_deref(), Some("speaker_00"));
        assert_eq!(aligned[2].speaker, None);
    }

    #[test]
    fn refinement_preserves_provisional_names() {
        let transcripts = vec![
            (1, 0.0, 2.0, Some("speaker_01".to_string())),
            (2, 2.0, 4.0, Some("speaker_00".to_string())),
        ];
        let turns = vec![
            DiarizationTurn {
                start: 0.0,
                end: 2.0,
                speaker: "speaker_00".into(),
            },
            DiarizationTurn {
                start: 2.0,
                end: 4.0,
                speaker: "speaker_01".into(),
            },
        ];
        let refined = refine_speaker_labels(&transcripts, &turns);
        assert_eq!(refined[0].speaker.as_deref(), Some("speaker_01"));
        assert_eq!(refined[1].speaker.as_deref(), Some("speaker_00"));
    }

    #[test]
    fn refinement_keeps_provisional_when_offline_cluster_has_no_one_to_one_match() {
        let transcripts = vec![
            (1, 0.0, 3.0, Some("speaker_00".to_string())),
            (2, 3.0, 4.0, Some("speaker_00".to_string())),
        ];
        let turns = vec![
            DiarizationTurn {
                start: 0.0,
                end: 3.0,
                speaker: "speaker_00".into(),
            },
            DiarizationTurn {
                start: 3.0,
                end: 4.0,
                speaker: "speaker_01".into(),
            },
        ];

        let refined = refine_speaker_labels(&transcripts, &turns);

        assert_eq!(refined[0].speaker.as_deref(), Some("speaker_00"));
        assert_eq!(refined[1].speaker.as_deref(), Some("speaker_00"));
    }
}
