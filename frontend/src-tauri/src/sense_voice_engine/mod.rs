//! Embedded SenseVoice support with native CoreML on Apple Silicon and
//! sherpa-onnx on other platforms.

pub mod commands;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod coreml;
mod engine;
mod model;

pub use engine::SenseVoiceEngine;
pub use model::{model_definition, DownloadProgress, ModelInfo, ModelStatus, SENSE_VOICE_MODEL};
