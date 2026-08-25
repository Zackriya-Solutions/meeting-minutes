// PR-34: Whisper engine fallback chain.
//
// Tracks engine-kind preference and consecutive failures. When N consecutive chunk
// transcriptions fail, the next engine in the chain becomes the preferred kind.
// The actual WhisperEngine instance swap is performed by the caller (typically
// ParallelProcessor) — this module only owns the state machine and emits the
// `EngineSwitched` recommendation.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use tracing::{info, warn};

/// Available STT engine kinds, ordered from most-preferred (top) to least-preferred (bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Cuda,
    Metal,
    Cpu,
    Parakeet,
}

impl EngineKind {
    pub fn name(self) -> 'static str {
        match self {
            EngineKind::Cuda => "CUDA",
            EngineKind::Metal => "Metal",
            EngineKind::Cpu => "CPU",
            EngineKind::Parakeet => "Parakeet",
        }
    }
}

/// Fallback engine state machine.
pub struct FallbackEngine {
    engines: Vec<EngineKind>,
    current_idx: AtomicUsize,
    consecutive_failures: AtomicU32,
    failure_threshold: u32,
}

impl FallbackEngine {
    pub fn new(engines: Vec<EngineKind>) -> Self {
        assert!(!engines.is_empty(), "engine chain must not be empty");
        Self {
            engines,
            current_idx: AtomicUsize::new(0),
            consecutive_failures: AtomicU32::new(0),
            failure_threshold: 3,
        }
    }

    pub fn with_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold.max(1);
        self
    }

    pub fn current(&self) -> EngineKind {
        let idx = self.current_idx.load(Ordering::Relaxed);
        self.engines[idx.min(self.engines.len() - 1)]
    }

    pub fn current_index(&self) -> usize {
        self.current_idx.load(Ordering::Relaxed)
    }

    pub fn engines(&self) -> &[EngineKind] {
        &self.engines
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Record a successful chunk transcription: reset failure counter.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Record a failed chunk transcription.
    ///
    /// Returns `Some(new_kind)` when the failure threshold is reached AND a next
    /// engine is available in the chain. Returns `None` if the threshold has not
    /// been reached yet, or the chain is exhausted.
    pub fn record_failure(&self) -> Option<EngineKind> {
        let n = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if n < self.failure_threshold {
            return None;
        }
        let cur = self.current_idx.load(Ordering::Relaxed);
        if cur + 1 >= self.engines.len() {
            warn!(
                "Whisper fallback threshold reached but chain exhausted at {:?}",
                self.current()
            );
            return None;
        }
        let prev = self.current();
        self.current_idx.store(cur + 1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let next = self.current();
        warn!(
            "Whisper fallback from {} to {} due to {} consecutive failures",
            prev.name(),
            next.name(),
            self.failure_threshold
        );
        info!("Whisper engine switched: {} -> {}", prev.name(), next.name());
        Some(next)
    }

    /// Reset to the first engine in the chain (e.g. on a new recording session).
    pub fn reset(&self) {
        self.current_idx.store(0, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn default_chain() -> Vec<EngineKind> {
        vec![EngineKind::Cuda, EngineKind::Metal, EngineKind::Cpu, EngineKind::Parakeet]
    }

    #[test]
    #[should_panic(expected = "chain must not be empty")]
    fn empty_chain_panics() {
        FallbackEngine::new(vec![]);
    }

    #[test]
    fn starts_at_first_engine() {
        let fb = FallbackEngine::new(default_chain());
        assert_eq!(fb.current(), EngineKind::Cuda);
        assert_eq!(fb.current_index(), 0);
        assert_eq!(fb.consecutive_failures(), 0);
    }

    #[test]
    fn record_success_resets_failure_counter() {
        let fb = FallbackEngine::new(default_chain());
        fb.record_failure();
        fb.record_failure();
        assert_eq!(fb.consecutive_failures(), 2);
        fb.record_success();
        assert_eq!(fb.consecutive_failures(), 0);
        assert_eq!(fb.current(), EngineKind::Cuda);
    }

    #[test]
    fn three_consecutive_failures_switch_to_metal() {
        let fb = FallbackEngine::new(default_chain());
        assert!(fb.record_failure().is_none());
        assert!(fb.record_failure().is_none());
        let switched = fb.record_failure();
        assert_eq!(switched, Some(EngineKind::Metal));
        assert_eq!(fb.current(), EngineKind::Metal);
        assert_eq!(fb.consecutive_failures(), 0);
    }

    #[test]
    fn cascades_through_full_chain() {
        let fb = FallbackEngine::new(default_chain());
        // Cuda -> Metal
        fb.record_failure(); fb.record_failure(); fb.record_failure();
        assert_eq!(fb.current(), EngineKind::Metal);
        // Metal -> CPU
        fb.record_failure(); fb.record_failure(); fb.record_failure();
        assert_eq!(fb.current(), EngineKind::Cpu);
        // CPU -> Parakeet
        fb.record_failure(); fb.record_failure(); fb.record_failure();
        assert_eq!(fb.current(), EngineKind::Parakeet);
        // Chain exhausted
        fb.record_failure(); fb.record_failure(); fb.record_failure();
        assert_eq!(fb.current(), EngineKind::Parakeet);
        assert!(fb.record_failure().is_none());
    }

    #[test]
    fn success_interrupts_failure_streak() {
        let fb = FallbackEngine::new(default_chain());
        fb.record_failure(); fb.record_failure();
        fb.record_success();
        fb.record_failure(); fb.record_failure();
        assert_eq!(fb.consecutive_failures(), 2);
        assert_eq!(fb.current(), EngineKind::Cuda);
    }

    #[test]
    fn custom_threshold() {
        let fb = FallbackEngine::new(default_chain()).with_threshold(5);
        for _ in 0..4 {
            assert!(fb.record_failure().is_none());
        }
        assert_eq!(fb.record_failure(), Some(EngineKind::Metal));
    }

    #[test]
    fn threshold_zero_clamps_to_one() {
        let fb = FallbackEngine::new(default_chain()).with_threshold(0);
        assert_eq!(fb.record_failure(), Some(EngineKind::Metal));
    }

    #[test]
    fn reset_returns_to_first_engine() {
        let fb = FallbackEngine::new(default_chain());
        fb.record_failure(); fb.record_failure(); fb.record_failure();
        assert_eq!(fb.current(), EngineKind::Metal);
        fb.reset();
        assert_eq!(fb.current(), EngineKind::Cuda);
        assert_eq!(fb.consecutive_failures(), 0);
    }

    #[test]
    fn engines_returns_full_chain() {
        let chain = vec![EngineKind::Cpu, EngineKind::Parakeet];
        let fb = FallbackEngine::new(chain);
        assert_eq!(fb.engines(), &[EngineKind::Cpu, EngineKind::Parakeet]);
    }

    #[test]
    fn engine_kind_name_strings() {
        assert_eq!(EngineKind::Cuda.name(), "CUDA");
        assert_eq!(EngineKind::Metal.name(), "Metal");
        assert_eq!(EngineKind::Cpu.name(), "CPU");
        assert_eq!(EngineKind::Parakeet.name(), "Parakeet");
    }
}
