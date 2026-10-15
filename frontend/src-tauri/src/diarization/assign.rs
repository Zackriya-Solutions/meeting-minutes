//! Map diarization turns onto transcript rows and VAD segments.
use super::cluster::cosine;
use super::Turn;
use crate::api::TranscriptSegment;
use crate::audio::vad::SpeechSegment;
use std::collections::HashMap;

/// Shortest turn that counts as another speaker inside a row; shorter fragments are usually
/// diarization noise and join a neighbour.
pub const MIXED_MIN_SECONDS: f64 = 1.5;
pub const NEAREST_TURN_MAX_GAP_S: f64 = 1.0;
pub const MIN_PIECE_S: f64 = 0.3;
/// Shortest piece sent to a transcription engine when a row or segment is cut at speaker
/// changes: whisper.cpp returns no text for input under one second.
pub const MIN_TRANSCRIBED_PIECE_S: f64 = 1.0;
pub const CARRY_OVER_MIN_SIMILARITY: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowSpan {
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowLabel {
    Unlabeled,
    Single(String),
    /// A clear mid-row speaker change; `pieces` cover the row contiguously.
    Mixed { majority: String, pieces: Vec<Turn> },
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// The speaker with the most seconds inside the span (ties go to the smaller key); None when no
/// turn overlaps it.
fn majority_speaker(span: RowSpan, turns: &[Turn]) -> Option<&str> {
    let mut totals: HashMap<&str, f64> = HashMap::new();
    for t in turns {
        let o = overlap(span.start_s, span.end_s, t.start_s, t.end_s);
        if o > 0.0 {
            *totals.entry(t.key.as_str()).or_default() += o;
        }
    }
    totals
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(b.0.cmp(a.0)))
        .map(|(k, _)| k)
}

fn nearest_turn(span: RowSpan, turns: &[Turn]) -> Option<&Turn> {
    turns
        .iter()
        .map(|t| {
            let gap = if t.end_s <= span.start_s { span.start_s - t.end_s } else { t.start_s - span.end_s };
            (gap.max(0.0), t)
        })
        .filter(|(gap, _)| *gap <= NEAREST_TURN_MAX_GAP_S)
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, t)| t)
}

/// Contiguous single-speaker pieces covering `span`. Empty when no turn overlaps it.
pub fn pieces_for_span(span: RowSpan, turns: &[Turn]) -> Vec<Turn> {
    let mut pieces: Vec<Turn> = Vec::new();
    for t in turns {
        let start = t.start_s.max(span.start_s);
        let end = t.end_s.min(span.end_s);
        if end <= start {
            continue;
        }
        match pieces.last_mut() {
            Some(last) if last.key == t.key => last.end_s = end,
            _ => pieces.push(Turn { start_s: start, end_s: end, key: t.key.clone() }),
        }
    }
    let mut merged = fold_short_pieces(pieces, MIN_PIECE_S);
    // Close gaps so no audio falls between pieces, and stretch to the span edges.
    for i in 1..merged.len() {
        let prev_end = merged[i - 1].end_s;
        merged[i].start_s = prev_end;
    }
    if let Some(first) = merged.first_mut() {
        first.start_s = span.start_s;
    }
    if let Some(last) = merged.last_mut() {
        last.end_s = span.end_s;
    }
    merged
}

/// Fold pieces shorter than `min_s` into their predecessor (or successor for the first one) and
/// join neighbours with the same speaker. Contiguous input stays contiguous; only a span shorter
/// than `min_s` can leave a piece that short.
pub fn fold_short_pieces(pieces: Vec<Turn>, min_s: f64) -> Vec<Turn> {
    let mut merged: Vec<Turn> = Vec::with_capacity(pieces.len());
    for p in pieces {
        if p.duration() < min_s {
            if let Some(last) = merged.last_mut() {
                last.end_s = p.end_s;
                continue;
            }
        }
        match merged.last_mut() {
            Some(last) if last.key == p.key => last.end_s = p.end_s,
            Some(last) if last.duration() < min_s => {
                *last = Turn { start_s: last.start_s, end_s: p.end_s, key: p.key.clone() };
            }
            _ => merged.push(p),
        }
    }
    merged
}

