use super::{DictationFailureCode, DictationPhase, DictationSession};
use sqlx::SqlitePool;

pub async fn save_session(
    pool: &SqlitePool,
    session: &DictationSession,
) -> Result<(), sqlx::Error> {
    let failure_code = session
        .failure
        .as_ref()
        .map(|failure| failure_code(failure.code));
    let failure_message = session
        .failure
        .as_ref()
        .map(|failure| failure.message.as_str());
    let retryable = session
        .failure
        .as_ref()
        .map(|failure| failure.retryable)
        .unwrap_or(false);

    sqlx::query(
        r#"
        INSERT INTO dictation_sessions (
            id, phase, raw_text, final_text, failure_code, failure_message,
            retryable, started_at, updated_at, completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            phase = excluded.phase,
            raw_text = excluded.raw_text,
            final_text = excluded.final_text,
            failure_code = excluded.failure_code,
            failure_message = excluded.failure_message,
            retryable = excluded.retryable,
            updated_at = excluded.updated_at,
            completed_at = excluded.completed_at
        "#,
    )
    .bind(session.id.to_string())
    .bind(phase_name(session.phase))
    .bind(session.raw_text.as_deref())
    .bind(session.final_text.as_deref())
    .bind(failure_code)
    .bind(failure_message)
    .bind(retryable)
    .bind(session.started_at)
    .bind(session.updated_at)
    .bind(session.phase.is_terminal().then_some(session.updated_at))
    .execute(pool)
    .await?;
    Ok(())
}

fn phase_name(phase: DictationPhase) -> &'static str {
    match phase {
        DictationPhase::Idle => "idle",
        DictationPhase::Listening => "listening",
        DictationPhase::Transcribing => "transcribing",
        DictationPhase::Cleaning => "cleaning",
        DictationPhase::Delivering => "delivering",
        DictationPhase::Completed => "completed",
        DictationPhase::Failed => "failed",
        DictationPhase::Cancelled => "cancelled",
    }
}

fn failure_code(code: DictationFailureCode) -> &'static str {
    match code {
        DictationFailureCode::ShortcutUnavailable => "shortcut_unavailable",
        DictationFailureCode::MicrophoneUnavailable => "microphone_unavailable",
        DictationFailureCode::AudioCaptureFailed => "audio_capture_failed",
        DictationFailureCode::ModelUnavailable => "model_unavailable",
        DictationFailureCode::TranscriptionFailed => "transcription_failed",
        DictationFailureCode::CleanupFailed => "cleanup_failed",
        DictationFailureCode::TargetLost => "target_lost",
        DictationFailureCode::SecureTarget => "secure_target",
        DictationFailureCode::ElevatedTarget => "elevated_target",
        DictationFailureCode::DeliveryFailed => "delivery_failed",
        DictationFailureCode::PersistenceFailed => "persistence_failed",
        DictationFailureCode::Internal => "internal",
    }
}
