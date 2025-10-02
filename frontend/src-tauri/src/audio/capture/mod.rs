// Audio capture implementations module

pub mod microphone;
pub mod system;
pub mod backend_config;

#[cfg(target_os = "macos")]
pub mod core_audio;

// Re-export capture functionality
pub use system::{
    SystemAudioCapture, SystemAudioStream,
    start_system_audio_capture, list_system_audio_devices,
    check_system_audio_permissions
};

#[cfg(target_os = "macos")]
pub use core_audio::{CoreAudioCapture, CoreAudioStream};

// Re-export backend configuration
pub use backend_config::{
    AudioCaptureBackend, BackendConfig, BACKEND_CONFIG,
    get_current_backend, set_current_backend, get_available_backends
};