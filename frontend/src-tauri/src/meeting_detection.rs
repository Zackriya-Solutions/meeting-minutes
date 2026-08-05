//! Privacy-preserving meeting detection.
//!
//! Process/audio evidence stays in memory. When meeting detection is enabled, a strong OS-level
//! microphone-session signal may request the existing recording pipeline to start and stop;
//! raw process names, browser URLs, and window titles are never persisted.

use crate::notifications::commands::NotificationManagerState;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State, UserAttentionType, Wry};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const PROCESS_LAUNCH_EVIDENCE_TTL: Duration = Duration::from_secs(90);
// A supported native client actively owning the microphone is already a strong signal.
// Start on the first observation (at most one poll late) so a privacy-preserving design
// does not need to keep an ambient pre-call audio buffer.
const REQUIRED_ACTIVE_POLLS: u8 = 2;
const STRONG_AUTO_LISTENING_POLLS: u8 = 1;
const REQUIRED_QUIET_POLLS: u8 = 2;
const AUTO_LISTENING_QUIET_POLLS: u8 = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingApp {
    Zoom,
    MicrosoftTeams,
    Telegram,
    YandexTelemost,
    SaluteJazz,
    BrowserCall,
}

impl MeetingApp {
    /// Product name used when titling an automatically captured meeting. The webview
    /// has its own localized labels for prompts; this is the Rust-side fallback for
    /// captures that complete without any UI involved.
    fn display(self) -> &'static str {
        match self {
            MeetingApp::Zoom => "Zoom",
            MeetingApp::MicrosoftTeams => "Microsoft Teams",
            MeetingApp::Telegram => "Telegram",
            MeetingApp::YandexTelemost => "Yandex Telemost",
            MeetingApp::SaluteJazz => "SaluteJazz",
            MeetingApp::BrowserCall => "Browser call",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    NativeProcess,
    MicrophoneActivity,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetectedEvent {
    pub apps: Vec<MeetingApp>,
    pub source: DetectionSource,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoListeningEvent {
    pub session_id: String,
    pub apps: Vec<MeetingApp>,
    pub source: DetectionSource,
}

#[derive(Debug, Default)]
struct AutoListeningSharedState {
    active_capture_session_id: Option<String>,
    start_failed: bool,
}

/// Owns the background task so it is started once and can be cancelled on app exit.
pub struct AutoMeetingDetectionState {
    runtime: Mutex<DetectionRuntime>,
    auto_listening: Arc<Mutex<AutoListeningSharedState>>,
}

struct DetectionRuntime {
    cancellation: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl Default for AutoMeetingDetectionState {
    fn default() -> Self {
        Self {
            runtime: Mutex::new(DetectionRuntime {
                cancellation: CancellationToken::new(),
                task: None,
            }),
            auto_listening: Arc::new(Mutex::new(AutoListeningSharedState::default())),
        }
    }
}

impl AutoMeetingDetectionState {
    fn cancellation_for_start(runtime: &mut DetectionRuntime) -> CancellationToken {
        if runtime.cancellation.is_cancelled() {
            runtime.cancellation = CancellationToken::new();
        }
        runtime.cancellation.clone()
    }

    pub fn start(&self, app: AppHandle<Wry>) {
        let Ok(mut runtime) = self.runtime.lock() else {
            return;
        };
        if runtime.task.is_some() {
            return;
        }

        let cancellation = Self::cancellation_for_start(&mut runtime);
        let auto_listening = self.auto_listening.clone();
        runtime.task = Some(tauri::async_runtime::spawn(async move {
            run_detection_loop(app, cancellation, auto_listening).await;
        }));
    }

    pub fn stop(&self) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.cancellation.cancel();
            if let Some(handle) = runtime.task.take() {
                handle.abort();
            }
        }
    }

    fn is_running(&self) -> bool {
        self.runtime
            .lock()
            .map(|runtime| runtime.task.is_some())
            .unwrap_or(false)
    }
}

#[derive(Debug, Serialize)]
pub struct AutoMeetingDetectionStatus {
    pub enabled: bool,
    pub running: bool,
    pub microphone_signal_supported: bool,
    pub auto_listening_enabled: bool,
    pub auto_listening_supported: bool,
    pub active_capture_session_id: Option<String>,
    pub poll_interval_seconds: u64,
}

/// Privacy-safe diagnostics: deliberately excludes process names, event history and times.
#[tauri::command]
pub async fn get_auto_meeting_detection_status(
    detector: State<'_, AutoMeetingDetectionState>,
    notifications: State<'_, NotificationManagerState<Wry>>,
) -> Result<AutoMeetingDetectionStatus, String> {
    let (enabled, auto_listening_enabled) = match notifications.read().await.as_ref() {
        Some(manager) => {
            let settings = manager.get_settings().await;
            (settings.auto_meeting_detection, settings.auto_listening)
        }
        None => (false, false),
    };
    let active_capture_session_id = detector
        .auto_listening
        .lock()
        .ok()
        .and_then(|state| state.active_capture_session_id.clone());
    Ok(AutoMeetingDetectionStatus {
        enabled,
        running: detector.is_running(),
        microphone_signal_supported: cfg!(target_os = "macos"),
        auto_listening_enabled,
        auto_listening_supported: cfg!(target_os = "macos"),
        active_capture_session_id,
        poll_interval_seconds: POLL_INTERVAL.as_secs(),
    })
}

#[derive(Debug, Default)]
struct DetectionSession {
    active_polls: u8,
    quiet_polls: u8,
    notified: bool,
    auto_listening_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectionEvent {
    None,
    SuggestRecording,
    /// The call a suggestion was raised for is over, or detection was switched off
    /// under it. Nothing is recording, so this only withdraws the prompt — without it
    /// an unanswered suggestion would sit on screen indefinitely.
    WithdrawSuggestion,
    StartAutoListening,
    StopAutoListening,
}

impl DetectionSession {
    fn observe(
        &mut self,
        active: bool,
        recording: bool,
        enabled: bool,
        auto_listening: bool,
    ) -> DetectionEvent {
        if !enabled {
            let had_suggestion = self.notified;
            if self.auto_listening_active {
                *self = Self::default();
                return DetectionEvent::StopAutoListening;
            }
            *self = Self::default();
            if had_suggestion {
                return DetectionEvent::WithdrawSuggestion;
            }
            return DetectionEvent::None;
        }

        if active {
            self.quiet_polls = 0;
            self.active_polls = self.active_polls.saturating_add(1);

            if self.auto_listening_active {
                return DetectionEvent::None;
            }

            // A call already being recorded is a consumed detection session. This prevents
            // an immediate prompt if the user stops recording while the call is still open.
            if recording {
                self.notified = true;
                return DetectionEvent::None;
            }

            let required_polls = if auto_listening {
                STRONG_AUTO_LISTENING_POLLS
            } else {
                REQUIRED_ACTIVE_POLLS
            };
            if !self.notified && self.active_polls >= required_polls {
                self.notified = true;
                if auto_listening {
                    self.auto_listening_active = true;
                    return DetectionEvent::StartAutoListening;
                }
                return DetectionEvent::SuggestRecording;
            }
            return DetectionEvent::None;
        }

        self.active_polls = 0;
        self.quiet_polls = self.quiet_polls.saturating_add(1);
        if self.auto_listening_active {
            if self.quiet_polls >= AUTO_LISTENING_QUIET_POLLS {
                *self = Self::default();
                return DetectionEvent::StopAutoListening;
            }
            return DetectionEvent::None;
        }
        if self.quiet_polls >= REQUIRED_QUIET_POLLS {
            let had_suggestion = self.notified;
            self.notified = false;
            self.quiet_polls = 0;
            if had_suggestion {
                return DetectionEvent::WithdrawSuggestion;
            }
        }
        DetectionEvent::None
    }
}

/// A process launch is useful evidence, but only for a bounded interval. Long-lived apps
/// must not keep the detector permanently active after one launch.
#[derive(Debug, Default)]
struct ProcessLaunchEvidence {
    previous_apps: Option<BTreeSet<MeetingApp>>,
    launched_at: BTreeMap<MeetingApp, Instant>,
}

impl ProcessLaunchEvidence {
    fn observe(
        &mut self,
        current_apps: &BTreeSet<MeetingApp>,
        now: Instant,
    ) -> BTreeSet<MeetingApp> {
        if let Some(previous) = &self.previous_apps {
            for app in current_apps.difference(previous) {
                self.launched_at.insert(*app, now);
            }
        }

        self.previous_apps = Some(current_apps.clone());
        self.launched_at.retain(|app, started_at| {
            current_apps.contains(app)
                && now.saturating_duration_since(*started_at) <= PROCESS_LAUNCH_EVIDENCE_TTL
        });
        self.launched_at.keys().copied().collect()
    }
}

/// Which consumer owns the current automatic capture session. Both are driven by
/// the same detector and the same `DetectionSession` state machine; only one can be
/// armed at a time, so a call is never captured twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoCaptureMode {
    /// The interactive recording session, with live transcription.
    Listening,
    /// A silent background capture, registered as audio-only when the call ends.
    Background,
}

async fn run_detection_loop(
    app: AppHandle<Wry>,
    cancellation: CancellationToken,
    auto_listening_state: Arc<Mutex<AutoListeningSharedState>>,
) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Err(error) = recover_interrupted_capture_sessions(state.db_manager.pool()).await {
            log::warn!("Could not recover interrupted capture sessions: {error}");
        }
    }
    let mut system = System::new_all();
    let mut process_evidence = ProcessLaunchEvidence::default();
    let mut session = DetectionSession::default();
    // A start attempt that failed must not be retried until the call signal has gone
    // quiet, whichever mode attempted it.
    let mut suppress_auto_capture_until_quiet = false;
    let mut armed_mode: Option<AutoCaptureMode> = None;
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            _ = interval.tick() => {
                let settings = detection_settings(&app).await;
                let recording = crate::audio::recording_commands::is_recording().await;
                if !settings.detection_enabled {
                    match session.observe(false, false, false, false) {
                        DetectionEvent::StopAutoListening => {
                            stop_armed_mode(&app, &auto_listening_state, armed_mode.take()).await;
                        }
                        DetectionEvent::WithdrawSuggestion => withdraw_detection(&app),
                        _ => {}
                    }
                    process_evidence = ProcessLaunchEvidence::default();
                    continue;
                }
                // A mode that is switched off mid-call must release the call it owns.
                let armed_mode_still_enabled = match armed_mode {
                    Some(AutoCaptureMode::Listening) => settings.auto_listening,
                    Some(AutoCaptureMode::Background) => settings.background_auto_recording,
                    None => true,
                };
                if session.auto_listening_active && !armed_mode_still_enabled {
                    session = DetectionSession::default();
                    stop_armed_mode(&app, &auto_listening_state, armed_mode.take()).await;
                    process_evidence = ProcessLaunchEvidence::default();
                    continue;
                }
                // The user took over manually. Finalize the background capture rather
                // than recording the same call twice from two microphones.
                if armed_mode == Some(AutoCaptureMode::Background) && recording {
                    log::info!("Interactive recording started; finalizing the background capture");
                    session = DetectionSession::default();
                    stop_armed_mode(&app, &auto_listening_state, armed_mode.take()).await;
                    process_evidence = ProcessLaunchEvidence::default();
                    continue;
                }

                system.refresh_processes(ProcessesToUpdate::All, true);
                let running_native_apps = collect_native_apps(&system);
                let launched_apps = process_evidence.observe(&running_native_apps, Instant::now());
                let microphone_apps = active_microphone_apps();
                let (candidates, source) = select_detection_signal(
                    launched_apps,
                    microphone_apps,
                    cfg!(target_os = "macos"),
                );
                if let Ok(mut shared) = auto_listening_state.lock() {
                    if shared.start_failed {
                        shared.start_failed = false;
                        session = DetectionSession {
                            active_polls: REQUIRED_ACTIVE_POLLS.saturating_sub(1),
                            ..DetectionSession::default()
                        };
                        // The webview already released that capture session, so the
                        // armed mode is stale and must not be stopped again later.
                        armed_mode = None;
                        suppress_auto_capture_until_quiet = true;
                    }
                }
                if candidates.is_empty() {
                    suppress_auto_capture_until_quiet = false;
                }
                let armable = !suppress_auto_capture_until_quiet;
                let mode_to_arm = mode_to_arm(
                    &candidates,
                    source,
                    DetectionSettings {
                        auto_listening: settings.auto_listening && armable,
                        background_auto_recording: settings.background_auto_recording && armable,
                        ..settings
                    },
                    cfg!(target_os = "macos"),
                );
                match session.observe(
                    !candidates.is_empty(),
                    recording,
                    true,
                    mode_to_arm.is_some(),
                ) {
                    DetectionEvent::None => {}
                    DetectionEvent::SuggestRecording => {
                        let Some(source) = source else { continue };
                        deliver_detection(
                            &app,
                            MeetingDetectedEvent {
                                apps: candidates.into_iter().collect(),
                                source,
                            },
                        )
                        .await;
                    }
                    DetectionEvent::WithdrawSuggestion => withdraw_detection(&app),
                    DetectionEvent::StartAutoListening => {
                        let Some(source) = source else { continue };
                        let Some(mode) = mode_to_arm else { continue };
                        let apps: Vec<_> = candidates.into_iter().collect();
                        let start = match mode {
                            AutoCaptureMode::Listening => start_auto_listening(
                                &app,
                                &auto_listening_state,
                                apps.clone(),
                                source,
                            )
                            .await,
                            AutoCaptureMode::Background => app
                                .state::<crate::background_capture::BackgroundCaptureState>()
                                .start(&app, &label_for(&apps))
                                .await,
                        };
                        match start {
                            Ok(()) => armed_mode = Some(mode),
                            Err(error) => {
                                log::warn!("{mode:?} start was rejected safely: {error}");
                                // Fall back to a confirmation prompt and wait for the
                                // signal to go quiet before arming again.
                                session = DetectionSession {
                                    active_polls: REQUIRED_ACTIVE_POLLS,
                                    notified: true,
                                    ..DetectionSession::default()
                                };
                                suppress_auto_capture_until_quiet = true;
                                deliver_detection(&app, MeetingDetectedEvent { apps, source }).await;
                            }
                        }
                    }
                    DetectionEvent::StopAutoListening => {
                        stop_armed_mode(&app, &auto_listening_state, armed_mode.take()).await;
                    }
                }
            }
        }
    }
}

