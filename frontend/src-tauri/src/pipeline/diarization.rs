//! Diarization + speaker identity resolution (PLAN.md Phase 2).
//!
//! HIGHEST-RISK phase per the plan. Everything here degrades safely: if the diarization
//! model is missing or fails, segments simply keep `speaker_id = NULL` and all search /
//! RAG / UI paths continue to work (PLAN.md Phase 2 degradation rules).
//!
//! The algorithmic core — overlap-based segment→cluster assignment, cosine speaker
//! matching against known profiles, embedding aggregation, and the two-stage clustering
//! (tight complete-linkage fragments → duration-weighted centroid consolidation) — is pure
//! and unit-tested. The ONNX wrapper ([`Diarizer`]) runs pyannote segmentation-3.0 plus a
//! WeSpeaker CAM++ speaker-embedding model, with the embedding frontend driven by the
//! model's own ONNX metadata (see [`crate::pipeline::kaldi_fbank`] for why that matters).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Result};

use super::kaldi_fbank::KaldiFbank;

/// Default cosine threshold for auto-assigning a cluster to an existing speaker profile
/// (cross-meeting identity). Validated on real-speech 3-voice meetings (CMU Arctic,
/// `measure_calibration` harness) with the corrected embedding frontend: duration-weighted
/// same-speaker cross-half centroid cosine measured 0.82–0.93, while cross-speaker
/// centroids measured ≤ 0.71 for distinct voices (a deliberately pathological pair of
/// near-identical US males reached 0.85). 0.75 sits in the measured gap with ≥ 0.05 margin
/// on both sides. Kept on the strict side because a false merge (two people folded into one
/// named profile) is worse than a false split (an extra unconfirmed profile, which orphan
/// GC removes once it is unreferenced).
pub const DEFAULT_SPEAKER_TAU: f32 = 0.75;

/// Minimum share of a segment that must be covered by a cluster's turns to attribute it;
/// below this the segment is left unattributed (PLAN.md: ambiguous <60% → NULL speaker).
pub const MIN_OVERLAP_RATIO: f32 = 0.60;

// ---- Diarization ONNX pipeline constants ----

/// Model file names in the diarizer model dir (see [`crate::pipeline::diarization_commands`]).
pub const SEGMENTATION_FILE: &str = "segmentation.onnx";
/// v2: 3D-Speaker CAM++ zh-en "advanced" (sherpa-onnx export). The v1 WeSpeaker
/// VoxCeleb CAM++ ("embedding.onnx") measured non-separable on real VoIP meeting audio
/// (same-speaker centroid cos 0.91–0.99 vs cross-speaker 0.62–0.92, overlapping ranges;
/// 44.8% speaker agreement on a 7-voice reference meeting). Same architecture and
/// runtime cost; the zh-en advanced training set lifts the same meeting to 85.8–93%
/// agreement with all 7 speakers found. The new filename retires stale v1 downloads.
pub const EMBEDDING_FILE: &str = "embedding.v2.onnx";

/// Segmentation runs on 16 kHz mono; pyannote segmentation-3.0 uses 10 s chunks.
const SEG_SAMPLE_RATE: usize = 16_000;
const SEG_WINDOW_SAMPLES: usize = SEG_SAMPLE_RATE * 10; // 160_000
const SEG_WINDOW_MS: f64 = 10_000.0;

/// pyannote-rs feeds the segmentation model raw i16 sample magnitudes cast to f32 (it reads
/// 16-bit PCM WAVs directly). We decode to f32 in [-1, 1], so we scale back up to match.
const SEG_WAVEFORM_SCALE: f32 = 32768.0;

/// pyannote segmentation-3.0 uses the powerset of {0,1,2} with at most 2 simultaneous
/// speakers: 7 classes. Argmax over these per frame gives the active local-speaker set.
const SEG_NUM_LOCAL_SPEAKERS: usize = 3;

/// Stage-1 agglomerative cut on **cosine distance** (`1 - cosine_similarity`), used with
/// **complete** linkage (see [`Linkage`]). Deliberately TIGHT: stage 1 only forms
/// high-purity fragments (turns that are near-certainly the same voice); consolidating
/// fragments of one speaker is stage 2's job ([`merge_clusters_by_centroid`]), which
/// compares duration-weighted fragment centroids instead of single-turn embeddings.
/// Calibrated on real-speech 3-voice meetings (CMU Arctic, `measure_calibration` harness):
/// per-turn same-speaker cosine spans 0.22–0.98 while cross-speaker reaches ~0.9 (the
/// sherpa-onnx reference implementation measures the same overlap on this data, so this is
/// the model's genuine noise level, not a frontend artifact) — no single-stage cut can both
/// consolidate a speaker and keep speakers apart. At 0.30, fragments measured 0.82–0.94
/// purity across regenerated meetings, the best observed operating point.
pub const DEFAULT_CLUSTER_DISTANCE_THRESHOLD: f32 = 0.30;

/// Stage-2 cut: fragments whose duration-weighted centroids have cosine similarity at or
/// above this merge into one speaker. Recalibrated 2026-07-20 on a real 31-min 7-voice
/// VoIP meeting with the v2 (CAM++ zh-en advanced) embedding, scored against a reference
/// diarization: speaker agreement by stage-2 value — 0.65 → 85.8% (28 clusters),
/// 0.75 → 93.1% (57 clusters), 0.80 → 93.3% (76 clusters), 0.85 → 93.2% (90 clusters).
/// 0.75 is the knee. Stricter cuts strand small fragments as phantom speakers, but that
/// is now handled by [`consolidate_minor_clusters`] (attaches minor clusters to major
/// ones) instead of loosening this cut — the old 0.65 value collapsed distinct speakers
/// into one giant cluster on real meeting audio.
pub const DEFAULT_CENTROID_MERGE_MIN_SIM: f32 = 0.75;

/// Turns shorter than this (after gap-merging) are dropped entirely — too little audio for
/// even a post-hoc embedding match.
pub const DEFAULT_MIN_TURN_MS: i64 = 250;

/// Adjacent same-speaker activity separated by less than this is merged into one turn.
pub const DEFAULT_MERGE_GAP_MS: i64 = 250;

/// Only turns at least this long participate in cluster *formation*. Shorter turns are
/// attached afterward to the most similar formed cluster (never spawn a new one) — a short
/// interjection ("yes", "okay") carries too little voice for a reliable embedding: on the
/// real-speech calibration meetings, pairs involving a sub-1.2 s turn averaged 0.38–0.45
/// same-speaker cosine vs 0.64–0.71 for long-turn pairs, indistinguishable from their
/// cross-speaker range. Letting them seed clusters is a direct over-clustering cause.
/// 1.5 s is a common floor for CAM++-class embeddings (pyannote/sherpa use 1–2 s).
pub const DEFAULT_MIN_CLUSTER_TURN_MS: i64 = 1500;

/// A short/overlap turn is attached to its best-matching cluster only when cosine similarity
/// reaches this floor; below it the turn is dropped from attribution (its transcript
/// segments stay NULL) rather than guessed. Deliberately low: attachment can only pick an
/// *existing* cluster, so it can never inflate the speaker count — this floor just suppresses
/// attaching pure noise.
pub const DEFAULT_SHORT_TURN_ASSIGN_MIN_SIM: f32 = 0.30;

/// Turns whose audio is more than this fraction overlapped speech (frames where the powerset
/// decode reported ≥2 simultaneous local speakers) are excluded from cluster *formation*:
/// their embedding is a blend of voices and would pollute a centroid. They are attached
/// afterward like short turns.
pub const DEFAULT_MAX_OVERLAP_FRAC: f32 = 0.50;

/// Clusters whose total speaking time reaches this floor are "major" — real meeting
/// participants. Smaller clusters are crumbs (a few stranded fragments each) and are
/// folded into their most-similar major cluster by [`consolidate_minor_clusters`].
/// Measured on the 7-voice reference meeting at stage-2 = 0.75: the 7 true speakers'
/// clusters held 24–616 s each while all 50 crumb clusters held ≤ 20 s.
pub const DEFAULT_MIN_MAJOR_CLUSTER_MS: i64 = 15_000;

/// Agglomerative linkage criterion for [`cluster_embeddings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    /// Merge cost = mean cross-pair distance. Sensitive to outlier-similar pairs.
    Average,
    /// Merge cost = max cross-pair distance. Robust to a single close cross pair; the v1
    /// default (matches sherpa-onnx's WeSpeaker clustering).
    Complete,
}

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
    let mut overlap_by_cluster: std::collections::HashMap<i64, i64> =
        std::collections::HashMap::new();
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

// ---- Pure pipeline logic (unit-tested; no ONNX) ----

/// The set of local speaker indices active for each pyannote powerset class (3 speakers,
/// max 2 simultaneous → 7 classes, generated in increasing-cardinality order):
/// 0:{} 1:{0} 2:{1} 3:{2} 4:{0,1} 5:{0,2} 6:{1,2}.
pub fn powerset_speakers(class: usize) -> &'static [usize] {
    const MAP: [&[usize]; 7] = [&[], &[0], &[1], &[2], &[0, 1], &[0, 2], &[1, 2]];
    MAP.get(class).copied().unwrap_or(&[])
}

/// Decode one segmentation window's logits `[num_frames, num_classes]` (row-major) into
/// per-frame per-local-speaker activity: `active[frame][local_speaker]`. Each frame takes
/// the argmax powerset class, then expands it to its member local speakers.
pub fn decode_powerset(
    logits: &[f32],
    num_frames: usize,
    num_classes: usize,
) -> Vec<[bool; SEG_NUM_LOCAL_SPEAKERS]> {
    let mut out = vec![[false; SEG_NUM_LOCAL_SPEAKERS]; num_frames];
    if num_classes == 0 {
        return out;
    }
    for (f, frame) in out.iter_mut().enumerate() {
        let row = &logits[f * num_classes..(f + 1) * num_classes];
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (c, &v) in row.iter().enumerate() {
            if v > best_v {
                best_v = v;
                best = c;
            }
        }
        for &spk in powerset_speakers(best) {
            if spk < SEG_NUM_LOCAL_SPEAKERS {
                frame[spk] = true;
            }
        }
    }
    out
}

/// Convert a single local speaker's per-frame boolean activity into `[start_ms, end_ms]`
/// turns. Contiguous active runs are formed, runs separated by `< merge_gap_ms` are merged,
/// then runs shorter than `min_turn_ms` are dropped. `frame_ms` is the per-frame duration
/// and `window_start_ms` the window's offset into the file.
pub fn runs_to_turns(
    active: &[bool],
    frame_ms: f64,
    window_start_ms: f64,
    min_turn_ms: i64,
    merge_gap_ms: i64,
) -> Vec<(i64, i64)> {
    // Collect raw contiguous runs as (start_frame, end_frame_exclusive).
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut cur: Option<usize> = None;
    for (f, &on) in active.iter().enumerate() {
        match (on, cur) {
            (true, None) => cur = Some(f),
            (false, Some(s)) => {
                runs.push((s, f));
                cur = None;
            }
            _ => {}
        }
    }
    if let Some(s) = cur {
        runs.push((s, active.len()));
    }

    // Frame run -> ms interval.
    let to_ms = |f: usize| window_start_ms + f as f64 * frame_ms;
    let mut turns: Vec<(i64, i64)> = runs
        .into_iter()
        .map(|(s, e)| (to_ms(s).round() as i64, to_ms(e).round() as i64))
        .collect();

    // Merge across small gaps.
    let mut merged: Vec<(i64, i64)> = Vec::with_capacity(turns.len());
    turns.sort_by_key(|t| t.0);
    for (s, e) in turns {
        if let Some(last) = merged.last_mut() {
            if s - last.1 < merge_gap_ms {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }

    // Drop turns that are too short to embed reliably.
    merged
        .into_iter()
        .filter(|(s, e)| e - s >= min_turn_ms)
        .collect()
}

/// Agglomerative clustering of L2-normalized embeddings, cut at `distance_threshold` on
/// cosine distance (`1 - cosine_similarity`) with the given [`Linkage`]. Returns a cluster
/// label in `0..k` for each input embedding. Uses `kodama` for the linkage, then a
/// union-find cut over the dendrogram steps below the threshold.
pub fn cluster_embeddings(
    embeddings: &[Vec<f32>],
    distance_threshold: f32,
    linkage: Linkage,
) -> Vec<usize> {
    let n = embeddings.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![0];
    }

    // Condensed upper-triangular cosine-distance matrix (row-major i<j).
    let mut condensed: Vec<f64> = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let d = 1.0 - cosine_similarity(&embeddings[i], &embeddings[j]);
            condensed.push(d as f64);
        }
    }

    let method = match linkage {
        Linkage::Average => kodama::Method::Average,
        Linkage::Complete => kodama::Method::Complete,
    };
    let dend = kodama::linkage(&mut condensed, n, method);

    // Union-find over the n leaves; process merges (ascending dissimilarity) below the cut.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    // Representative leaf for each cluster id (leaves are 0..n; merges create n, n+1, ...).
    let mut rep: Vec<usize> = (0..n).collect();
    let thr = distance_threshold as f64;
    for step in dend.steps() {
        let a = rep[step.cluster1];
        let b = rep[step.cluster2];
        rep.push(a);
        if step.dissimilarity < thr {
            let ra = find(&mut parent, a);
            let rb = find(&mut parent, b);
            if ra != rb {
                parent[ra] = rb;
            }
        }
    }

    // Relabel connected components to dense 0..k in first-seen order.
    let mut label_of: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut labels = vec![0usize; n];
    for i in 0..n {
        let root = find(&mut parent, i);
        let next = label_of.len();
        let label = *label_of.entry(root).or_insert(next);
        labels[i] = label;
    }
    labels
}

