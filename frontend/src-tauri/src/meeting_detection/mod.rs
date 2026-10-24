//! Opt-in, local call detection. Recording and persistence use the same commands
//! as a manual start/stop, including model readiness and permission checks.
#[cfg(target_os = "macos")]
mod macos;
mod signals;
mod state;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};
use tauri_plugin_store::StoreExt;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Preferences {
    pub enabled: bool,
    pub browser_calls: bool,
    pub stop_when_call_ends: bool,
}

#[derive(Serialize)]
pub struct Settings {
    preferences: Preferences,
    supported: bool,
}

fn supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::supported()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn preferences<R: Runtime>(app: &AppHandle<R>) -> Result<Preferences, String> {
    let store = app
        .store("call-detection.json")
        .map_err(|e| e.to_string())?;
    match store.get("preferences") {
        Some(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
        None => Ok(Preferences {
            stop_when_call_ends: true,
            ..Default::default()
        }),
    }
}

#[tauri::command]
pub fn get_call_detection_settings<R: Runtime>(app: AppHandle<R>) -> Result<Settings, String> {
    Ok(Settings {
        preferences: preferences(&app)?,
        supported: supported(),
    })
}

#[tauri::command]
pub fn set_call_detection_preferences<R: Runtime>(
    app: AppHandle<R>,
    preferences: Preferences,
) -> Result<(), String> {
    if preferences.enabled && !supported() {
        return Err("Automatic call recording requires macOS 14.2 or later".into());
    }
    let store = app
        .store("call-detection.json")
        .map_err(|e| e.to_string())?;
    let previous = store.get("preferences");
    store.set(
        "preferences",
        serde_json::to_value(&preferences).map_err(|e| e.to_string())?,
    );
    if let Err(error) = store.save() {
        // Keep the live watcher consistent with the UI when persistence fails.
        match previous {
            Some(value) => store.set("preferences", value),
            None => {
                store.delete("preferences");
            }
        }
        return Err(error.to_string());
    }
    Ok(())
}

pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    #[cfg(target_os = "macos")]
    tauri::async_runtime::spawn(async move {
        use crate::audio::{recording_commands as recording, recording_preferences};
        use state::{Action, CallState};
        let clock = std::time::Instant::now();
        let mut call = CallState::default();
        let mut owned_name: Option<String> = None;
        let mut previous_tick = 0;
        let mut reported_error = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let Ok(prefs) = preferences(&app) else {
                continue;
            };
            if !prefs.enabled || !supported() {
                // Disabling detection relinquishes ownership. The current
                // recording remains available for the user to stop manually.
                call = CallState::default();
                owned_name = None;
                continue;
            }
            let now = clock.elapsed().as_secs();
            let current_name = recording::get_recording_meeting_name().await.ok().flatten();
            if owned_name.is_some() && owned_name != current_name {
                owned_name = None;
            }
            let ours = owned_name.is_some();
            let sample = if now.saturating_sub(previous_tick) > 10 && previous_tick != 0 {
                // Sleep/wake or a stalled probe is not evidence that a call ended.
                None
            } else {
                let browsers = prefs.browser_calls;
                match tauri::async_runtime::spawn_blocking(move || macos::probe(ours, browsers))
                    .await
                {
                    Ok(Ok(label)) => {
                        reported_error = false;
                        Some(label)
                    }
                    _ => {
                        if !reported_error {
                            let _ = app.emit("automatic-recording-error", "Call detection could not check audio or browser access. Existing recordings will continue. Check Privacy & Security settings.");
                            reported_error = true;
                        }
                        None
                    }
                }
            };
            previous_tick = now;
            let recording_active = recording::is_recording().await;
            // Losing ownership to a manual Stop/Start must suppress auto-stop,
            // even when the replacement manual recording is already running.
            if !ours {
                call.relinquish();
            }
            let action = call.update(
                now,
                sample.as_ref().map(|label| label.is_some()),
                recording_active,
            );
            match action {
                Action::Start => {
                    if !crate::onboarding::load_onboarding_status(&app)
                        .await
                        .map(|s| s.completed)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let label = sample.flatten().unwrap_or_else(|| "Call".into());
                    // Nanoseconds distinguish back-to-back sessions without
                    // storing or logging a conference URL or participant names.
                    let name = format!(
                        "{} {}",
                        label,
                        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S%.9f")
                    );
                    let Ok(devices) = recording_preferences::load_recording_preferences(&app).await
                    else {
                        continue;
                    };
                    match crate::start_recording_with_devices_and_meeting(
                        app.clone(),
                        devices.preferred_mic_device,
                        devices.preferred_system_device,
                        Some(name.clone()),
                    )
                    .await
                    {
                        Ok(()) => {
                            owned_name = Some(name);
                            call.started();
                            let _ = app.emit("automatic-recording-started", ());
                        }
                        Err(_) => {
                            let _ = app.emit("automatic-recording-error", "Automatic recording could not start. Check your transcription model and audio permissions, then start recording manually.");
                        }
                    }
                }
                Action::Stop => {
                    if let Some(name) = owned_name.take() {
                        if prefs.stop_when_call_ends {
                            match recording::stop_automatic_recording(app.clone(), &name).await {
                                Ok(()) => {
                                    let _ = app.emit("recording-stop-complete", true);
                                }
                                Err(_) => {
                                    let _ = app.emit("automatic-recording-error", "Automatic stop could not finish. Check the current recording in Meetily.");
                                }
                            }
                        }
                    }
                }
                Action::None => {}
            }
        }
    });
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}
