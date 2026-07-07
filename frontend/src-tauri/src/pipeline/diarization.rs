//! Diarization + speaker identity resolution (PLAN.md Phase 2).
//!
//! HIGHEST-RISK phase per the plan. Everything here degrades safely: if the diarization
//! model is missing or fails, segments simply keep `speaker_id = NULL` and all search /
//! RAG / UI paths continue to work (PLAN.md Phase 2 degradation rules).
//!
//! The algorithmic core — overlap-based segment→cluster assignment, cosine speaker
//! matching against known profiles, and running-average embedding folding — is pure and
//! unit-tested. The ONNX model wrapper ([`Diarizer`]) is a scaffold: it reports
//! unavailable until the pyannote/Sortformer export ships (see docs/diarization-eval.md,
//! to be produced during the 3-day eval timebox).

use std::path::PathBuf;

use anyhow::{anyhow, Result};

/// Default cosine threshold for auto-assigning a cluster to an existing speaker
/// (PLAN.md §11 #4, configurable, tuned on real data).
pub const DEFAULT_SPEAKER_TAU: f32 = 0.75;

/// Minimum share of a segment that must be covered by a cluster's turns to attribute it;
/// below this the segment is left unattributed (PLAN.md: ambiguous <60% → NULL speaker).
pub const MIN_OVERLAP_RATIO: f32 = 0.60;

/// A diarized turn: some cluster spoke during [start_ms, end_ms].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeakerTurn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub cluster_id: i64,
}

/// Cosine similarity of two equal-length vectors. Returns 0 for degenerate inputs.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Assign a segment to the cluster with maximum time-overlap. Returns `None` when the
/// best cluster covers less than [`MIN_OVERLAP_RATIO`] of the segment (ambiguous).
pub fn assign_segment(
    seg_start_ms: i64,
    seg_end_ms: i64,
    turns: &[SpeakerTurn],
    min_overlap_ratio: f32,
) -> Option<i64> {
    let seg_len = (seg_end_ms - seg_start_ms).max(0);
    if seg_len == 0 {
        return None;
    }
    // Sum overlap per cluster.
    let mut overlap_by_cluster: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for t in turns {
        let start = seg_start_ms.max(t.start_ms);
        let end = seg_end_ms.min(t.end_ms);
        let ov = (end - start).max(0);
        if ov > 0 {
            *overlap_by_cluster.entry(t.cluster_id).or_insert(0) += ov;
        }
    }
    let (best_cluster, best_overlap) = overlap_by_cluster
        .into_iter()
        // Deterministic tie-break: larger overlap, then smaller cluster id.
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?;

    if (best_overlap as f32) / (seg_len as f32) >= min_overlap_ratio {
        Some(best_cluster)
    } else {
        None
    }
}

/// Match a cluster's mean embedding against known speaker profiles. Returns the id of the
/// best speaker above `tau`, else `None` (caller creates a new unconfirmed speaker).
pub fn match_speaker(
    cluster_embedding: &[f32],
    known: &[(i64, Vec<f32>)],
    tau: f32,
) -> Option<i64> {
    let mut best: Option<(i64, f32)> = None;
    for (id, emb) in known {
        let sim = cosine_similarity(cluster_embedding, emb);
        if best.map_or(true, |(_, s)| sim > s) {
            best = Some((*id, sim));
        }
    }
    best.filter(|(_, sim)| *sim >= tau).map(|(id, _)| id)
}

/// Fold a new cluster embedding into an existing speaker profile via running average.
/// `count` is how many embeddings the profile already averages.
pub fn fold_embedding(existing: &[f32], count: u32, new: &[f32]) -> Vec<f32> {
    if existing.len() != new.len() || existing.is_empty() {
        return new.to_vec();
    }
    let n = count as f32;
    existing
        .iter()
        .zip(new.iter())
        .map(|(e, x)| (e * n + x) / (n + 1.0))
        .collect()
}

/// Where to find the diarization model (directory with the ONNX export).
#[derive(Debug, Clone)]
pub struct DiarizerConfig {
    pub model_dir: PathBuf,
}

impl DiarizerConfig {
    pub fn is_available(&self) -> bool {
        self.model_dir.join("model.onnx").exists()
    }
}

/// ONNX diarization model wrapper. SCAFFOLD: returns unavailable until the model ships.
pub struct Diarizer {
    #[allow(dead_code)]
    config: DiarizerConfig,
}

impl Diarizer {
    pub fn load(config: DiarizerConfig) -> Result<Self> {
        if !config.is_available() {
            return Err(anyhow!(
                "diarization model not found at {} — segments stay unattributed",
                config.model_dir.display()
            ));
        }
        Ok(Self { config })
    }

    /// Run diarization on an audio file → speaker turns + per-cluster mean embeddings.
    /// SCAFFOLD: real ONNX inference lands after the Phase 2 model eval.
    pub fn diarize(&self, _audio_path: &std::path::Path) -> Result<DiarizationResult> {
        Err(anyhow!("diarization ONNX inference not yet wired"))
    }
}

/// Output of a diarization run.
pub struct DiarizationResult {
    pub turns: Vec<SpeakerTurn>,
    /// (cluster_id, mean voice embedding) for identity resolution.
    pub cluster_embeddings: Vec<(i64, Vec<f32>)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_basic() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0); // mismatched len
    }

    #[test]
    fn segment_assigned_to_dominant_cluster() {
        // Segment [0,1000]; cluster 1 covers [0,800], cluster 2 covers [800,1000].
        let turns = vec![
            SpeakerTurn { start_ms: 0, end_ms: 800, cluster_id: 1 },
            SpeakerTurn { start_ms: 800, end_ms: 1000, cluster_id: 2 },
        ];
        assert_eq!(assign_segment(0, 1000, &turns, MIN_OVERLAP_RATIO), Some(1));
    }

    #[test]
    fn ambiguous_segment_is_unattributed() {
        // Cluster 1 covers only 40% of the segment -> below 60% -> None.
        let turns = vec![SpeakerTurn { start_ms: 0, end_ms: 400, cluster_id: 1 }];
        assert_eq!(assign_segment(0, 1000, &turns, MIN_OVERLAP_RATIO), None);
    }

    #[test]
    fn speaker_matching_respects_threshold() {
        let known = vec![(7, vec![1.0, 0.0, 0.0]), (9, vec![0.0, 1.0, 0.0])];
        // Near speaker 7.
        assert_eq!(match_speaker(&[0.99, 0.14, 0.0], &known, 0.75), Some(7));
        // Orthogonal to all -> below threshold -> None (new speaker).
        assert_eq!(match_speaker(&[0.0, 0.0, 1.0], &known, 0.75), None);
    }

    #[test]
    fn running_average_folds_toward_new() {
        // Profile averaging 1 sample [0,0]; fold in [2,2] -> mean [1,1].
        assert_eq!(fold_embedding(&[0.0, 0.0], 1, &[2.0, 2.0]), vec![1.0, 1.0]);
        // Empty existing -> take new.
        assert_eq!(fold_embedding(&[], 0, &[3.0]), vec![3.0]);
    }
}