/// Which automatic mode, if either, should take the current call.
///
/// Auto-listening wins: it produces a live transcript, and two capture paths must
/// never open the microphone for the same call. Background capture therefore only
/// arms for calls auto-listening declines — a browser or Telegram call, or every
/// call when auto-listening is switched off.
fn mode_to_arm(
    candidates: &BTreeSet<MeetingApp>,
    source: Option<DetectionSource>,
    settings: DetectionSettings,
    microphone_signal_supported: bool,
) -> Option<AutoCaptureMode> {
    if should_auto_listen(
        candidates,
        source,
        settings.auto_listening,
        microphone_signal_supported,
    ) {
        return Some(AutoCaptureMode::Listening);
    }
    if should_capture_in_background(
        candidates,
        source,
        settings.background_auto_recording,
        microphone_signal_supported,
    ) {
        return Some(AutoCaptureMode::Background);
    }
    None
}

/// Background capture accepts every recognized client, including browsers and
/// Telegram, because it is silent and reversible: nothing is shown, nothing is
/// transcribed, and a capture that turns out to be a voice message rather than a
/// call is discarded by the minimum-duration guard instead of reaching the library.
fn should_capture_in_background(
    candidates: &BTreeSet<MeetingApp>,
    source: Option<DetectionSource>,
    enabled: bool,
    microphone_signal_supported: bool,
) -> bool {
    enabled
        && microphone_signal_supported
        && source == Some(DetectionSource::MicrophoneActivity)
        && !candidates.is_empty()
}

