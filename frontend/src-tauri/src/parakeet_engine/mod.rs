//! Parakeet (NVIDIA NeMo) speech recognition engine module.
//!
//! This module provides a high-performance alternative to Whisper for speech-to-text transcription.
//! Parakeet offers significantly faster processing (up to Real time on modern hardware)
//! with comparable accuracy.
//!
//! # Features
//!
//! - **High Performance**: Real time on M4 Max, 20x on Zen 3, 5x on Skylake
//! - **Int8 Quantization**: Reduced memory footprint with minimal accuracy loss
//! - **ONNX Runtime**: Cross-platform support via ONNX
//! - **Unified API**: Compatible interface with Whisper engine
//!
//! # Module Structure
//!
//! - `parakeet_engine`: Main engine implementation
//! - `model`: ONNX model wrapper and inference logic
//! - `commands`: Tauri command interface for frontend integration

pub mod parakeet_engine;
pub mod model;
pub mod commands;

pub use parakeet_engine::{ParakeetEngine, ParakeetEngineError, QuantizationType, ModelInfo, ModelStatus};
pub use model::{ParakeetModel, ParakeetError, TimestampedResult};
pub use commands::*;
