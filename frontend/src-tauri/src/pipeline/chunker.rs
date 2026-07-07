//! Deterministic transcript chunker (PLAN.md Phase 1).
//!
//! Groups ordered segments into retrieval chunks of ~200–400 tokens with a small
//! segment overlap. Never splits inside a segment (so timestamps stay exact), and is
//! deterministic — the same input always yields the same chunks, which is what makes
//! backfill (Phase 5) idempotent.
//!
//! Token counting is pluggable: the real embedder tokenizer is passed in from Phase 1's
//! embedder; [`approx_token_count`] is the default placeholder until that is wired.

/// An input segment (a row from `transcripts`). Timing is milliseconds — callers
/// convert from `transcripts.audio_start_time` seconds (× 1000).
#[derive(Debug, Clone)]
pub struct Segment {
    pub id: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// An output chunk, ready to insert into the `chunks` table.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub first_segment_id: String,
    pub last_segment_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub token_count: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkConfig {
    /// Soft lower bound: don't close a chunk below this unless input is exhausted.
    pub min_tokens: usize,
    /// Soft upper bound: close the chunk before exceeding this (but always emit at
    /// least one whole segment, even if that single segment is larger).
    pub max_tokens: usize,
    /// Number of trailing segments re-included at the start of the next chunk.
    pub overlap_segments: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            min_tokens: 200,
            max_tokens: 400,
            overlap_segments: 1,
        }
    }
}

/// Placeholder token counter: whitespace-delimited word count. Deterministic and
/// language-agnostic; replaced by the embedder's real tokenizer in Phase 1.
pub fn approx_token_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Chunk `segments` (assumed already ordered by start time) using `count_tokens` to
/// measure length. Returns chunks in order.
pub fn chunk_segments(
    segments: &[Segment],
    config: &ChunkConfig,
    count_tokens: impl Fn(&str) -> usize,
) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    if segments.is_empty() {
        return chunks;
    }

    // Precompute per-segment token counts once.
    let seg_tokens: Vec<usize> = segments.iter().map(|s| count_tokens(&s.text)).collect();

    let mut start = 0usize;
    while start < segments.len() {
        let mut end = start; // exclusive upper bound, grown below
        let mut tokens = 0usize;

        // Always include at least one segment (never split a segment).
        while end < segments.len() {
            let next = seg_tokens[end];
            // Stop before exceeding max, but only once we have >= min tokens and at
            // least one segment already included.
            if end > start && tokens + next > config.max_tokens && tokens >= config.min_tokens {
                break;
            }
            tokens += next;
            end += 1;
            // If we've reached max after including a segment, close here.
            if tokens >= config.max_tokens {
                break;
            }
        }

        let slice = &segments[start..end];
        let text = slice
            .iter()
            .map(|s| s.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        chunks.push(Chunk {
            first_segment_id: slice[0].id.clone(),
            last_segment_id: slice[slice.len() - 1].id.clone(),
            start_ms: slice[0].start_ms,
            end_ms: slice[slice.len() - 1].end_ms,
            text,
            token_count: tokens,
        });

        if end >= segments.len() {
            break;
        }

        // Advance with overlap, but guarantee forward progress (avoid infinite loops
        // when a chunk is as short as the overlap window).
        let next_start = end.saturating_sub(config.overlap_segments);
        start = next_start.max(start + 1);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(id: &str, text: &str, start_ms: i64, end_ms: i64) -> Segment {
        Segment { id: id.into(), text: text.into(), start_ms, end_ms }
    }

    // One "word" == one token under approx_token_count; build segments of known size.
    fn words(n: usize) -> String {
        vec!["w"; n].join(" ")
    }

    #[test]
    fn empty_input_yields_no_chunks() {
        let chunks = chunk_segments(&[], &ChunkConfig::default(), approx_token_count);
        assert!(chunks.is_empty());
    }

    #[test]
    fn respects_min_max_and_never_splits_segments() {
        let cfg = ChunkConfig { min_tokens: 20, max_tokens: 40, overlap_segments: 1 };
        // 6 segments of 15 tokens each = 90 tokens total.
        let segs: Vec<Segment> = (0..6)
            .map(|i| seg(&format!("s{i}"), &words(15), i as i64 * 1000, (i as i64 + 1) * 1000))
            .collect();
        let chunks = chunk_segments(&segs, &cfg, approx_token_count);

        assert!(!chunks.is_empty());
        for c in &chunks {
            // Each chunk holds whole segments (multiples of 15 tokens) and respects max
            // once min is met.
            assert_eq!(c.token_count % 15, 0, "chunks are whole segments");
            assert!(c.token_count <= cfg.max_tokens || c.token_count == 15);
        }
        // First chunk: 15 (<min) + 15 = 30 (>=min), adding a 3rd (45) would exceed max.
        assert_eq!(chunks[0].token_count, 30);
        assert_eq!(chunks[0].first_segment_id, "s0");
        assert_eq!(chunks[0].last_segment_id, "s1");
    }

    #[test]
    fn overlap_reincludes_trailing_segment() {
        let cfg = ChunkConfig { min_tokens: 20, max_tokens: 40, overlap_segments: 1 };
        let segs: Vec<Segment> = (0..6)
            .map(|i| seg(&format!("s{i}"), &words(15), i as i64 * 1000, (i as i64 + 1) * 1000))
            .collect();
        let chunks = chunk_segments(&segs, &cfg, approx_token_count);
        // chunk0 = [s0,s1]; overlap 1 => chunk1 starts at s1.
        assert_eq!(chunks[0].last_segment_id, "s1");
        assert_eq!(chunks[1].first_segment_id, "s1");
    }

    #[test]
    fn oversized_single_segment_becomes_its_own_chunk() {
        let cfg = ChunkConfig { min_tokens: 20, max_tokens: 40, overlap_segments: 1 };
        let segs = vec![seg("big", &words(100), 0, 1000), seg("s1", &words(10), 1000, 2000)];
        let chunks = chunk_segments(&segs, &cfg, approx_token_count);
        assert_eq!(chunks[0].first_segment_id, "big");
        assert_eq!(chunks[0].last_segment_id, "big");
        assert_eq!(chunks[0].token_count, 100, "cannot split a segment");
    }

    #[test]
    fn is_deterministic_and_covers_boundaries() {
        let cfg = ChunkConfig::default();
        let segs: Vec<Segment> = (0..20)
            .map(|i| seg(&format!("s{i}"), &words(60), i as i64 * 1000, (i as i64 + 1) * 1000))
            .collect();
        let a = chunk_segments(&segs, &cfg, approx_token_count);
        let b = chunk_segments(&segs, &cfg, approx_token_count);
        assert_eq!(a, b, "same input -> same chunks (backfill idempotency)");
        // Timestamps come straight from the segment boundaries.
        assert_eq!(a[0].start_ms, 0);
        assert_eq!(a.last().unwrap().end_ms, 20_000);
    }
}
