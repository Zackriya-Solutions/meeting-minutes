//! Vector math and agglomerative (average-linkage) clustering on cosine similarity.
use std::cmp::Ordering;
use std::collections::HashMap;

/// Above this many embeddings, a time-uniform subset is clustered and the rest join the nearest
/// resulting centroid. Keeps the similarity matrix near 64 MB (roughly 2.5-4 h of speech).
pub const MAX_CLUSTER_POINTS: usize = 4000;

pub fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Cosine similarity; 0 when either vector has zero length or the lengths differ
/// (for example a centroid stored by a different embedding model).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let na = dot(a, a).sqrt();
    let nb = dot(b, b).sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot(a, b) / (na * nb)
    }
}

/// Weighted mean of L2-normalised vectors, re-normalised. Vectors that are not finite after
/// normalisation are skipped, so one NaN embedding cannot poison a centroid.
pub fn weighted_centroid(vectors: &[&[f32]], weights: &[f64]) -> Vec<f32> {
    let dim = vectors.first().map(|v| v.len()).unwrap_or(0);
    let mut acc = vec![0f64; dim];
    for (v, &w) in vectors.iter().zip(weights) {
        let mut n = v.to_vec();
        l2_normalize(&mut n);
        if !n.iter().all(|x| x.is_finite()) {
            continue;
        }
        for (a, x) in acc.iter_mut().zip(&n) {
            *a += w * *x as f64;
        }
    }
    let mut out: Vec<f32> = acc.into_iter().map(|x| x as f32).collect();
    l2_normalize(&mut out);
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClusterStop {
    /// Merge while the closest clusters' average cosine similarity is at least this value.
    Threshold(f32),
    /// Merge until exactly this many clusters remain (clamped to the number of points).
    Count(usize),
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut cur = x;
        while self.0[cur] != root {
            let next = self.0[cur];
            self.0[cur] = root;
            cur = next;
        }
        root
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[rb] = ra;
        }
    }
}

/// Average-linkage agglomerative clustering. Returns one label per input, numbered 0.. in order
/// of first occurrence. More than MAX_CLUSTER_POINTS inputs (which arrive in window order) are
/// clustered through a time-uniform subset; the other inputs join the nearest centroid.
pub fn agglomerative(embeddings: &[Vec<f32>], stop: ClusterStop) -> Vec<usize> {
    Linkage::new(embeddings).cut(stop)
}

/// The average-linkage merges of a set of embeddings, built once and cut at any stop.
struct Linkage<'a> {
    embeddings: &'a [Vec<f32>],
    /// The clustered inputs and their embeddings, when only a subset was clustered.
    subset: Option<(Vec<usize>, Vec<Vec<f32>>)>,
    /// Merges of the clustered points, most similar first.
    merges: Vec<(usize, usize, f32)>,
}

impl<'a> Linkage<'a> {
    fn new(embeddings: &'a [Vec<f32>]) -> Self {
        let n = embeddings.len();
        if n <= MAX_CLUSTER_POINTS {
            return Self { embeddings, subset: None, merges: dense_merges(embeddings) };
        }
        let idx: Vec<usize> = (0..MAX_CLUSTER_POINTS).map(|i| i * n / MAX_CLUSTER_POINTS).collect();
        let points: Vec<Vec<f32>> = idx.iter().map(|&i| embeddings[i].clone()).collect();
        let merges = dense_merges(&points);
        Self { embeddings, subset: Some((idx, points)), merges }
    }

