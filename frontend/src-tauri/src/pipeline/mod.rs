//! Post-meeting processing pipeline (PLAN.md Phase 1+): chunking, embedding, etc.

pub mod chunker;
pub mod commands;
pub mod diarization;
pub mod diarization_commands;
pub mod embedder;
pub mod extraction;
pub mod kaldi_fbank;
