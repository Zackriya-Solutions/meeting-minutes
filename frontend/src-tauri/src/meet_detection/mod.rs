pub mod commands;
pub mod window_scan;

use crate::audio::devices::{microphone, speakers};
use crate::audio::recording_commands::{self, RecordingArgs};
use crate::audio::recording_preferences;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Consecutive "no Meet window" polls to wait through before auto-stopping, so a brief tab
/// switch or window-manager hiccup doesn't cut a call short. ~20s at a 5s poll interval.
const STOP_DEBOUNCE_POLLS: u32 = 4;
/// After a failed auto-start (e.g. no audio device available), wait this long before retrying
/// rather than hammering it every poll for as long as the call stays open.
const AUTO_START_RETRY_COOLDOWN: Duration = Duration::from_secs(60);
/// How often to re-log the missing-permission warning — otherwise it fires every single 5s
/// poll for as long as the toggle is on and permission isn't granted, drowning out other logs.
const PERMISSION_WARNING_INTERVAL: Duration = Duration::from_secs(300);

/// Tracks whether the *current* recording was started by this detector, so auto-stop never
/// touches a recording the user started manually for something other than a Meet call.
static AUTO_STARTED_BY_DETECTION: AtomicBool = AtomicBool::new(false);

/// Resolves which mic/system device names to record with, the same way the manual "Record"
/// button's device selection ends up resolved before reaching the Rust side: prefer the user's
/// saved device preference, falling back to the OS default. Unlike the manual flow, nothing in
/// the frontend resolves this for an auto-triggered recording, so it has to happen here —
/// passing `None`/`None` through leaves both mic and system audio unset and recording fails
/// outright with "No audio streams could be created".
async fn resolve_default_devices<R: Runtime>(app: &AppHandle<R>) -> (Option<String>, Option<String>) {
    let prefs = recording_preferences::load_recording_preferences(app).await.ok();

    // `parse_audio_device` (called downstream) requires the "(input)"/"(output)" suffix
    // `AudioDevice`'s Display impl produces — a bare device name is rejected. Preferences are
    // assumed to already be stored in that format, matching how the frontend saves them.
    let mic_name = prefs
        .as_ref()
        .and_then(|p| p.preferred_mic_device.clone())
        .or_else(|| microphone::default_input_device().ok().map(|d| d.to_string()));

    let system_name = prefs
        .as_ref()
        .and_then(|p| p.preferred_system_device.clone())
        .or_else(|| speakers::default_output_device().ok().map(|d| d.to_string()));

    (mic_name, system_name)
}

/// Spawns the background poller that watches for an active Google Meet call and auto
/// starts/stops Meetily's existing recording pipeline. The loop always runs (one settings
/// read + one window scan every 5s) but only acts while `auto_detect_meet_enabled` is on,
/// so flipping the Settings toggle takes effect without an app restart.
pub fn spawn_meet_detection_task<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut consecutive_absent: u32 = 0;
        let mut last_start_failure: Option<Instant> = None;
        let mut last_permission_warning: Option<Instant> = None;

        loop {
            tokio::time::sleep(POLL_INTERVAL).await;

            // AppState isn't registered until the DB is initialized, which on first launch is
            // deferred until the user finishes onboarding (see database/setup.rs) — so unlike
            // most command handlers, this loop can legitimately run before it exists.
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let pool = state.db_manager.pool().clone();

            let enabled = match SettingsRepository::get_auto_detect_meet_enabled(&pool).await {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to read auto-detect-meet setting: {}", e);
                    continue;
                }
            };

            if !enabled {
                consecutive_absent = 0;
                continue;
            }

            if !crate::audio::permissions::check_window_title_read_permission() {
                let should_warn = last_permission_warning
                    .is_none_or(|t| t.elapsed() >= PERMISSION_WARNING_INTERVAL);
                if should_warn {
                    warn!(
                        "[meet-detect] auto-detect is on but Screen Recording permission isn't \
                         granted — window titles are invisible, so detection can never fire. \
                         Grant it in System Settings > Privacy & Security > Screen Recording."
                    );
                    last_permission_warning = Some(Instant::now());
                }
            }

            let meet_active = tokio::task::spawn_blocking(window_scan::scan_for_active_meet_call)
                .await
                .unwrap_or(false);
            debug!("[meet-detect] poll: meet_active={}", meet_active);
            let currently_recording = recording_commands::is_recording().await;

            if meet_active {
                consecutive_absent = 0;
                let cooling_down = last_start_failure
                    .is_some_and(|t| t.elapsed() < AUTO_START_RETRY_COOLDOWN);
                if !currently_recording && !cooling_down {
                    let (mic_name, system_name) = resolve_default_devices(&app).await;
                    if mic_name.is_none() && system_name.is_none() {
                        warn!(
                            "[meet-detect] no microphone or system audio device available — \
                             skipping auto-start"
                        );
                        last_start_failure = Some(Instant::now());
                    } else {
                        info!("Detected an active Google Meet call — auto-starting recording");
                        match recording_commands::start_recording_with_devices_and_meeting(
                            app.clone(),
                            mic_name,
                            system_name,
                            None,
                        )
                        .await
                        {
                            Ok(()) => {
                                AUTO_STARTED_BY_DETECTION.store(true, Ordering::SeqCst);
                                last_start_failure = None;
                            }
                            Err(e) => {
                                warn!("Auto-start recording failed: {}", e);
                                last_start_failure = Some(Instant::now());
                            }
                        }
                    }
                }
                continue;
            }

            if currently_recording && AUTO_STARTED_BY_DETECTION.load(Ordering::SeqCst) {
                consecutive_absent += 1;
                if consecutive_absent >= STOP_DEBOUNCE_POLLS {
                    info!("Google Meet call no longer detected — auto-stopping recording");
                    AUTO_STARTED_BY_DETECTION.store(false, Ordering::SeqCst);
                    consecutive_absent = 0;
                    if let Err(e) = recording_commands::stop_recording(
                        app.clone(),
                        RecordingArgs {
                            save_path: String::new(),
                        },
                    )
                    .await
                    {
                        warn!("Auto-stop recording failed: {}", e);
                    }
                }
            } else {
                consecutive_absent = 0;
            }
        }
    });
}
