use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationPhase {
    Idle,
    Listening,
    Transcribing,
    Cleaning,
    Delivering,
    Completed,
    Failed,
    Cancelled,
}

impl DictationPhase {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Idle, Self::Listening)
                | (Self::Listening, Self::Transcribing)
                | (Self::Transcribing, Self::Cleaning)
                | (Self::Transcribing, Self::Delivering)
                | (Self::Cleaning, Self::Delivering)
                | (Self::Delivering, Self::Completed)
                | (Self::Listening, Self::Cancelled)
                | (Self::Transcribing, Self::Cancelled)
                | (Self::Cleaning, Self::Cancelled)
                | (Self::Delivering, Self::Cancelled)
                | (Self::Listening, Self::Failed)
                | (Self::Transcribing, Self::Failed)
                | (Self::Cleaning, Self::Failed)
                | (Self::Delivering, Self::Failed)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationFailureCode {
    ShortcutUnavailable,
    MicrophoneUnavailable,
    AudioCaptureFailed,
    ModelUnavailable,
    TranscriptionFailed,
    CleanupFailed,
    TargetLost,
    SecureTarget,
    ElevatedTarget,
    DeliveryFailed,
    PersistenceFailed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationFailure {
    pub code: DictationFailureCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictationSession {
    pub id: Uuid,
    pub phase: DictationPhase,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub raw_text: Option<String>,
    pub final_text: Option<String>,
    pub target_process: Option<String>,
    pub failure: Option<DictationFailure>,
}

impl DictationSession {
    pub fn begin(now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            phase: DictationPhase::Listening,
            started_at: now,
            updated_at: now,
            raw_text: None,
            final_text: None,
            target_process: None,
            failure: None,
        }
    }

    pub fn transition(
        &mut self,
        next: DictationPhase,
        now: DateTime<Utc>,
    ) -> Result<(), DictationTransitionError> {
        if !self.phase.can_transition_to(next) {
            return Err(DictationTransitionError {
                from: self.phase,
                to: next,
            });
        }

        self.phase = next;
        self.updated_at = now;
        Ok(())
    }

    pub fn record_transcript(&mut self, text: String) {
        self.raw_text = Some(text);
    }

    pub fn record_target_process(&mut self, target_process: String) {
        self.target_process = Some(target_process);
    }

    pub fn record_cleaned_text(&mut self, text: String) {
        self.final_text = Some(text);
    }

    pub fn use_raw_fallback(&mut self) {
        self.final_text = self.raw_text.clone();
    }

    pub fn fail(
        &mut self,
        failure: DictationFailure,
        now: DateTime<Utc>,
    ) -> Result<(), DictationTransitionError> {
        self.transition(DictationPhase::Failed, now)?;
        self.failure = Some(failure);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictationTransitionError {
    pub from: DictationPhase,
    pub to: DictationPhase,
}

impl fmt::Display for DictationTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid dictation transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for DictationTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 10, 0, second)
            .single()
            .unwrap()
    }

    #[test]
    fn successful_session_follows_public_lifecycle() {
        let mut session = DictationSession::begin(at(0));
        assert_eq!(session.phase, DictationPhase::Listening);

        session
            .transition(DictationPhase::Transcribing, at(1))
            .unwrap();
        session.record_transcript("hello world".into());
        session.transition(DictationPhase::Cleaning, at(2)).unwrap();
        session.record_cleaned_text("Hello world.".into());
        session
            .transition(DictationPhase::Delivering, at(3))
            .unwrap();
        session
            .transition(DictationPhase::Completed, at(4))
            .unwrap();

        assert!(session.phase.is_terminal());
        assert_eq!(session.final_text.as_deref(), Some("Hello world."));
        assert!(session.failure.is_none());
    }

    #[test]
    fn cleanup_can_fall_back_to_raw_transcript() {
        let mut session = DictationSession::begin(at(0));
        session
            .transition(DictationPhase::Transcribing, at(1))
            .unwrap();
        session.record_transcript("keep the raw words".into());
        session.transition(DictationPhase::Cleaning, at(2)).unwrap();
        session.use_raw_fallback();

        assert_eq!(session.final_text, session.raw_text);
    }

    #[test]
    fn terminal_session_cannot_restart() {
        let mut session = DictationSession::begin(at(0));
        session
            .transition(DictationPhase::Cancelled, at(1))
            .unwrap();

        let error = session
            .transition(DictationPhase::Listening, at(2))
            .unwrap_err();

        assert_eq!(error.from, DictationPhase::Cancelled);
        assert_eq!(error.to, DictationPhase::Listening);
    }

    #[test]
    fn failure_keeps_a_stable_actionable_code() {
        let mut session = DictationSession::begin(at(0));
        session
            .transition(DictationPhase::Transcribing, at(1))
            .unwrap();
        session
            .fail(
                DictationFailure {
                    code: DictationFailureCode::ModelUnavailable,
                    message: "Download or select a transcription model.".into(),
                    retryable: true,
                },
                at(2),
            )
            .unwrap();

        assert_eq!(session.phase, DictationPhase::Failed);
        assert_eq!(
            session.failure.as_ref().map(|failure| failure.code),
            Some(DictationFailureCode::ModelUnavailable)
        );
    }
}
