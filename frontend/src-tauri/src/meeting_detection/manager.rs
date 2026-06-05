use super::{
    confidence::ConfidenceState,
    process_detector::TeamsProcessDetector,
    types::{
        DetectionAction, DetectionSample, MeetingDetectionSettings, MeetingDetectionStatus,
        TeamsCallLikelyEndedPayload, TeamsCallLikelyStartedPayload, END_EVENT,
        POLL_INTERVAL_SECONDS, START_EVENT,
    },
    windows_audio::is_any_process_render_audio_active,
};
use anyhow::Result;
use log::{error, info, warn};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Wry};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub struct MeetingDetectionManager {
    app_handle: AppHandle<Wry>,
    settings: Arc<RwLock<MeetingDetectionSettings>>,
    status: Arc<RwLock<MeetingDetectionStatus>>,
    cancellation: Arc<RwLock<Option<CancellationToken>>>,
}

impl MeetingDetectionManager {
    pub async fn new(app_handle: AppHandle<Wry>) -> Result<Self> {
        let settings = load_settings().await.unwrap_or_default();
        let status = MeetingDetectionStatus::stopped(settings.clone());

        Ok(Self {
            app_handle,
            settings: Arc::new(RwLock::new(settings)),
            status: Arc::new(RwLock::new(status)),
            cancellation: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn start_if_enabled(&self) {
        let enabled = {
            let settings = self.settings.read().await;
            settings.meeting_detection_enabled && settings.teams_detection_enabled
        };

        if enabled {
            self.start().await;
        }
    }

    pub async fn start(&self) {
        {
            let cancellation = self.cancellation.read().await;
            if cancellation.is_some() {
                return;
            }
        }

        let token = CancellationToken::new();
        {
            let mut cancellation = self.cancellation.write().await;
            *cancellation = Some(token.clone());
        }

        {
            let mut status = self.status.write().await;
            status.running = true;
            status.last_error = None;
        }

        let app_handle = self.app_handle.clone();
        let settings = Arc::clone(&self.settings);
        let status = Arc::clone(&self.status);
        let cancellation = Arc::clone(&self.cancellation);

        tauri::async_runtime::spawn(async move {
            info!("Starting Teams meeting detection task");
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(POLL_INTERVAL_SECONDS));
            let mut confidence_state = ConfidenceState::default();
            let mut teams_process_detector = TeamsProcessDetector::new();

            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        info!("Teams meeting detection task stopped");
                        let mut status = status.write().await;
                        status.running = false;
                        break;
                    }
                    _ = interval.tick() => {
                        let current_settings = settings.read().await.clone();
                        if !current_settings.meeting_detection_enabled || !current_settings.teams_detection_enabled {
                            continue;
                        }

                        let teams_process = teams_process_detector.detect();
                        let mut last_error = None;
                        let teams_audio_active = match is_any_process_render_audio_active(&teams_process.process_ids) {
                            Ok(active) => active,
                            Err(error) => {
                                let message = error.to_string();
                                warn!("Teams audio detection failed: {}", message);
                                last_error = Some(message);
                                false
                            }
                        };

                        let is_recording = crate::audio::recording_commands::is_recording().await;
                        let now_ms = now_ms();
                        let sample = DetectionSample {
                            teams_running: teams_process.running,
                            teams_audio_active,
                            is_recording,
                            now_ms,
                        };

                        let action = confidence_state.update(
                            sample,
                            current_settings.teams_prompt_cooldown_minutes,
                        );

                        {
                            let mut status = status.write().await;
                            status.settings = current_settings.clone();
                            status.running = true;
                            status.teams_running = sample.teams_running;
                            status.teams_audio_active = sample.teams_audio_active;
                            status.confidence = sample.confidence();
                            status.consecutive_active_polls = confidence_state.consecutive_active_polls;
                            status.consecutive_inactive_polls = confidence_state.consecutive_inactive_polls;
                            status.last_error = last_error;
                        }

                        match action {
                            Some(DetectionAction::PromptStart) if current_settings.teams_prompt_start => {
                                let payload = TeamsCallLikelyStartedPayload {
                                    event_type: START_EVENT.to_string(),
                                    confidence: sample.confidence(),
                                    teams_running: sample.teams_running,
                                    teams_audio_active: sample.teams_audio_active,
                                    poll_interval_seconds: POLL_INTERVAL_SECONDS,
                                };
                                if let Err(error) = app_handle.emit(START_EVENT, payload) {
                                    error!("Failed to emit Teams start prompt event: {}", error);
                                }
                            }
                            Some(DetectionAction::PromptStop) if current_settings.teams_prompt_stop => {
                                let payload = TeamsCallLikelyEndedPayload {
                                    event_type: END_EVENT.to_string(),
                                    confidence: sample.confidence(),
                                    teams_running: sample.teams_running,
                                    teams_audio_active: sample.teams_audio_active,
                                    inactive_polls: confidence_state.consecutive_inactive_polls,
                                };
                                if let Err(error) = app_handle.emit(END_EVENT, payload) {
                                    error!("Failed to emit Teams end prompt event: {}", error);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            let mut cancellation = cancellation.write().await;
            *cancellation = None;
        });
    }

    pub async fn stop(&self) {
        let token = {
            let cancellation = self.cancellation.read().await;
            cancellation.clone()
        };

        if let Some(token) = token {
            token.cancel();
        }

        let mut status = self.status.write().await;
        status.running = false;
    }

    pub async fn get_settings(&self) -> MeetingDetectionSettings {
        self.settings.read().await.clone()
    }

    pub async fn update_settings(&self, next_settings: MeetingDetectionSettings) -> Result<()> {
        save_settings(&next_settings).await?;
        {
            let mut settings = self.settings.write().await;
            *settings = next_settings.clone();
        }
        {
            let mut status = self.status.write().await;
            status.settings = next_settings.clone();
        }

        if next_settings.meeting_detection_enabled && next_settings.teams_detection_enabled {
            self.start().await;
        } else {
            self.stop().await;
        }

        Ok(())
    }

    pub async fn get_status(&self) -> MeetingDetectionStatus {
        self.status.read().await.clone()
    }
}

fn settings_path() -> Result<PathBuf> {
    let mut path =
        dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
    path.push("meetily");
    path.push("meeting_detection.json");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    Ok(path)
}

async fn load_settings() -> Result<MeetingDetectionSettings> {
    let path = settings_path()?;
    if !path.exists() {
        let default_settings = MeetingDetectionSettings::default();
        save_settings(&default_settings).await?;
        return Ok(default_settings);
    }

    let content = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&content)?)
}

async fn save_settings(settings: &MeetingDetectionSettings) -> Result<()> {
    let path = settings_path()?;
    let content = serde_json::to_string_pretty(settings)?;
    tokio::fs::write(path, content).await?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
