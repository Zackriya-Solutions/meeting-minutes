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
use std::sync::Mutex;

use anyhow::{anyhow, Result};

use super::kaldi_fbank::KaldiFbank;

/// Default cosine threshold for auto-assigning a cluster to an existing speaker
/// (PLAN.md §11 #4, configurable, tuned on real data).
pub const DEFAULT_SPEAKER_TAU: f32 = 0.75;

/// Minimum share of a segment that must be covered by a cluster's turns to attribute it;
/// below this the segment is left unattributed (PLAN.md: ambiguous <60% → NULL speaker).
pub const MIN_OVERLAP_RATIO: f32 = 0.60;

// ---- Diarization ONNX pipeline constants ----

/// Model file names in the diarizer model dir (see [`crate::pipeline::diarization_commands`]).
pub const SEGMENTATION_FILE: &str = "segmentation.onnx";
pub const EMBEDDING_FILE: &str = "embedding.onnx";

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

/// Agglomerative-clustering cut threshold on **cosine distance** (`1 - cosine_similarity`).
/// Default 0.5 is sherpa-onnx's `FastClusteringConfig` default for these WeSpeaker
/// VoxCeleb embeddings (CAM++ / ResNet34) — a reasonable, documented v1 starting point;
/// tune on real meetings later.
pub const DEFAULT_CLUSTER_DISTANCE_THRESHOLD: f32 = 0.5;

/// Turns shorter than this (after gap-merging) are dropped — too little audio for a
/// reliable speaker embedding.
pub const DEFAULT_MIN_TURN_MS: i64 = 250;

/// Adjacent same-speaker activity separated by less than this is merged into one turn.
pub const DEFAULT_MERGE_GAP_MS: i64 = 250;

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

// ---- Pure pipeline logic (unit-tested; no ONNX) ----

/// The set of local speaker indices active for each pyannote powerset class (3 speakers,
/// max 2 simultaneous → 7 classes, generated in increasing-cardinality order):
/// 0:{} 1:{0} 2:{1} 3:{2} 4:{0,1} 5:{0,2} 6:{1,2}.
pub fn powerset_speakers(class: usize) -> &'static [usize] {
    const MAP: [&[usize]; 7] = [
        &[],
        &[0],
        &[1],
        &[2],
        &[0, 1],
        &[0, 2],
        &[1, 2],
    ];
    MAP.get(class).copied().unwrap_or(&[])
}

/// Decode one segmentation window's logits `[num_frames, num_classes]` (row-major) into
/// per-frame per-local-speaker activity: `active[frame][local_speaker]`. Each frame takes
/// the argmax powerset class, then expands it to its member local speakers.
pub fn decode_powerset(logits: &[f32], num_frames: usize, num_classes: usize) -> Vec<[bool; SEG_NUM_LOCAL_SPEAKERS]> {
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
    merged.into_iter().filter(|(s, e)| e - s >= min_turn_ms).collect()
}

