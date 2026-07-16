//! Privacy-preserving meeting detection.
//!
//! The detector keeps process/audio evidence in memory only. It never records audio,
//! inspects browser URLs or window titles, or starts a recording without an explicit
//! user action.

use crate::notifications::commands::NotificationManagerState;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State, Wry};
use tokio_util::sync::CancellationToken;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const PROCESS_LAUNCH_EVIDENCE_TTL: Duration = Duration::from_secs(90);
const REQUIRED_ACTIVE_POLLS: u8 = 3;
const REQUIRED_QUIET_POLLS: u8 = 2;

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

/// Owns the background task so it is started once and can be cancelled on app exit.
pub struct AutoMeetingDetectionState {
    runtime: Mutex<DetectionRuntime>,
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
        runtime.task = Some(tauri::async_runtime::spawn(async move {
            run_detection_loop(app, cancellation).await;
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
    pub poll_interval_seconds: u64,
}

/// Privacy-safe diagnostics: deliberately excludes process names, event history and times.
#[tauri::command]
pub async fn get_auto_meeting_detection_status(
    detector: State<'_, AutoMeetingDetectionState>,
    notifications: State<'_, NotificationManagerState<Wry>>,
) -> Result<AutoMeetingDetectionStatus, String> {
    let enabled = match notifications.read().await.as_ref() {
        Some(manager) => manager.get_settings().await.auto_meeting_detection,
        None => false,
    };
    Ok(AutoMeetingDetectionStatus {
        enabled,
        running: detector.is_running(),
        microphone_signal_supported: cfg!(target_os = "macos"),
        poll_interval_seconds: POLL_INTERVAL.as_secs(),
    })
}

#[derive(Debug, Default)]
struct DetectionSession {
    active_polls: u8,
    quiet_polls: u8,
    notified: bool,
}

impl DetectionSession {
    /// Returns true exactly once for one continuous meeting signal.
    fn observe(&mut self, active: bool, recording: bool, enabled: bool) -> bool {
        if !enabled {
            *self = Self::default();
            return false;
        }

        if active {
            self.quiet_polls = 0;
            self.active_polls = self.active_polls.saturating_add(1);

            // A call already being recorded is a consumed detection session. This prevents
            // an immediate prompt if the user stops recording while the call is still open.
            if recording {
                self.notified = true;
                return false;
            }

            if !self.notified && self.active_polls >= REQUIRED_ACTIVE_POLLS {
                self.notified = true;
                return true;
            }
            return false;
        }

        self.active_polls = 0;
        self.quiet_polls = self.quiet_polls.saturating_add(1);
        if self.quiet_polls >= REQUIRED_QUIET_POLLS {
            self.notified = false;
            self.quiet_polls = 0;
        }
        false
    }
}

/// A process launch is useful evidence, but only for a bounded interval. Long-lived apps
/// such as Telegram must not keep the detector permanently active after one launch.
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

async fn run_detection_loop(app: AppHandle<Wry>, cancellation: CancellationToken) {
    let mut system = System::new_all();
    let mut process_evidence = ProcessLaunchEvidence::default();
    let mut session = DetectionSession::default();
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            _ = interval.tick() => {
                let enabled = detection_enabled(&app).await;
                if !enabled {
                    // Opt-out means no process or microphone inspection, not merely no prompt.
                    session = DetectionSession::default();
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
                let recording = crate::audio::recording_commands::is_recording().await;
                if session.observe(!candidates.is_empty(), recording, true) {
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
            }
        }
    }
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
    // opening Telegram, Teams, or Zoom does not mean a meeting has started. Platforms that do
    // not yet expose a microphone-session observer keep the bounded process-launch fallback.
    if !microphone_signal_supported && !launched_apps.is_empty() {
        return (launched_apps, Some(DetectionSource::NativeProcess));
    }
    (BTreeSet::new(), None)
}

async fn detection_enabled(app: &AppHandle<Wry>) -> bool {
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    match manager.as_ref() {
        Some(manager) => manager.get_settings().await.auto_meeting_detection,
        None => false,
    }
}

async fn deliver_detection(app: &AppHandle<Wry>, event: MeetingDetectedEvent) {
    let main_is_focused = app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    });

    if main_is_focused {
        let _ = app.emit("auto-meeting-detected", event);
        return;
    }

    // Desktop notification actions are not consistently available across platforms.
    // The native notification only reminds the user; recording still starts from the app.
    let state = app.state::<NotificationManagerState<Wry>>();
    let manager = state.read().await;
    if let Some(manager) = manager.as_ref() {
        let _ = manager.show_meeting_detected().await;
    }
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

    fn apps(values: &[MeetingApp]) -> BTreeSet<MeetingApp> {
        values.iter().copied().collect()
    }

    #[test]
    fn classifies_supported_native_clients_without_matching_helpers_or_unrelated_apps() {
        assert_eq!(classify_native_process("zoom.us"), Some(MeetingApp::Zoom));
        assert_eq!(
            classify_native_process("ms-teams.exe"),
            Some(MeetingApp::MicrosoftTeams)
        );
        assert_eq!(
            classify_native_process("Telegram"),
            Some(MeetingApp::Telegram)
        );
        assert_eq!(
            classify_native_process("Yandex.Telemost"),
            Some(MeetingApp::YandexTelemost)
        );
        assert_eq!(
            classify_native_process("Jazz"),
            Some(MeetingApp::SaluteJazz)
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
    }

    #[test]
    fn macos_requires_microphone_evidence_instead_of_process_launch_only() {
        let (candidates, source) =
            select_detection_signal(apps(&[MeetingApp::Telegram]), BTreeSet::new(), true);
        assert!(candidates.is_empty());
        assert_eq!(source, None);

        let (candidates, source) = select_detection_signal(
            apps(&[MeetingApp::Telegram]),
            apps(&[MeetingApp::BrowserCall]),
            true,
        );
        assert_eq!(candidates, apps(&[MeetingApp::BrowserCall]));
        assert_eq!(source, Some(DetectionSource::MicrophoneActivity));
    }

    #[test]
    fn unsupported_platforms_keep_bounded_native_process_fallback() {
        let (candidates, source) =
            select_detection_signal(apps(&[MeetingApp::Zoom]), BTreeSet::new(), false);
        assert_eq!(candidates, apps(&[MeetingApp::Zoom]));
        assert_eq!(source, Some(DetectionSource::NativeProcess));
    }

    #[test]
    fn detector_requires_stable_evidence_and_coalesces_one_notification_per_session() {
        let mut session = DetectionSession::default();
        assert!(!session.observe(true, false, true));
        assert!(!session.observe(true, false, true));
        assert!(session.observe(true, false, true));
        assert!(!session.observe(true, false, true));
        assert!(!session.observe(false, false, true));
        assert!(!session.observe(false, false, true));
        assert!(!session.observe(true, false, true));
        assert!(!session.observe(true, false, true));
        assert!(session.observe(true, false, true));
    }

    #[test]
    fn recording_consumes_the_detection_session() {
        let mut session = DetectionSession::default();
        assert!(!session.observe(true, true, true));
        assert!(!session.observe(true, true, true));
        assert!(!session.observe(true, false, true));
    }

    #[test]
    fn disabled_detection_resets_pending_evidence() {
        let mut session = DetectionSession::default();
        assert!(!session.observe(true, false, true));
        assert!(!session.observe(true, false, false));
        assert!(!session.observe(true, false, true));
        assert!(!session.observe(true, false, true));
        assert!(session.observe(true, false, true));
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
            .observe(&apps(&[MeetingApp::Telegram]), now)
            .is_empty());
        let launched = evidence.observe(
            &apps(&[MeetingApp::Telegram, MeetingApp::Zoom]),
            now + Duration::from_secs(1),
        );
        assert_eq!(launched, apps(&[MeetingApp::Zoom]));

        let expired = evidence.observe(
            &apps(&[MeetingApp::Telegram, MeetingApp::Zoom]),
            now + PROCESS_LAUNCH_EVIDENCE_TTL + Duration::from_secs(2),
        );
        assert!(expired.is_empty());
    }
}
