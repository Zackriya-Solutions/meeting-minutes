use super::{
    history, ActivationBus, ActivationEvent, DictationFailure, DictationFailureCode,
    DictationPhase, DictationSession, ShortAudioCapture,
};
use crate::audio::transcription::TranscriptionEngine;
use crate::state::AppState;
use chrono::Utc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};

struct ActiveCapture {
    session: DictationSession,
    stop: mpsc::Sender<()>,
    worker: JoinHandle<Result<Vec<f32>, super::ShortAudioError>>,
    #[cfg(target_os = "windows")]
    target: Result<super::WindowsTarget, String>,
}

pub fn start_coordinator(app: AppHandle) {
    let mut activations = app.state::<ActivationBus>().subscribe();
    tauri::async_runtime::spawn(async move {
        let mut active: Option<ActiveCapture> = None;
        while let Ok(event) = activations.recv().await {
            match event {
                ActivationEvent::Started if active.is_none() => {
                    let mut session = DictationSession::begin(Utc::now());
                    #[cfg(target_os = "windows")]
                    let target = super::WindowsTarget::capture().map_err(|error| {
                        log::warn!(
                            "dictation_target_capture_failed code=target_lost error={error}"
                        );
                        error.to_string()
                    });
                    #[cfg(target_os = "windows")]
                    if let Ok(target) = target.as_ref() {
                        session.record_target_process(target.process_label());
                    }
                    #[cfg(target_os = "windows")]
                    let overlay_anchor = target
                        .as_ref()
                        .ok()
                        .and_then(super::WindowsTarget::focused_control_anchor);
                    #[cfg(not(target_os = "windows"))]
                    let overlay_anchor = None;
                    super::prepare_overlay_for_activation(&app, overlay_anchor);
                    let (stop, stop_receiver) = mpsc::channel();
                    let worker = std::thread::spawn(move || {
                        let capture = ShortAudioCapture::start()?;
                        stop_receiver
                            .recv()
                            .map_err(|_| super::ShortAudioError::BufferUnavailable)?;
                        capture.finish()
                    });
                    emit_phase(&app, session.phase, None);
                    active = Some(ActiveCapture {
                        session,
                        stop,
                        worker,
                        #[cfg(target_os = "windows")]
                        target,
                    });
                }
                ActivationEvent::Stopped => {
                    if let Some(capture) = active.take() {
                        finish_dictation(&app, capture).await;
                    }
                }
                ActivationEvent::Started => {
                    log::warn!("dictation_activation_ignored code=already_listening");
                }
            }
        }
        log::error!("dictation_activation_bus_closed code=internal");
    });
}