fn should_auto_listen(
    candidates: &BTreeSet<MeetingApp>,
    source: Option<DetectionSource>,
    enabled: bool,
    microphone_signal_supported: bool,
) -> bool {
    enabled
        && microphone_signal_supported
        && source == Some(DetectionSource::MicrophoneActivity)
        // A browser or Telegram may own the microphone for dictation or a voice message,
        // so on their own they stay confirmation prompts. They must not veto a dedicated
        // client, though: a browser tab that keeps the microphone open in the background
        // is common, and a Zoom or Teams call running next to it is still call-specific
        // evidence. Requiring at least one dedicated client also keeps an empty candidate
        // set — no signal at all — from reading as "nothing ambiguous here".
        && candidates.iter().any(|app| !is_ambiguous_client(*app))
}

/// Clients that hold the microphone for things other than calls (dictation, voice
/// messages), so their presence alone is not enough to record unattended.
fn is_ambiguous_client(app: MeetingApp) -> bool {
    matches!(app, MeetingApp::BrowserCall | MeetingApp::Telegram)
}

fn select_detection_signal(
    launched_apps: BTreeSet<MeetingApp>,
    microphone_apps: BTreeSet<MeetingApp>,
    microphone_signal_supported: bool,
) -> (BTreeSet<MeetingApp>, Option<DetectionSource>) {
    if !microphone_apps.is_empty() {
        return (microphone_apps, Some(DetectionSource::MicrophoneActivity));
    }
    // On macOS we can observe actual microphone use. A process launch alone is too weak:
    // opening Teams or Zoom does not mean a meeting has started. Platforms that do
    // not yet expose a microphone-session observer keep the bounded process-launch fallback.
    if !microphone_signal_supported && !launched_apps.is_empty() {
        return (launched_apps, Some(DetectionSource::NativeProcess));
    }
    (BTreeSet::new(), None)
}

#[derive(Debug, Clone, Copy, Default)]
struct DetectionSettings {
    detection_enabled: bool,
    auto_listening: bool,
    background_auto_recording: bool,
}

async fn detection_settings(app: &AppHandle<Wry>) -> DetectionSettings {
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    match manager.as_ref() {
        Some(manager) => {
            let settings = manager.get_settings().await;
            DetectionSettings {
                detection_enabled: settings.auto_meeting_detection,
                auto_listening: settings.auto_listening,
                background_auto_recording: settings.background_auto_recording,
            }
        }
        None => DetectionSettings::default(),
    }
}

/// Human-readable list of the detected clients, used to title a captured meeting.
fn label_for(apps: &[MeetingApp]) -> String {
    let names: Vec<&str> = apps.iter().map(|app| app.display()).collect();
    names.join(", ")
}

/// Release whichever consumer owns the current automatic capture session.
async fn stop_armed_mode(
    app: &AppHandle<Wry>,
    shared: &Arc<Mutex<AutoListeningSharedState>>,
    mode: Option<AutoCaptureMode>,
) {
    match mode {
        Some(AutoCaptureMode::Background) => {
            app.state::<crate::background_capture::BackgroundCaptureState>()
                .stop_and_finalize(app)
                .await;
        }
        // With no mode recorded there may still be an open capture session row from
        // an earlier start; `stop_auto_listening` is a no-op when there is not.
        Some(AutoCaptureMode::Listening) | None => stop_auto_listening(app, shared).await,
    }
}

async fn start_auto_listening(
    app: &AppHandle<Wry>,
    shared: &Arc<Mutex<AutoListeningSharedState>>,
    apps: Vec<MeetingApp>,
    source: DetectionSource,
) -> Result<(), String> {
    let session_id = Uuid::new_v4().to_string();
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "application database state is unavailable".to_string())?;
    let client_kinds = serde_json::to_string(&apps).unwrap_or_else(|_| "[]".to_string());
    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|error| format!("could not begin capture session: {error}"))?;
    sqlx::query(
        "INSERT INTO capture_sessions( \
                id, source, client_kinds, status, capture_started_at, retention_expires_at \
             ) VALUES(?, ?, ?, 'start_requested', datetime('now'), \
                      datetime('now', '+' || (SELECT unpromoted_retention_minutes \
                        FROM capture_retention_policy WHERE id=1) || ' minutes'))",
    )
    .bind(&session_id)
    .bind(match source {
        DetectionSource::NativeProcess => "native_process",
        DetectionSource::MicrophoneActivity => "microphone_activity",
    })
    .bind(client_kinds)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("could not persist capture session: {error}"))?;
    sqlx::query(
        "INSERT INTO capture_observations( \
                capture_session_id, offset_ms, signal_kind, signal_state, source, confidence \
             ) VALUES(?, 0, 'client', 'active', ?, 0.95)",
    )
    .bind(&session_id)
    .bind(match source {
        DetectionSource::NativeProcess => "native_process",
        DetectionSource::MicrophoneActivity => "microphone_activity",
    })
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("could not persist capture transition: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("could not commit capture session: {error}"))?;
    if let Ok(mut shared) = shared.lock() {
        shared.active_capture_session_id = Some(session_id.clone());
    }
    if let Err(error) = app.emit(
        "auto-listening-start-requested",
        AutoListeningEvent {
            session_id: session_id.clone(),
            apps,
            source,
        },
    ) {
        if let Ok(mut shared) = shared.lock() {
            shared.active_capture_session_id = None;
        }
        let _ = sqlx::query(
            "UPDATE capture_sessions SET status='failed', failure_reason='start_failed', \
                    updated_at=datetime('now') WHERE id=?",
        )
        .bind(&session_id)
        .execute(state.db_manager.pool())
        .await;
        return Err(format!("could not request recording start: {error}"));
    }
    Ok(())
}

async fn stop_auto_listening(app: &AppHandle<Wry>, shared: &Arc<Mutex<AutoListeningSharedState>>) {
    let session_id = shared
        .lock()
        .ok()
        .and_then(|mut state| state.active_capture_session_id.take());
    let Some(session_id) = session_id else { return };

    if let Some(state) = app.try_state::<AppState>() {
        if let Err(error) = sqlx::query(
            "UPDATE capture_sessions \
             SET signal_ended_at=datetime('now'), ended_at=datetime('now'), \
                 status=CASE WHEN status='failed' THEN status ELSE 'stop_requested' END, \
                 end_reason=COALESCE(end_reason, 'call_signal_ended'), \
                 updated_at=datetime('now') \
             WHERE id=?",
        )
        .bind(&session_id)
        .execute(state.db_manager.pool())
        .await
        {
            log::warn!("Could not persist auto-listening capture stop: {error}");
        } else if let Err(error) = sqlx::query(
            "INSERT INTO capture_observations( \
                capture_session_id, offset_ms, signal_kind, signal_state, source, confidence \
             ) SELECT id, MAX(0, CAST((julianday('now')-julianday(detected_at))*86400000 AS INTEGER)), \
                      'client', 'inactive', source, 0.95 FROM capture_sessions WHERE id=?",
        )
        .bind(&session_id)
        .execute(state.db_manager.pool())
        .await
        {
            log::warn!("Could not persist capture end transition: {error}");
        }
    }

    let _ = app.emit(
        "auto-listening-stop-requested",
        serde_json::json!({ "session_id": session_id }),
    );
}

