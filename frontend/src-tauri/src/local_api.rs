//! Local HTTP control API.
//!
//! Lets other local processes start/stop recordings and check recording status.
//! Bound to 127.0.0.1 only — same trust level as the system tray, which uses the
//! identical start/stop mechanisms (sessionStorage auto-start flag for start,
//! `stop_recording_and_notify` for stop).
//!
//! Endpoints:
//! - `GET  /status` → `{"recording": bool}`
//! - `POST /start`  → body `{"title"?: string, "metadata"?: string}`; metadata is
//!   injected by the frontend as the first transcript segment of the meeting
//! - `POST /stop`   → `{"status": "stopped"}`; meeting save runs asynchronously
//!   in the frontend exactly like a tray-initiated stop

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Wry};

use crate::audio::recording_commands;
use crate::tray::{set_tray_state, update_tray_menu, RecordingState};

const LOCAL_API_ADDR: &str = "127.0.0.1:5172";

/// How long `/start` waits for the frontend to actually begin recording before
/// reporting a timeout (model checks and device setup can take a few seconds).
const START_TIMEOUT_MS: u64 = 15_000;
const START_POLL_INTERVAL_MS: u64 = 250;

#[derive(Debug, Default, Deserialize)]
struct StartRequest {
    /// Optional meeting title (replaces the auto-generated "Meeting DD_MM_YY..." name).
    title: Option<String>,
    /// Optional free-form text dropped at the top of the transcript.
    metadata: Option<String>,
}

/// Run the local API server. Never returns under normal operation; logs and
/// exits if the port cannot be bound so the app itself is unaffected.
pub async fn serve(app: AppHandle<Wry>) {
    let router = Router::new()
        .route("/status", get(status_handler))
        .route("/start", post(start_handler))
        .route("/stop", post(stop_handler))
        .with_state(app);

    let listener = match tokio::net::TcpListener::bind(LOCAL_API_ADDR).await {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("Local API: failed to bind {}: {}", LOCAL_API_ADDR, e);
            return;
        }
    };

    log::info!("Local API listening on http://{}", LOCAL_API_ADDR);

    if let Err(e) = axum::serve(listener, router).await {
        log::error!("Local API server error: {}", e);
    }
}

async fn status_handler(State(_app): State<AppHandle<Wry>>) -> Json<Value> {
    Json(json!({ "recording": recording_commands::is_recording().await }))
}

async fn start_handler(
    State(app): State<AppHandle<Wry>>,
    body: Option<Json<StartRequest>>,
) -> (StatusCode, Json<Value>) {
    let request = body.map(|Json(r)| r).unwrap_or_default();

    if recording_commands::is_recording().await {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "already recording" })),
        );
    }

    let Some(window) = app.get_webview_window("main") else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "main window not available" })),
        );
    };

    set_tray_state(&app, RecordingState::Starting);

    // Stage start parameters for the frontend auto-start flow (same mechanism as
    // the tray). serde_json::to_string produces a safe JS string literal for the
    // arbitrary user-provided values.
    if let Some(title) = &request.title {
        let _ = window.eval(&format!(
            "sessionStorage.setItem('externalMeetingTitle', {})",
            serde_json::to_string(title).expect("string serialization cannot fail")
        ));
    }
    if let Some(metadata) = &request.metadata {
        let _ = window.eval(&format!(
            "sessionStorage.setItem('externalMeetingMetadata', {})",
            serde_json::to_string(metadata).expect("string serialization cannot fail")
        ));
    }
    let _ = window.eval("sessionStorage.setItem('autoStartRecording', 'true')");
    let _ = window.eval("window.location.assign('/')");

    // Wait for the frontend to actually start recording (it performs model
    // readiness checks and may legitimately refuse, e.g. no model downloaded).
    let mut waited_ms = 0;
    while waited_ms < START_TIMEOUT_MS {
        tokio::time::sleep(std::time::Duration::from_millis(START_POLL_INTERVAL_MS)).await;
        waited_ms += START_POLL_INTERVAL_MS;

        if recording_commands::is_recording().await {
            return (
                StatusCode::OK,
                Json(json!({ "status": "recording", "title": request.title })),
            );
        }
    }

    // Resync tray menu since we optimistically set it to "Starting"
    update_tray_menu(&app);

    (
        StatusCode::GATEWAY_TIMEOUT,
        Json(json!({
            "error": "recording did not start within 15s (transcription model may be missing or downloading)"
        })),
    )
}

async fn stop_handler(State(app): State<AppHandle<Wry>>) -> (StatusCode, Json<Value>) {
    if !recording_commands::is_recording().await {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "not recording" })),
        );
    }

    set_tray_state(&app, RecordingState::Stopping);

    match recording_commands::stop_recording_and_notify(app.clone()).await {
        Ok(_) => {
            log::info!("Local API: recording stopped successfully");
            (StatusCode::OK, Json(json!({ "status": "stopped" })))
        }
        Err(e) => {
            log::error!("Local API: failed to stop recording: {}", e);
            // Revert tray state on error
            update_tray_menu(&app);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            )
        }
    }
}
