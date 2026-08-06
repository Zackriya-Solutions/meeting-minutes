// audio/transcription/mod.rs
//
// Live transcription. One transcribe.cpp stream per meeting; the provider trait,
// engine enum, and per-family providers are gone along with the two engines they
// existed to abstract over.

pub mod stream_worker;

pub use stream_worker::{
    reset_speech_detected_flag, start_transcription_task, TranscriptPartial, TranscriptUpdate,
};
