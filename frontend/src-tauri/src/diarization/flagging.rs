// diarization/flagging.rs
//
// Detect saved voice profiles that are likely contaminated or mutually
// confusable, so the user can review and prune them. A profile is "confusable"
// when its closest exemplar to some OTHER saved voice is still high AFTER
// anisotropy correction (centered cosine) — meaning the two would frequently be
// mislabeled for each other. That is usually a sign that one profile was
// contaminated by another person's audio, or that two voices are genuinely too
// similar to separate. Pure (no I/O); mirrors the matcher's centering.

use super::clustering::cosine_similarity;
use super::normalize::{center_normalized, cohort_mean};

/// A saved profile that collides with another saved voice.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfusableFlag {
    /// The flagged profile.
    pub name: String,
    /// The other saved voice it is closest to.
    pub confused_with: String,
    /// Mean centered cosine over all exemplar pairs (higher = more confusable).
    pub score: f32,
}

/// Centered cosine at/above which two DIFFERENT saved voices are treated as
/// confusable. Just below the 0.60 auto-adopt bar: a pair this close in the
/// residual space will often be mislabeled for one another.
///
/// Applied to a MEAN over exemplar pairs (see `flag_confusable_profiles`), not
/// a maximum, so the bar means the same thing regardless of how many meetings
/// a profile has accumulated.
pub const CONFUSABLE_THRESHOLD: f32 = 0.55;

/// Minimum number of exemplars across all profiles before we trust the cohort
/// mean estimate enough to flag. Below this we return nothing rather than raise
/// false alarms (raw anisotropy would make everything look confusable).
pub const MIN_EXEMPLARS_FOR_FLAGGING: usize = 8;