    fn cut(&self, stop: ClusterStop) -> Vec<usize> {
        let Some((idx, points)) = &self.subset else {
            return cut_merges(self.embeddings.len(), &self.merges, stop);
        };
        let sub_labels = cut_merges(points.len(), &self.merges, stop);
        let k = cluster_count(&sub_labels);
        let centroids = cluster_centroids(points, &vec![1.0; points.len()], &sub_labels, k);
        let n = self.embeddings.len();
        let mut in_subset = vec![false; n];
        let mut raw = vec![0usize; n];
        for (&i, &l) in idx.iter().zip(&sub_labels) {
            in_subset[i] = true;
            raw[i] = l;
        }
        for i in 0..n {
            if !in_subset[i] {
                raw[i] = (0..k)
                    .max_by(|&a, &b| {
                        cosine(&self.embeddings[i], &centroids[a])
                            .partial_cmp(&cosine(&self.embeddings[i], &centroids[b]))
                            .unwrap_or(Ordering::Equal)
                    })
                    .unwrap_or(0);
            }
        }
        renumber(raw.into_iter())
    }
}

/// A cluster carrying less speech than this (summed over the overlapping windows its embeddings
/// come from, roughly four times the speech itself) is crosstalk or noise rather than a speaker.
pub const MIN_SPEAKER_WEIGHT_S: f64 = 20.0;
/// How far `cluster_speakers` tightens the threshold, step by step, to find more speakers.
const MAX_COUNT_THRESHOLD: f32 = 0.95;
const COUNT_THRESHOLD_STEP: f32 = 0.05;

/// Groups speaker embeddings, each carrying `weights[i]` seconds of speech, into speakers.
/// Average-linkage clusters below MIN_SPEAKER_WEIGHT_S join the nearest larger cluster. With
/// `num_speakers`, the threshold is tightened until at least that many speakers emerge, and the
/// most similar speakers are then merged down to the count. Labels are numbered 0.. in order of
/// first occurrence.
pub fn cluster_speakers(embeddings: &[Vec<f32>], weights: &[f64], num_speakers: Option<usize>, threshold: f32) -> Vec<usize> {
    let linkage = Linkage::new(embeddings);
    let run = |t: f32| fold_small_clusters(embeddings, weights, linkage.cut(ClusterStop::Threshold(t)));
    let Some(n) = num_speakers.map(|n| n.max(1)) else {
        return run(threshold);
    };
    let mut t = threshold;
    let mut labels = run(t);
    while cluster_count(&labels) < n && t < MAX_COUNT_THRESHOLD {
        t += COUNT_THRESHOLD_STEP;
        labels = run(t);
    }
    let k = cluster_count(&labels);
    if k <= n {
        return labels;
    }
    let centroids = cluster_centroids(embeddings, weights, &labels, k);
    let merged = agglomerative(&centroids, ClusterStop::Count(n));
    renumber(labels.into_iter().map(|l| merged[l]))
}

fn cluster_count(labels: &[usize]) -> usize {
    labels.iter().max().map_or(0, |m| m + 1)
}

/// Speech-weighted centroid of each cluster 0..k.
pub(crate) fn cluster_centroids(embeddings: &[Vec<f32>], weights: &[f64], labels: &[usize], k: usize) -> Vec<Vec<f32>> {
    (0..k)
        .map(|c| {
            let (vs, ws): (Vec<&[f32]>, Vec<f64>) = labels
                .iter()
                .zip(embeddings.iter().zip(weights))
                .filter(|(&l, _)| l == c)
                .map(|(_, (e, &w))| (e.as_slice(), w))
                .unzip();
            weighted_centroid(&vs, &ws)
        })
        .collect()
}

/// Index of the centroid most similar to `embedding`.
pub fn nearest_centroid(embedding: &[f32], centroids: &[Vec<f32>]) -> usize {
    (0..centroids.len())
        .max_by(|&a, &b| {
            cosine(embedding, &centroids[a]).partial_cmp(&cosine(embedding, &centroids[b])).unwrap_or(Ordering::Equal)
        })
        .unwrap_or(0)
}

