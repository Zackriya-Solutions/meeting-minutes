// src/audio/mod.rs
pub mod core;
pub mod audio_processing;
pub mod encode;
pub mod ffmpeg;
pub mod vad;

// New simplified audio system
pub mod recording_state;
pub mod pipeline;
pub mod stream;
pub mod recording_manager;
pub mod recording_commands;
pub mod recording_preferences;
pub mod recording_saver;

pub use core::{
    default_input_device, default_output_device, get_device_and_config, list_audio_devices,
    parse_audio_device, trigger_audio_permission,
    AudioDevice, AudioTranscriptionEngine, DeviceControl, DeviceType,
    LAST_AUDIO_CAPTURE,
};

// Export new simplified components
pub use recording_state::{RecordingState, AudioChunk, AudioError};
pub use pipeline::{AudioPipelineManager};
pub use stream::{AudioStreamManager};
pub use recording_manager::{RecordingManager};
pub use recording_commands::{
    start_recording, start_recording_with_devices, stop_recording,
    is_recording, get_transcription_status, RecordingArgs, TranscriptionStatus, TranscriptUpdate
};
pub use recording_preferences::{
    RecordingPreferences, get_default_recordings_folder
};
pub use recording_saver::RecordingSaver;
pub use encode::{
    encode_single_audio, AudioInput
};

pub use vad::{extract_speech_16k};