/// Flag profiles that collide with another saved voice. `profiles` is
/// (name, exemplars). For each profile with at least one other profile within
/// `CONFUSABLE_THRESHOLD` (centered), reports its single worst collision.
/// Returns empty when there are too few profiles/exemplars to judge reliably.
///
/// Pairs are scored by the MEAN centered cosine over all exemplar
/// combinations. An earlier version used the maximum, which is an
/// extreme-value statistic: the more exemplars a profile accumulates, the more
/// chances there are for some pair to clear the bar, so warnings multiplied
/// with ordinary use. Measured on a real store, mean max-score rose from 0.042
/// at one comparison to 0.410 at thirty-six purely as a function of count.
///
/// The trade-off is deliberate. Averaging dilutes a single contaminated
/// exemplar — exactly the Alice-holds-Camilia case — so this function will not
/// catch that, and is not meant to. Duplicate/corrupted exemplars are detected
/// separately and precisely by
/// `SpeakerProfilesRepository::duplicate_exemplars`, which compares individual
/// exemplars against a near-1.0 bar and can name the offending row. Keeping
/// the two apart lets each use the statistic that suits it: a maximum for
/// "these are literally the same recording", a mean for "these two voices are
/// genuinely hard to tell apart".
pub fn flag_confusable_profiles(profiles: &[(String, Vec<Vec<f32>>)]) -> Vec<ConfusableFlag> {
    if profiles.len() < 2 {
        return Vec::new();
    }
    let cohort: Vec<&[f32]> = profiles
        .iter()
        .flat_map(|(_, ex)| ex.iter().map(|e| e.as_slice()))
        .collect();
    if cohort.len() < MIN_EXEMPLARS_FOR_FLAGGING {
        return Vec::new();
    }
    let mean = match cohort_mean(&cohort) {
        Some(m) => m,
        None => return Vec::new(),
    };

    // Pre-center every profile's exemplars once.
    let centered: Vec<Vec<Vec<f32>>> = profiles
        .iter()
        .map(|(_, ex)| ex.iter().map(|e| center_normalized(e, &mean)).collect())
        .collect();

    let mut flags = Vec::new();
    for i in 0..profiles.len() {
        let mut worst: Option<(usize, f32)> = None;
        for j in 0..profiles.len() {
            if i == j {
                continue;
            }
            // Mean over every exemplar pair between profile i and profile j.
            // Independent of how many exemplars each side happens to hold.
            let mut acc = 0.0f32;
            let mut n = 0usize;
            for a in &centered[i] {
                for b in &centered[j] {
                    acc += cosine_similarity(a, b);
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            let score = acc / n as f32;
            if score >= CONFUSABLE_THRESHOLD && worst.map_or(true, |(_, w)| score > w) {
                worst = Some((j, score));
            }
        }
        if let Some((j, score)) = worst {
            flags.push(ConfusableFlag {
                name: profiles[i].0.clone(),
                confused_with: profiles[j].0.clone(),
                score,
            });
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.into_iter().map(|x| x / n).collect()
    }

    /// Number of dimensions in the synthetic voice space. Must exceed the
    /// highest axis any test uses.
    const DIMS: usize = 12;

    /// Anisotropic voice on a given axis (shared component + speaker bump).
    fn voice(axis: usize, jitter: f32) -> Vec<f32> {
        let mut v = vec![3.0f32; DIMS];
        v[axis] += 1.0 + jitter;
        unit(v)
    }

    #[test]
    fn distinct_voices_are_not_flagged() {
        // 8 clearly distinct voices (one axis each) -> none confusable.
        let profiles: Vec<(String, Vec<Vec<f32>>)> =
            (0..8).map(|i| (format!("P{i}"), vec![voice(i, 0.0)])).collect();
        assert!(flag_confusable_profiles(&profiles).is_empty());
    }

    #[test]
    fn genuinely_similar_voices_are_flagged() {
        // 8 distinct voices, plus "Bob" whose voice sits essentially on top of
        // Alice's. Consistently confusable, so both should be flagged.
        let mut profiles: Vec<(String, Vec<Vec<f32>>)> =
            (0..8).map(|i| (format!("P{i}"), vec![voice(i, 0.0)])).collect();
        profiles.push(("Alice".to_string(), vec![voice(9, 0.0)]));
        profiles.push(("Bob".to_string(), vec![voice(9, 0.02)]));

        let flags = flag_confusable_profiles(&profiles);
        assert!(
            flags.iter().any(|f| f.name == "Bob" && f.confused_with == "Alice"),
            "Bob should collide with Alice: {flags:?}"
        );
        assert!(
            flags.iter().any(|f| f.name == "Alice" && f.confused_with == "Bob"),
            "Alice should collide with Bob: {flags:?}"
        );
    }

    /// The reason this uses a mean rather than a maximum. Under max-linkage,
    /// giving a profile more exemplars raises its score against everyone,
    /// so warnings appear purely because the user recorded more meetings.
    #[test]
    fn score_does_not_inflate_with_exemplar_count() {
        let few: Vec<(String, Vec<Vec<f32>>)> = (0..8)
            .map(|i| (format!("P{i}"), vec![voice(i, 0.0)]))
            .collect();

        // Same eight distinct speakers, but each now carries six exemplars
        // with ordinary within-speaker jitter.
        let many: Vec<(String, Vec<Vec<f32>>)> = (0..8)
            .map(|i| {
                let exemplars = (0..6).map(|k| voice(i, k as f32 * 0.05)).collect();
                (format!("P{i}"), exemplars)
            })
            .collect();

        assert!(
            flag_confusable_profiles(&few).is_empty(),
            "distinct voices with one exemplar each should not be flagged"
        );
        assert!(
            flag_confusable_profiles(&many).is_empty(),
            "the same distinct voices should not become confusable just by \
             accumulating exemplars"
        );
    }

    /// A single contaminated exemplar is deliberately NOT this function's job —
    /// it is caught precisely by duplicate detection in the profile repository.
    /// Pinned so the separation of concerns is not silently undone.
    #[test]
    fn single_contaminated_exemplar_is_left_to_duplicate_detection() {
        let mut profiles: Vec<(String, Vec<Vec<f32>>)> =
            (0..8).map(|i| (format!("P{i}"), vec![voice(i, 0.0)])).collect();
        profiles.push(("Alice".to_string(), vec![voice(9, 0.0)]));
        // Bob is his own voice five times over, plus one stray copy of Alice.
        let mut bob: Vec<Vec<f32>> = (0..5).map(|k| voice(10, k as f32 * 0.05)).collect();
        bob.push(voice(9, 0.0));
        profiles.push(("Bob".to_string(), bob));

        let flags = flag_confusable_profiles(&profiles);
        assert!(
            !flags.iter().any(|f| f.name == "Bob" && f.confused_with == "Alice"),
            "one stray exemplar in six should be diluted here, not flagged: {flags:?}"
        );
    }

    #[test]
    fn too_few_exemplars_returns_empty() {
        let profiles = vec![
            ("A".to_string(), vec![voice(0, 0.0)]),
            ("B".to_string(), vec![voice(0, 0.02)]), // identical-ish but too few overall
        ];
        assert!(flag_confusable_profiles(&profiles).is_empty());
    }
}
