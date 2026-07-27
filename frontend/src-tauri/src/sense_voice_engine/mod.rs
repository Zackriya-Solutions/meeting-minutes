//! Embedded SenseVoice support backed by sherpa-onnx.

pub mod commands;
mod engine;
mod model;

pub use engine::SenseVoiceEngine;
pub use model::{DownloadProgress, ModelInfo, ModelStatus, SENSE_VOICE_MODEL};
