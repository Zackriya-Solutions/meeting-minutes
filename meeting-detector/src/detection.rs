//! Meeting detection — a self-contained copy of the main app's heuristic
//! (`frontend/src-tauri/src/meeting_detection.rs`), trimmed to the pure signal
//! + state-machine core needed to auto-start/stop a mic recording.
//!
//! Difference from the main app: this is a headless auto-recorder, so every
//! recognized meeting client that owns the microphone drives the auto-listen
//! start/stop path (the main app keeps browser/Telegram as manual prompts). A
//! minimum-duration guard in the registration step discards false positives such
//! as short voice messages.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, System};

use crate::app::Shared;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const PROCESS_LAUNCH_EVIDENCE_TTL: Duration = Duration::from_secs(90);
const REQUIRED_ACTIVE_POLLS: u8 = 2;
const STRONG_AUTO_LISTENING_POLLS: u8 = 1;
const REQUIRED_QUIET_POLLS: u8 = 2;
// ~46s of the meeting client not owning the mic before we conclude the call ended.
const AUTO_LISTENING_QUIET_POLLS: u8 = 23;

/// A detection outcome delivered from the background poll thread to the UI thread.
#[derive(Debug, Clone)]
pub enum Signal {
    /// A recognized meeting client took over the microphone — start recording.
    MeetingStarted { label: String },
    /// The meeting client released the microphone long enough — stop recording.
    MeetingStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeetingApp {
    Zoom,
    MicrosoftTeams,
    Telegram,
    YandexTelemost,
    SaluteJazz,
    BrowserCall,
}

impl MeetingApp {
    fn display(self) -> &'static str {
        match self {
            MeetingApp::Zoom => "Zoom",
            MeetingApp::MicrosoftTeams => "Microsoft Teams",
            MeetingApp::Telegram => "Telegram",
            MeetingApp::YandexTelemost => "Yandex Telemost",
            MeetingApp::SaluteJazz => "SaluteJazz",
            MeetingApp::BrowserCall => "a browser call",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetectionSource {
    NativeProcess,
    MicrophoneActivity,
}

/// Spawn the background detection thread. `emit` is invoked (from that thread)
/// whenever a start/stop signal fires.
pub fn spawn<F>(shared: Arc<Shared>, emit: F)
where
    F: Fn(Signal) + Send + 'static,
{
    std::thread::Builder::new()
        .name("meeting-detection".into())
        .spawn(move || run_loop(shared, emit))
        .expect("spawn detection thread");
}

fn run_loop<F: Fn(Signal)>(shared: Arc<Shared>, emit: F) {
    let mut system = System::new();
    let mut evidence = ProcessLaunchEvidence::default();
    let mut session = DetectionSession::default();

    loop {
        std::thread::sleep(POLL_INTERVAL);

        let enabled = shared.enabled.load(Ordering::SeqCst);
        let recording = shared.recording.load(Ordering::SeqCst);

        if !enabled {
            // Flush a stop if a recording was in progress, then idle.
            if matches!(
                session.observe(false, recording, false, false),
                DetectionEvent::StopAutoListening
            ) {
                emit(Signal::MeetingStopped);
            }
            evidence = ProcessLaunchEvidence::default();
            continue;
        }

        system.refresh_processes(ProcessesToUpdate::All, true);
        let running_native_apps = collect_native_apps(&system);
        let launched_apps = evidence.observe(&running_native_apps, Instant::now());
        let microphone_apps = active_microphone_apps();
        let (candidates, source) = select_detection_signal(launched_apps, microphone_apps);
        let strong = should_auto_listen(&candidates, source, enabled);

        match session.observe(!candidates.is_empty(), recording, enabled, strong) {
            DetectionEvent::StartAutoListening => {
                emit(Signal::MeetingStarted {
                    label: label_for(&candidates),
                });
            }
            DetectionEvent::StopAutoListening => emit(Signal::MeetingStopped),
            DetectionEvent::SuggestRecording => {
                // A weaker signal (e.g. a native process launch on a platform with
                // no mic observer). We don't auto-record on it because the state
                // machine emits no matching stop for that path.
                log::debug!("meeting suggested but not auto-recording (weak signal)");
            }
            DetectionEvent::None => {}
        }
    }
}

fn label_for(apps: &BTreeSet<MeetingApp>) -> String {
    if apps.is_empty() {
        return "a meeting".to_string();
    }
    apps.iter()
        .map(|a| a.display())
        .collect::<Vec<_>>()
        .join(", ")
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
            if self.auto_listening_active {
                *self = Self::default();
                return DetectionEvent::StopAutoListening;
            }
            *self = Self::default();
            return DetectionEvent::None;
        }

        if active {
            self.quiet_polls = 0;
            self.active_polls = self.active_polls.saturating_add(1);

            if self.auto_listening_active {
                return DetectionEvent::None;
            }

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
            self.notified = false;
            self.quiet_polls = 0;
        }
        DetectionEvent::None
    }
}

/// A process launch is useful evidence, but only for a bounded interval so a
/// long-lived app (e.g. Telegram) does not keep the detector active forever.
#[derive(Debug, Default)]
struct ProcessLaunchEvidence {
    previous_apps: Option<BTreeSet<MeetingApp>>,
    launched_at: BTreeMap<MeetingApp, Instant>,
}

impl ProcessLaunchEvidence {
    fn observe(&mut self, current_apps: &BTreeSet<MeetingApp>, now: Instant) -> BTreeSet<MeetingApp> {
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

/// Headless auto-record policy: any recognized meeting client owning the mic is a
/// strong enough signal (macOS-only mic observer). Unlike the main app we do not
/// exclude browser/Telegram — a min-duration guard filters false positives later.
fn should_auto_listen(
    candidates: &BTreeSet<MeetingApp>,
    source: Option<DetectionSource>,
    enabled: bool,
) -> bool {
    enabled && source == Some(DetectionSource::MicrophoneActivity) && !candidates.is_empty()
}

fn select_detection_signal(
    launched_apps: BTreeSet<MeetingApp>,
    microphone_apps: BTreeSet<MeetingApp>,
) -> (BTreeSet<MeetingApp>, Option<DetectionSource>) {
    if !microphone_apps.is_empty() {
        return (microphone_apps, Some(DetectionSource::MicrophoneActivity));
    }
    // Process-launch is a weak fallback only; on macOS the mic observer is the
    // real signal, so an unaccompanied launch stays a (non-auto) suggestion.
    if !launched_apps.is_empty() {
        return (launched_apps, Some(DetectionSource::NativeProcess));
    }
    (BTreeSet::new(), None)
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
    BROWSER_MARKERS.iter().any(|marker| value.starts_with(marker))
}

/// macOS CoreAudio per-process microphone-session signal. Filters to processes
/// actively running input, then keeps only recognized meeting clients — so our
/// own recording process (a different bundle id) is invisible here and cannot
/// create a start/stop feedback loop.
fn active_microphone_apps() -> BTreeSet<MeetingApp> {
    use cidre::core_audio as ca;

    let processes = match ca::System::processes() {
        Ok(processes) => processes,
        Err(error) => {
            log::debug!("CoreAudio processes() failed: {error:?}");
            return BTreeSet::new();
        }
    };

    let total = processes.len();
    let mut running_input = 0usize;
    let mut apps = BTreeSet::new();

    for process in processes {
        if !process.is_running_input().unwrap_or(false) {
            continue;
        }
        running_input += 1;
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
        let classified = classify_audio_process(&bundle_id, &display_name);
        // Run with RUST_LOG=debug to see exactly what CoreAudio reports while
        // you're in a call — useful for confirming detection and spotting a
        // meeting app that isn't recognized yet.
        log::debug!("mic-active process: bundle='{bundle_id}' name='{display_name}' -> {classified:?}");
        if let Some(app) = classified {
            apps.insert(app);
        }
    }

    if running_input > 0 {
        log::debug!("CoreAudio: {total} audio processes, {running_input} running input; recognized meeting apps: {apps:?}");
    }

    apps
}
