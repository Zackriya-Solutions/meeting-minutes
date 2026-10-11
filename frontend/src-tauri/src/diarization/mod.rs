//! Offline speaker diarization: who spoke when, per individual person.

pub mod assign;
pub mod cluster;
pub mod commands;
pub mod diarizer;
pub mod embedding;
pub mod fbank;
pub mod jobs;
pub mod models;
pub mod naming;
pub mod people;
pub mod reconstruct;
pub mod segmentation;
pub mod timing;

use serde::{Deserialize, Serialize};

/// A contiguous stretch of speech attributed to one speaker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub start_s: f64,
    pub end_s: f64,
    pub key: String,
}

impl Turn {
    pub fn duration(&self) -> f64 {
        (self.end_s - self.start_s).max(0.0)
    }
}

/// Intra-op threads for diarization models given `cores`: half of them, at least one, so
/// identification leaves room for live transcription and the rest of the system.
pub(crate) fn intra_op_threads_for(cores: usize) -> usize {
    (cores / 2).max(1)
}

/// Intra-op threads for diarization model sessions on this machine.
pub(crate) fn intra_op_threads() -> usize {
    intra_op_threads_for(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// Session builder shared by the diarization models: CPU, full graph optimisation and
/// `intra_op_threads()` threads.
pub(crate) fn session_builder() -> anyhow::Result<ort::session::builder::SessionBuilder> {
    Ok(ort::session::Session::builder()?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
        .with_execution_providers(vec![ort::execution_providers::CPUExecutionProvider::default().build()])?
        .with_intra_threads(intra_op_threads())?)
}

/// A custom metadata value of a loaded model, if present.
pub(crate) fn model_metadata(session: &ort::session::Session, key: &str) -> Option<String> {
    session.metadata().ok().and_then(|m| m.custom(key).ok().flatten())
}

/// Returned when a diarization run is cancelled by the user.
#[derive(thiserror::Error, Debug)]
#[error("Speaker identification cancelled")]
pub struct Cancelled;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diarization_uses_half_the_cores_and_at_least_one() {
        assert_eq!(intra_op_threads_for(16), 8);
        assert_eq!(intra_op_threads_for(7), 3);
        assert_eq!(intra_op_threads_for(2), 1);
        assert_eq!(intra_op_threads_for(1), 1);
        assert_eq!(intra_op_threads_for(0), 1);
    }
}