/// Agglomerative (average-linkage) clustering of L2-normalized embeddings, cut at
/// `distance_threshold` on cosine distance (`1 - cosine_similarity`). Returns a cluster
/// label in `0..k` for each input embedding. Uses `kodama` for the linkage, then a
/// union-find cut over the dendrogram steps below the threshold.
pub fn cluster_embeddings(embeddings: &[Vec<f32>], distance_threshold: f32) -> Vec<usize> {
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

    let dend = kodama::linkage(&mut condensed, n, kodama::Method::Average);

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

/// Merge adjacent same-cluster turns (sorted by start) separated by `< merge_gap_ms`.
fn merge_same_cluster(mut turns: Vec<SpeakerTurn>, merge_gap_ms: i64) -> Vec<SpeakerTurn> {
    turns.sort_by(|a, b| a.start_ms.cmp(&b.start_ms).then(a.cluster_id.cmp(&b.cluster_id)));
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
    /// Cosine-distance cut for agglomerative clustering.
    pub cluster_distance_threshold: f32,
    pub min_turn_ms: i64,
    pub merge_gap_ms: i64,
}

impl Default for DiarizationParams {
    fn default() -> Self {
        Self {
            cluster_distance_threshold: DEFAULT_CLUSTER_DISTANCE_THRESHOLD,
            min_turn_ms: DEFAULT_MIN_TURN_MS,
            merge_gap_ms: DEFAULT_MERGE_GAP_MS,
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

        log::info!(
            "diarizer loaded: seg out='{seg_output_name}', emb in='{emb_input_name}' out='{emb_output_name}'"
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
        })
    }

    /// Run diarization on an audio file → speaker turns + per-cluster mean embeddings.
    ///
    /// Pipeline: decode to 16 kHz mono → slide 10 s segmentation windows → powerset-decode
    /// per-local-speaker turns → embed each turn (kaldi fbank + WeSpeaker ONNX, L2-norm) →
    /// agglomerative clustering → merged turns + one mean embedding per cluster.
    pub fn diarize(&self, audio_path: &std::path::Path) -> Result<DiarizationResult> {
        // 1) Decode to 16 kHz mono f32 in [-1, 1] (reuses the shared audio decoder).
        let decoded = crate::audio::decoder::decode_audio_file(audio_path)
            .map_err(|e| anyhow!("diarize: decode {}: {e}", audio_path.display()))?;
        let waveform = decoded.to_whisper_format(); // 16 kHz mono
        if waveform.is_empty() {
            return Ok(DiarizationResult { turns: Vec::new(), cluster_embeddings: Vec::new() });
        }
        let total_ms = (waveform.len() as f64 / SEG_SAMPLE_RATE as f64) * 1000.0;

        // 2) Slide non-overlapping 10 s windows; the final window is zero-padded.
        let mut raw_turns: Vec<(i64, i64)> = Vec::new();
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
                            raw_turns.push((s, e));
                        }
                    }
                }
            }
            win_start += SEG_WINDOW_SAMPLES;
        }

        if raw_turns.is_empty() {
            return Ok(DiarizationResult { turns: Vec::new(), cluster_embeddings: Vec::new() });
        }

        // 3) One speaker embedding per raw turn.
        let mut kept_turns: Vec<(i64, i64)> = Vec::with_capacity(raw_turns.len());
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(raw_turns.len());
        for (s, e) in raw_turns {
            let start_sample = ((s as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize;
            let end_sample = (((e as f64 / 1000.0) * SEG_SAMPLE_RATE as f64) as usize).min(waveform.len());
            if end_sample <= start_sample {
                continue;
            }
            match self.embed_turn(&waveform[start_sample..end_sample])? {
                Some(emb) => {
                    kept_turns.push((s, e));
                    embeddings.push(emb);
                }
                None => {} // too short to featurize; drop
            }
        }

        if embeddings.is_empty() {
            return Ok(DiarizationResult { turns: Vec::new(), cluster_embeddings: Vec::new() });
        }

        // 4) Cluster turn embeddings into global speakers.
        let labels = cluster_embeddings(&embeddings, self.params.cluster_distance_threshold);

        // 5) Build merged turns + per-cluster mean embeddings.
        let turns: Vec<SpeakerTurn> = kept_turns
            .iter()
            .zip(&labels)
            .map(|(&(start_ms, end_ms), &label)| SpeakerTurn {
                start_ms,
                end_ms,
                cluster_id: label as i64,
            })
            .collect();
        let turns = merge_same_cluster(turns, self.params.merge_gap_ms);

        let num_clusters = labels.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let mut sums: Vec<Vec<f32>> = vec![Vec::new(); num_clusters];
        let mut counts: Vec<u32> = vec![0; num_clusters];
        for (emb, &label) in embeddings.iter().zip(&labels) {
            let acc = &mut sums[label];
            if acc.is_empty() {
                *acc = vec![0.0; emb.len()];
            }
            for (a, x) in acc.iter_mut().zip(emb) {
                *a += x;
            }
            counts[label] += 1;
        }
        let mut cluster_embeddings: Vec<(i64, Vec<f32>)> = Vec::with_capacity(num_clusters);
        for (label, (mut acc, count)) in sums.into_iter().zip(counts).enumerate() {
            if count == 0 {
                continue;
            }
            for a in acc.iter_mut() {
                *a /= count as f32;
            }
            l2_normalize(&mut acc);
            cluster_embeddings.push((label as i64, acc));
        }

        Ok(DiarizationResult { turns, cluster_embeddings })
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

    /// Compute an L2-normalized speaker embedding for one turn's 16 kHz audio.
    /// Returns `None` when the slice is too short to yield any fbank frame.
    fn embed_turn(&self, samples: &[f32]) -> Result<Option<Vec<f32>>> {
        use ort::value::TensorRef;

        let feats = self.fbank.compute(samples); // [num_frames, 80], CMN-normalized
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
        let active = [true, true, true, false, true, true, false, false, false, true];
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
        let labels = cluster_embeddings(&[a1, a2, b1, b2], 0.5);
        assert_eq!(labels[0], labels[1], "a1,a2 should share a cluster");
        assert_eq!(labels[2], labels[3], "b1,b2 should share a cluster");
        assert_ne!(labels[0], labels[2], "the two groups should differ");
    }

    #[test]
    fn cluster_edge_cases() {
        assert_eq!(cluster_embeddings(&[], 0.5), Vec::<usize>::new());
        assert_eq!(cluster_embeddings(&[vec![1.0, 0.0]], 0.5), vec![0]);
    }

    #[test]
    fn cluster_all_one_when_similar() {
        // Three near-identical vectors -> one cluster.
        let v = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.99, 0.14, 0.0],
            vec![0.98, 0.2, 0.0],
        ];
        let labels = cluster_embeddings(&v, 0.5);
        assert!(labels.iter().all(|&l| l == labels[0]));
    }

    #[test]
    fn merge_same_cluster_joins_adjacent() {
        let turns = vec![
            SpeakerTurn { start_ms: 0, end_ms: 500, cluster_id: 0 },
            SpeakerTurn { start_ms: 600, end_ms: 900, cluster_id: 0 }, // gap 100 < 250 -> merge
            SpeakerTurn { start_ms: 950, end_ms: 1200, cluster_id: 1 }, // different cluster
            SpeakerTurn { start_ms: 5000, end_ms: 5500, cluster_id: 0 }, // far gap -> separate
        ];
        let merged = merge_same_cluster(turns, 250);
        assert_eq!(merged.len(), 3);
        assert_eq!((merged[0].start_ms, merged[0].end_ms, merged[0].cluster_id), (0, 900, 0));
        assert_eq!((merged[1].start_ms, merged[1].end_ms, merged[1].cluster_id), (950, 1200, 1));
        assert_eq!((merged[2].start_ms, merged[2].end_ms, merged[2].cluster_id), (5000, 5500, 0));
    }

    // ---- Integration test (requires the real ONNX models) ----
    //
    // Runs the full `diarize()` cascade to prove the ONNX graphs execute (shapes match, no
    // runtime errors). Skips gracefully unless the models are present. Point it at a model
    // dir via `MEETILY_DIARIZATION_MODEL_DIR` and (optionally) a real speech WAV via
    // `MEETILY_DIARIZATION_TEST_WAV`; otherwise it synthesizes a two-segment tone WAV.
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
        let result = diarizer.diarize(&wav_path).expect("diarize should not error");
        eprintln!(
            "diarize ran: {} turns, {} clusters",
            result.turns.len(),
            result.cluster_embeddings.len()
        );
        for t in &result.turns {
            eprintln!("  turn [{:>6}..{:>6}] ms -> cluster {}", t.start_ms, t.end_ms, t.cluster_id);
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
        f.write_all(&((36 + data_bytes) as u32).to_le_bytes()).unwrap();
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
}