/// Moves the members of clusters below MIN_SPEAKER_WEIGHT_S to the nearest larger cluster.
/// When no cluster is large enough (very short audio), the clusters are kept as they are.
fn fold_small_clusters(embeddings: &[Vec<f32>], weights: &[f64], labels: Vec<usize>) -> Vec<usize> {
    let k = cluster_count(&labels);
    let mut weight = vec![0f64; k];
    for (&l, &w) in labels.iter().zip(weights) {
        weight[l] += w;
    }
    let large: Vec<usize> = (0..k).filter(|&c| weight[c] >= MIN_SPEAKER_WEIGHT_S).collect();
    if large.is_empty() || large.len() == k {
        return labels;
    }
    let centroids = cluster_centroids(embeddings, weights, &labels, k);
    let folded = labels.iter().enumerate().map(|(i, &l)| {
        if weight[l] >= MIN_SPEAKER_WEIGHT_S {
            return l;
        }
        *large
            .iter()
            .max_by(|&&a, &&b| {
                cosine(&embeddings[i], &centroids[a])
                    .partial_cmp(&cosine(&embeddings[i], &centroids[b]))
                    .unwrap_or(Ordering::Equal)
            })
            .expect("at least one large cluster")
    });
    renumber(folded)
}

fn renumber(labels: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut relabel: HashMap<usize, usize> = HashMap::new();
    labels
        .map(|c| {
            let next = relabel.len();
            *relabel.entry(c).or_insert(next)
        })
        .collect()
}

/// Average-linkage merges by nearest-neighbour chain (O(n²) time and memory), most similar
/// first; used for up to MAX_CLUSTER_POINTS inputs.
fn dense_merges(embeddings: &[Vec<f32>]) -> Vec<(usize, usize, f32)> {
    let n = embeddings.len();
    if n == 0 {
        return Vec::new();
    }
    let normed: Vec<Vec<f32>> = embeddings
        .iter()
        .map(|e| {
            let mut v = e.clone();
            l2_normalize(&mut v);
            v
        })
        .collect();
    let mut sim = vec![0f32; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let s = dot(&normed[i], &normed[j]);
            // A NaN embedding must not break the chain: treat it as maximally dissimilar.
            let s = if s.is_finite() { s } else { -1.0 };
            sim[i * n + j] = s;
            sim[j * n + i] = s;
        }
    }

    let mut size = vec![1usize; n];
    let mut active = vec![true; n];
    let mut merges: Vec<(usize, usize, f32)> = Vec::with_capacity(n - 1);
    let mut chain: Vec<usize> = Vec::new();
    let mut remaining = n;

    while remaining > 1 {
        if chain.is_empty() {
            chain.push((0..n).find(|&i| active[i]).expect("an active cluster"));
        }
        loop {
            let c = *chain.last().expect("non-empty chain");
            let prev = if chain.len() >= 2 { Some(chain[chain.len() - 2]) } else { None };
            let mut best = prev;
            let mut best_sim = prev.map(|p| sim[c * n + p]).unwrap_or(f32::NEG_INFINITY);
            for j in 0..n {
                if j != c && active[j] && sim[c * n + j] > best_sim {
                    best = Some(j);
                    best_sim = sim[c * n + j];
                }
            }
            let best = best.expect("at least two active clusters");
            if Some(best) == prev {
                chain.pop();
                chain.pop();
                let (a, b) = (c, best);
                let (sa, sb) = (size[a] as f32, size[b] as f32);
                for j in 0..n {
                    if active[j] && j != a && j != b {
                        let s = (sa * sim[a * n + j] + sb * sim[b * n + j]) / (sa + sb);
                        sim[a * n + j] = s;
                        sim[j * n + a] = s;
                    }
                }
                size[a] += size[b];
                active[b] = false;
                merges.push((a, b, best_sim));
                remaining -= 1;
                break;
            }
            chain.push(best);
        }
    }

    // Average linkage is monotone, so applying merges from most to least similar
    // reproduces the dendrogram.
    merges.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap_or(Ordering::Equal));
    merges
}

