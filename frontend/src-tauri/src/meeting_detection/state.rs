//! Pure call lifecycle policy. Times are monotonic seconds, not wall-clock time.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    Start,
    Stop,
}

#[derive(Default)]
pub struct CallState {
    first_seen: Option<u64>,
    last_seen: Option<u64>,
    attempted: bool,
    owned: bool,
}

impl CallState {
    /// Unknown probes must not end a recording. A failed start or a manual stop
    /// suppresses further attempts until the call has ended for a full minute.
    pub fn update(&mut self, now: u64, detected: Option<bool>, recording: bool) -> Action {
        if self.owned && !recording {
            self.owned = false;
        }
        match detected {
            Some(true) => {
                self.last_seen = Some(now);
                let first = *self.first_seen.get_or_insert(now);
                if !self.attempted && now.saturating_sub(first) >= 4 {
                    self.attempted = true;
                    if !recording {
                        return Action::Start;
                    }
                }
            }
            Some(false) => {
                // A brief microphone permission check must not qualify as a call.
                if !self.attempted {
                    self.first_seen = None;
                }
                if self
                    .last_seen
                    .is_some_and(|last| now.saturating_sub(last) >= 60)
                {
                    let stop = self.owned;
                    *self = Self::default();
                    if stop {
                        return Action::Stop;
                    }
                }
            }
            None => {
                self.first_seen = None;
                // Restart the end grace period after a probe failure or sleep.
                if self.last_seen.is_some() {
                    self.last_seen = Some(now);
                }
            }
        }
        Action::None
    }

    pub fn relinquish(&mut self) {
        self.owned = false;
    }

    pub fn started(&mut self) {
        self.owned = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_sustained_call_and_attempts_once() {
        let mut s = CallState::default();
        assert_eq!(s.update(0, Some(true), false), Action::None);
        assert_eq!(s.update(2, Some(true), false), Action::None);
        assert_eq!(s.update(4, Some(true), false), Action::Start);
        assert_eq!(s.update(8, Some(true), false), Action::None);
    }

    #[test]
    fn short_permission_checks_do_not_start() {
        let mut s = CallState::default();
        s.update(0, Some(true), false);
        s.update(2, Some(false), false);
        assert_eq!(s.update(4, Some(true), false), Action::None);
    }

    #[test]
    fn only_owned_recordings_stop_after_grace_period() {
        let mut s = CallState::default();
        s.update(0, Some(true), false);
        s.update(4, Some(true), false);
        s.started();
        assert_eq!(s.update(62, Some(false), true), Action::None);
        assert_eq!(s.update(64, Some(false), true), Action::Stop);
        assert_eq!(s.update(66, Some(false), true), Action::None);
    }

    #[test]
    fn manual_recordings_are_never_owned_or_stopped() {
        let mut s = CallState::default();
        s.update(0, Some(true), true);
        assert_eq!(s.update(4, Some(true), true), Action::None);
        assert_eq!(s.update(64, Some(false), true), Action::None);
    }

    #[test]
    fn manual_stop_suppresses_restart_until_next_call() {
        let mut s = CallState::default();
        s.update(0, Some(true), false);
        s.update(4, Some(true), false);
        s.started();
        assert_eq!(s.update(8, Some(true), false), Action::None);
        assert_eq!(s.update(68, Some(false), false), Action::None);
        s.update(70, Some(true), false);
        assert_eq!(s.update(74, Some(true), false), Action::Start);
    }

    #[test]
    fn manual_replacement_never_inherits_automatic_ownership() {
        let mut s = CallState::default();
        s.update(0, Some(true), false);
        s.update(4, Some(true), false);
        s.started();
        s.relinquish();
        assert_eq!(s.update(64, Some(false), true), Action::None);
    }

    #[test]
    fn unknown_samples_and_returning_audio_do_not_stop() {
        let mut s = CallState::default();
        s.update(0, Some(true), false);
        s.update(4, Some(true), false);
        s.started();
        assert_eq!(s.update(100, None, true), Action::None);
        assert_eq!(s.update(102, Some(false), true), Action::None);
        assert_eq!(s.update(104, Some(true), true), Action::None);
        assert_eq!(s.update(120, Some(false), true), Action::None);
    }
}
