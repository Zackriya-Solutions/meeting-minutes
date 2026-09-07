//! Apple Speech Recognition engine module.
//!
//! Uses macOS native SFSpeechRecognizer for on-device speech-to-text transcription.
//! Only available on macOS — stubbed out on other platforms.

#[cfg(target_os = "macos")]
mod engine;
pub mod commands;

#[cfg(target_os = "macos")]
pub use engine::AppleSpeechEngine;

#[cfg(not(target_os = "macos"))]
pub use commands::AppleSpeechEngine;