/// How far (in seconds of the row's average speaking rate) a text cut may move from the
/// estimated speaker change to land on a sentence end, or on a clause mark.
const SENTENCE_CUT_TOLERANCE_S: f64 = 2.0;
const CLAUSE_CUT_TOLERANCE_S: f64 = 1.0;

fn ends_sentence(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | '…' | '。' | '？' | '！')
}

fn ends_clause(c: char) -> bool {
    matches!(c, ',' | ';' | ':' | '，' | '、' | '；' | '：')
}

fn is_wide_punctuation(c: char) -> bool {
    matches!(c, '。' | '？' | '！' | '，' | '、' | '；' | '：')
}

/// Chinese and Japanese characters, which are written without spaces between words.
fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

/// Splits a row's existing text among its pieces without re-transcribing. Each cut goes where
/// the speaker changes, estimated from the row's average speaking rate, moved to the nearest
/// sentence end within SENTENCE_CUT_TOLERANCE_S, else the nearest clause mark within
/// CLAUSE_CUT_TOLERANCE_S, else the nearest word gap. Chinese and Japanese text, which has no
/// word gaps, may also be cut next to any of its characters. None when a piece would be empty.
pub fn split_text_at_turns(text: &str, span: RowSpan, pieces: &[Turn]) -> Option<Vec<String>> {
    let chars: Vec<char> = text.trim().chars().collect();
    let len = chars.len();
    let duration = span.end_s - span.start_s;
    if pieces.len() < 2 || len == 0 || duration <= 0.0 {
        return None;
    }
    // A cut at p puts chars[..p] in one piece and chars[p..] in the next.
    let after_mark = |p: usize, mark: fn(char) -> bool| {
        mark(chars[p - 1]) && (chars[p].is_whitespace() || is_wide_punctuation(chars[p - 1]))
    };
    let word_start = |p: usize| {
        (chars[p - 1].is_whitespace() && !chars[p].is_whitespace()) || is_cjk(chars[p - 1]) || is_cjk(chars[p])
    };
    let rate = len as f64 / duration;

    let mut cuts = Vec::with_capacity(pieces.len() - 1);
    let mut prev = 0usize;
    for piece in &pieces[..pieces.len() - 1] {
        let target = ((piece.end_s - span.start_s) / duration * len as f64).clamp(0.0, len as f64);
        let nearest = |ok: &dyn Fn(usize) -> bool, tolerance: f64| {
            (prev + 1..len)
                .filter(|&p| ok(p) && (p as f64 - target).abs() <= tolerance)
                .min_by(|&a, &b| (a as f64 - target).abs().total_cmp(&(b as f64 - target).abs()))
        };
        let cut = nearest(&|p| after_mark(p, ends_sentence), SENTENCE_CUT_TOLERANCE_S * rate)
            .or_else(|| nearest(&|p| after_mark(p, ends_clause), CLAUSE_CUT_TOLERANCE_S * rate))
            .or_else(|| nearest(&word_start, f64::INFINITY))?;
        cuts.push(cut);
        prev = cut;
    }
    cuts.push(len);

    let mut start = 0;
    let mut out = Vec::with_capacity(cuts.len());
    for cut in cuts {
        let piece: String = chars[start..cut].iter().collect::<String>().trim().to_string();
        if piece.is_empty() {
            return None;
        }
        out.push(piece);
        start = cut;
    }
    Some(out)
}

/// Label each row (None = row has no audio timing) by majority overlap. A row is mixed when, after
/// folding fragments shorter than MIXED_MIN_SECONDS into their neighbours, more than one speaker
/// still has a turn in it; those turns are its pieces.
pub fn label_rows(rows: &[Option<RowSpan>], turns: &[Turn]) -> Vec<RowLabel> {
    rows.iter()
        .map(|row| {
            let Some(span) = *row else { return RowLabel::Unlabeled };
            let Some(majority) = majority_speaker(span, turns) else {
                return nearest_turn(span, turns)
                    .map(|t| RowLabel::Single(t.key.clone()))
                    .unwrap_or(RowLabel::Unlabeled);
            };
            let pieces = fold_short_pieces(pieces_for_span(span, turns), MIXED_MIN_SECONDS);
            if pieces.len() > 1 {
                return RowLabel::Mixed { majority: majority.to_string(), pieces };
            }
            RowLabel::Single(majority.to_string())
        })
        .collect()
}

