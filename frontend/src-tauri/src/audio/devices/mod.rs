// Audio device management module
// Re-exports all device-related functionality to preserve API surface

pub mod discovery;
pub mod microphone;
pub mod speakers;
pub mod configuration;
pub mod platform;

// Re-export all public functions to preserve existing API
pub use discovery::{list_audio_devices, trigger_audio_permission};
pub use microphone::default_input_device;
pub use speakers::default_output_device;
pub use configuration::{get_device_and_config, parse_audio_device, AudioDevice, DeviceType, DeviceControl, AudioTranscriptionEngine, LAST_AUDIO_CAPTURE};