async fn finish_dictation(app: &AppHandle, capture: ActiveCapture) {
    let ActiveCapture {
        mut session,
        stop,
        worker,
        #[cfg(target_os = "windows")]
        target,
    } = capture;
    if let Err(error) = session.transition(DictationPhase::Transcribing, Utc::now()) {
        log::error!("dictation_transition_failed code=internal error={error}");
        return;
    }
    emit_phase(app, session.phase, None);

    let _ = stop.send(());
    let samples = match tauri::async_runtime::spawn_blocking(move || worker.join()).await {
        Ok(Ok(Ok(samples))) => samples,
        Ok(Ok(Err(error))) => {
            let code = if error == super::ShortAudioError::NoMicrophone {
                DictationFailureCode::MicrophoneUnavailable
            } else {
                DictationFailureCode::AudioCaptureFailed
            };
            fail_and_persist(app, &mut session, code, error.to_string(), true).await;
            return;
        }
        Ok(Err(_)) | Err(_) => {
            fail_and_persist(
                app,
                &mut session,
                DictationFailureCode::AudioCaptureFailed,
                "The microphone capture thread stopped unexpectedly.".into(),
                true,
            )
            .await;
            return;
        }
    };

    let transcript = match transcribe(app, samples).await {
        Ok(text) if !text.trim().is_empty() => text.trim().to_owned(),
        Ok(_) => {
            fail_and_persist(
                app,
                &mut session,
                DictationFailureCode::TranscriptionFailed,
                "No speech was detected.".into(),
                true,
            )
            .await;
            return;
        }
        Err(error) => {
            fail_and_persist(
                app,
                &mut session,
                DictationFailureCode::TranscriptionFailed,
                error,
                true,
            )
            .await;
            return;
        }
    };

    session.record_transcript(transcript.clone());
    if let Err(error) = session.transition(DictationPhase::Cleaning, Utc::now()) {
        log::error!("dictation_transition_failed code=internal error={error}");
        return;
    }
    emit_phase(app, session.phase, None);

    let cleanup =
        super::cleanup_transcript(transcript, crate::get_language_preference_internal()).await;
    if let Some(reason) = cleanup.fallback_reason {
        log::warn!(
            "dictation_cleanup_raw_fallback session_id={} reason={reason:?}",
            session.id
        );
    }
    session.record_cleaned_text(cleanup.text);
    if let Err(error) = session.transition(DictationPhase::Delivering, Utc::now()) {
        log::error!("dictation_transition_failed code=internal error={error}");
        return;
    }

    // History is written before delivery. A failed target can never destroy
    // the only copy of the transcript.
    if !persist(app, &session).await {
        fail_and_persist(
            app,
            &mut session,
            DictationFailureCode::PersistenceFailed,
            "Could not save dictation history before delivery.".into(),
            true,
        )
        .await;
        return;
    }
    emit_phase(app, session.phase, None);

    #[cfg(target_os = "windows")]
    let delivery = {
        let text = session.final_text.clone().unwrap_or_default();
        tauri::async_runtime::spawn_blocking(move || {
            let target = target.map_err(|error| format!("TargetLost: {error}"))?;
            let mut clipboard = super::WindowsClipboard;
            let mut paste = super::WindowsPaste::for_target(target);
            super::deliver_text(&mut clipboard, &mut paste, &text)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result)
    };

    #[cfg(not(target_os = "windows"))]
    let delivery: Result<(), String> = Err("System-wide delivery is not implemented here.".into());

    match delivery {
        Ok(_) => {
            let _ = session.transition(DictationPhase::Completed, Utc::now());
            persist(app, &session).await;
            emit_phase(app, session.phase, None);
            log::info!("dictation_completed session_id={}", session.id);
        }
        Err(error) => {
            let code = if error.contains("ElevatedTarget") {
                DictationFailureCode::ElevatedTarget
            } else if error.contains("SecureTarget") {
                DictationFailureCode::SecureTarget
            } else if error.contains("ForegroundTarget")
                || error.contains("FocusedControl")
                || error.starts_with("TargetLost:")
            {
                DictationFailureCode::TargetLost
            } else {
                DictationFailureCode::DeliveryFailed
            };
            fail_and_persist(app, &mut session, code, error, true).await;
        }
    }
}

async fn transcribe(app: &AppHandle, samples: Vec<f32>) -> Result<String, String> {
    crate::audio::transcription::validate_transcription_model_ready(app).await?;
    let engine = crate::audio::transcription::get_or_init_transcription_engine(app).await?;
    match engine {
        TranscriptionEngine::Whisper(engine) => engine
            .transcribe_audio(samples, crate::get_language_preference_internal())
            .await
            .map_err(|error| error.to_string()),
        TranscriptionEngine::Parakeet(engine) => engine
            .transcribe_audio(samples)
            .await
            .map_err(|error| error.to_string()),
        TranscriptionEngine::Provider(provider) => provider
            .transcribe(samples, crate::get_language_preference_internal())
            .await
            .map(|result| result.text)
            .map_err(|error| error.to_string()),
    }
}

async fn fail_and_persist(
    app: &AppHandle,
    session: &mut DictationSession,
    code: DictationFailureCode,
    message: String,
    retryable: bool,
) {
    let failure = DictationFailure {
        code,
        message: message.clone(),
        retryable,
    };
    if let Err(error) = session.fail(failure, Utc::now()) {
        log::error!("dictation_transition_failed code=internal error={error}");
    }
    log_failure(session);
    persist(app, session).await;
    emit_phase(app, session.phase, Some(message));
}

async fn persist(app: &AppHandle, session: &DictationSession) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        log::error!(
            "dictation_history_save_failed code=persistence_failed session_id={} error=database_state_unavailable",
            session.id
        );
        return false;
    };
    match history::save_session(state.db_manager.pool(), session).await {
        Ok(()) => true,
        Err(error) => {
            log::error!(
                "dictation_history_save_failed code=persistence_failed session_id={} error={}",
                session.id,
                error
            );
            false
        }
    }
}

fn emit_phase(app: &AppHandle, phase: DictationPhase, message: Option<String>) {
    let _ = app.emit(
        "dictation-state",
        serde_json::json!({ "phase": phase, "message": message }),
    );
    super::show_overlay_if_enabled(app);
}

fn log_failure(session: &DictationSession) {
    if let Some(failure) = &session.failure {
        log::error!(
            "dictation_failed session_id={} code={:?} retryable={} error={}",
            session.id,
            failure.code,
            failure.retryable,
            failure.message
        );
    }
}
