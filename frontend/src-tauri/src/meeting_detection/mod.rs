pub mod commands;
pub mod confidence;
pub mod manager;
pub mod process_detector;
pub mod types;
pub mod windows_audio;

pub use commands::{
    get_meeting_detection_settings, get_meeting_detection_status, set_meeting_detection_settings,
    start_meeting_detection, stop_meeting_detection, MeetingDetectionManagerState,
};
pub use manager::MeetingDetectionManager;
pub use types::{
    MeetingDetectionSettings, MeetingDetectionStatus, TeamsCallLikelyEndedPayload,
    TeamsCallLikelyStartedPayload,
};
