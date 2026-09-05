//! Embedded Qwen3-ASR support backed by sherpa-onnx.

pub mod commands;
mod engine;
mod model;

pub use engine::QwenAsrEngine;
pub use model::{DownloadProgress, ModelInfo, ModelStatus, QWEN3_ASR_MODEL};
