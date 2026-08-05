//! Stage 1 of the Deep Analytics pipeline: deterministic conversation analytics.
//! No LLM — pure functions over the ordered transcript segments, so they are cheap,
//! reproducible, and unit-testable.
//!
//! Transcript segments only reliably carry a START time (`audio_start_time`), so per-
//! segment spoken duration is estimated from word count (bounded by the next segment's
//! start) and pauses are the gaps between an utterance's estimated end and the next
//! utterance's start. When no segment has a real timestamp (live / unsaved meetings) a
//! synthetic cumulative timeline is built so the report still renders.

/// Estimated speaking rate. ~150 words/min ≈ 2.5 words/sec.
const WORDS_PER_SEC: f64 = 2.5;
/// Floor for a single utterance's spoken duration (seconds).
const MIN_UTTERANCE_SECS: f64 = 0.5;
/// Pause thresholds (seconds).
const PAUSE_SHORT: f64 = 3.0;
const PAUSE_LONG: f64 = 10.0;

/// One transcript segment reduced to what the analytics need. `speaker_key` groups
/// segments that belong to the same speaker (stable across the meeting); `speaker_label`
/// is the human-facing name shown in the report.
#[derive(Debug, Clone)]
pub struct DynSegment {
    pub start: Option<f64>,
    pub text: String,
    pub speaker_key: String,
    pub speaker_label: String,
}

/// A segment placed on a concrete timeline (`start`/`end` in seconds). Parallel to the
/// input `DynSegment` slice (same length, same order) — index `i` here is transcript
/// anchor `#t{i}` in the rendered HTML.
#[derive(Debug, Clone)]
pub struct TimedSegment {
    pub start: f64,
    pub end: f64,
    pub speaker_key: String,
}

/// Per-speaker rollup, sorted most-talkative first. `palette_index` assigns color slots
/// (0..=3 -> `--s1`..`--s4`, >=4 -> muted gray) by talk-time order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpeakerDyn {
    pub key: String,
    pub label: String,
    pub talk_secs: f64,
    pub talk_share: f64,
    pub questions: usize,
    pub turns: usize,
    pub palette_index: usize,
}

/// Whole-meeting deterministic analytics.
///
/// `Deserialize` is here so the meeting screen can read these metrics back out of a
/// completed report's artifacts snapshot (see [`crate::report::sections`]).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Dynamics {
    pub duration_secs: f64,
    /// Fraction of wall-clock time that was speech (0..1).
    pub speech_density: f64,
    /// Merged turns: consecutive same-speaker segments collapse into one turn.
    pub turn_count: usize,
    pub total_questions: usize,
    /// Gaps strictly greater than 3s (includes the >10s ones).
    pub pauses_over_3s: usize,
    /// Gaps strictly greater than 10s (subset of `pauses_over_3s`).
    pub pauses_over_10s: usize,
    pub speakers: Vec<SpeakerDyn>,
}

fn estimate_dur(text: &str) -> f64 {
    let words = text.split_whitespace().count();
    ((words as f64) / WORDS_PER_SEC).max(MIN_UTTERANCE_SECS)
}

/// Build a concrete timeline from segments that only carry start times. Returned vector
/// is parallel to `segments`.
pub fn timeline(segments: &[DynSegment]) -> Vec<TimedSegment> {
    let n = segments.len();
    if n == 0 {
        return Vec::new();
    }
    let ests: Vec<f64> = segments.iter().map(|s| estimate_dur(&s.text)).collect();

    let any_real = segments
        .iter()
        .any(|s| s.start.is_some_and(|t| t.is_finite() && t >= 0.0));

    let mut starts = vec![0.0_f64; n];
    if any_real {
        let mut last = 0.0_f64;
        for i in 0..n {
            let t = segments[i]
                .start
                .filter(|t| t.is_finite() && *t >= 0.0)
                .unwrap_or(last)
                .max(last); // enforce monotonic non-decreasing
            starts[i] = t;
            last = t;
        }
    } else {
        let mut cum = 0.0_f64;
        for i in 0..n {
            starts[i] = cum;
            cum += ests[i];
        }
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let end = if i + 1 < n && starts[i + 1] > starts[i] {
            // bounded by the next utterance's start; at least a thin sliver
            let next = starts[i + 1];
            (starts[i] + ests[i]).min(next).max(starts[i])
        } else {
            starts[i] + ests[i]
        };
        out.push(TimedSegment {
            start: starts[i],
            end,
            speaker_key: segments[i].speaker_key.clone(),
        });
    }
    out
}