/// Labels of `n` points after applying `merges` (most similar first) down to `stop`. The merges
/// form a spanning tree over the points, so applying m of them leaves exactly n - m clusters.
fn cut_merges(n: usize, merges: &[(usize, usize, f32)], stop: ClusterStop) -> Vec<usize> {
    let to_apply = match stop {
        ClusterStop::Threshold(t) => merges.iter().take_while(|m| m.2 >= t).count(),
        ClusterStop::Count(k) => n.saturating_sub(k.max(1)),
    };
    let mut uf = UnionFind::new(n);
    for &(a, b, _) in merges.iter().take(to_apply) {
        uf.union(a, b);
    }
    renumber((0..n).map(|i| uf.find(i)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(base: &[f32], jitter: f32, i: usize) -> Vec<f32> {
        base.iter()
            .enumerate()
            .map(|(d, x)| x + jitter * (((i * 7 + d * 3) % 5) as f32 - 2.0))
            .collect()
    }

    fn three_groups() -> Vec<Vec<f32>> {
        let a = [1.0, 0.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0, 0.0];
        let c = [0.0, 0.0, 1.0, 0.0];
        let mut v = Vec::new();
        for i in 0..4 {
            v.push(around(&a, 0.05, i));
            v.push(around(&b, 0.05, i));
            v.push(around(&c, 0.05, i));
        }
        v
    }

    #[test]
    fn nearest_centroid_picks_the_most_similar() {
        let centroids = vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.0, 0.0, 1.0]];
        assert_eq!(nearest_centroid(&[0.1, 0.2, 0.9], &centroids), 2);
        assert_eq!(nearest_centroid(&[0.8, 0.3, 0.0], &centroids), 0);
    }

    #[test]
    fn cosine_of_parallel_and_orthogonal_vectors() {
        assert!((cosine(&[1.0, 2.0], &[2.0, 4.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 3.0]).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn cosine_of_different_lengths_is_zero() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn weighted_centroid_leans_to_heavier_vector_and_is_normalised() {
        let c = weighted_centroid(&[&[1.0, 0.0], &[0.0, 1.0]], &[3.0, 1.0]);
        assert!(c[0] > c[1]);
        let norm = (c[0] * c[0] + c[1] * c[1]).sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn threshold_separates_three_groups() {
        let labels = agglomerative(&three_groups(), ClusterStop::Threshold(0.5));
        assert_eq!(labels.len(), 12);
        assert_eq!(labels.iter().max(), Some(&2));
        for i in 0..4 {
            assert_eq!(labels[i * 3], labels[0]);
            assert_eq!(labels[i * 3 + 1], labels[1]);
            assert_eq!(labels[i * 3 + 2], labels[2]);
        }
        assert_eq!(&labels[0..3], &[0, 1, 2]);
    }

    #[test]
    fn count_forces_exact_number_of_clusters() {
        let labels = agglomerative(&three_groups(), ClusterStop::Count(2));
        let distinct: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(distinct.len(), 2);
        let one = agglomerative(&three_groups(), ClusterStop::Count(1));
        assert!(one.iter().all(|&l| l == 0));
    }

    #[test]
    fn count_larger_than_points_keeps_every_point_separate() {
        let labels = agglomerative(&three_groups()[..3].to_vec(), ClusterStop::Count(10));
        assert_eq!(labels, vec![0, 1, 2]);
    }

    #[test]
    fn strict_threshold_keeps_points_apart_and_empty_input_is_empty() {
        let labels = agglomerative(&three_groups(), ClusterStop::Threshold(0.9999));
        assert_eq!(labels.iter().max(), Some(&11));
        assert!(agglomerative(&[], ClusterStop::Threshold(0.5)).is_empty());
    }

    #[test]
    fn non_finite_embeddings_do_not_panic() {
        let labels = agglomerative(&[vec![f32::NAN, 0.0], vec![1.0, 0.0], vec![0.9, 0.1]], ClusterStop::Threshold(0.5));
        assert_eq!(labels, vec![0, 1, 1], "the NaN point stays in its own cluster");
        let c = weighted_centroid(&[&[f32::NAN, 0.0], &[1.0, 0.0]], &[1.0, 1.0]);
        assert!(c.iter().all(|x| x.is_finite()));
    }

    /// Three speakers with plenty of speech plus two stray snippets (crosstalk, noise) that
    /// resemble nobody. Each speaker point carries 10 s of speech, each stray 1 s.
    fn speakers_with_strays() -> (Vec<Vec<f32>>, Vec<f64>) {
        let mut points = three_groups();
        let mut weights = vec![10.0; points.len()];
        points.push(vec![0.0, 0.0, 0.0, 1.0]);
        points.push(vec![0.1, 0.0, 0.05, -1.0]);
        weights.extend([1.0, 1.0]);
        (points, weights)
    }

    #[test]
    fn stray_snippets_join_the_nearest_speaker() {
        let (points, weights) = speakers_with_strays();
        let labels = cluster_speakers(&points, &weights, None, 0.5);
        assert_eq!(labels.iter().max(), Some(&2), "{labels:?}");
        assert_eq!(&labels[..12], &three_groups_labels());
        assert_eq!(labels[13], labels[0], "the second stray leans towards the first speaker");
    }

    #[test]
    fn exact_count_ignores_stray_snippets() {
        let (points, weights) = speakers_with_strays();
        let labels = cluster_speakers(&points, &weights, Some(3), 0.5);
        assert_eq!(labels.iter().max(), Some(&2), "{labels:?}");
        assert_eq!(&labels[..12], &three_groups_labels());
    }

    #[test]
    fn exact_count_merges_the_closest_speakers() {
        let (points, weights) = speakers_with_strays();
        let labels = cluster_speakers(&points, &weights, Some(2), 0.5);
        assert_eq!(labels.iter().max(), Some(&1), "{labels:?}");
        let one = cluster_speakers(&points, &weights, Some(1), 0.5);
        assert!(one.iter().all(|&l| l == 0));
    }

    #[test]
    fn exact_count_splits_voices_the_threshold_would_join() {
        // Two speakers whose voices are similar (cosine about 0.6) merge at threshold 0.5,
        // but asking for two speakers keeps them apart.
        let a = [1.0, 0.0, 0.0, 0.0];
        let b = [0.6, 0.8, 0.0, 0.0];
        let points: Vec<Vec<f32>> = (0..8).map(|i| around(if i % 2 == 0 { &a } else { &b }, 0.02, i)).collect();
        let weights = vec![10.0; points.len()];
        assert!(cluster_speakers(&points, &weights, None, 0.5).iter().all(|&l| l == 0));
        let labels = cluster_speakers(&points, &weights, Some(2), 0.5);
        for (i, &l) in labels.iter().enumerate() {
            assert_eq!(l, i % 2, "{labels:?}");
        }
    }

    #[test]
    fn short_audio_with_only_small_clusters_keeps_them() {
        let points = three_groups();
        let weights = vec![1.0; points.len()];
        assert_eq!(cluster_speakers(&points, &weights, None, 0.5), three_groups_labels());
        assert!(cluster_speakers(&[], &[], None, 0.5).is_empty());
        assert!(cluster_speakers(&[], &[], Some(2), 0.5).is_empty());
    }

    fn three_groups_labels() -> Vec<usize> {
        (0..12).map(|i| i % 3).collect()
    }

    #[test]
    fn large_inputs_are_clustered_through_a_subset() {
        let bases = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let n = MAX_CLUSTER_POINTS + 500;
        let points: Vec<Vec<f32>> = (0..n).map(|i| around(&bases[i % 3], 0.05, i)).collect();
        for stop in [ClusterStop::Threshold(0.5), ClusterStop::Count(3)] {
            let labels = agglomerative(&points, stop);
            assert_eq!(labels.len(), n);
            assert_eq!(&labels[0..3], &[0, 1, 2], "{stop:?}");
            for (i, &l) in labels.iter().enumerate() {
                assert_eq!(l, labels[i % 3], "{stop:?}: point {i} left its group");
            }
        }
    }
}
