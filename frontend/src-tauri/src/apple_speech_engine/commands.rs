//! Tauri commands for Apple Speech engine initialization and status.

use log::{info, error};
use std::sync::{Arc, Mutex};

/// Stub engine for non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub struct AppleSpeechEngine;

#[cfg(not(target_os = "macos"))]
impl AppleSpeechEngine {
    pub async fn is_model_loaded(&self) -> bool {
        false
    }
    pub async fn get_current_model(&self) -> Option<String> {
        None
    }
    pub async fn transcribe_audio(&self, _audio: Vec<f32>) -> anyhow::Result<(String, Option<f32>, bool)> {
        Err(anyhow::anyhow!("Apple Speech is only available on macOS"))
    }
}

/// Global Apple Speech engine instance (matches WHISPER_ENGINE / PARAKEET_ENGINE pattern).
pub static APPLE_SPEECH_ENGINE: Mutex<Option<Arc<super::AppleSpeechEngine>>> = Mutex::new(None);

/// Initialize the Apple Speech engine.
/// Checks availability and requests authorization if needed.
#[cfg(target_os = "macos")]
pub async fn apple_speech_init() -> Result<(), String> {
    // Check if already initialized
    {
        let guard = APPLE_SPEECH_ENGINE.lock().unwrap();
        if let Some(ref engine) = *guard {
            if futures_util::FutureExt::now_or_never(engine.is_model_loaded()).unwrap_or(false) {
                info!("🍎 Apple Speech engine already initialized");
                return Ok(());
            }
        }
    }

    // Request authorization
    super::AppleSpeechEngine::request_authorization()
        .await
        .map_err(|e| format!("Authorization failed: {}", e))?;

    // Create engine
    let engine = super::AppleSpeechEngine::new()
        .map_err(|e| format!("Failed to create Apple Speech engine: {}", e))?;

    if !engine.is_model_loaded().await {
        return Err("Apple Speech recognizer is not available on this device".to_string());
    }

    let engine = Arc::new(engine);
    {
        let mut guard = APPLE_SPEECH_ENGINE.lock().unwrap();
        *guard = Some(engine);
    }

    info!("🍎 Apple Speech engine initialized successfully");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub async fn apple_speech_init() -> Result<(), String> {
    Err("Apple Speech is only available on macOS".to_string())
}

/// Check if Apple Speech is available on this device.
#[cfg(target_os = "macos")]
pub fn apple_speech_is_available() -> bool {
    let guard = APPLE_SPEECH_ENGINE.lock().unwrap();
    guard.is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn apple_speech_is_available() -> bool {
    false
}