/// Cut VAD segments at speaker changes so each piece carries one speaker. Pieces are at least
/// `MIN_TRANSCRIBED_PIECE_S` long, because each one is transcribed on its own.
pub fn split_segments_at_turns(segments: Vec<SpeechSegment>, turns: &[Turn], sample_rate: usize) -> Vec<SpeechSegment> {
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments {
        let span = RowSpan { start_s: seg.start_timestamp_ms / 1000.0, end_s: seg.end_timestamp_ms / 1000.0 };
        let pieces = fold_short_pieces(pieces_for_span(span, turns), MIN_TRANSCRIBED_PIECE_S);
        if pieces.len() <= 1 {
            out.push(seg);
            continue;
        }
        let to_index = |t_s: f64| -> usize {
            (((t_s - span.start_s) * sample_rate as f64).round().max(0.0) as usize).min(seg.samples.len())
        };
        for p in &pieces {
            let (a, b) = (to_index(p.start_s), to_index(p.end_s));
            if b <= a {
                continue;
            }
            out.push(SpeechSegment {
                samples: seg.samples[a..b].to_vec(),
                start_timestamp_ms: p.start_s * 1000.0,
                end_timestamp_ms: p.end_s * 1000.0,
                confidence: seg.confidence,
            });
        }
    }
    out
}

/// Set each segment's speaker by majority overlap (segments already split per speaker).
pub fn label_segments(segments: &mut [TranscriptSegment], turns: &[Turn]) {
    let spans: Vec<Option<RowSpan>> = segments
        .iter()
        .map(|s| match (s.audio_start_time, s.audio_end_time) {
            (Some(a), Some(b)) => Some(RowSpan { start_s: a, end_s: b }),
            _ => None,
        })
        .collect();
    for (seg, label) in segments.iter_mut().zip(label_rows(&spans, turns)) {
        seg.speaker = match label {
            RowLabel::Unlabeled => None,
            RowLabel::Single(k) | RowLabel::Mixed { majority: k, .. } => Some(k),
        };
    }
}

/// Greedy one-to-one pairing, best score first. `pairs` holds (score, left index, right index)
/// with indices below `left` and `right`; each index is used at most once. Returns the kept
/// pairs, best first. Equal scores keep their input order.
pub fn greedy_pairs(mut pairs: Vec<(f32, usize, usize)>, left: usize, right: usize) -> Vec<(f32, usize, usize)> {
    pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut used_left = vec![false; left];
    let mut used_right = vec![false; right];
    pairs
        .into_iter()
        .filter(|&(_, i, j)| {
            if used_left[i] || used_right[j] {
                return false;
            }
            used_left[i] = true;
            used_right[j] = true;
            true
        })
        .collect()
}

