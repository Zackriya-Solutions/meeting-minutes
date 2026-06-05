use super::types::{
    DetectionAction, DetectionSample, ACTIVE_CONFIDENCE_THRESHOLD, ACTIVE_POLLS_REQUIRED,
    INACTIVE_POLLS_REQUIRED,
};

#[derive(Debug, Clone)]
pub struct ConfidenceState {
    pub consecutive_active_polls: u32,
    pub consecutive_inactive_polls: u32,
    pub last_start_prompt_ms: Option<u64>,
    pub last_stop_prompt_ms: Option<u64>,
    pub start_prompted_this_session: bool,
    pub stop_prompted_this_session: bool,
    pub last_teams_running: bool,
    pub last_teams_audio_active: bool,
    pub last_confidence: u8,
}

impl Default for ConfidenceState {
    fn default() -> Self {
        Self {
            consecutive_active_polls: 0,
            consecutive_inactive_polls: 0,
            last_start_prompt_ms: None,
            last_stop_prompt_ms: None,
            start_prompted_this_session: false,
            stop_prompted_this_session: false,
            last_teams_running: false,
            last_teams_audio_active: false,
            last_confidence: 0,
        }
    }
}

impl ConfidenceState {
    pub fn update(
        &mut self,
        sample: DetectionSample,
        cooldown_minutes: u64,
    ) -> Option<DetectionAction> {
        let confidence = sample.confidence();
        let is_active = confidence >= ACTIVE_CONFIDENCE_THRESHOLD;

        if !sample.teams_running {
            self.consecutive_active_polls = 0;
            self.consecutive_inactive_polls = 0;
            self.start_prompted_this_session = false;
            self.stop_prompted_this_session = false;
        } else if is_active {
            self.consecutive_active_polls = self.consecutive_active_polls.saturating_add(1);
            self.consecutive_inactive_polls = 0;
        } else {
            self.consecutive_inactive_polls = self.consecutive_inactive_polls.saturating_add(1);
            self.consecutive_active_polls = 0;
        }

        self.last_teams_running = sample.teams_running;
        self.last_teams_audio_active = sample.teams_audio_active;
        self.last_confidence = confidence;

        if sample.is_recording {
            if sample.teams_running
                && !is_active
                && self.consecutive_inactive_polls >= INACTIVE_POLLS_REQUIRED
                && !self.stop_prompted_this_session
                && !Self::in_cooldown(self.last_stop_prompt_ms, sample.now_ms, cooldown_minutes)
            {
                self.stop_prompted_this_session = true;
                self.last_stop_prompt_ms = Some(sample.now_ms);
                return Some(DetectionAction::PromptStop);
            }

            return None;
        }

        if sample.teams_running
            && is_active
            && self.consecutive_active_polls >= ACTIVE_POLLS_REQUIRED
            && !self.start_prompted_this_session
            && !Self::in_cooldown(self.last_start_prompt_ms, sample.now_ms, cooldown_minutes)
        {
            self.start_prompted_this_session = true;
            self.last_start_prompt_ms = Some(sample.now_ms);
            return Some(DetectionAction::PromptStart);
        }

        None
    }

    fn in_cooldown(last_prompt_ms: Option<u64>, now_ms: u64, cooldown_minutes: u64) -> bool {
        let Some(last_prompt_ms) = last_prompt_ms else {
            return false;
        };
        let cooldown_ms = cooldown_minutes.saturating_mul(60_000);
        now_ms.saturating_sub(last_prompt_ms) < cooldown_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        teams_running: bool,
        audio: bool,
        is_recording: bool,
        now_ms: u64,
    ) -> DetectionSample {
        DetectionSample {
            teams_running,
            teams_audio_active: audio,
            is_recording,
            now_ms,
        }
    }

    #[test]
    fn no_teams_never_prompts() {
        let mut state = ConfidenceState::default();
        assert_eq!(state.update(sample(false, false, false, 0), 30), None);
        assert_eq!(state.update(sample(false, true, false, 15_000), 30), None);
    }

    #[test]
    fn teams_process_alone_does_not_prompt() {
        let mut state = ConfidenceState::default();
        assert_eq!(state.update(sample(true, false, false, 0), 30), None);
        assert_eq!(state.update(sample(true, false, false, 15_000), 30), None);
        assert_eq!(state.update(sample(true, false, false, 30_000), 30), None);
    }

    #[test]
    fn active_audio_for_two_polls_prompts_start_once() {
        let mut state = ConfidenceState::default();
        assert_eq!(state.update(sample(true, true, false, 0), 30), None);
        assert_eq!(
            state.update(sample(true, true, false, 15_000), 30),
            Some(DetectionAction::PromptStart)
        );
        assert_eq!(state.update(sample(true, true, false, 30_000), 30), None);
    }

    #[test]
    fn inactive_audio_while_recording_prompts_stop_once() {
        let mut state = ConfidenceState::default();
        for index in 0..(INACTIVE_POLLS_REQUIRED - 1) {
            assert_eq!(
                state.update(sample(true, false, true, u64::from(index) * 15_000), 30),
                None
            );
        }

        assert_eq!(
            state.update(sample(true, false, true, 75_000), 30),
            Some(DetectionAction::PromptStop)
        );
        assert_eq!(state.update(sample(true, false, true, 90_000), 30), None);
    }

    #[test]
    fn teams_exit_resets_session_prompt_flags() {
        let mut state = ConfidenceState::default();
        assert_eq!(state.update(sample(true, true, false, 0), 0), None);
        assert_eq!(
            state.update(sample(true, true, false, 15_000), 0),
            Some(DetectionAction::PromptStart)
        );
        assert_eq!(state.update(sample(false, false, false, 30_000), 0), None);
        assert_eq!(state.update(sample(true, true, false, 45_000), 0), None);
        assert_eq!(
            state.update(sample(true, true, false, 60_000), 0),
            Some(DetectionAction::PromptStart)
        );
    }

    #[test]
    fn cooldown_prevents_repeated_prompt_after_session_reset() {
        let mut state = ConfidenceState::default();
        assert_eq!(state.update(sample(true, true, false, 0), 30), None);
        assert_eq!(
            state.update(sample(true, true, false, 15_000), 30),
            Some(DetectionAction::PromptStart)
        );
        assert_eq!(state.update(sample(false, false, false, 30_000), 30), None);
        assert_eq!(state.update(sample(true, true, false, 45_000), 30), None);
        assert_eq!(state.update(sample(true, true, false, 60_000), 30), None);
    }
}