impl Dynamics {
    pub fn compute(segments: &[DynSegment]) -> Dynamics {
        let timed = timeline(segments);
        Self::from_timed(segments, &timed)
    }

    /// Compute from a pre-built timeline (avoids recomputing it when the caller already
    /// has one for rendering).
    pub fn from_timed(segments: &[DynSegment], timed: &[TimedSegment]) -> Dynamics {
        let n = segments.len();
        if n == 0 {
            return Dynamics {
                duration_secs: 0.0,
                speech_density: 0.0,
                turn_count: 0,
                total_questions: 0,
                pauses_over_3s: 0,
                pauses_over_10s: 0,
                speakers: Vec::new(),
            };
        }

        // Per-speaker accumulation, preserving first-appearance order for deterministic
        // tie-breaking.
        struct Acc {
            key: String,
            label: String,
            talk_secs: f64,
            questions: usize,
            turns: usize,
            order: usize,
        }
        let mut accs: Vec<Acc> = Vec::new();
        let mut index_of = std::collections::HashMap::<String, usize>::new();

        let mut prev_key: Option<&str> = None;
        let mut turn_count = 0usize;
        let mut total_questions = 0usize;

        for (i, seg) in segments.iter().enumerate() {
            let idx = *index_of.entry(seg.speaker_key.clone()).or_insert_with(|| {
                let order = accs.len();
                accs.push(Acc {
                    key: seg.speaker_key.clone(),
                    label: seg.speaker_label.clone(),
                    talk_secs: 0.0,
                    questions: 0,
                    turns: 0,
                    order,
                });
                order
            });

            let dur = (timed[i].end - timed[i].start).max(0.0);
            accs[idx].talk_secs += dur;

            let q = seg.text.matches('?').count();
            accs[idx].questions += q;
            total_questions += q;

            let is_new_turn = prev_key != Some(seg.speaker_key.as_str());
            if is_new_turn {
                turn_count += 1;
                accs[idx].turns += 1;
            }
            prev_key = Some(seg.speaker_key.as_str());
        }

        // Pauses: gaps between an utterance's end and the next utterance's start.
        let mut pauses_over_3s = 0usize;
        let mut pauses_over_10s = 0usize;
        for i in 0..n.saturating_sub(1) {
            let gap = timed[i + 1].start - timed[i].end;
            if gap > PAUSE_SHORT {
                pauses_over_3s += 1;
            }
            if gap > PAUSE_LONG {
                pauses_over_10s += 1;
            }
        }

        let first_start = timed.first().map(|t| t.start).unwrap_or(0.0);
        let last_end = timed.last().map(|t| t.end).unwrap_or(0.0);
        let duration_secs = (last_end - first_start).max(0.0);
        let total_talk: f64 = accs.iter().map(|a| a.talk_secs).sum();
        let speech_density = if duration_secs > 0.0 {
            (total_talk / duration_secs).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Sort by talk time desc, tie-break by first appearance for stable colors.
        let mut order: Vec<usize> = (0..accs.len()).collect();
        order.sort_by(|&a, &b| {
            accs[b]
                .talk_secs
                .partial_cmp(&accs[a].talk_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(accs[a].order.cmp(&accs[b].order))
        });

        let speakers = order
            .into_iter()
            .enumerate()
            .map(|(palette_index, idx)| {
                let a = &accs[idx];
                SpeakerDyn {
                    key: a.key.clone(),
                    label: a.label.clone(),
                    talk_secs: a.talk_secs,
                    talk_share: if total_talk > 0.0 {
                        a.talk_secs / total_talk
                    } else {
                        0.0
                    },
                    questions: a.questions,
                    turns: a.turns,
                    palette_index,
                }
            })
            .collect();

        Dynamics {
            duration_secs,
            speech_density,
            turn_count,
            total_questions,
            pauses_over_3s,
            pauses_over_10s,
            speakers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, key: &str, label: &str, text: &str) -> DynSegment {
        DynSegment {
            start: Some(start),
            text: text.to_string(),
            speaker_key: key.to_string(),
            speaker_label: label.to_string(),
        }
    }

    #[test]
    fn merged_turns_collapse_consecutive_same_speaker() {
        let segs = vec![
            seg(0.0, "a", "A", "раз"),
            seg(2.0, "a", "A", "два"),
            seg(4.0, "b", "B", "три"),
            seg(6.0, "a", "A", "четыре"),
        ];
        let d = Dynamics::compute(&segs);
        // A,A -> 1 turn, B -> 1 turn, A -> 1 turn = 3 merged turns.
        assert_eq!(d.turn_count, 3);
        let a = d.speakers.iter().find(|s| s.key == "a").unwrap();
        let b = d.speakers.iter().find(|s| s.key == "b").unwrap();
        assert_eq!(a.turns, 2);
        assert_eq!(b.turns, 1);
    }

    #[test]
    fn questions_counted_per_speaker_and_total() {
        let segs = vec![
            seg(0.0, "a", "A", "Готово? Точно?"),
            seg(3.0, "b", "B", "Да."),
            seg(5.0, "a", "A", "А когда?"),
        ];
        let d = Dynamics::compute(&segs);
        assert_eq!(d.total_questions, 3);
        let a = d.speakers.iter().find(|s| s.key == "a").unwrap();
        assert_eq!(a.questions, 3);
        let b = d.speakers.iter().find(|s| s.key == "b").unwrap();
        assert_eq!(b.questions, 0);
    }

    #[test]
    fn pauses_detect_short_and_long_gaps() {
        // Short utterances (est ~0.5s each). Gaps chosen around the thresholds.
        let segs = vec![
            seg(0.0, "a", "A", "тут"),    // end ~0.5
            seg(5.0, "b", "B", "там"),    // gap ~4.5s  -> >3s
            seg(6.0, "a", "A", "ага"),    // gap ~0.5s  -> none
            seg(20.0, "b", "B", "конец"), // gap ~13.5s -> >3s AND >10s
        ];
        let d = Dynamics::compute(&segs);
        assert_eq!(d.pauses_over_3s, 2);
        assert_eq!(d.pauses_over_10s, 1);
    }

    #[test]
    fn talk_share_orders_speakers_and_assigns_palette() {
        // B speaks far more words -> larger talk share -> palette slot 0.
        let segs = vec![
            seg(0.0, "a", "A", "коротко"),
            seg(
                10.0,
                "b",
                "B",
                "это длинная реплика в которой очень много слов подряд идёт и идёт и идёт",
            ),
        ];
        let d = Dynamics::compute(&segs);
        assert_eq!(d.speakers[0].key, "b");
        assert_eq!(d.speakers[0].palette_index, 0);
        assert_eq!(d.speakers[1].palette_index, 1);
        let total: f64 = d.speakers.iter().map(|s| s.talk_share).sum();
        assert!((total - 1.0).abs() < 1e-6, "shares sum to 1");
    }

    #[test]
    fn empty_input_is_safe() {
        let d = Dynamics::compute(&[]);
        assert_eq!(d.turn_count, 0);
        assert_eq!(d.duration_secs, 0.0);
        assert!(d.speakers.is_empty());
    }

    #[test]
    fn synthetic_timeline_when_no_timestamps() {
        let segs = vec![
            DynSegment {
                start: None,
                text: "первое".into(),
                speaker_key: "a".into(),
                speaker_label: "A".into(),
            },
            DynSegment {
                start: None,
                text: "второе".into(),
                speaker_key: "b".into(),
                speaker_label: "B".into(),
            },
        ];
        let timed = timeline(&segs);
        assert_eq!(timed.len(), 2);
        assert!(
            timed[1].start >= timed[0].end - 1e-9,
            "second follows first"
        );
        let d = Dynamics::from_timed(&segs, &timed);
        assert!(d.duration_secs > 0.0);
    }
}