/// Stage-2 clustering: greedily merge clusters whose duration-weighted centroids are at
/// least `min_similarity` cosine-similar, recomputing centroids after each merge (closest
/// pair first). Returns dense relabeled cluster ids (first-seen order).
///
/// Rationale: single-turn embeddings are noisy (measured same-speaker pairs span 0.43–0.98
/// on real speech), so one-stage HAC has no workable cut. Duration-weighted centroids over
/// stage-1 fragments are far more stable (same-speaker 0.82–0.93 vs cross ≤ 0.71), giving
/// this stage a wide, calibrated margin.
pub fn merge_clusters_by_centroid(
    embeddings: &[Vec<f32>],
    durations_ms: &[i64],
    labels: &[usize],
    min_similarity: f32,
) -> Vec<usize> {
    merge_clusters_by_centroid_with_hint(embeddings, durations_ms, labels, min_similarity, None)
}

/// [`merge_clusters_by_centroid`] with an optional expected-speaker-count hint (the
/// in-meeting control pill's "expected speakers"). With `num_speakers = Some(k)`,
/// threshold merges stop once `k` clusters remain — the user asserted at least `k`
/// distinct voices, so stage 2 must not collapse below that. The hint does NOT force
/// merging down to `k`: closest-centroid forced merges measured harmful on real meeting
/// audio (85.8% → 58.2% speaker agreement on the 7-voice reference meeting — the
/// closest pair by centroid is often two distinct voices, not two fragments of one).
/// Reaching the hint count from above is [`consolidate_minor_clusters`]'s job, which
/// attaches crumbs to majors instead of merging major pairs. If stage 1 formed fewer
/// than `k` fragments nothing merges (a fragment cannot be split).
pub fn merge_clusters_by_centroid_with_hint(
    embeddings: &[Vec<f32>],
    durations_ms: &[i64],
    labels: &[usize],
    min_similarity: f32,
    num_speakers: Option<usize>,
) -> Vec<usize> {
    let n = labels.len();
    if n == 0 {
        return Vec::new();
    }
    let k = labels.iter().copied().max().unwrap_or(0) + 1;
    // members[c] = indices of turns currently in cluster c (clusters may become empty
    // after being merged away).
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); k];
    for (i, &l) in labels.iter().enumerate() {
        members[l].push(i);
    }

    let centroid_of = |idxs: &[usize]| -> Vec<f32> {
        let items: Vec<(&[f32], i64)> = idxs
            .iter()
            .map(|&i| (embeddings[i].as_slice(), durations_ms[i]))
            .collect();
        duration_weighted_centroid(&items)
    };
    let mut centroids: Vec<Vec<f32>> = members.iter().map(|m| centroid_of(m)).collect();

    loop {
        // Find the most similar live pair.
        let mut live = 0usize;
        let mut best: Option<(f32, usize, usize)> = None;
        for a in 0..members.len() {
            if members[a].is_empty() {
                continue;
            }
            live += 1;
            for b in (a + 1)..members.len() {
                if members[b].is_empty() {
                    continue;
                }
                let sim = cosine_similarity(&centroids[a], &centroids[b]);
                if best.map_or(true, |(s, _, _)| sim > s) {
                    best = Some((sim, a, b));
                }
            }
        }
        // Merge while the closest pair clears the floor; with a hint, additionally stop
        // once the asserted count is reached (never collapse below what the user said).
        let clears_floor = best.map_or(false, |(sim, _, _)| sim >= min_similarity);
        let should_merge = match num_speakers {
            Some(k) => clears_floor && live > k.max(1),
            None => clears_floor,
        };
        match best {
            Some((_, a, b)) if should_merge => {
                let moved = std::mem::take(&mut members[b]);
                members[a].extend(moved);
                centroids[a] = centroid_of(&members[a]);
                centroids[b] = Vec::new();
            }
            _ => break,
        }
    }

    // Dense relabel in first-seen order over the original turn order.
    let mut cluster_of_turn = vec![0usize; n];
    for (c, m) in members.iter().enumerate() {
        for &i in m {
            cluster_of_turn[i] = c;
        }
    }
    let mut relabel: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut out = vec![0usize; n];
    for (i, &c) in cluster_of_turn.iter().enumerate() {
        let next = relabel.len();
        out[i] = *relabel.entry(c).or_insert(next);
    }
    out
}

/// Stage 3: fold crumb clusters into meeting participants. Stage 2's tight cut (see
/// [`DEFAULT_CENTROID_MERGE_MIN_SIM`]) deliberately leaves small stranded fragments as
/// their own clusters rather than risk collapsing two voices; this pass reassigns each
/// such minor cluster's turns to the most-similar *major* cluster.
///
/// Majors are clusters whose total speaking time reaches `min_major_ms` — or, when the
/// user asserted a speaker count, exactly the `num_speakers` longest-speaking clusters.
/// A minor cluster attaches when its centroid's cosine similarity to the best major
/// reaches `attach_floor`; below the floor it stays standalone (a genuinely distinct
/// rare voice should not be forced onto someone else) — except under a hint, where the
/// user's count is trusted and attachment is unconditional.
///
/// Measured on the 7-voice reference meeting (stage-2 = 0.75): the 7 majors held 87% of
/// attributed speech at 82–98% purity; the 50 minors were ≤ 20 s crumbs.
pub fn consolidate_minor_clusters(
    embeddings: &[Vec<f32>],
    durations_ms: &[i64],
    labels: &[usize],
    num_speakers: Option<usize>,
    min_major_ms: i64,
    attach_floor: f32,
) -> Vec<usize> {
    let n = labels.len();
    if n == 0 {
        return Vec::new();
    }
    let k = labels.iter().copied().max().unwrap_or(0) + 1;
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); k];
    for (i, &l) in labels.iter().enumerate() {
        members[l].push(i);
    }
    let total_ms: Vec<i64> = members
        .iter()
        .map(|m| m.iter().map(|&i| durations_ms[i].max(0)).sum())
        .collect();
    let centroids: Vec<Vec<f32>> = members
        .iter()
        .map(|m| {
            let items: Vec<(&[f32], i64)> = m
                .iter()
                .map(|&i| (embeddings[i].as_slice(), durations_ms[i]))
                .collect();
            duration_weighted_centroid(&items)
        })
        .collect();

    let mut majors: Vec<usize> = match num_speakers {
        Some(hint) => {
            let mut by_dur: Vec<usize> = (0..k).collect();
            by_dur.sort_by_key(|&c| std::cmp::Reverse(total_ms[c]));
            by_dur.truncate(hint.max(1));
            by_dur
        }
        None => (0..k).filter(|&c| total_ms[c] >= min_major_ms).collect(),
    };
    if majors.is_empty() {
        // Short recording where nobody reaches the floor: the longest cluster is the
        // meeting's dominant voice; everything else attaches to it or stays put.
        if let Some(largest) = (0..k).max_by_key(|&c| total_ms[c]) {
            majors.push(largest);
        }
    }

    let mut target: Vec<usize> = (0..k).collect();
    for c in 0..k {
        if majors.contains(&c) || members[c].is_empty() {
            continue;
        }
        let best = majors
            .iter()
            .map(|&m| (m, cosine_similarity(&centroids[c], &centroids[m])))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((m, sim)) = best {
            if num_speakers.is_some() || sim >= attach_floor {
                target[c] = m;
            }
        }
    }

    // Dense relabel in first-seen order over the original turn order.
    let mut relabel: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut out = vec![0usize; n];
    for (i, &l) in labels.iter().enumerate() {
        let next = relabel.len();
        out[i] = *relabel.entry(target[l]).or_insert(next);
    }
    out
}

/// Duration-weighted, L2-normalized mean of embeddings. `items` = `(embedding, weight_ms)`;
/// a 30 s turn thus counts 60× a 0.5 s turn toward the cluster's identity centroid. Skips
/// dimension-mismatched embeddings. Empty/zero-weight input yields an empty vec.
pub fn duration_weighted_centroid(items: &[(&[f32], i64)]) -> Vec<f32> {
    let dim = items
        .iter()
        .map(|(e, _)| e.len())
        .find(|&d| d > 0)
        .unwrap_or(0);
    if dim == 0 {
        return Vec::new();
    }
    let mut acc = vec![0f64; dim];
    let mut wsum = 0f64;
    for (e, w) in items {
        if e.len() != dim {
            continue;
        }
        let w = (*w).max(1) as f64; // guard against zero/negative weights
        for (a, x) in acc.iter_mut().zip(e.iter()) {
            *a += *x as f64 * w;
        }
        wsum += w;
    }
    if wsum <= 0.0 {
        return Vec::new();
    }
    let mut out: Vec<f32> = acc.iter().map(|a| (a / wsum) as f32).collect();
    l2_normalize(&mut out);
    out
}

/// Attach a turn to the most cosine-similar cluster centroid. Returns `Some(index)` when the
/// best similarity reaches `min_similarity`, else `None` (caller drops the turn from
/// attribution). Used for short / heavily-overlapped turns that must not form new clusters.
pub fn nearest_cluster(
    embedding: &[f32],
    centroids: &[Vec<f32>],
    min_similarity: f32,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, c) in centroids.iter().enumerate() {
        let sim = cosine_similarity(embedding, c);
        if best.map_or(true, |(_, s)| sim > s) {
            best = Some((i, sim));
        }
    }
    best.filter(|(_, s)| *s >= min_similarity).map(|(i, _)| i)
}