#[derive(Debug, Deserialize)]
pub struct AutoListeningStartResult {
    pub session_id: String,
    pub success: bool,
    pub failure_reason: Option<String>,
}

#[tauri::command]
pub async fn report_auto_listening_start(
    input: AutoListeningStartResult,
    state: State<'_, AppState>,
    detector: State<'_, AutoMeetingDetectionState>,
) -> Result<(), String> {
    let failed_session = (!input.success).then(|| input.session_id.clone());
    let persisted = persist_auto_listening_start(state.db_manager.pool(), input).await;
    if let Some(session_id) = failed_session {
        if let Ok(mut shared) = detector.auto_listening.lock() {
            if shared.active_capture_session_id.as_deref() == Some(session_id.as_str()) {
                shared.active_capture_session_id = None;
            }
            shared.start_failed = true;
        }
    }
    persisted
}

async fn persist_auto_listening_start(
    pool: &sqlx::SqlitePool,
    input: AutoListeningStartResult,
) -> Result<(), String> {
    let status = if input.success { "recording" } else { "failed" };
    let failure_reason = input.failure_reason.filter(|reason| {
        matches!(
            reason.as_str(),
            "model_unavailable" | "permission_denied" | "start_failed"
        )
    });
    let update = sqlx::query(
        "UPDATE capture_sessions \
         SET status=?, \
             recording_started_at=CASE WHEN ? THEN datetime('now') ELSE recording_started_at END, \
             failure_reason=?, updated_at=datetime('now') \
         WHERE id=?",
    )
    .bind(status)
    .bind(input.success)
    .bind(failure_reason)
    .bind(&input.session_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if update.rows_affected() != 1 {
        return Err(format!("Capture session {} not found", input.session_id));
    }
    if input.success {
        sqlx::query(
            "INSERT INTO capture_observations( \
                capture_session_id, offset_ms, signal_kind, signal_state, source, confidence \
             ) SELECT id, MAX(0, CAST((julianday('now')-julianday(detected_at))*86400000 AS INTEGER)), \
                      'recording', 'started', 'recording_pipeline', 1.0 \
               FROM capture_sessions WHERE id=?",
        )
        .bind(&input.session_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn link_auto_listening_meeting(
    session_id: String,
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    persist_auto_listening_meeting(state.db_manager.pool(), &session_id, &meeting_id).await
}

async fn persist_auto_listening_meeting(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    meeting_id: &str,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE capture_sessions \
         SET meeting_id=?, status='saved', ended_at=COALESCE(ended_at, datetime('now')), \
             updated_at=datetime('now') \
         WHERE id=?",
    )
    .bind(meeting_id)
    .bind(session_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE meetings SET \
            retention_days=COALESCE(retention_days, (SELECT saved_audio_retention_days \
                FROM capture_retention_policy WHERE id=1)), \
            retention_expires_at=COALESCE(retention_expires_at, CASE WHEN (SELECT \
                saved_audio_retention_days FROM capture_retention_policy WHERE id=1) IS NULL \
                THEN NULL ELSE datetime('now', '+' || (SELECT saved_audio_retention_days \
                FROM capture_retention_policy WHERE id=1) || ' days') END) \
         WHERE id=?",
    )
    .bind(meeting_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let end_offset_ms: Option<i64> = sqlx::query_scalar(
        "SELECT CAST(MAX(audio_end_time) * 1000.0 AS INTEGER) \
         FROM transcripts WHERE meeting_id=?",
    )
    .bind(meeting_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let existing_call_window: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM meeting_windows WHERE capture_session_id=? AND meeting_id=? \
         AND boundary_source='call_signal' ORDER BY id LIMIT 1",
    )
    .bind(session_id)
    .bind(meeting_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    if let Some(window_id) = existing_call_window {
        sqlx::query(
            "UPDATE meeting_windows SET end_offset_ms=?, updated_at=datetime('now') WHERE id=?",
        )
        .bind(end_offset_ms)
        .bind(window_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query(
            "INSERT INTO meeting_windows( \
                 capture_session_id, meeting_id, start_offset_ms, end_offset_ms, \
                 boundary_source, confidence \
             ) VALUES(?, ?, 0, ?, 'call_signal', 0.8)",
        )
        .bind(session_id)
        .bind(meeting_id)
        .bind(end_offset_ms)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    // A long transcript gap is a reviewable split candidate, never an automatic
    // destructive split. Multiple content windows can refer to the same saved audio.
    sqlx::query(
        "DELETE FROM meeting_windows WHERE capture_session_id=? AND meeting_id=? \
         AND boundary_source='content_window' AND review_status='pending'",
    )
    .bind(session_id)
    .bind(meeting_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let segments: Vec<(f64, f64)> = sqlx::query_as(
        "SELECT audio_start_time, audio_end_time FROM transcripts WHERE meeting_id=? \
         ORDER BY audio_start_time",
    )
    .bind(meeting_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let mut content_windows = Vec::new();
    if let Some((first_start, first_end)) = segments.first().copied() {
        let mut window_start = (first_start * 1000.0).max(0.0) as i64;
        let mut window_end = (first_end * 1000.0).max(0.0) as i64;
        for (start, end) in segments.iter().copied().skip(1) {
            let start_ms = (start * 1000.0).max(0.0) as i64;
            let end_ms = (end * 1000.0).max(0.0) as i64;
            if start_ms.saturating_sub(window_end) >= 300_000 {
                content_windows.push((window_start, window_end));
                window_start = start_ms;
            }
            window_end = window_end.max(end_ms);
        }
        content_windows.push((window_start, window_end));
    }
    if content_windows.len() > 1 {
        for (start_ms, end_ms) in content_windows {
            sqlx::query(
                "INSERT INTO meeting_windows( \
                    capture_session_id, meeting_id, start_offset_ms, end_offset_ms, \
                    suggested_start_ms, suggested_end_ms, boundary_source, confidence \
                 ) VALUES(?, ?, ?, ?, ?, ?, 'content_window', 0.72)",
            )
            .bind(session_id)
            .bind(meeting_id)
            .bind(start_ms)
            .bind(end_ms)
            .bind(start_ms)
            .bind(end_ms)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        }
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    // Every automatically captured meeting enters the reviewable Inbox first. Type and
    // series placement are suggestions; a classifier failure must never make saving fail.
    if let Err(error) =
        crate::learning::classification::prepare_saved_meeting(pool, meeting_id).await
    {
        log::warn!(
            "Could not prepare auto-listening meeting {meeting_id} for classification: {error}"
        );
    }
    Ok(())
}

async fn recover_interrupted_capture_sessions(pool: &sqlx::SqlitePool) -> Result<(), String> {
    sqlx::query(
        "UPDATE capture_sessions SET \
            status=CASE WHEN meeting_id IS NULL THEN 'failed' ELSE 'recovered' END, \
            failure_reason=CASE WHEN meeting_id IS NULL THEN 'start_failed' ELSE failure_reason END, \
            end_reason='app_interrupted', ended_at=COALESCE(ended_at, datetime('now')), \
            updated_at=datetime('now') \
         WHERE status IN ('candidate', 'start_requested', 'recording', 'stop_requested')",
    )
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn purge_expired_capture_data(pool: &sqlx::SqlitePool) -> Result<usize, String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let expired: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM capture_sessions WHERE meeting_id IS NULL \
         AND retention_expires_at IS NOT NULL \
         AND datetime(retention_expires_at)<=datetime('now')",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    for session_id in &expired {
        sqlx::query("DELETE FROM capture_observations WHERE capture_session_id=?")
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        sqlx::query(
            "UPDATE capture_sessions SET status='discarded', end_reason='retention_expired', \
             updated_at=datetime('now') WHERE id=?",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(expired.len())
}

pub async fn purge_expired_saved_captures(pool: &sqlx::SqlitePool) -> Result<Vec<String>, String> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT DISTINCT m.id, m.folder_path FROM meetings m \
         JOIN capture_sessions cs ON cs.meeting_id=m.id AND cs.capture_mode='auto' \
         WHERE m.retention_expires_at IS NOT NULL \
           AND datetime(m.retention_expires_at)<=datetime('now') \
         ORDER BY m.retention_expires_at",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut deleted = Vec::new();
    for (meeting_id, folder_path) in rows {
        if let Some(folder_path) = folder_path.filter(|value| !value.trim().is_empty()) {
            if let Err(error) =
                crate::api::api::delete_recording_folder(std::path::Path::new(&folder_path))
            {
                log::warn!(
                    "Expired auto-capture {meeting_id} retained because its folder could not be safely deleted: {error}"
                );
                continue;
            }
        }
        if crate::database::repositories::meeting::MeetingsRepository::delete_meeting(
            pool,
            &meeting_id,
        )
        .await
        .map_err(|error| error.to_string())?
        {
            deleted.push(meeting_id);
        }
    }
    Ok(deleted)
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureRetentionPolicy {
    pub unpromoted_retention_minutes: i64,
    pub saved_audio_retention_days: Option<i64>,
    pub local_only: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCaptureRetentionPolicy {
    pub unpromoted_retention_minutes: i64,
    pub saved_audio_retention_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingWindowRow {
    pub id: i64,
    pub capture_session_id: String,
    pub meeting_id: String,
    pub start_offset_ms: i64,
    pub end_offset_ms: Option<i64>,
    pub suggested_start_ms: Option<i64>,
    pub suggested_end_ms: Option<i64>,
    pub confirmed_start_ms: Option<i64>,
    pub confirmed_end_ms: Option<i64>,
    pub boundary_source: String,
    pub confidence: Option<f64>,
    pub review_status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMeetingWindowInput {
    pub window_id: i64,
    pub status: String,
    pub start_offset_ms: Option<i64>,
    pub end_offset_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitMeetingWindowInput {
    pub window_id: i64,
    pub split_offset_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeMeetingWindowsInput {
    pub first_window_id: i64,
    pub second_window_id: i64,
}

#[tauri::command]
pub async fn get_capture_retention_policy(
    state: State<'_, AppState>,
) -> Result<CaptureRetentionPolicy, String> {
    let row = sqlx::query(
        "SELECT unpromoted_retention_minutes, saved_audio_retention_days, local_only \
         FROM capture_retention_policy WHERE id=1",
    )
    .fetch_one(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(CaptureRetentionPolicy {
        unpromoted_retention_minutes: row.get("unpromoted_retention_minutes"),
        saved_audio_retention_days: row.get("saved_audio_retention_days"),
        local_only: row.get::<i64, _>("local_only") != 0,
    })
}

#[tauri::command]
pub async fn update_capture_retention_policy(
    input: UpdateCaptureRetentionPolicy,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !(1..=1440).contains(&input.unpromoted_retention_minutes)
        || input
            .saved_audio_retention_days
            .is_some_and(|days| !(1..=3650).contains(&days))
    {
        return Err("Capture retention values are outside supported bounds".to_string());
    }
    sqlx::query(
        "UPDATE capture_retention_policy SET unpromoted_retention_minutes=?, \
            saved_audio_retention_days=?, local_only=1, \
            updated_at=datetime('now') WHERE id=1",
    )
    .bind(input.unpromoted_retention_minutes)
    .bind(input.saved_audio_retention_days)
    .execute(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn list_meeting_windows(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MeetingWindowRow>, String> {
    let rows = sqlx::query(
        "SELECT id, capture_session_id, meeting_id, start_offset_ms, end_offset_ms, \
                suggested_start_ms, suggested_end_ms, confirmed_start_ms, confirmed_end_ms, \
                boundary_source, confidence, review_status FROM meeting_windows \
         WHERE meeting_id=? AND review_status<>'superseded' ORDER BY start_offset_ms, id",
    )
    .bind(meeting_id)
    .fetch_all(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| MeetingWindowRow {
            id: row.get("id"),
            capture_session_id: row.get("capture_session_id"),
            meeting_id: row.get("meeting_id"),
            start_offset_ms: row.get("start_offset_ms"),
            end_offset_ms: row.get("end_offset_ms"),
            suggested_start_ms: row.get("suggested_start_ms"),
            suggested_end_ms: row.get("suggested_end_ms"),
            confirmed_start_ms: row.get("confirmed_start_ms"),
            confirmed_end_ms: row.get("confirmed_end_ms"),
            boundary_source: row.get("boundary_source"),
            confidence: row.get("confidence"),
            review_status: row.get("review_status"),
        })
        .collect())
}

#[tauri::command]
pub async fn review_meeting_window(
    input: ReviewMeetingWindowInput,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !matches!(input.status.as_str(), "accepted" | "rejected") {
        return Err("status must be accepted or rejected".to_string());
    }
    let row = sqlx::query("SELECT start_offset_ms, end_offset_ms FROM meeting_windows WHERE id=?")
        .bind(input.window_id)
        .fetch_optional(state.db_manager.pool())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Meeting window {} not found", input.window_id))?;
    let start = input
        .start_offset_ms
        .unwrap_or_else(|| row.get("start_offset_ms"));
    let end = input
        .end_offset_ms
        .or_else(|| row.get::<Option<i64>, _>("end_offset_ms"));
    if start < 0 || end.is_some_and(|value| value < start) {
        return Err("Meeting window bounds are invalid".to_string());
    }
    sqlx::query(
        "UPDATE meeting_windows SET review_status=?, is_confirmed=?, \
            confirmed_start_ms=CASE WHEN ?='accepted' THEN ? ELSE NULL END, \
            confirmed_end_ms=CASE WHEN ?='accepted' THEN ? ELSE NULL END, \
            updated_at=datetime('now') WHERE id=?",
    )
    .bind(&input.status)
    .bind(i64::from(input.status == "accepted"))
    .bind(&input.status)
    .bind(start)
    .bind(&input.status)
    .bind(end)
    .bind(input.window_id)
    .execute(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn split_meeting_window(
    input: SplitMeetingWindowInput,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let row = sqlx::query(
        "SELECT capture_session_id, meeting_id, start_offset_ms, end_offset_ms \
         FROM meeting_windows WHERE id=? AND review_status<>'superseded'",
    )
    .bind(input.window_id)
    .fetch_optional(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("Meeting window {} not found", input.window_id))?;
    let start: i64 = row.get("start_offset_ms");
    let end: Option<i64> = row.get("end_offset_ms");
    if input.split_offset_ms <= start || end.is_none_or(|value| input.split_offset_ms >= value) {
        return Err("Split point must be inside the meeting window".to_string());
    }
    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE meeting_windows SET end_offset_ms=?, boundary_source='manual', \
            review_status='pending', is_confirmed=0, updated_at=datetime('now') WHERE id=?",
    )
    .bind(input.split_offset_ms)
    .bind(input.window_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let new_id: i64 = sqlx::query_scalar(
        "INSERT INTO meeting_windows( \
            capture_session_id, meeting_id, start_offset_ms, end_offset_ms, \
            boundary_source, confidence \
         ) VALUES(?, ?, ?, ?, 'manual', 1.0) RETURNING id",
    )
    .bind(row.get::<String, _>("capture_session_id"))
    .bind(row.get::<String, _>("meeting_id"))
    .bind(input.split_offset_ms)
    .bind(end)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(new_id)
}

#[tauri::command]
pub async fn merge_meeting_windows(
    input: MergeMeetingWindowsInput,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT id, capture_session_id, meeting_id, start_offset_ms, end_offset_ms \
         FROM meeting_windows WHERE id IN (?, ?) AND review_status<>'superseded'",
    )
    .bind(input.first_window_id)
    .bind(input.second_window_id)
    .fetch_all(state.db_manager.pool())
    .await
    .map_err(|error| error.to_string())?;
    if rows.len() != 2
        || rows[0].get::<String, _>("capture_session_id")
            != rows[1].get::<String, _>("capture_session_id")
        || rows[0].get::<String, _>("meeting_id") != rows[1].get::<String, _>("meeting_id")
    {
        return Err("Windows must belong to the same capture and meeting".to_string());
    }
    let start = rows
        .iter()
        .map(|row| row.get::<i64, _>("start_offset_ms"))
        .min()
        .unwrap_or(0);
    let end = rows
        .iter()
        .filter_map(|row| row.get::<Option<i64>, _>("end_offset_ms"))
        .max();
    let mut tx = state
        .db_manager
        .pool()
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE meeting_windows SET start_offset_ms=?, end_offset_ms=?, \
            boundary_source='manual', review_status='pending', is_confirmed=0, \
            updated_at=datetime('now') WHERE id=?",
    )
    .bind(start)
    .bind(end)
    .bind(input.first_window_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "UPDATE meeting_windows SET review_status='superseded', updated_at=datetime('now') WHERE id=?",
    )
    .bind(input.second_window_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn deliver_detection(app: &AppHandle<Wry>, event: MeetingDetectedEvent) {
    let main_is_focused = app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    });

    // Always deliver the prompt to the webview. Meeting apps normally own focus while a
    // call is active, and relying exclusively on a native notification in that state makes
    // the prompt disappear when macOS notifications are denied or delivered quietly. The
    // webview listener remains mounted while the window is hidden and will show the prompt
    // when the user returns to Memento.
    let _ = app.emit("auto-meeting-detected", event);

    if main_is_focused {
        return;
    }

    // Native notifications can be hidden by Focus mode, denied permissions, or quiet
    // delivery. Request OS-level attention as a second, non-focus-stealing signal. On macOS
    // this bounces the Dock icon until Memento becomes active; on Windows it flashes the
    // taskbar entry. Activating the app clears the signal automatically.
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.request_user_attention(Some(UserAttentionType::Critical)) {
            log::warn!("Failed to request meeting detection attention: {error}");
        }
    }

    // Desktop notification actions are not consistently available across platforms.
    // The native notification only reminds the user; recording still starts from the app.
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    if let Some(manager) = manager.as_ref() {
        let _ = manager.show_meeting_detected().await;
    }
}

/// Take back a prompt the user never answered. Only the webview banner is withdrawn:
/// a native notification is already dismissible by the user, and nothing is recording.
fn withdraw_detection(app: &AppHandle<Wry>) {
    let _ = app.emit("auto-meeting-detection-ended", ());
}

fn collect_native_apps(system: &System) -> BTreeSet<MeetingApp> {
    let mut apps = BTreeSet::new();
    for process in system.processes().values() {
        let name = process.name().to_string_lossy();
        if let Some(app) = classify_native_process(&name) {
            apps.insert(app);
            continue;
        }
        if let Some(executable) = process.exe().and_then(|path| path.file_name()) {
            if let Some(app) = classify_native_process(&executable.to_string_lossy()) {
                apps.insert(app);
            }
        }
    }
    apps
}

fn normalized_identity(value: &str) -> String {
    let lowercase = value.trim().to_lowercase();
    let without_executable_suffix = lowercase
        .strip_suffix(".exe")
        .or_else(|| lowercase.strip_suffix(".app"))
        .unwrap_or(&lowercase);
    without_executable_suffix
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn classify_native_process(value: &str) -> Option<MeetingApp> {
    match normalized_identity(value).as_str() {
        "zoom" | "zoomus" => Some(MeetingApp::Zoom),
        "msteams" | "microsoftteams" | "teams" => Some(MeetingApp::MicrosoftTeams),
        "telegram" | "telegramdesktop" => Some(MeetingApp::Telegram),
        "telemost" | "yandextelemost" => Some(MeetingApp::YandexTelemost),
        "jazz" | "salutejazz" | "sberjazz" => Some(MeetingApp::SaluteJazz),
        _ => None,
    }
}

fn classify_audio_process(bundle_id: &str, display_name: &str) -> Option<MeetingApp> {
    let bundle = bundle_id.to_ascii_lowercase();
    if bundle.contains("zoom") {
        return Some(MeetingApp::Zoom);
    }
    if bundle.contains("microsoft.teams") || bundle.contains("msteams") {
        return Some(MeetingApp::MicrosoftTeams);
    }
    if bundle.contains("telegram") || bundle.contains("telegra") {
        return Some(MeetingApp::Telegram);
    }
    if bundle.contains("telemost") {
        return Some(MeetingApp::YandexTelemost);
    }
    if bundle.contains("salutejazz") || bundle.contains("jazz-app") {
        return Some(MeetingApp::SaluteJazz);
    }
    if is_browser_identity(&bundle) || is_browser_identity(&normalized_identity(display_name)) {
        return Some(MeetingApp::BrowserCall);
    }

    // Some native clients do not expose a bundle identifier for helper audio processes.
    classify_native_process(display_name)
}

fn is_browser_identity(value: &str) -> bool {
    if matches!(value, "arc" | "opera") {
        return true;
    }
    const BROWSER_MARKERS: &[&str] = &[
        "com.google.chrome",
        "googlechrome",
        "com.apple.safari",
        "safari",
        "org.mozilla.firefox",
        "firefox",
        "com.microsoft.edgemac",
        "microsoftedge",
        "ru.yandex.desktop.yandex-browser",
        "yandexbrowser",
        "com.brave.browser",
        "bravebrowser",
        "company.thebrowser.browser",
        "arcbrowser",
        "com.operasoftware.opera",
        "operabrowser",
    ];
    BROWSER_MARKERS
        .iter()
        .any(|marker| value.starts_with(marker))
}

#[cfg(target_os = "macos")]
fn active_microphone_apps() -> BTreeSet<MeetingApp> {
    use cidre::core_audio as ca;

    let Ok(processes) = ca::System::processes() else {
        return BTreeSet::new();
    };

    processes
        .into_iter()
        .filter(|process| process.is_running_input().unwrap_or(false))
        .filter_map(|process| {
            let bundle_id = process
                .bundle_id()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let display_name = process
                .pid()
                .ok()
                .and_then(cidre::ns::RunningApp::with_pid)
                .and_then(|app| app.localized_name().map(|name| name.to_string()))
                .unwrap_or_default();
            classify_audio_process(&bundle_id, &display_name)
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn active_microphone_apps() -> BTreeSet<MeetingApp> {
    // A browser process alone is not enough evidence of a call. Platform-specific
    // microphone-session observers can be added here without changing the state machine.
    BTreeSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn apps(values: &[MeetingApp]) -> BTreeSet<MeetingApp> {
        values.iter().copied().collect()
    }

    async fn auto_listening_test_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE meetings(id TEXT PRIMARY KEY, retention_days INTEGER, retention_expires_at TEXT)",
            "CREATE TABLE transcripts(meeting_id TEXT, audio_start_time REAL DEFAULT 0, audio_end_time REAL)",
            "CREATE TABLE capture_sessions(\
                id TEXT PRIMARY KEY, status TEXT NOT NULL, recording_started_at TEXT, \
                failure_reason TEXT, meeting_id TEXT, detected_at TEXT DEFAULT CURRENT_TIMESTAMP, \
                source TEXT DEFAULT 'microphone_activity', end_reason TEXT, ended_at TEXT, updated_at TEXT\
            )",
            "CREATE TABLE capture_observations(\
                id INTEGER PRIMARY KEY, capture_session_id TEXT, offset_ms INTEGER, \
                signal_kind TEXT, signal_state TEXT, source TEXT, confidence REAL\
            )",
            "CREATE TABLE capture_retention_policy(\
                id INTEGER PRIMARY KEY, unpromoted_retention_minutes INTEGER, \
                saved_audio_retention_days INTEGER, local_only INTEGER\
            )",
            "CREATE TABLE meeting_windows(\
                id INTEGER PRIMARY KEY, capture_session_id TEXT NOT NULL, meeting_id TEXT NOT NULL, \
                start_offset_ms INTEGER NOT NULL, end_offset_ms INTEGER, \
                suggested_start_ms INTEGER, suggested_end_ms INTEGER, confirmed_start_ms INTEGER, \
                confirmed_end_ms INTEGER, boundary_source TEXT NOT NULL, confidence REAL, \
                is_confirmed INTEGER DEFAULT 0, review_status TEXT DEFAULT 'pending', \
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP\
            )",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO capture_retention_policy VALUES(1, 15, NULL, 1)")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn persisted_auto_listening_session_links_to_saved_meeting_window() {
        let pool = auto_listening_test_pool().await;
        sqlx::query(
            "INSERT INTO capture_sessions(id, status) VALUES('capture-1', 'start_requested')",
        )
        .execute(&pool)
        .await
        .unwrap();
        persist_auto_listening_start(
            &pool,
            AutoListeningStartResult {
                session_id: "capture-1".into(),
                success: true,
                failure_reason: None,
            },
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO meetings(id) VALUES('meeting-1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO transcripts(meeting_id, audio_end_time) VALUES('meeting-1', 12.345)",
        )
        .execute(&pool)
        .await
        .unwrap();

        persist_auto_listening_meeting(&pool, "capture-1", "meeting-1")
            .await
            .unwrap();

        let capture: (String, Option<String>, bool) = sqlx::query_as(
            "SELECT status, meeting_id, recording_started_at IS NOT NULL \
             FROM capture_sessions WHERE id='capture-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(capture, ("saved".into(), Some("meeting-1".into()), true));
        let window: (i64, Option<i64>, String) = sqlx::query_as(
            "SELECT start_offset_ms, end_offset_ms, boundary_source \
             FROM meeting_windows WHERE capture_session_id='capture-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(window, (0, Some(12_345), "call_signal".into()));
    }

    #[tokio::test]
    async fn persisted_start_failure_keeps_only_bounded_reason_codes() {
        let pool = auto_listening_test_pool().await;
        sqlx::query(
            "INSERT INTO capture_sessions(id, status) VALUES('capture-2', 'start_requested')",
        )
        .execute(&pool)
        .await
        .unwrap();
        persist_auto_listening_start(
            &pool,
            AutoListeningStartResult {
                session_id: "capture-2".into(),
                success: false,
                failure_reason: Some("raw provider error with sensitive context".into()),
            },
        )
        .await
        .unwrap();
        let failure: (String, Option<String>) = sqlx::query_as(
            "SELECT status, failure_reason FROM capture_sessions WHERE id='capture-2'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(failure, ("failed".into(), None));
    }

    #[test]
    fn classifies_supported_native_clients_without_matching_helpers_or_unrelated_apps() {
        assert_eq!(classify_native_process("zoom.us"), Some(MeetingApp::Zoom));
        assert_eq!(
            classify_native_process("ms-teams.exe"),
            Some(MeetingApp::MicrosoftTeams)
        );
        assert_eq!(
            classify_native_process("Yandex.Telemost"),
            Some(MeetingApp::YandexTelemost)
        );
        assert_eq!(
            classify_native_process("Jazz"),
            Some(MeetingApp::SaluteJazz)
        );

        assert_eq!(
            classify_native_process("Telegram"),
            Some(MeetingApp::Telegram)
        );

        assert_eq!(classify_native_process("zoomautoupdater"), None);
        assert_eq!(classify_native_process("TeamViewer"), None);
        assert_eq!(classify_native_process("chrome_crashpad_handler"), None);
    }

    #[test]
    fn microphone_activity_recognizes_browsers_and_native_bundle_ids() {
        assert_eq!(
            classify_audio_process("com.google.Chrome", "Google Chrome Helper (Renderer)"),
            Some(MeetingApp::BrowserCall)
        );
        assert_eq!(
            classify_audio_process("salutejazz.jazz-app", "Jazz Helper"),
            Some(MeetingApp::SaluteJazz)
        );
        assert_eq!(
            classify_audio_process("", "Arc"),
            Some(MeetingApp::BrowserCall)
        );
        assert_eq!(
            classify_audio_process("", "Opera"),
            Some(MeetingApp::BrowserCall)
        );
        assert_eq!(
            classify_audio_process("com.microsoft.VSCode", "Code Helper"),
            None
        );
        assert_eq!(
            classify_audio_process("org.telegram.desktop", "Telegram"),
            Some(MeetingApp::Telegram)
        );
    }

    #[test]
    fn macos_requires_microphone_evidence_instead_of_process_launch_only() {
        let (candidates, source) =
            select_detection_signal(apps(&[MeetingApp::Zoom]), BTreeSet::new(), true);
        assert!(candidates.is_empty());
        assert_eq!(source, None);

        let (candidates, source) = select_detection_signal(
            apps(&[MeetingApp::Zoom]),
            apps(&[MeetingApp::BrowserCall]),
            true,
        );
        assert_eq!(candidates, apps(&[MeetingApp::BrowserCall]));
        assert_eq!(source, Some(DetectionSource::MicrophoneActivity));
    }

    #[test]
    fn browser_microphone_activity_remains_confirmation_only() {
        assert!(!should_auto_listen(
            &apps(&[MeetingApp::BrowserCall]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
        assert!(should_auto_listen(
            &apps(&[MeetingApp::Zoom]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
        assert!(!should_auto_listen(
            &apps(&[MeetingApp::Telegram]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
    }

    #[test]
    fn an_ambiguous_client_does_not_veto_a_dedicated_one() {
        // A browser tab holding the microphone next to a Zoom call is the common case,
        // and it must not downgrade the call to a confirmation prompt.
        assert!(should_auto_listen(
            &apps(&[MeetingApp::BrowserCall, MeetingApp::Zoom]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
        assert!(should_auto_listen(
            &apps(&[MeetingApp::Telegram, MeetingApp::MicrosoftTeams]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
        assert!(!should_auto_listen(
            &apps(&[MeetingApp::BrowserCall, MeetingApp::Telegram]),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
    }

    #[test]
    fn no_candidates_never_arms_auto_listening() {
        assert!(!should_auto_listen(
            &BTreeSet::new(),
            Some(DetectionSource::MicrophoneActivity),
            true,
            true,
        ));
    }

    #[test]
    fn background_capture_accepts_every_recognized_client_on_a_microphone_signal() {
        for app in [
            MeetingApp::Zoom,
            MeetingApp::BrowserCall,
            MeetingApp::Telegram,
        ] {
            assert!(
                should_capture_in_background(
                    &apps(&[app]),
                    Some(DetectionSource::MicrophoneActivity),
                    true,
                    true,
                ),
                "{app:?} should be captured in the background"
            );
        }
    }

    #[test]
    fn background_capture_needs_the_setting_a_signal_and_a_supported_platform() {
        let candidates = apps(&[MeetingApp::Zoom]);
        assert!(!should_capture_in_background(
            &candidates,
            Some(DetectionSource::MicrophoneActivity),
            false,
            true,
        ));
        assert!(!should_capture_in_background(
            &candidates,
            Some(DetectionSource::MicrophoneActivity),
            true,
            false,
        ));
        // Process-launch evidence alone never starts a silent recording.
        assert!(!should_capture_in_background(
            &candidates,
            Some(DetectionSource::NativeProcess),
            true,
            true,
        ));
        assert!(!should_capture_in_background(
            &BTreeSet::new(),
            None,
            true,
            true,
        ));
    }

    #[test]
    fn auto_listening_wins_over_background_capture_for_the_same_call() {
        let both_on = DetectionSettings {
            detection_enabled: true,
            auto_listening: true,
            background_auto_recording: true,
        };
        let source = Some(DetectionSource::MicrophoneActivity);

        // A native client that auto-listening accepts is recorded live, once.
        assert_eq!(
            mode_to_arm(&apps(&[MeetingApp::Zoom]), source, both_on, true),
            Some(AutoCaptureMode::Listening)
        );

        // Auto-listening keeps browser and Telegram calls as confirmation prompts, so
        // background capture is what actually records them.
        assert_eq!(
            mode_to_arm(&apps(&[MeetingApp::BrowserCall]), source, both_on, true),
            Some(AutoCaptureMode::Background)
        );
    }

    #[test]
    fn background_capture_takes_native_calls_when_auto_listening_is_off() {
        let settings = DetectionSettings {
            detection_enabled: true,
            auto_listening: false,
            background_auto_recording: true,
        };
        assert_eq!(
            mode_to_arm(
                &apps(&[MeetingApp::Zoom]),
                Some(DetectionSource::MicrophoneActivity),
                settings,
                true,
            ),
            Some(AutoCaptureMode::Background)
        );
    }

    #[test]
    fn nothing_arms_when_both_modes_are_off() {
        let settings = DetectionSettings {
            detection_enabled: true,
            auto_listening: false,
            background_auto_recording: false,
        };
        assert_eq!(
            mode_to_arm(
                &apps(&[MeetingApp::Zoom]),
                Some(DetectionSource::MicrophoneActivity),
                settings,
                true,
            ),
            None
        );
    }

    #[test]
    fn label_names_every_detected_client() {
        assert_eq!(
            label_for(&[MeetingApp::Zoom, MeetingApp::BrowserCall]),
            "Zoom, Browser call"
        );
        assert_eq!(label_for(&[]), "");
    }

    #[test]
    fn unsupported_platforms_keep_bounded_native_process_fallback() {
        let (candidates, source) =
            select_detection_signal(apps(&[MeetingApp::Zoom]), BTreeSet::new(), false);
        assert_eq!(candidates, apps(&[MeetingApp::Zoom]));
        assert_eq!(source, Some(DetectionSource::NativeProcess));
    }

    #[test]
    fn detector_coalesces_one_notification_per_active_session() {
        let mut session = DetectionSession::default();
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::SuggestRecording
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(false, false, true, false),
            DetectionEvent::None
        );
        // The call is over, so the unanswered prompt is taken back before the session
        // is free to suggest again.
        assert_eq!(
            session.observe(false, false, true, false),
            DetectionEvent::WithdrawSuggestion
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::SuggestRecording
        );
    }

    #[test]
    fn a_suggestion_is_withdrawn_only_once_and_only_when_one_was_raised() {
        let mut session = DetectionSession::default();
        // Quiet polls with nothing suggested stay silent.
        for _ in 0..4 {
            assert_eq!(
                session.observe(false, false, true, false),
                DetectionEvent::None
            );
        }

        session.observe(true, false, true, false);
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::SuggestRecording
        );
        session.observe(false, false, true, false);
        assert_eq!(
            session.observe(false, false, true, false),
            DetectionEvent::WithdrawSuggestion
        );
        // Withdrawn once: staying quiet must not re-emit it every poll.
        for _ in 0..4 {
            assert_eq!(
                session.observe(false, false, true, false),
                DetectionEvent::None
            );
        }
    }

    #[test]
    fn switching_detection_off_takes_back_an_open_suggestion() {
        let mut session = DetectionSession::default();
        session.observe(true, false, true, false);
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::SuggestRecording
        );
        assert_eq!(
            session.observe(false, false, false, false),
            DetectionEvent::WithdrawSuggestion
        );
        assert_eq!(
            session.observe(false, false, false, false),
            DetectionEvent::None
        );
    }

    #[test]
    fn recording_consumes_the_detection_session() {
        let mut session = DetectionSession::default();
        assert_eq!(
            session.observe(true, true, true, true),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, true, true, true),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, true, true),
            DetectionEvent::None
        );
    }

    #[test]
    fn disabled_detection_resets_pending_evidence() {
        let mut session = DetectionSession::default();
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, false, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::None
        );
        assert_eq!(
            session.observe(true, false, true, false),
            DetectionEvent::SuggestRecording
        );
    }

    #[test]
    fn auto_listening_starts_once_and_waits_through_short_signal_gaps() {
        let mut session = DetectionSession::default();
        assert_eq!(
            session.observe(true, false, true, true),
            DetectionEvent::StartAutoListening
        );
        assert_eq!(
            session.observe(true, true, true, true),
            DetectionEvent::None
        );
        for _ in 0..AUTO_LISTENING_QUIET_POLLS - 1 {
            assert_eq!(
                session.observe(false, true, true, true),
                DetectionEvent::None
            );
        }
        assert_eq!(
            session.observe(true, true, true, true),
            DetectionEvent::None
        );
        for _ in 0..AUTO_LISTENING_QUIET_POLLS - 1 {
            assert_eq!(
                session.observe(false, true, true, true),
                DetectionEvent::None
            );
        }
        assert_eq!(
            session.observe(false, true, true, true),
            DetectionEvent::StopAutoListening
        );
    }

    #[test]
    fn disabling_detection_stops_an_active_auto_listening_session() {
        let mut session = DetectionSession::default();
        assert_eq!(
            session.observe(true, false, true, true),
            DetectionEvent::StartAutoListening
        );
        assert_eq!(
            session.observe(false, true, false, false),
            DetectionEvent::StopAutoListening
        );
    }

    #[test]
    fn cancelled_runtime_gets_a_fresh_token_before_restart() {
        let mut runtime = DetectionRuntime {
            cancellation: CancellationToken::new(),
            task: None,
        };
        runtime.cancellation.cancel();

        let next = AutoMeetingDetectionState::cancellation_for_start(&mut runtime);

        assert!(!next.is_cancelled());
        assert!(!runtime.cancellation.is_cancelled());
    }

    #[test]
    fn process_launches_are_not_reported_from_the_startup_baseline_and_expire() {
        let now = Instant::now();
        let mut evidence = ProcessLaunchEvidence::default();

        assert!(evidence
            .observe(&apps(&[MeetingApp::MicrosoftTeams]), now)
            .is_empty());
        let launched = evidence.observe(
            &apps(&[MeetingApp::MicrosoftTeams, MeetingApp::Zoom]),
            now + Duration::from_secs(1),
        );
        assert_eq!(launched, apps(&[MeetingApp::Zoom]));

        let expired = evidence.observe(
            &apps(&[MeetingApp::MicrosoftTeams, MeetingApp::Zoom]),
            now + PROCESS_LAUNCH_EVIDENCE_TTL + Duration::from_secs(2),
        );
        assert!(expired.is_empty());
    }
}