/// Greedy one-to-one match of new speakers to the meeting's previous speakers by voice
/// similarity, highest first. `new` holds (key, centroid); `old` holds the previous centroids.
/// Every previous voice takes part, named or not, so a name cannot move onto another voice.
/// Returns new key → index into `old`.
pub fn carry_over(new: &[(String, Vec<f32>)], old: &[Vec<f32>], min_similarity: f32) -> HashMap<String, usize> {
    let mut pairs: Vec<(f32, usize, usize)> = Vec::new();
    for (i, (_, ne)) in new.iter().enumerate() {
        for (j, oe) in old.iter().enumerate() {
            let s = cosine(ne, oe);
            if s >= min_similarity {
                pairs.push((s, i, j));
            }
        }
    }
    greedy_pairs(pairs, new.len(), old.len())
        .into_iter()
        .map(|(_, i, j)| (new[i].0.clone(), j))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::vad::SpeechSegment;

    fn turn(s: f64, e: f64, k: &str) -> Turn {
        Turn { start_s: s, end_s: e, key: k.to_string() }
    }
    fn span(s: f64, e: f64) -> Option<RowSpan> {
        Some(RowSpan { start_s: s, end_s: e })
    }

    fn split(text: &str, end_s: f64, changes: &[f64]) -> Option<Vec<String>> {
        let mut bounds = vec![0.0];
        bounds.extend_from_slice(changes);
        bounds.push(end_s);
        let pieces: Vec<Turn> = bounds.windows(2).enumerate().map(|(i, w)| turn(w[0], w[1], &format!("spk_{}", i % 2))).collect();
        split_text_at_turns(text, RowSpan { start_s: 0.0, end_s }, &pieces)
    }

    #[test]
    fn text_split_cuts_at_the_sentence_nearest_the_change() {
        let text = "Hello there, how are you doing today? I am fine thanks for asking.";
        assert_eq!(
            split(text, 10.0, &[5.0]).unwrap(),
            vec!["Hello there, how are you doing today?", "I am fine thanks for asking."]
        );
    }

    #[test]
    fn text_split_prefers_a_sentence_end_over_a_closer_comma() {
        let text = "We should ship it, I think. Sure, let's do it.";
        assert_eq!(split(text, 10.0, &[4.5]).unwrap(), vec!["We should ship it, I think.", "Sure, let's do it."]);
    }

    #[test]
    fn text_split_falls_back_to_the_nearest_word_gap() {
        let text = "one two three four five six seven eight";
        assert_eq!(split(text, 10.0, &[5.0]).unwrap(), vec!["one two three four", "five six seven eight"]);
    }

    #[test]
    fn text_split_handles_several_changes() {
        let text = "First one here. Second one here. Third one here.";
        assert_eq!(
            split(text, 10.0, &[3.3, 6.7]).unwrap(),
            vec!["First one here.", "Second one here.", "Third one here."]
        );
    }

    #[test]
    fn text_split_cuts_text_without_word_gaps_at_its_punctuation() {
        assert_eq!(split("你好吗？我很好。", 4.0, &[2.0]).unwrap(), vec!["你好吗？", "我很好。"]);
    }

    #[test]
    fn text_split_refuses_text_it_cannot_cut() {
        assert_eq!(split("Yes.", 4.0, &[2.0]), None);
        assert_eq!(split("", 4.0, &[2.0]), None);
        assert_eq!(split("Only one piece.", 4.0, &[]), None);
    }

    #[test]
    fn majority_speaker_labels_a_row() {
        let turns = vec![turn(0.0, 4.0, "spk_0"), turn(4.0, 5.0, "spk_1")];
        // 4 s vs 1 s: second speaker below 1.5 s, so not mixed.
        assert_eq!(label_rows(&[span(0.0, 5.0)], &turns), vec![RowLabel::Single("spk_0".into())]);
    }

    #[test]
    fn clear_mid_row_change_marks_row_mixed_with_covering_pieces() {
        let turns = vec![turn(0.0, 3.0, "spk_0"), turn(3.2, 6.0, "spk_1")];
        let labels = label_rows(&[span(0.5, 6.0)], &turns);
        match &labels[0] {
            RowLabel::Mixed { majority, pieces } => {
                assert_eq!(majority, "spk_1");
                assert_eq!(pieces.len(), 2);
                assert_eq!(pieces[0].start_s, 0.5);
                assert_eq!(pieces[0].end_s, pieces[1].start_s, "pieces are contiguous");
                assert_eq!(pieces[1].end_s, 6.0);
                assert_eq!(pieces[0].key, "spk_0");
            }
            other => panic!("expected mixed, got {other:?}"),
        }
    }

    #[test]
    fn a_short_reply_in_a_long_row_marks_it_mixed() {
        // 5.7 s of spk_3 at the end of a 24 s row (24 %) is a real turn.
        let turns = vec![turn(70.0, 88.8, "spk_0"), turn(88.8, 94.5, "spk_3")];
        match &label_rows(&[span(70.7, 94.5)], &turns)[0] {
            RowLabel::Mixed { majority, pieces } => {
                assert_eq!(majority, "spk_0");
                assert_eq!(pieces.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), vec!["spk_0", "spk_3"]);
            }
            other => panic!("expected mixed, got {other:?}"),
        }
    }

    #[test]
    fn every_turn_of_a_busy_row_becomes_a_piece() {
        let turns = vec![turn(20.4, 22.3, "spk_1"), turn(22.3, 24.7, "spk_2"), turn(24.7, 32.9, "spk_3"), turn(32.9, 47.8, "spk_0")];
        let RowLabel::Mixed { majority, pieces } = &label_rows(&[span(20.4, 47.8)], &turns)[0] else {
            panic!("expected mixed");
        };
        assert_eq!(majority, "spk_0");
        assert_eq!(pieces.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), vec!["spk_1", "spk_2", "spk_3", "spk_0"]);
    }

    #[test]
    fn scattered_blips_of_another_voice_do_not_mark_a_row_mixed() {
        // spk_1 totals 2 s, but in 1 s fragments: no turn of its own.
        let turns = vec![
            turn(0.0, 3.0, "spk_0"),
            turn(3.0, 4.0, "spk_1"),
            turn(4.0, 7.0, "spk_0"),
            turn(7.0, 8.0, "spk_1"),
            turn(8.0, 10.0, "spk_0"),
        ];
        assert_eq!(label_rows(&[span(0.0, 10.0)], &turns), vec![RowLabel::Single("spk_0".into())]);
    }

    #[test]
    fn row_without_overlap_takes_nearest_turn_within_one_second() {
        let turns = vec![turn(0.0, 1.0, "spk_0"), turn(10.0, 11.0, "spk_1")];
        let labels = label_rows(&[span(1.5, 2.0), span(5.0, 6.0)], &turns);
        assert_eq!(labels, vec![RowLabel::Single("spk_0".into()), RowLabel::Unlabeled]);
    }

    #[test]
    fn identify_rows_without_timing_stay_unlabelled() {
        let turns = vec![turn(0.0, 10.0, "spk_0")];
        assert_eq!(label_rows(&[None, span(1.0, 2.0)], &turns), vec![RowLabel::Unlabeled, RowLabel::Single("spk_0".into())]);
    }

    #[test]
    fn tiny_pieces_join_a_neighbour() {
        let turns = vec![turn(0.0, 3.0, "spk_0"), turn(3.0, 3.1, "spk_2"), turn(3.1, 6.0, "spk_1")];
        let pieces = pieces_for_span(RowSpan { start_s: 0.0, end_s: 6.0 }, &turns);
        assert_eq!(pieces.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), vec!["spk_0", "spk_1"]);
    }

    #[test]
    fn vad_segments_are_cut_at_speaker_changes() {
        let seg = SpeechSegment { samples: vec![0.1; 16000 * 4], start_timestamp_ms: 1000.0, end_timestamp_ms: 5000.0, confidence: 0.9 };
        let turns = vec![turn(0.0, 3.0, "spk_0"), turn(3.0, 9.0, "spk_1")];
        let out = split_segments_at_turns(vec![seg], &turns, 16000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].samples.len(), 16000 * 2);
        assert_eq!(out[1].samples.len(), 16000 * 2);
        assert_eq!(out[0].end_timestamp_ms, 3000.0);
        assert_eq!(out[1].start_timestamp_ms, 3000.0);
    }

    #[test]
    fn vad_segment_pieces_are_never_shorter_than_a_second() {
        let seg = |s: f64, e: f64| SpeechSegment {
            samples: vec![0.1; ((e - s) * 16000.0).round() as usize],
            start_timestamp_ms: s * 1000.0,
            end_timestamp_ms: e * 1000.0,
            confidence: 0.9,
        };
        let turns = vec![
            turn(0.0, 3.0, "spk_0"),
            turn(3.0, 3.6, "spk_1"),
            turn(3.6, 6.0, "spk_2"),
            turn(10.0, 12.6, "spk_0"),
            turn(12.6, 13.3, "spk_1"),
            turn(13.3, 16.0, "spk_0"),
        ];
        let out = split_segments_at_turns(vec![seg(0.0, 6.0), seg(10.0, 16.0)], &turns, 16000);
        for piece in &out {
            let seconds = (piece.end_timestamp_ms - piece.start_timestamp_ms) / 1000.0;
            assert!(seconds >= MIN_TRANSCRIBED_PIECE_S, "piece {:?} is {seconds}s", (piece.start_timestamp_ms, piece.end_timestamp_ms));
            assert_eq!(piece.samples.len(), (seconds * 16000.0).round() as usize);
        }
        let bounds: Vec<(f64, f64)> = out.iter().map(|p| (p.start_timestamp_ms, p.end_timestamp_ms)).collect();
        assert_eq!(bounds, vec![(0.0, 3600.0), (3600.0, 6000.0), (10000.0, 16000.0)]);
        assert_eq!(out.iter().map(|p| p.samples.len()).sum::<usize>(), 2 * 6 * 16000, "no audio is dropped");
    }

    #[test]
    fn segment_shorter_than_a_second_is_not_cut() {
        let seg = SpeechSegment { samples: vec![0.1; 12800], start_timestamp_ms: 0.0, end_timestamp_ms: 800.0, confidence: 0.9 };
        let out = split_segments_at_turns(vec![seg], &[turn(0.0, 0.4, "spk_0"), turn(0.4, 0.8, "spk_1")], 16000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].samples.len(), 12800);
    }

    #[test]
    fn short_pieces_fold_into_a_neighbour_at_the_transcription_minimum() {
        let pieces = vec![turn(0.0, 0.6, "spk_1"), turn(0.6, 3.0, "spk_0"), turn(3.0, 3.7, "spk_1"), turn(3.7, 6.0, "spk_2")];
        let folded = fold_short_pieces(pieces, MIN_TRANSCRIBED_PIECE_S);
        assert_eq!(folded, vec![turn(0.0, 3.7, "spk_0"), turn(3.7, 6.0, "spk_2")]);
    }

    #[test]
    fn single_speaker_segment_is_untouched() {
        let seg = SpeechSegment { samples: vec![0.1; 1600], start_timestamp_ms: 0.0, end_timestamp_ms: 100.0, confidence: 0.9 };
        let out = split_segments_at_turns(vec![seg.clone()], &[turn(0.0, 1.0, "spk_0")], 16000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].samples.len(), seg.samples.len());
    }

    #[test]
    fn names_carry_over_to_best_matching_new_speaker() {
        let new = vec![("spk_0".to_string(), vec![0.0, 1.0]), ("spk_1".to_string(), vec![1.0, 0.1])];
        let old = vec![vec![1.0, 0.0], vec![-1.0, 0.0]];
        let matched = carry_over(&new, &old, CARRY_OVER_MIN_SIMILARITY);
        assert_eq!(matched.get("spk_1"), Some(&0));
        assert_eq!(matched.get("spk_0"), None, "the second old voice is below the similarity floor");
    }

    #[test]
    fn unnamed_old_voice_keeps_name_from_moving() {
        // Old voice 0 is unnamed (cosine 0.92 with spk_0); old voice 1 is Noah (0.66 with spk_0,
        // 0.63 with spk_1). Ignoring the unnamed voice would hand Noah's name to spk_0.
        let new = vec![("spk_0".to_string(), vec![0.92, 0.3919]), ("spk_1".to_string(), vec![-0.5405, 0.8413])];
        let old = vec![vec![1.0, 0.0], vec![0.3129, 0.9498]];
        let matched = carry_over(&new, &old, CARRY_OVER_MIN_SIMILARITY);
        assert_eq!(matched.get("spk_0"), Some(&0));
        assert_eq!(matched.get("spk_1"), Some(&1));
    }

    #[test]
    fn greedy_pairs_take_the_best_first_one_to_one() {
        // (0, 1) wins at 0.9, which rules out (1, 1) and (0, 0); (1, 0) is what is left.
        let pairs = vec![(0.7, 0, 0), (0.9, 0, 1), (0.8, 1, 1), (0.6, 1, 0)];
        assert_eq!(greedy_pairs(pairs, 2, 2), vec![(0.9, 0, 1), (0.6, 1, 0)]);
        assert!(greedy_pairs(Vec::new(), 3, 3).is_empty());
    }
}
