use serde::{Deserialize, Serialize};

pub const START_EVENT: &str = "teams-call-likely-started";
pub const END_EVENT: &str = "teams-call-likely-ended";
pub const POLL_INTERVAL_SECONDS: u64 = 15;
pub const ACTIVE_POLLS_REQUIRED: u32 = 2;
pub const INACTIVE_POLLS_REQUIRED: u32 = 5;
pub const ACTIVE_CONFIDENCE_THRESHOLD: u8 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDetectionSettings {
    pub meeting_detection_enabled: bool,
    pub teams_detection_enabled: bool,
    pub teams_prompt_start: bool,
    pub teams_prompt_stop: bool,
    pub teams_prompt_cooldown_minutes: u64,
}

impl Default for MeetingDetectionSettings {
    fn default() -> Self {
        Self {
            meeting_detection_enabled: false,
            teams_detection_enabled: true,
            teams_prompt_start: true,
            teams_prompt_stop: true,
            teams_prompt_cooldown_minutes: 30,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DetectionSample {
    pub teams_running: bool,
    pub teams_audio_active: bool,
    pub is_recording: bool,
    pub now_ms: u64,
}

impl DetectionSample {
    pub fn confidence(&self) -> u8 {
        let mut score = 0;
        if self.teams_running {
            score += 1;
        }
        if self.teams_audio_active {
            score += 2;
        }
        score
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsCallLikelyStartedPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub confidence: u8,
    pub teams_running: bool,
    pub teams_audio_active: bool,
    pub poll_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsCallLikelyEndedPayload {
    #[serde(rename = "type")]
    pub event_type: String,
    pub confidence: u8,
    pub teams_running: bool,
    pub teams_audio_active: bool,
    pub inactive_polls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingDetectionStatus {
    pub settings: MeetingDetectionSettings,
    pub running: bool,
    pub teams_running: bool,
    pub teams_audio_active: bool,
    pub confidence: u8,
    pub consecutive_active_polls: u32,
    pub consecutive_inactive_polls: u32,
    pub last_error: Option<String>,
}

impl MeetingDetectionStatus {
    pub fn stopped(settings: MeetingDetectionSettings) -> Self {
        Self {
            settings,
            running: false,
            teams_running: false,
            teams_audio_active: false,
            confidence: 0,
            consecutive_active_polls: 0,
            consecutive_inactive_polls: 0,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionAction {
    PromptStart,
    PromptStop,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_opt_in_with_teams_prompts_enabled() {
        let settings = MeetingDetectionSettings::default();

        assert!(!settings.meeting_detection_enabled);
        assert!(settings.teams_detection_enabled);
        assert!(settings.teams_prompt_start);
        assert!(settings.teams_prompt_stop);
        assert_eq!(settings.teams_prompt_cooldown_minutes, 30);
    }

    #[test]
    fn stopped_status_uses_default_inactive_detector_state() {
        let settings = MeetingDetectionSettings::default();
        let status = MeetingDetectionStatus::stopped(settings.clone());

        assert_eq!(
            status.settings.meeting_detection_enabled,
            settings.meeting_detection_enabled
        );
        assert!(!status.running);
        assert!(!status.teams_running);
        assert!(!status.teams_audio_active);
        assert_eq!(status.confidence, 0);
        assert_eq!(status.consecutive_active_polls, 0);
        assert_eq!(status.consecutive_inactive_polls, 0);
        assert!(status.last_error.is_none());
    }
}