/// Group `(cluster_idx, embedding, weight_ms)` items into `num_clusters` duration-weighted,
/// L2-normalized centroids (return index = cluster). Empty clusters yield an empty vec.
fn duration_weighted_centroids<'a>(
    num_clusters: usize,
    items: impl Iterator<Item = (usize, &'a [f32], i64)>,
) -> Vec<Vec<f32>> {
    let mut buckets: Vec<Vec<(&'a [f32], i64)>> = vec![Vec::new(); num_clusters];
    for (c, e, w) in items {
        if c < num_clusters {
            buckets[c].push((e, w));
        }
    }
    buckets
        .into_iter()
        .map(|b| duration_weighted_centroid(&b))
        .collect()
}

/// Fraction of a turn `[s_ms, e_ms]` that falls on frames the powerset decode marked as
/// overlapped speech (≥2 simultaneous local speakers), computed against one window's frame
/// grid. A turn produced by [`runs_to_turns`] lies within its window, so a single window's
/// `active` frames cover it. Returns 0 for an empty range.
fn window_overlap_fraction(
    active: &[[bool; SEG_NUM_LOCAL_SPEAKERS]],
    frame_ms: f64,
    window_start_ms: f64,
    s_ms: i64,
    e_ms: i64,
) -> f32 {
    let (mut total, mut overlapped) = (0i64, 0i64);
    for (fi, fr) in active.iter().enumerate() {
        let fs = (window_start_ms + fi as f64 * frame_ms).round() as i64;
        let fe = (window_start_ms + (fi + 1) as f64 * frame_ms).round() as i64;
        let o = (e_ms.min(fe) - s_ms.max(fs)).max(0);
        if o > 0 {
            total += o;
            if fr.iter().filter(|b| **b).count() >= 2 {
                overlapped += o;
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        overlapped as f32 / total as f32
    }
}

/// Merge adjacent same-cluster turns (sorted by start) separated by `< merge_gap_ms`.
fn merge_same_cluster(mut turns: Vec<SpeakerTurn>, merge_gap_ms: i64) -> Vec<SpeakerTurn> {
    turns.sort_by(|a, b| {
        a.start_ms
            .cmp(&b.start_ms)
            .then(a.cluster_id.cmp(&b.cluster_id))
    });
    let mut out: Vec<SpeakerTurn> = Vec::with_capacity(turns.len());
    for t in turns {
        if let Some(last) = out.last_mut() {
            if last.cluster_id == t.cluster_id && t.start_ms - last.end_ms < merge_gap_ms {
                last.end_ms = last.end_ms.max(t.end_ms);
                continue;
            }
        }
        out.push(t);
    }
    out
}

/// Tunable diarization parameters. Kept separate from [`DiarizerConfig`] (which stays a
/// single `model_dir` field so existing call sites keep compiling); defaults come from the
/// `DEFAULT_*` constants above.
#[derive(Debug, Clone, Copy)]
pub struct DiarizationParams {
    /// Stage-1 cosine-distance cut for agglomerative fragment formation (tight).
    pub cluster_distance_threshold: f32,
    pub min_turn_ms: i64,
    pub merge_gap_ms: i64,
    /// Minimum turn length (ms) to participate in cluster formation; shorter turns are
    /// attached post-hoc. See [`DEFAULT_MIN_CLUSTER_TURN_MS`].
    pub min_cluster_turn_ms: i64,
    /// Cosine floor for attaching a short/overlap turn to a formed cluster.
    pub short_turn_assign_min_similarity: f32,
    /// Turns more overlapped than this are excluded from cluster formation.
    pub max_overlap_frac: f32,
    /// Agglomerative linkage criterion (stage 1).
    pub linkage: Linkage,
    /// Stage-2 floor: fragments merge while their duration-weighted centroids are at least
    /// this cosine-similar. See [`DEFAULT_CENTROID_MERGE_MIN_SIM`].
    pub centroid_merge_min_similarity: f32,
    /// Expected speaker count (user hint from the in-meeting control pill). When set,
    /// stage 2 never merges below this many clusters, and consolidation keeps exactly
    /// the `k` longest-speaking clusters as majors, attaching the rest to them. It does
    /// NOT force-merge major clusters together — measured on the 7-voice reference
    /// meeting, forcing closest-centroid merges down to the hint count dropped speaker
    /// agreement from 85.8% to 58.2% (the greedy pair choice merges distinct voices).
    /// `None` = automatic (threshold + duration-floor based) estimation.
    pub num_speakers: Option<usize>,
    /// Clusters below this total speaking time are folded into a major cluster during
    /// consolidation. See [`DEFAULT_MIN_MAJOR_CLUSTER_MS`].
    pub min_major_cluster_ms: i64,
}

impl Default for DiarizationParams {
    fn default() -> Self {
        Self {
            cluster_distance_threshold: DEFAULT_CLUSTER_DISTANCE_THRESHOLD,
            min_turn_ms: DEFAULT_MIN_TURN_MS,
            merge_gap_ms: DEFAULT_MERGE_GAP_MS,
            min_cluster_turn_ms: DEFAULT_MIN_CLUSTER_TURN_MS,
            short_turn_assign_min_similarity: DEFAULT_SHORT_TURN_ASSIGN_MIN_SIM,
            max_overlap_frac: DEFAULT_MAX_OVERLAP_FRAC,
            linkage: Linkage::Complete,
            centroid_merge_min_similarity: DEFAULT_CENTROID_MERGE_MIN_SIM,
            num_speakers: None,
            min_major_cluster_ms: DEFAULT_MIN_MAJOR_CLUSTER_MS,
        }
    }
}

/// Where to find the diarization model (directory with the two ONNX exports).
#[derive(Debug, Clone)]
pub struct DiarizerConfig {
    pub model_dir: PathBuf,
}

impl DiarizerConfig {
    pub fn segmentation_path(&self) -> PathBuf {
        self.model_dir.join(SEGMENTATION_FILE)
    }
    pub fn embedding_path(&self) -> PathBuf {
        self.model_dir.join(EMBEDDING_FILE)
    }
    /// Both the segmentation and speaker-embedding ONNX files must be present.
    pub fn is_available(&self) -> bool {
        self.segmentation_path().exists() && self.embedding_path().exists()
    }
}

/// ONNX diarization engine: a pyannote-style cascade (segmentation → per-turn speaker
/// embeddings → agglomerative clustering), all local CPU inference.
///
/// Graceful degradation is preserved: [`Diarizer::load`] returns `Err` when either model
/// file is missing (callers leave segments unattributed), and [`Diarizer::diarize`] never
/// panics — inference errors surface as `Err`.
pub struct Diarizer {
    #[allow(dead_code)]
    config: DiarizerConfig,
    params: DiarizationParams,
    seg_session: Mutex<ort::session::Session>,
    emb_session: Mutex<ort::session::Session>,
    seg_output_name: String,
    emb_input_name: String,
    emb_output_name: String,
    fbank: KaldiFbank,
    /// Waveform scale for the embedding fbank, from the model's `normalize_samples`
    /// metadata (sherpa-onnx convention): `0` → ×32768 (i16 range), else 1.0.
    emb_waveform_scale: f32,
    /// Whether to apply per-utterance CMN, from `feature_normalize_type=global-mean`.
    emb_apply_cmn: bool,
}

impl Diarizer {
    /// Load both ONNX models with default [`DiarizationParams`].
    pub fn load(config: DiarizerConfig) -> Result<Self> {
        Self::load_with_params(config, DiarizationParams::default())
    }

    /// Load with explicit tuning parameters (used by tests / future tuning UI).
    pub fn load_with_params(config: DiarizerConfig, params: DiarizationParams) -> Result<Self> {
        if !config.is_available() {
            return Err(anyhow!(
                "diarization models not found at {} ({} + {} required) — segments stay unattributed",
                config.model_dir.display(),
                SEGMENTATION_FILE,
                EMBEDDING_FILE
            ));
        }
        let seg_session = build_session(&config.segmentation_path())?;
        let emb_session = build_session(&config.embedding_path())?;

        let seg_output_name = seg_session
            .outputs
            .first()
            .map(|o| o.name.clone())
            .ok_or_else(|| anyhow!("segmentation model has no outputs"))?;
        let emb_input_name = emb_session
            .inputs
            .first()
            .map(|i| i.name.clone())
            .ok_or_else(|| anyhow!("embedding model has no inputs"))?;
        let emb_output_name = emb_session
            .outputs
            .first()
            .map(|o| o.name.clone())
            .ok_or_else(|| anyhow!("embedding model has no outputs"))?;

        // Frontend convention from the model's own ONNX metadata (sherpa-onnx semantics).
        // `normalize_samples=0` → the model wants i16-scale waveforms (×32768); CMN only
        // when `feature_normalize_type=global-mean`. WeSpeaker VoxCeleb exports declare
        // normalize_samples=0 and no feature_normalize_type; 3D-Speaker exports declare
        // normalize_samples=1 + global-mean. Defaults (no metadata): [-1,1], no CMN —
        // matching sherpa's `FeatureExtractorConfig::normalize_samples = true` default.
        let (emb_waveform_scale, emb_apply_cmn) = match emb_session.metadata() {
            Ok(meta) => {
                let normalize_samples = meta
                    .custom("normalize_samples")
                    .ok()
                    .flatten()
                    .map(|v| v.trim() != "0")
                    .unwrap_or(true);
                let apply_cmn = meta
                    .custom("feature_normalize_type")
                    .ok()
                    .flatten()
                    .map(|v| v.trim() == "global-mean")
                    .unwrap_or(false);
                (
                    if normalize_samples {
                        1.0
                    } else {
                        SEG_WAVEFORM_SCALE
                    },
                    apply_cmn,
                )
            }
            Err(e) => {
                log::warn!("[diarize] embedding model metadata unavailable ({e}); assuming [-1,1] input, no CMN");
                (1.0, false)
            }
        };

        log::info!(
            "diarizer loaded: seg out='{seg_output_name}', emb in='{emb_input_name}' \
             out='{emb_output_name}', emb scale={emb_waveform_scale}, cmn={emb_apply_cmn}"
        );

        Ok(Self {
            config,
            params,
            seg_session: Mutex::new(seg_session),
            emb_session: Mutex::new(emb_session),
            seg_output_name,
            emb_input_name,
            emb_output_name,
            fbank: KaldiFbank::new(),
            emb_waveform_scale,
            emb_apply_cmn,
        })
    }

    /// Extract one duration-weighted voice embedding for each already-diarized speaker.
    ///
    /// Cloud diarization provides a good speaker timeline but its numeric speaker ids are
    /// local to one recording. Running only the local embedding model over that timeline
    /// gives the identity-learning pipeline the same privacy-preserving representation as
    /// full local diarization, without replacing the cloud speaker turns.
    pub fn embed_labeled_turns(
        &self,
        audio_path: &std::path::Path,
        turns: &[SpeakerTurn],
    ) -> Result<Vec<(i64, Vec<f32>)>> {
        const MAX_TURNS_PER_SPEAKER: usize = 8;
        const MAX_AUDIO_MS_PER_SPEAKER: i64 = 60_000;

        let decoded = crate::audio::decoder::decode_audio_file(audio_path)
            .map_err(|e| anyhow!("speaker embedding: decode {}: {e}", audio_path.display()))?;
        let waveform = decoded.to_whisper_format();
        if waveform.is_empty() {
            return Ok(Vec::new());
        }

        let mut by_speaker: BTreeMap<i64, Vec<SpeakerTurn>> = BTreeMap::new();
        for turn in turns
            .iter()
            .copied()
            .filter(|turn| turn.end_ms - turn.start_ms >= DEFAULT_MIN_CLUSTER_TURN_MS)
        {
            by_speaker.entry(turn.cluster_id).or_default().push(turn);
        }

        let mut result = Vec::with_capacity(by_speaker.len());
        for (speaker_id, mut speaker_turns) in by_speaker {
            speaker_turns.sort_by_key(|turn| std::cmp::Reverse(turn.end_ms - turn.start_ms));
            let mut embedded = Vec::new();
            let mut embedded_ms = 0_i64;
            for turn in speaker_turns.into_iter().take(MAX_TURNS_PER_SPEAKER) {
                if embedded_ms >= MAX_AUDIO_MS_PER_SPEAKER {
                    break;
                }
                let start =
                    ((turn.start_ms.max(0) as usize) * SEG_SAMPLE_RATE / 1000).min(waveform.len());
                let end =
                    ((turn.end_ms.max(0) as usize) * SEG_SAMPLE_RATE / 1000).min(waveform.len());
                if end <= start {
                    continue;
                }
                let duration_ms =
                    (turn.end_ms - turn.start_ms).min(MAX_AUDIO_MS_PER_SPEAKER - embedded_ms);
                let capped_end = (start + duration_ms as usize * SEG_SAMPLE_RATE / 1000).min(end);
                if let Some(embedding) = self.embed_turn(&waveform[start..capped_end])? {
                    embedded.push((embedding, duration_ms));
                    embedded_ms += duration_ms;
                }
            }
            let refs = embedded
                .iter()
                .map(|(embedding, duration_ms)| (embedding.as_slice(), *duration_ms))
                .collect::<Vec<_>>();
            let centroid = duration_weighted_centroid(&refs);
            if !centroid.is_empty() {
                result.push((speaker_id, centroid));
            }
        }
        Ok(result)
    }

    /// Run diarization on an audio file → speaker turns + per-cluster mean embeddings.
    ///
    /// Pipeline: decode to 16 kHz mono → slide 10 s segmentation windows → powerset-decode
    /// per-local-speaker turns (tracking overlap fraction) → embed each turn (kaldi fbank +
    /// WeSpeaker ONNX, L2-norm). Only *long, clean* turns form clusters (complete-linkage
    /// agglomerative); short and heavily-overlapped turns are attached afterward to the most
    /// similar formed cluster and can never spawn a new one — the fix for within-run
    /// over-clustering. Cluster identity embeddings are duration-weighted means.
    pub fn diarize(&self, audio_path: &std::path::Path) -> Result<DiarizationResult> {
        // 1) Decode to 16 kHz mono f32 in [-1, 1] (reuses the shared audio decoder).
        let decoded = crate::audio::decoder::decode_audio_file(audio_path)
            .map_err(|e| anyhow!("diarize: decode {}: {e}", audio_path.display()))?;
        let waveform = decoded.to_whisper_format(); // 16 kHz mono
        if waveform.is_empty() {
            return Ok(DiarizationResult {
                turns: Vec::new(),
                cluster_embeddings: Vec::new(),
            });
        }
        let total_ms = (waveform.len() as f64 / SEG_SAMPLE_RATE as f64) * 1000.0;

        // 2) Slide non-overlapping 10 s windows; the final window is zero-padded. Track each
        //    turn's overlap fraction (share of frames the powerset decode reported ≥2
        //    simultaneous speakers) so blended-voice turns can be kept out of formation.
        let mut raw_turns: Vec<(i64, i64, f32)> = Vec::new(); // (start_ms, end_ms, overlap_frac)
        let mut win_start = 0usize;
        let mut window_buf = vec![0f32; SEG_WINDOW_SAMPLES];
        while win_start < waveform.len() {
            let end = (win_start + SEG_WINDOW_SAMPLES).min(waveform.len());
            let n = end - win_start;
            for (dst, src) in window_buf.iter_mut().zip(&waveform[win_start..end]) {
                *dst = *src * SEG_WAVEFORM_SCALE;
            }
            for x in window_buf.iter_mut().skip(n) {
                *x = 0.0;
            }

            let active = self.run_segmentation(&window_buf)?; // Vec<[bool; 3]>
            let num_frames = active.len();
            if num_frames > 0 {
                let frame_ms = SEG_WINDOW_MS / num_frames as f64;
                let window_start_ms = (win_start as f64 / SEG_SAMPLE_RATE as f64) * 1000.0;
                for spk in 0..SEG_NUM_LOCAL_SPEAKERS {
                    let column: Vec<bool> = active.iter().map(|f| f[spk]).collect();
                    for (s, e) in runs_to_turns(
                        &column,
                        frame_ms,
                        window_start_ms,
                        self.params.min_turn_ms,
                        self.params.merge_gap_ms,
                    ) {
                        // Clamp to the real (unpadded) audio extent.
                        let e = e.min(total_ms.round() as i64);
                        if e - s >= self.params.min_turn_ms {
                            let ov =
                                window_overlap_fraction(&active, frame_ms, window_start_ms, s, e);
                            raw_turns.push((s, e, ov));
                        }
                    }
                }
            }
            win_start += SEG_WINDOW_SAMPLES;
        }

        if raw_turns.is_empty() {
            return Ok(DiarizationResult {
                turns: Vec::new(),
                cluster_embeddings: Vec::new(),
            });
        }

        // 3) One speaker embedding per raw turn (drop ones too short to featurize).
        struct Emb {
            start_ms: i64,
            end_ms: i64,
            dur_ms: i64,
            overlap_frac: f32,
            embedding: Vec<f32>,
        }
        let mut embs: Vec<Emb> = Vec::with_capacity(raw_turns.len());
        for (s, e, ov) in raw_turns {
            let start_sample = ((s as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize;
            let end_sample =
                (((e as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize).min(waveform.len());
            if end_sample <= start_sample {
                continue;
            }
            if let Some(embedding) = self.embed_turn(&waveform[start_sample..end_sample])? {
                embs.push(Emb {
                    start_ms: s,
                    end_ms: e,
                    dur_ms: e - s,
                    overlap_frac: ov,
                    embedding,
                });
            }
        }

        if embs.is_empty() {
            return Ok(DiarizationResult {
                turns: Vec::new(),
                cluster_embeddings: Vec::new(),
            });
        }

        // 4) Formation set: long, low-overlap turns. Short / heavily-overlapped turns are
        //    attached afterward (never form a cluster). If nothing qualifies (e.g. every turn
        //    is short), fall back to clustering all turns so we still attribute something.
        let mut formation: Vec<usize> = (0..embs.len())
            .filter(|&i| {
                embs[i].dur_ms >= self.params.min_cluster_turn_ms
                    && embs[i].overlap_frac <= self.params.max_overlap_frac
            })
            .collect();
        if formation.is_empty() {
            log::warn!("[diarize] no turns long/clean enough to form clusters; using all turns");
            formation = (0..embs.len()).collect();
        }

        let formation_embs: Vec<Vec<f32>> = formation
            .iter()
            .map(|&i| embs[i].embedding.clone())
            .collect();
        let formation_durs: Vec<i64> = formation.iter().map(|&i| embs[i].dur_ms).collect();
        // Stage 1: tight complete-linkage HAC -> high-purity fragments.
        let fragment_labels = cluster_embeddings(
            &formation_embs,
            self.params.cluster_distance_threshold,
            self.params.linkage,
        );
        // Stage 2: consolidate fragments of one speaker via duration-weighted centroids
        // (with a speaker-count hint acting as a merge stop, never a forced merge).
        let merged_labels = merge_clusters_by_centroid_with_hint(
            &formation_embs,
            &formation_durs,
            &fragment_labels,
            self.params.centroid_merge_min_similarity,
            self.params.num_speakers,
        );
        // Stage 3: fold crumb clusters into the meeting's major participants.
        let labels = consolidate_minor_clusters(
            &formation_embs,
            &formation_durs,
            &merged_labels,
            self.params.num_speakers,
            self.params.min_major_cluster_ms,
            self.params.short_turn_assign_min_similarity,
        );
        let num_clusters = labels.iter().copied().max().map(|m| m + 1).unwrap_or(0);

        // 5) Provisional duration-weighted centroids from formation turns, used to attach the
        //    remaining short/overlap turns to their nearest cluster.
        let provisional = duration_weighted_centroids(
            num_clusters,
            formation
                .iter()
                .enumerate()
                .map(|(fi, &i)| (labels[fi], embs[i].embedding.as_slice(), embs[i].dur_ms)),
        );

        // Assign a cluster to every turn: formation turns take their clustering label; the
        // rest attach to the nearest provisional centroid (or are dropped below the floor).
        let mut turn_cluster: Vec<Option<i64>> = vec![None; embs.len()];
        for (fi, &i) in formation.iter().enumerate() {
            turn_cluster[i] = Some(labels[fi] as i64);
        }
        for i in 0..embs.len() {
            if turn_cluster[i].is_some() {
                continue;
            }
            if let Some(c) = nearest_cluster(
                &embs[i].embedding,
                &provisional,
                self.params.short_turn_assign_min_similarity,
            ) {
                turn_cluster[i] = Some(c as i64);
            }
        }

        // 6) Merged turns for transcript attribution.
        let turns: Vec<SpeakerTurn> = embs
            .iter()
            .zip(&turn_cluster)
            .filter_map(|(e, c)| {
                c.map(|cluster_id| SpeakerTurn {
                    start_ms: e.start_ms,
                    end_ms: e.end_ms,
                    cluster_id,
                })
            })
            .collect();
        let turns = merge_same_cluster(turns, self.params.merge_gap_ms);

        // 7) Final duration-weighted identity embedding per cluster, over every turn assigned
        //    to it (formation + attached).
        let final_centroids = duration_weighted_centroids(
            num_clusters,
            (0..embs.len()).filter_map(|i| {
                turn_cluster[i].map(|c| (c as usize, embs[i].embedding.as_slice(), embs[i].dur_ms))
            }),
        );
        let cluster_embeddings: Vec<(i64, Vec<f32>)> = final_centroids
            .into_iter()
            .enumerate()
            .filter(|(_, emb)| !emb.is_empty())
            .map(|(label, emb)| (label as i64, emb))
            .collect();

        Ok(DiarizationResult {
            turns,
            cluster_embeddings,
        })
    }

    /// Run the segmentation model on one 16 kHz window (i16-scaled f32, length
    /// `SEG_WINDOW_SAMPLES`) → per-frame per-local-speaker activity.
    fn run_segmentation(&self, window: &[f32]) -> Result<Vec<[bool; SEG_NUM_LOCAL_SPEAKERS]>> {
        use ort::value::TensorRef;

        // pyannote segmentation input shape: [batch, channel, samples] = [1, 1, N].
        let array = ndarray::Array3::from_shape_vec((1, 1, window.len()), window.to_vec())
            .map_err(|e| anyhow!("segmentation input reshape: {e}"))?;

        let mut sess = self.seg_session.lock().unwrap();
        let input = TensorRef::from_array_view(array.view())
            .map_err(|e| anyhow!("ort segmentation input: {e}"))?;
        let outputs = sess
            .run(ort::inputs![input])
            .map_err(|e| anyhow!("ort segmentation run: {e}"))?;
        let value = outputs
            .get(&self.seg_output_name)
            .ok_or_else(|| anyhow!("segmentation output '{}' missing", self.seg_output_name))?;
        let arr = value
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("ort segmentation extract: {e}"))?;
        let shape = arr.shape().to_vec(); // [1, num_frames, num_classes]
        if shape.len() != 3 {
            return Err(anyhow!("segmentation output rank {} != 3", shape.len()));
        }
        let num_frames = shape[1];
        let num_classes = shape[2];
        let flat: Vec<f32> = arr.iter().copied().collect();
        Ok(decode_powerset(&flat, num_frames, num_classes))
    }

    /// Compute an L2-normalized speaker embedding for one turn's 16 kHz audio
    /// (f32 in [-1, 1]). Returns `None` when the slice is too short to yield a fbank frame.
    ///
    /// Frontend follows the model's own ONNX metadata (what sherpa-onnx does for the same
    /// file — see `load_with_params`): `wespeaker_en_voxceleb_CAM++.onnx` declares
    /// `normalize_samples=0` (multiply samples by 32768 — WeSpeaker trains on
    /// `torchaudio.load(normalize=False)` audio) and no `feature_normalize_type` (NO
    /// per-utterance CMN). The previous frontend (inherited from pyannote-rs/knf-rs)
    /// unconditionally fed [-1, 1] samples and applied CMN; measured on real speech that
    /// made embeddings nearly speaker-agnostic (same-speaker cosine ≈ cross ≈ 0.6) — the
    /// root cause of one meeting diarizing into 15 "speakers". See
    /// [`crate::pipeline::kaldi_fbank`] module docs for the full analysis.
    fn embed_turn(&self, samples: &[f32]) -> Result<Option<Vec<f32>>> {
        use ort::value::TensorRef;

        let scaled: Vec<f32> = samples
            .iter()
            .map(|s| s * self.emb_waveform_scale)
            .collect();
        let feats = if self.emb_apply_cmn {
            self.fbank.compute(&scaled)
        } else {
            self.fbank.compute_raw(&scaled)
        };
        if feats.shape()[0] == 0 {
            return Ok(None);
        }
        let feats = feats.insert_axis(ndarray::Axis(0)); // [1, num_frames, 80]

        let mut sess = self.emb_session.lock().unwrap();
        let input = TensorRef::from_array_view(feats.view())
            .map_err(|e| anyhow!("ort embedding input: {e}"))?;
        let outputs = sess
            .run(ort::inputs![self.emb_input_name.as_str() => input])
            .map_err(|e| anyhow!("ort embedding run: {e}"))?;
        let value = outputs
            .get(&self.emb_output_name)
            .ok_or_else(|| anyhow!("embedding output '{}' missing", self.emb_output_name))?;
        let arr = value
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("ort embedding extract: {e}"))?;
        let mut emb: Vec<f32> = arr.iter().copied().collect();
        l2_normalize(&mut emb);
        Ok(Some(emb))
    }
}

fn build_session(model_path: &std::path::Path) -> Result<ort::session::Session> {
    use ort::execution_providers::CPUExecutionProvider;
    use ort::session::{builder::GraphOptimizationLevel, Session};
    Session::builder()
        .map_err(|e| anyhow!("ort builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("ort opt level: {e}"))?
        .with_execution_providers([CPUExecutionProvider::default().build()])
        .map_err(|e| anyhow!("ort execution providers: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow!("ort load {}: {e}", model_path.display()))
}

/// L2-normalize in place (cosine similarity → dot product). Mirrors `embedder::l2_normalize`.
fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
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
            SpeakerTurn {
                start_ms: 0,
                end_ms: 800,
                cluster_id: 1,
            },
            SpeakerTurn {
                start_ms: 800,
                end_ms: 1000,
                cluster_id: 2,
            },
        ];
        assert_eq!(assign_segment(0, 1000, &turns, MIN_OVERLAP_RATIO), Some(1));
    }

    #[test]
    fn ambiguous_segment_is_unattributed() {
        // Cluster 1 covers only 40% of the segment -> below 60% -> None.
        let turns = vec![SpeakerTurn {
            start_ms: 0,
            end_ms: 400,
            cluster_id: 1,
        }];
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

    #[test]
    fn powerset_classes_expand_to_speaker_sets() {
        assert_eq!(powerset_speakers(0), &[] as &[usize]);
        assert_eq!(powerset_speakers(1), &[0]);
        assert_eq!(powerset_speakers(2), &[1]);
        assert_eq!(powerset_speakers(3), &[2]);
        assert_eq!(powerset_speakers(4), &[0, 1]);
        assert_eq!(powerset_speakers(5), &[0, 2]);
        assert_eq!(powerset_speakers(6), &[1, 2]);
        // Out-of-range -> empty.
        assert_eq!(powerset_speakers(99), &[] as &[usize]);
    }

    #[test]
    fn powerset_decode_argmaxes_and_expands() {
        // 3 frames, 7 classes. Frame 0 -> class 1 ({0}); frame 1 -> class 6 ({1,2});
        // frame 2 -> class 0 ({}).
        let mut logits = vec![0.0f32; 3 * 7];
        logits[0 * 7 + 1] = 5.0; // frame 0 argmax = class 1
        logits[1 * 7 + 6] = 5.0; // frame 1 argmax = class 6
        logits[2 * 7 + 0] = 5.0; // frame 2 argmax = class 0
        let active = decode_powerset(&logits, 3, 7);
        assert_eq!(active[0], [true, false, false]);
        assert_eq!(active[1], [false, true, true]);
        assert_eq!(active[2], [false, false, false]);
    }

    #[test]
    fn runs_to_turns_merges_gaps_and_drops_short() {
        // frame_ms = 100 ms, window offset 0. Active pattern (10 frames):
        // [T T T . T T . . . T]
        //  0-300ms run, gap 1 frame (100ms) < merge_gap(250) -> merges to 0-600ms,
        //  then a lone frame at 900-1000ms (100ms) dropped by min_turn(250).
        let active = [
            true, true, true, false, true, true, false, false, false, true,
        ];
        let turns = runs_to_turns(&active, 100.0, 0.0, 250, 250);
        assert_eq!(turns, vec![(0, 600)]);
    }

    #[test]
    fn runs_to_turns_respects_window_offset() {
        // Single 500 ms run starting at frame 1, window starts at 10_000 ms.
        let active = [false, true, true, true, true, true, false];
        let turns = runs_to_turns(&active, 100.0, 10_000.0, 250, 250);
        assert_eq!(turns, vec![(10_100, 10_600)]);
    }

    #[test]
    fn cluster_separates_two_groups() {
        // Two tight groups of near-identical unit vectors, orthogonal to each other.
        let a1 = vec![1.0, 0.0, 0.0];
        let a2 = vec![0.98, 0.19, 0.0]; // ~cos 0.98 with a1
        let b1 = vec![0.0, 0.0, 1.0];
        let b2 = vec![0.0, 0.19, 0.98];
        let labels = cluster_embeddings(&[a1, a2, b1, b2], 0.5, Linkage::Complete);
        assert_eq!(labels[0], labels[1], "a1,a2 should share a cluster");
        assert_eq!(labels[2], labels[3], "b1,b2 should share a cluster");
        assert_ne!(labels[0], labels[2], "the two groups should differ");
    }

    #[test]
    fn cluster_edge_cases() {
        assert_eq!(
            cluster_embeddings(&[], 0.5, Linkage::Complete),
            Vec::<usize>::new()
        );
        assert_eq!(
            cluster_embeddings(&[vec![1.0, 0.0]], 0.5, Linkage::Complete),
            vec![0]
        );
    }

    #[test]
    fn cluster_all_one_when_similar() {
        // Three near-identical vectors -> one cluster.
        let v = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.99, 0.14, 0.0],
            vec![0.98, 0.2, 0.0],
        ];
        let labels = cluster_embeddings(&v, 0.5, Linkage::Complete);
        assert!(labels.iter().all(|&l| l == labels[0]));
    }

    #[test]
    fn complete_linkage_is_stricter_than_average_at_stage1_cut() {
        // Stage-1 fragments must be high-purity: complete linkage admits a turn only when
        // EVERY member pair is within the cut, while average linkage lets one close pair
        // compensate for a far one. Geometry (unit vectors, angles): x1∠x2 = 18°,
        // y∠x1 = 35°, y∠x2 = 53°. At the stage-1 cut 0.30 (cosine distance):
        //   pairwise distances: x1x2 ≈ 0.049, x1y ≈ 0.181, x2y ≈ 0.399
        //   average({x1,x2}, y) = (0.181 + 0.399)/2 ≈ 0.290 < 0.30 → average merges y in;
        //   complete({x1,x2}, y) = 0.399 ≥ 0.30 → complete keeps y out.
        let x1 = vec![1.0f32, 0.0];
        let x2 = vec![18f32.to_radians().cos(), 18f32.to_radians().sin()];
        let y = vec![35f32.to_radians().cos(), -(35f32.to_radians().sin())];
        let embs = vec![x1, x2, y];
        let avg = cluster_embeddings(&embs, DEFAULT_CLUSTER_DISTANCE_THRESHOLD, Linkage::Average);
        let comp = cluster_embeddings(&embs, DEFAULT_CLUSTER_DISTANCE_THRESHOLD, Linkage::Complete);
        assert_eq!(
            avg[0], avg[2],
            "average linkage absorbs y via the close pair"
        );
        assert_eq!(
            comp[0], comp[1],
            "tight pair stays together under complete linkage"
        );
        assert_ne!(
            comp[0], comp[2],
            "complete linkage keeps the far turn out of the fragment"
        );
    }

    #[test]
    fn duration_weighted_centroid_favors_long_turns() {
        // One long turn near +x, many short turns near +y. Duration weighting must pull the
        // centroid toward +x (the long turn), not toward the short-turn majority.
        let long = vec![1.0, 0.0];
        let short = vec![0.0, 1.0];
        let items: Vec<(&[f32], i64)> = vec![
            (long.as_slice(), 30_000), // 30 s
            (short.as_slice(), 500),
            (short.as_slice(), 500),
            (short.as_slice(), 500),
        ];
        let c = duration_weighted_centroid(&items);
        assert!(c[0] > c[1], "long turn should dominate: {c:?}");
        // Unit-norm.
        let norm = (c[0] * c[0] + c[1] * c[1]).sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
        // Equal weighting (what we replaced) would have given c[1] > c[0].
        assert!(c[0] > 0.9, "30s vs 3×0.5s should be strongly +x: {c:?}");
    }

    #[test]
    fn duration_weighted_centroid_edge_cases() {
        assert!(duration_weighted_centroid(&[]).is_empty());
        // All-empty embeddings → empty.
        let empty: &[f32] = &[];
        assert!(duration_weighted_centroid(&[(empty, 1000)]).is_empty());
        // Dimension mismatch is skipped, not panicked.
        let a = vec![1.0, 0.0];
        let bad = vec![1.0];
        let items: Vec<(&[f32], i64)> = vec![(a.as_slice(), 1000), (bad.as_slice(), 1000)];
        let c = duration_weighted_centroid(&items);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn centroid_merge_consolidates_fragments_of_one_speaker() {
        // Speaker X fragments: {0,1} and {2}; speaker Y fragment: {3,4}. X's fragments are
        // centroid-similar (all near +x), Y is orthogonal. min_sim 0.7 must merge X's two
        // fragments and keep Y separate.
        let embs = vec![
            vec![1.0, 0.05, 0.0],
            vec![0.99, 0.1, 0.0],
            vec![0.97, 0.0, 0.2],
            vec![0.0, 1.0, 0.0],
            vec![0.05, 0.99, 0.0],
        ];
        let durs = vec![3000, 2000, 2500, 4000, 1500];
        let labels = vec![0, 0, 1, 2, 2];
        let merged = merge_clusters_by_centroid(&embs, &durs, &labels, 0.7);
        assert_eq!(merged[0], merged[1]);
        assert_eq!(merged[0], merged[2], "X's fragments should consolidate");
        assert_eq!(merged[3], merged[4]);
        assert_ne!(merged[0], merged[3], "Y stays a separate speaker");
        // Dense labels 0..k.
        assert_eq!(merged.iter().copied().max().unwrap(), 1);
    }

    #[test]
    fn centroid_merge_respects_floor_and_edge_cases() {
        // Two orthogonal singleton fragments below the floor stay apart.
        let embs = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let durs = vec![1000, 1000];
        let merged = merge_clusters_by_centroid(&embs, &durs, &[0, 1], 0.7);
        assert_ne!(merged[0], merged[1]);
        // Empty input.
        assert!(merge_clusters_by_centroid(&[], &[], &[], 0.7).is_empty());
        // Single cluster passes through.
        let one = merge_clusters_by_centroid(&[vec![1.0, 0.0]], &[500], &[0], 0.7);
        assert_eq!(one, vec![0]);
    }

    #[test]
    fn speaker_count_hint_never_forces_sub_floor_merges() {
        // Three fragments: two +x-ish voices below the floor (cos ≈ 0.71 < 0.85) plus one
        // orthogonal +y voice. The hint must NOT force-merge distinct voices in stage 2 —
        // reaching the hint count from above is consolidation's job.
        let embs = vec![
            vec![1.0, 0.0],
            vec![0.71, 0.71], // cos 0.71 with both neighbors
            vec![0.0, 1.0],
        ];
        let durs = vec![3000, 3000, 3000];
        let labels = vec![0, 1, 2];

        let auto = merge_clusters_by_centroid_with_hint(&embs, &durs, &labels, 0.85, None);
        assert_eq!(
            auto.iter().copied().max().unwrap() + 1,
            3,
            "floor keeps all apart"
        );

        let hinted = merge_clusters_by_centroid_with_hint(&embs, &durs, &labels, 0.85, Some(2));
        assert_eq!(
            hinted.iter().copied().max().unwrap() + 1,
            3,
            "hint is a merge stop, never a forced merge"
        );
    }

    #[test]
    fn consolidation_attaches_crumbs_to_majors() {
        // Two long-speaking majors on distinct axes plus one 2 s crumb near the +x major.
        // The crumb folds into +x; the majors stay apart.
        let embs = vec![
            vec![1.0, 0.0],   // major A, 60 s
            vec![0.0, 1.0],   // major B, 40 s
            vec![0.95, 0.31], // crumb, cos ≈ 0.95 with A
        ];
        let durs = vec![60_000, 40_000, 2_000];
        let labels = vec![0, 1, 2];

        let out = consolidate_minor_clusters(&embs, &durs, &labels, None, 15_000, 0.30);
        assert_eq!(out[0], out[2], "crumb attaches to its closest major");
        assert_ne!(out[0], out[1], "majors stay distinct");
    }

    #[test]
    fn consolidation_keeps_dissimilar_crumb_standalone_without_hint() {
        // A crumb orthogonal to every major stays its own cluster (rare distinct voice)
        // unless the user asserted a count.
        let embs = vec![
            vec![1.0, 0.0, 0.0], // major, 60 s
            vec![0.0, 1.0, 0.0], // major, 40 s
            vec![0.0, 0.0, 1.0], // orthogonal crumb, 2 s
        ];
        let durs = vec![60_000, 40_000, 2_000];
        let labels = vec![0, 1, 2];

        let auto = consolidate_minor_clusters(&embs, &durs, &labels, None, 15_000, 0.30);
        assert_eq!(
            auto.iter().copied().max().unwrap() + 1,
            3,
            "orthogonal crumb survives without a hint"
        );

        let hinted = consolidate_minor_clusters(&embs, &durs, &labels, Some(2), 15_000, 0.30);
        assert_eq!(
            hinted.iter().copied().max().unwrap() + 1,
            2,
            "a hint trusts the user's count and attaches unconditionally"
        );
    }

    #[test]
    fn consolidation_hint_picks_longest_speaking_majors() {
        // Hint of 2 keeps the two longest-speaking clusters as majors even when a third
        // cluster clears the duration floor.
        let embs = vec![
            vec![1.0, 0.0],   // 60 s
            vec![0.0, 1.0],   // 40 s
            vec![0.71, 0.71], // 20 s — above the 15 s floor, but hint says 2 speakers
        ];
        let durs = vec![60_000, 40_000, 20_000];
        let labels = vec![0, 1, 2];

        let out = consolidate_minor_clusters(&embs, &durs, &labels, Some(2), 15_000, 0.30);
        assert_eq!(out.iter().copied().max().unwrap() + 1, 2);
        assert_ne!(out[0], out[1], "the two longest stay distinct");
    }

    #[test]
    fn consolidation_short_meeting_falls_back_to_largest_cluster() {
        // Nobody reaches the major floor: the longest cluster anchors, similar crumbs
        // attach to it.
        let embs = vec![vec![1.0, 0.0], vec![0.98, 0.19]];
        let durs = vec![8_000, 3_000];
        let labels = vec![0, 1];

        let out = consolidate_minor_clusters(&embs, &durs, &labels, None, 15_000, 0.30);
        assert_eq!(out[0], out[1], "crumb folds into the dominant voice");
    }

    #[test]
    fn speaker_count_hint_blocks_merging_below_target() {
        // Two near-identical fragments (cos ≈ 0.99, far above the 0.65 floor). Auto merges
        // them into one speaker; num_speakers=2 asserts two people, so they must stay apart.
        let embs = vec![vec![1.0, 0.0], vec![0.99, 0.14]];
        let durs = vec![2000, 2000];
        let labels = vec![0, 1];

        let auto = merge_clusters_by_centroid_with_hint(&embs, &durs, &labels, 0.65, None);
        assert_eq!(auto[0], auto[1], "similar fragments auto-merge");

        let hinted = merge_clusters_by_centroid_with_hint(&embs, &durs, &labels, 0.65, Some(2));
        assert_ne!(hinted[0], hinted[1], "hint pins the count at 2");
    }

    #[test]
    fn speaker_count_hint_edge_cases() {
        let embs = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let durs = vec![1000, 1000];
        // Fewer fragments than the hint: nothing to split, labels pass through.
        let under = merge_clusters_by_centroid_with_hint(&embs, &durs, &[0, 1], 0.65, Some(5));
        assert_ne!(under[0], under[1]);
        // Orthogonal voices never merge in stage 2 regardless of hint (below the floor);
        // collapsing to the hint count is consolidation's job.
        let zero = merge_clusters_by_centroid_with_hint(&embs, &durs, &[0, 1], 0.65, Some(0));
        assert_ne!(zero[0], zero[1]);
        let one = consolidate_minor_clusters(&embs, &durs, &[0, 1], Some(1), 15_000, 0.30);
        assert_eq!(one[0], one[1], "hint of 1 consolidates to one speaker");
    }

    #[test]
    fn centroid_merge_is_duration_weighted() {
        // Fragment 0: one LONG turn at +x plus a short noisy turn near +y. Its
        // duration-weighted centroid must stay close to +x, letting it merge with
        // fragment 1 (pure +x) despite the noisy short member.
        let embs = vec![
            vec![1.0, 0.0],  // long, +x
            vec![0.0, 1.0],  // short noise, +y
            vec![0.98, 0.2], // fragment 1, +x-ish
        ];
        let durs = vec![20_000, 400, 5_000];
        let labels = vec![0, 0, 1];
        let merged = merge_clusters_by_centroid(&embs, &durs, &labels, 0.85);
        assert_eq!(
            merged[0], merged[2],
            "duration weighting should dominate the noise"
        );
    }

    #[test]
    fn nearest_cluster_attaches_or_drops_by_floor() {
        let centroids = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]];
        // Clearly speaker 0.
        assert_eq!(nearest_cluster(&[0.95, 0.1, 0.0], &centroids, 0.3), Some(0));
        // Clearly speaker 1.
        assert_eq!(nearest_cluster(&[0.1, 0.95, 0.0], &centroids, 0.3), Some(1));
        // Orthogonal to both, best sim ≈ 0 < floor → dropped (segments stay NULL).
        assert_eq!(nearest_cluster(&[0.0, 0.0, 1.0], &centroids, 0.3), None);
        // No centroids → nothing to attach to.
        assert_eq!(nearest_cluster(&[1.0, 0.0, 0.0], &[], 0.3), None);
    }

    #[test]
    fn merge_same_cluster_joins_adjacent() {
        let turns = vec![
            SpeakerTurn {
                start_ms: 0,
                end_ms: 500,
                cluster_id: 0,
            },
            SpeakerTurn {
                start_ms: 600,
                end_ms: 900,
                cluster_id: 0,
            }, // gap 100 < 250 -> merge
            SpeakerTurn {
                start_ms: 950,
                end_ms: 1200,
                cluster_id: 1,
            }, // different cluster
            SpeakerTurn {
                start_ms: 5000,
                end_ms: 5500,
                cluster_id: 0,
            }, // far gap -> separate
        ];
        let merged = merge_same_cluster(turns, 250);
        assert_eq!(merged.len(), 3);
        assert_eq!(
            (merged[0].start_ms, merged[0].end_ms, merged[0].cluster_id),
            (0, 900, 0)
        );
        assert_eq!(
            (merged[1].start_ms, merged[1].end_ms, merged[1].cluster_id),
            (950, 1200, 1)
        );
        assert_eq!(
            (merged[2].start_ms, merged[2].end_ms, merged[2].cluster_id),
            (5000, 5500, 0)
        );
    }

    // ---- Integration test (requires the real ONNX models) ----
    //
    // Runs the full `diarize()` cascade to prove the ONNX graphs execute (shapes match, no
    // runtime errors). Skips gracefully unless the models are present. Point it at a model
    // dir via `MEETILY_DIARIZATION_MODEL_DIR` and (optionally) a real speech WAV via
    // `MEETILY_DIARIZATION_TEST_WAV`; otherwise it synthesizes a two-segment tone WAV
    // (note: buzz tones are not speech — pyannote segmentation typically yields 0 turns for
    // them, and the test then only asserts the cascade completed without error; measured
    // cluster counts on real 2/3-voice speech are recorded in the DEFAULT_* constant docs).
    //
    //   MEETILY_DIARIZATION_MODEL_DIR=/path/to/models/diarization \
    //     cargo test -p meetily --lib pipeline::diarization::tests::diarize_end_to_end -- --ignored --nocapture
    #[test]
    #[ignore]
    fn diarize_end_to_end() {
        let model_dir = match std::env::var("MEETILY_DIARIZATION_MODEL_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_MODEL_DIR not set");
                return;
            }
        };
        let config = DiarizerConfig { model_dir };
        if !config.is_available() {
            eprintln!("skip: models not present at {}", config.model_dir.display());
            return;
        }

        // Resolve an input WAV: env override, else synthesize a two-tone 48 kHz mono WAV.
        let (_tmp, wav_path) = match std::env::var("MEETILY_DIARIZATION_TEST_WAV") {
            Ok(p) => (None, std::path::PathBuf::from(p)),
            Err(_) => {
                let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
                write_two_tone_wav(tmp.path());
                let path = tmp.path().to_path_buf();
                (Some(tmp), path)
            }
        };

        let diarizer = Diarizer::load(config).expect("load diarizer");
        let result = diarizer
            .diarize(&wav_path)
            .expect("diarize should not error");
        eprintln!(
            "diarize ran: {} turns, {} clusters",
            result.turns.len(),
            result.cluster_embeddings.len()
        );
        for t in &result.turns {
            eprintln!(
                "  turn [{:>6}..{:>6}] ms -> cluster {}",
                t.start_ms, t.end_ms, t.cluster_id
            );
        }
        // Each cluster embedding is L2-normalized.
        for (id, emb) in &result.cluster_embeddings {
            let norm = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (norm - 1.0).abs() < 1e-3 || norm == 0.0,
                "cluster {id} embedding not unit-norm: {norm}"
            );
            assert!(!emb.is_empty(), "cluster {id} embedding empty");
        }
    }

    // Probe embedding stability: determinism, same-utterance halves, and pure-clip matrix.
    //   MEETILY_DIARIZATION_MODEL_DIR=... MEETILY_DIARIZATION_TEST_WAV=... \
    //     cargo test ... embed_stability_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn embed_stability_probe() {
        let model_dir = match std::env::var("MEETILY_DIARIZATION_MODEL_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => {
                eprintln!("skip: no model dir");
                return;
            }
        };
        let wav = match std::env::var("MEETILY_DIARIZATION_TEST_WAV") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => {
                eprintln!("skip: no wav");
                return;
            }
        };
        let config = DiarizerConfig { model_dir };
        if !config.is_available() {
            eprintln!("skip: models absent");
            return;
        }
        let d = Diarizer::load(config).unwrap();

        let decoded = crate::audio::decoder::decode_audio_file(&wav).unwrap();
        let waveform = decoded.to_whisper_format();
        let slice = |s_ms: usize, e_ms: usize| -> &[f32] {
            &waveform[(s_ms * 16).min(waveform.len())..(e_ms * 16).min(waveform.len())]
        };

        // 1) Determinism: same input twice.
        let a1 = d.embed_turn(slice(6655, 10000)).unwrap().unwrap();
        let a2 = d.embed_turn(slice(6655, 10000)).unwrap().unwrap();
        eprintln!(
            "determinism (same slice twice): cos = {:.6}",
            cosine_similarity(&a1, &a2)
        );

        // 2) Same-utterance halves (B speaks continuously 6655..12828).
        let h1 = d.embed_turn(slice(6655, 10000)).unwrap().unwrap();
        let h2 = d.embed_turn(slice(10017, 11851)).unwrap().unwrap();
        let h2b = d.embed_turn(slice(10017, 12800)).unwrap().unwrap();
        let full = d.embed_turn(slice(6655, 12800)).unwrap().unwrap();
        eprintln!(
            "same utterance: h1 vs h2       = {:.3}",
            cosine_similarity(&h1, &h2)
        );
        eprintln!(
            "same utterance: h1 vs h2(long) = {:.3}",
            cosine_similarity(&h1, &h2b)
        );
        eprintln!(
            "same utterance: h1 vs full     = {:.3}",
            cosine_similarity(&h1, &full)
        );
        eprintln!(
            "same utterance: h2 vs full     = {:.3}",
            cosine_similarity(&h2, &full)
        );

        // 3) Length sensitivity: nested prefixes of one utterance.
        for len_ms in [500usize, 1000, 1500, 2000, 3000] {
            let e = d.embed_turn(slice(6655, 6655 + len_ms)).unwrap().unwrap();
            eprintln!(
                "prefix {:>4}ms vs full-utterance: cos = {:.3}",
                len_ms,
                cosine_similarity(&e, &full)
            );
        }

        // 4) Optional: pure clip files (MEETILY_DIARIZATION_CLIP_DIR with clip_NN_S.wav).
        if let Ok(dir) = std::env::var("MEETILY_DIARIZATION_CLIP_DIR") {
            let mut entries: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |x| x == "wav"))
                .collect();
            entries.sort();
            let mut embs: Vec<(String, Vec<f32>)> = Vec::new();
            for p in entries {
                let dec = crate::audio::decoder::decode_audio_file(&p).unwrap();
                let w = dec.to_whisper_format();
                if let Some(e) = d.embed_turn(&w).unwrap() {
                    let name = p.file_stem().unwrap().to_string_lossy().to_string();
                    // Speaker tag: prefix before '_' when longer than 1 char (arctic
                    // "bdl_a0001"), else the last char (say clips "clip_00_A").
                    let spk = match name.split_once('_') {
                        Some((pre, _)) if pre.len() > 1 => pre.to_string(),
                        _ => name.chars().last().unwrap().to_string(),
                    };
                    embs.push((spk, e));
                }
            }
            let (mut same, mut cross) = (Vec::new(), Vec::new());
            let mut worst_same = (1.0f32, String::new());
            let mut best_cross = (-1.0f32, String::new());
            for i in 0..embs.len() {
                for j in (i + 1)..embs.len() {
                    let sim = cosine_similarity(&embs[i].1, &embs[j].1);
                    if embs[i].0 == embs[j].0 {
                        if sim < worst_same.0 {
                            worst_same = (sim, format!("{i} vs {j}"));
                        }
                        same.push(sim);
                    } else {
                        if sim > best_cross.0 {
                            best_cross = (sim, format!("{i} vs {j}"));
                        }
                        cross.push(sim);
                    }
                }
            }
            let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
            eprintln!(
                "clips: same n={} mean={:.3} worst={:.3} ({})",
                same.len(),
                mean(&same),
                worst_same.0,
                worst_same.1
            );
            eprintln!(
                "clips: cross n={} mean={:.3} best={:.3} ({})",
                cross.len(),
                mean(&cross),
                best_cross.0,
                best_cross.1
            );
        }
    }

    // ---- Measurement harness (ignored; drives parameter calibration) ----
    //
    // Runs the segmentation→embed stage on a real multi-voice WAV with a known ground-truth
    // timeline, then reports same- vs cross-speaker embedding similarity distributions
    // (split long/short turn), cluster counts at a sweep of distance thresholds, and tau
    // (cross-half same-speaker vs cross-speaker centroid similarity). Env-gated:
    //   MEETILY_DIARIZATION_MODEL_DIR=<dir with segmentation.onnx+embedding.onnx>
    //   MEETILY_DIARIZATION_TEST_WAV=<meeting.wav>
    //   MEETILY_DIARIZATION_TRUTH=<start_ms,end_ms,speaker CSV>
    //   cargo test -p meetily --lib pipeline::diarization::tests::measure_calibration -- --ignored --nocapture
    #[test]
    #[ignore]
    fn measure_calibration() {
        let model_dir = match std::env::var("MEETILY_DIARIZATION_MODEL_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_MODEL_DIR not set");
                return;
            }
        };
        let wav = match std::env::var("MEETILY_DIARIZATION_TEST_WAV") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_TEST_WAV not set");
                return;
            }
        };
        let truth_csv = match std::env::var("MEETILY_DIARIZATION_TRUTH") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_TRUTH not set");
                return;
            }
        };
        let config = DiarizerConfig { model_dir };
        if !config.is_available() {
            eprintln!("skip: models absent");
            return;
        }

        // Ground-truth intervals.
        let truth_txt = std::fs::read_to_string(&truth_csv).unwrap();
        let truth: Vec<(i64, i64, String)> = truth_txt
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let c: Vec<&str> = l.split(',').collect();
                (
                    c[0].trim().parse().unwrap(),
                    c[1].trim().parse().unwrap(),
                    c[2].trim().to_string(),
                )
            })
            .collect();
        let truth_of = |s: i64, e: i64| -> String {
            let mut best = ("?".to_string(), 0i64);
            for (ts, te, spk) in &truth {
                let ov = (e.min(*te) - s.max(*ts)).max(0);
                if ov > best.1 {
                    best = (spk.clone(), ov);
                }
            }
            best.0
        };

        // Capture-all params: tiny min_turn so short interjections are embedded too.
        let params = DiarizationParams {
            min_turn_ms: 200,
            ..Default::default()
        };
        let diarizer = Diarizer::load_with_params(config, params).unwrap();

        let decoded = crate::audio::decoder::decode_audio_file(&wav).unwrap();
        let waveform = decoded.to_whisper_format();
        let total_ms = (waveform.len() as f64 / SEG_SAMPLE_RATE as f64) * 1000.0;

        // Per-turn: (start,end,dur,overlap_frac,truth_spk,embedding). Also a global per-frame
        // active-count timeline for overlap fraction.
        #[derive(Clone)]
        struct T {
            s: i64,
            e: i64,
            dur: i64,
            ov: f32,
            spk: String,
            emb: Vec<f32>,
        }
        let mut frames_active: Vec<(i64, i64, u8)> = Vec::new(); // (s_ms,e_ms,count)
        let mut turns_meta: Vec<(i64, i64)> = Vec::new();

        let mut win_start = 0usize;
        let mut window_buf = vec![0f32; SEG_WINDOW_SAMPLES];
        while win_start < waveform.len() {
            let end = (win_start + SEG_WINDOW_SAMPLES).min(waveform.len());
            let n = end - win_start;
            for (dst, src) in window_buf.iter_mut().zip(&waveform[win_start..end]) {
                *dst = *src * SEG_WAVEFORM_SCALE;
            }
            for x in window_buf.iter_mut().skip(n) {
                *x = 0.0;
            }
            let active = diarizer.run_segmentation(&window_buf).unwrap();
            let num_frames = active.len();
            if num_frames > 0 {
                let frame_ms = SEG_WINDOW_MS / num_frames as f64;
                let window_start_ms = (win_start as f64 / SEG_SAMPLE_RATE as f64) * 1000.0;
                for (fi, fr) in active.iter().enumerate() {
                    let fs = (window_start_ms + fi as f64 * frame_ms).round() as i64;
                    let fe = (window_start_ms + (fi + 1) as f64 * frame_ms).round() as i64;
                    let cnt = fr.iter().filter(|b| **b).count() as u8;
                    frames_active.push((fs, fe, cnt));
                }
                for spk in 0..SEG_NUM_LOCAL_SPEAKERS {
                    let column: Vec<bool> = active.iter().map(|f| f[spk]).collect();
                    for (s, e) in runs_to_turns(
                        &column,
                        frame_ms,
                        window_start_ms,
                        200,
                        DEFAULT_MERGE_GAP_MS,
                    ) {
                        let e = e.min(total_ms.round() as i64);
                        if e - s >= 200 {
                            turns_meta.push((s, e));
                        }
                    }
                }
            }
            win_start += SEG_WINDOW_SAMPLES;
        }
        turns_meta.sort_by_key(|t| t.0);

        let ov_frac = |s: i64, e: i64| -> f32 {
            let (mut tot, mut ov) = (0i64, 0i64);
            for (fs, fe, cnt) in &frames_active {
                let o = (e.min(*fe) - s.max(*fs)).max(0);
                if o > 0 {
                    tot += o;
                    if *cnt >= 2 {
                        ov += o;
                    }
                }
            }
            if tot == 0 {
                0.0
            } else {
                ov as f32 / tot as f32
            }
        };

        let mut turns: Vec<T> = Vec::new();
        for (s, e) in turns_meta {
            let ss = ((s as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize;
            let ee = (((e as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize).min(waveform.len());
            if ee <= ss {
                continue;
            }
            if let Some(emb) = diarizer.embed_turn(&waveform[ss..ee]).unwrap() {
                turns.push(T {
                    s,
                    e,
                    dur: e - s,
                    ov: ov_frac(s, e),
                    spk: truth_of(s, e),
                    emb,
                });
            }
        }

        eprintln!("\n==== {} embedded turns ====", turns.len());
        for t in &turns {
            let b = if t.dur < 1200 { "SHORT" } else { "long " };
            eprintln!(
                "  [{:>6}..{:>6}] {} {} dur={:>5} ovlp={:.2}",
                t.s, t.e, t.spk, b, t.dur, t.ov
            );
        }

        // Similarity distributions.
        let stat = |v: &[f32]| -> (f32, f32, f32, usize) {
            if v.is_empty() {
                return (0.0, 0.0, 0.0, 0);
            }
            let mut s = v.to_vec();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = s.iter().sum::<f32>() / s.len() as f32;
            (s[0], mean, s[s.len() - 1], s.len())
        };
        let is_long = |t: &T| t.dur >= 1200;
        let (mut ll_same, mut ll_cross) = (vec![], vec![]);
        let (mut sh_same, mut sh_cross) = (vec![], vec![]);
        for i in 0..turns.len() {
            for j in (i + 1)..turns.len() {
                let sim = cosine_similarity(&turns[i].emb, &turns[j].emb);
                let same = turns[i].spk == turns[j].spk;
                let both_long = is_long(&turns[i]) && is_long(&turns[j]);
                let involves_short = !is_long(&turns[i]) || !is_long(&turns[j]);
                if both_long {
                    if same {
                        ll_same.push(sim)
                    } else {
                        ll_cross.push(sim)
                    }
                }
                if involves_short {
                    if same {
                        sh_same.push(sim)
                    } else {
                        sh_cross.push(sim)
                    }
                }
            }
        }
        let p = |name: &str, v: &[f32]| {
            let (mn, me, mx, n) = stat(v);
            eprintln!("  {name:<22} n={n:<4} min={mn:.3} mean={me:.3} max={mx:.3}");
        };
        eprintln!("\n==== pairwise cosine similarity ====");
        p("LONG-LONG same", &ll_same);
        p("LONG-LONG cross", &ll_cross);
        p("involves-SHORT same", &sh_same);
        p("involves-SHORT cross", &sh_cross);

        // Variant: embed only the CENTRAL <=3 s of each long turn (length-normalized
        // embeddings) and recompute the LONG-LONG stats.
        {
            let mut c_embs: Vec<(String, Vec<f32>)> = Vec::new();
            for t in turns.iter().filter(|t| is_long(t)) {
                let mid = (t.s + t.e) / 2;
                let (cs, ce) = if t.dur > 3000 {
                    (mid - 1500, mid + 1500)
                } else {
                    (t.s, t.e)
                };
                let ss = ((cs as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize;
                let ee =
                    (((ce as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize).min(waveform.len());
                if let Some(e) = diarizer.embed_turn(&waveform[ss..ee]).unwrap() {
                    c_embs.push((t.spk.clone(), e));
                }
            }
            let (mut cs_same, mut cs_cross) = (vec![], vec![]);
            for i in 0..c_embs.len() {
                for j in (i + 1)..c_embs.len() {
                    let sim = cosine_similarity(&c_embs[i].1, &c_embs[j].1);
                    if c_embs[i].0 == c_embs[j].0 {
                        cs_same.push(sim)
                    } else {
                        cs_cross.push(sim)
                    }
                }
            }
            p("central-3s same", &cs_same);
            p("central-3s cross", &cs_cross);
        }

        // Threshold sweep (LONG turns only vs ALL turns).
        let long_embs: Vec<Vec<f32>> = turns
            .iter()
            .filter(|t| is_long(t))
            .map(|t| t.emb.clone())
            .collect();
        let long_spks: Vec<String> = turns
            .iter()
            .filter(|t| is_long(t))
            .map(|t| t.spk.clone())
            .collect();
        let all_embs: Vec<Vec<f32>> = turns.iter().map(|t| t.emb.clone()).collect();
        let purity = |labels: &[usize], spks: &[String]| -> f32 {
            use std::collections::HashMap;
            let mut by: HashMap<usize, HashMap<&str, i64>> = HashMap::new();
            for (l, s) in labels.iter().zip(spks) {
                *by.entry(*l).or_default().entry(s.as_str()).or_default() += 1;
            }
            let tot: i64 = labels.len() as i64;
            let correct: i64 = by.values().map(|m| *m.values().max().unwrap()).sum();
            if tot == 0 {
                0.0
            } else {
                correct as f32 / tot as f32
            }
        };
        let all_spks: Vec<String> = turns.iter().map(|t| t.spk.clone()).collect();
        for (name, method) in [
            ("AVERAGE", Linkage::Average),
            ("COMPLETE", Linkage::Complete),
        ] {
            eprintln!("\n==== cluster count sweep ({name} linkage) ====");
            eprintln!("  thr | k(LONG only) purity | k(ALL turns) purity");
            for thr in [0.3, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.8] {
                let ll = cluster_embeddings(&long_embs, thr, method);
                let kl = ll.iter().copied().max().map(|m| m + 1).unwrap_or(0);
                let al = cluster_embeddings(&all_embs, thr, method);
                let ka = al.iter().copied().max().map(|m| m + 1).unwrap_or(0);
                eprintln!(
                    "  {thr:.2} |   k={kl:<2} {:.3}       |   k={ka:<2} {:.3}",
                    purity(&ll, &long_spks),
                    purity(&al, &all_spks)
                );
            }
        }

        // Two-stage clustering on the formation set: stage-1 fragments at the default tight
        // cut, then a sweep of the stage-2 centroid-merge floor (k + purity at each value).
        let dp = DiarizationParams::default();
        let formation: Vec<&T> = turns
            .iter()
            .filter(|t| t.dur >= dp.min_cluster_turn_ms && t.ov <= dp.max_overlap_frac)
            .collect();
        let f_embs: Vec<Vec<f32>> = formation.iter().map(|t| t.emb.clone()).collect();
        let f_durs: Vec<i64> = formation.iter().map(|t| t.dur).collect();
        let f_spks: Vec<String> = formation.iter().map(|t| t.spk.clone()).collect();
        let frag_labels = cluster_embeddings(&f_embs, dp.cluster_distance_threshold, dp.linkage);
        let kf = frag_labels
            .iter()
            .copied()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        eprintln!(
            "\n==== stage-1 fragments (thr={}, {:?}): k={} purity={:.3} ====",
            dp.cluster_distance_threshold,
            dp.linkage,
            kf,
            purity(&frag_labels, &f_spks)
        );
        eprintln!("==== stage-2 centroid-merge sweep ====");
        eprintln!("  min_sim | k purity");
        for min_sim in [0.60, 0.65, 0.70, 0.75, 0.80, 0.85, 0.90] {
            let merged = merge_clusters_by_centroid(&f_embs, &f_durs, &frag_labels, min_sim);
            let k = merged.iter().copied().max().map(|m| m + 1).unwrap_or(0);
            eprintln!(
                "  {min_sim:.2}    | k={k:<2} {:.3}",
                purity(&merged, &f_spks)
            );
        }
        let f_labels = merge_clusters_by_centroid(
            &f_embs,
            &f_durs,
            &frag_labels,
            dp.centroid_merge_min_similarity,
        );
        eprintln!(
            "==== two-stage @ defaults (stage2={}) ====",
            dp.centroid_merge_min_similarity
        );
        for (t, l) in formation.iter().zip(&f_labels) {
            eprintln!(
                "  cluster {l}: [{:>6}..{:>6}] {} dur={}",
                t.s, t.e, t.spk, t.dur
            );
        }
        // Most-suspicious pairs: top cross-speaker similarities and bottom same-speaker ones.
        let mut cross_pairs: Vec<(f32, usize, usize)> = Vec::new();
        let mut same_pairs: Vec<(f32, usize, usize)> = Vec::new();
        for i in 0..turns.len() {
            for j in (i + 1)..turns.len() {
                if !is_long(&turns[i]) || !is_long(&turns[j]) {
                    continue;
                }
                let sim = cosine_similarity(&turns[i].emb, &turns[j].emb);
                if turns[i].spk == turns[j].spk {
                    same_pairs.push((sim, i, j));
                } else {
                    cross_pairs.push((sim, i, j));
                }
            }
        }
        cross_pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        same_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        eprintln!("  top cross-speaker sims:");
        for (sim, i, j) in cross_pairs.iter().take(3) {
            eprintln!(
                "    {:.3}  [{}..{}]{} vs [{}..{}]{}",
                sim,
                turns[*i].s,
                turns[*i].e,
                turns[*i].spk,
                turns[*j].s,
                turns[*j].e,
                turns[*j].spk
            );
        }
        eprintln!("  bottom same-speaker sims:");
        for (sim, i, j) in same_pairs.iter().take(3) {
            eprintln!(
                "    {:.3}  [{}..{}]{} vs [{}..{}]{}",
                sim,
                turns[*i].s,
                turns[*i].e,
                turns[*i].spk,
                turns[*j].s,
                turns[*j].e,
                turns[*j].spk
            );
        }

        // Full-pipeline cluster count with the NEW defaults (formation/attach + complete linkage).
        let new_result = {
            let cfg2 = DiarizerConfig {
                model_dir: std::path::PathBuf::from(
                    std::env::var("MEETILY_DIARIZATION_MODEL_DIR").unwrap(),
                ),
            };
            let d2 = Diarizer::load(cfg2).unwrap();
            d2.diarize(&wav).unwrap()
        };
        eprintln!("\n==== full diarize() with NEW defaults ====");
        eprintln!(
            "  clusters = {}  turns = {}",
            new_result.cluster_embeddings.len(),
            new_result.turns.len()
        );

        // Tau: per-speaker duration-weighted centroid from first vs second half of that
        // speaker's LONG turns; cross-half same-speaker vs cross-speaker centroid cosine.
        use std::collections::BTreeMap;
        let mut by_spk: BTreeMap<String, Vec<&T>> = BTreeMap::new();
        for t in &turns {
            if is_long(t) {
                by_spk.entry(t.spk.clone()).or_default().push(t);
            }
        }
        let centroid = |ts: &[&T]| -> Vec<f32> {
            if ts.is_empty() {
                return vec![];
            }
            let dim = ts[0].emb.len();
            let mut acc = vec![0f32; dim];
            let mut w = 0f32;
            for t in ts {
                let dw = t.dur as f32;
                for (a, x) in acc.iter_mut().zip(&t.emb) {
                    *a += x * dw;
                }
                w += dw;
            }
            for a in acc.iter_mut() {
                *a /= w;
            }
            let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for a in acc.iter_mut() {
                    *a /= norm;
                }
            }
            acc
        };
        let mut half_centroids: BTreeMap<String, (Vec<f32>, Vec<f32>)> = BTreeMap::new();
        for (spk, ts) in &by_spk {
            let mid = ts.len() / 2;
            half_centroids.insert(spk.clone(), (centroid(&ts[..mid]), centroid(&ts[mid..])));
        }
        eprintln!("\n==== tau validation (duration-weighted half centroids) ====");
        for (spk, (h1, h2)) in &half_centroids {
            eprintln!(
                "  {spk} same-speaker cross-half cos = {:.3}",
                cosine_similarity(h1, h2)
            );
        }
        let spks: Vec<&String> = half_centroids.keys().collect();
        for i in 0..spks.len() {
            for j in (i + 1)..spks.len() {
                let a = &half_centroids[spks[i]].0;
                let b = &half_centroids[spks[j]].0;
                eprintln!(
                    "  {} vs {} cross-speaker cos = {:.3}",
                    spks[i],
                    spks[j],
                    cosine_similarity(a, b)
                );
            }
        }
    }

    /// Write a minimal 16-bit PCM mono 48 kHz WAV: 4 s of a 160 Hz buzz, 1 s silence, 4 s of
    /// a 240 Hz buzz (two distinct "voices"), each with a couple of harmonics.
    #[cfg(test)]
    fn write_two_tone_wav(path: &std::path::Path) {
        use std::io::Write;
        const SR: u32 = 48_000;
        let mut samples: Vec<i16> = Vec::new();
        let tone = |secs: f64, f0: f64| -> Vec<i16> {
            let n = (secs * SR as f64) as usize;
            (0..n)
                .map(|i| {
                    let t = i as f64 / SR as f64;
                    let v = 0.5 * (2.0 * std::f64::consts::PI * f0 * t).sin()
                        + 0.25 * (2.0 * std::f64::consts::PI * 2.0 * f0 * t).sin()
                        + 0.12 * (2.0 * std::f64::consts::PI * 3.0 * f0 * t).sin();
                    (v * 12_000.0) as i16
                })
                .collect()
        };
        samples.extend(tone(4.0, 160.0));
        samples.extend(vec![0i16; SR as usize]); // 1 s silence
        samples.extend(tone(4.0, 240.0));

        let data_bytes = samples.len() * 2;
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&((36 + data_bytes) as u32).to_le_bytes())
            .unwrap();
        f.write_all(b"WAVE").unwrap();
        f.write_all(b"fmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap(); // fmt chunk size
        f.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        f.write_all(&1u16.to_le_bytes()).unwrap(); // mono
        f.write_all(&SR.to_le_bytes()).unwrap();
        f.write_all(&(SR * 2).to_le_bytes()).unwrap(); // byte rate
        f.write_all(&2u16.to_le_bytes()).unwrap(); // block align
        f.write_all(&16u16.to_le_bytes()).unwrap(); // bits per sample
        f.write_all(b"data").unwrap();
        f.write_all(&(data_bytes as u32).to_le_bytes()).unwrap();
        for s in samples {
            f.write_all(&s.to_le_bytes()).unwrap();
        }
        f.flush().unwrap();
    }

    // Research harness: end-to-end diarization of a real meeting WAV with the shipped
    // default params; writes predicted turns as CSV (start_ms,end_ms,cluster_id) for
    // offline scoring against a ground-truth timeline. Env-gated:
    //   MEETILY_DIARIZATION_MODEL_DIR=<dir with segmentation.onnx+embedding.onnx>
    //   MEETILY_DIARIZATION_TEST_WAV=<meeting.wav>
    //   RESEARCH_OUT=<turns.csv>
    //   cargo test -p meetily --lib pipeline::diarization::tests::research_diarize_wav -- --ignored --nocapture
    #[test]
    #[ignore]
    fn research_diarize_wav() {
        let model_dir = match std::env::var("MEETILY_DIARIZATION_MODEL_DIR") {
            Ok(d) => std::path::PathBuf::from(d),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_MODEL_DIR not set");
                return;
            }
        };
        let wav = match std::env::var("MEETILY_DIARIZATION_TEST_WAV") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => {
                eprintln!("skip: MEETILY_DIARIZATION_TEST_WAV not set");
                return;
            }
        };
        let out = std::env::var("RESEARCH_OUT").unwrap_or_else(|_| "research_turns.csv".into());
        let num_speakers = std::env::var("RESEARCH_NUM_SPEAKERS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok());
        let centroid_merge_min_similarity = std::env::var("RESEARCH_CENTROID_MERGE_SIM")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(DEFAULT_CENTROID_MERGE_MIN_SIM);
        let cluster_distance_threshold = std::env::var("RESEARCH_CLUSTER_DIST")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(DEFAULT_CLUSTER_DISTANCE_THRESHOLD);

        let params = DiarizationParams {
            num_speakers,
            centroid_merge_min_similarity,
            cluster_distance_threshold,
            ..Default::default()
        };
        let diarizer = Diarizer::load_with_params(DiarizerConfig { model_dir }, params).unwrap();
        let started = std::time::Instant::now();
        let result = diarizer.diarize(&wav).unwrap();
        eprintln!(
            "diarize: {} turns, {} clusters, {:.1}s wall",
            result.turns.len(),
            result.cluster_embeddings.len(),
            started.elapsed().as_secs_f64()
        );
        let mut csv = String::from("start_ms,end_ms,cluster_id\n");
        for t in &result.turns {
            csv.push_str(&format!("{},{},{}\n", t.start_ms, t.end_ms, t.cluster_id));
        }
        std::fs::write(&out, csv).unwrap();
        eprintln!("wrote {out}");
    }
}
