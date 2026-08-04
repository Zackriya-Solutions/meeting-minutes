use super::recording_state::DeviceType;

/// Which audio channel a transcript segment originated from.
pub enum SpeakerChannel {
    Microphone,
    System,
}

impl From<&DeviceType> for SpeakerChannel {
    fn from(device_type: &DeviceType) -> Self {
        match device_type {
            DeviceType::Microphone => SpeakerChannel::Microphone,
            DeviceType::System => SpeakerChannel::System,
        }
    }
}

/// A human-readable speaker label derived from the audio channel.
///
/// Extensible: future work can parameterise display_name from config or
/// ML-based per-person identification without changing the call sites.
pub struct SpeakerLabel {
    pub display_name: String,
}

impl SpeakerLabel {
    pub fn for_channel(channel: &SpeakerChannel) -> Self {
        let display_name = match channel {
            SpeakerChannel::Microphone => "U",
            SpeakerChannel::System => "ZEM",
        };
        SpeakerLabel {
            display_name: display_name.to_string(),
        }
    }
}
