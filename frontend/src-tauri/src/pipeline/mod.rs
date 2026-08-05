//! Post-meeting processing pipeline (PLAN.md Phase 1+): chunking, embedding, etc.

pub mod chunker;
pub mod commands;
pub mod diarization;
pub mod diarization_commands;
pub mod embedder;
pub mod extraction;
pub mod extraction_persistence;
pub mod kaldi_fbank;
pub mod speaker_names;
pub mod speaker_naming;
