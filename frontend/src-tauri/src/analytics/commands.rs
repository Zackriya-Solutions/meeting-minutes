//! Compatibility commands for builds that previously exposed usage analytics.
//!
//! Telemetry is intentionally disabled in this fork. Keeping these commands as
//! local no-ops lets existing UI call sites continue to work while ensuring the
//! application has no analytics client, API key, host, or event transport.

use std::collections::HashMap;

use tauri::command;

#[command]
pub async fn init_analytics() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn disable_analytics() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_event(
    _event_name: String,
    _properties: Option<HashMap<String, String>>,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn identify_user(
    _user_id: String,
    _properties: Option<HashMap<String, String>>,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_meeting_started(_meeting_id: String) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_recording_started(_meeting_id: String) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_recording_stopped(
    _meeting_id: String,
    _duration_seconds: Option<u64>,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_meeting_deleted(_meeting_id: String) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_settings_changed(
    _setting_type: String,
    _new_value: String,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_feature_used(_feature_name: String) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn is_analytics_enabled() -> bool {
    false
}

#[command]
pub async fn start_analytics_session(_user_id: String) -> Result<String, String> {
    Ok(String::new())
}

#[command]
pub async fn end_analytics_session() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_daily_active_user() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_user_first_launch() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_summary_generation_started(
    _model_provider: String,
    _model_name: String,
    _transcript_length: usize,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_summary_generation_completed(
    _model_provider: String,
    _model_name: String,
    _success: bool,
    _duration_seconds: Option<u64>,
    _error_message: Option<String>,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_summary_regenerated(
    _model_provider: String,
    _model_name: String,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_model_changed(
    _old_provider: String,
    _old_model: String,
    _new_provider: String,
    _new_model: String,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_custom_prompt_used(_prompt_length: usize) -> Result<(), String> {
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[command]
pub async fn track_meeting_ended(
    _transcription_provider: String,
    _transcription_model: String,
    _summary_provider: String,
    _summary_model: String,
    _total_duration_seconds: Option<f64>,
    _active_duration_seconds: f64,
    _pause_duration_seconds: f64,
    _microphone_device_type: String,
    _system_audio_device_type: String,
    _chunks_processed: u64,
    _transcript_segments_count: u64,
    _had_fatal_error: bool,
) -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_analytics_enabled() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_analytics_disabled() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn track_analytics_transparency_viewed() -> Result<(), String> {
    Ok(())
}

#[command]
pub async fn is_analytics_session_active() -> bool {
    false
}
