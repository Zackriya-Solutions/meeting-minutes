// audio/transcription/gemini_live/mod.rs
//
// Gemini Live (gemini-3.5-transcribe-live) cloud transcription engine. Fork addition:
// everything lives in this module; the rest of the app only gains `"geminiLive"`
// match arms that call into the functions below.

pub mod commands;
pub mod connection;
pub mod dedup;
pub mod pcm;
pub mod protocol;
pub mod provider;
pub mod settings;
#[cfg(test)]
mod tests;

use super::engine::TranscriptionEngine;
use super::provider::TranscriptionProvider;
use log::{info, warn};
use provider::{GeminiLiveProvider, GeminiLiveStatus, StatusCallback};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Value of `transcript_settings.provider` that selects this engine.
pub const PROVIDER_ID: &str = "geminiLive";

fn pool<R: Runtime>(app: &AppHandle<R>) -> sqlx::SqlitePool {
    app.state::<crate::state::AppState>().db_manager.pool().clone()
}

/// The transcription key if set, otherwise the Gemini key saved for summaries.
pub async fn resolve_api_key(pool: &sqlx::SqlitePool) -> Result<Option<String>, String> {
    let (_, own) = settings::load(pool).await.map_err(|e| e.to_string())?;
    Ok(match own {
        Some(key) => Some(key),
        None => settings::summary_gemini_key(pool).await,
    })
}

/// Load the configured local engine (auto-selecting a downloaded model) for fallback.
async fn load_fallback<R: Runtime>(
    app: &AppHandle<R>,
    fallback_provider: &str,
) -> Result<Arc<dyn TranscriptionProvider>, String> {
    if fallback_provider == "localWhisper" {
        crate::whisper_engine::commands::whisper_init().await?;
        crate::whisper_engine::commands::whisper_validate_model_ready_with_config(app).await?;
        let engine = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or("Whisper engine not initialized")?;
        Ok(Arc::new(super::WhisperProvider::new(engine)))
    } else {
        crate::parakeet_engine::commands::parakeet_init().await?;
        crate::parakeet_engine::commands::parakeet_validate_model_ready_with_config(app).await?;
        let engine = crate::parakeet_engine::commands::PARAKEET_ENGINE
            .lock()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or("Parakeet engine not initialized")?;
        Ok(Arc::new(super::ParakeetProvider::new(engine)))
    }
}

fn fallback_label(fallback_provider: &str) -> &'static str {
    if fallback_provider == "localWhisper" { "Whisper" } else { "Parakeet" }
}

/// Pre-recording check: an API key exists and the local fallback model is ready.
pub async fn validate_ready<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let pool = pool(app);
    if resolve_api_key(&pool).await?.is_none() {
        return Err("Gemini Live needs an API key. Add it in Settings → Transcription.".into());
    }
    let (settings, _) = settings::load(&pool).await.map_err(|e| e.to_string())?;
    load_fallback(app, &settings.fallback_provider).await.map(|_| ()).map_err(|e| {
        format!(
            "Gemini Live uses {} as a backup when the connection fails, but it isn't ready: {}",
            fallback_label(&settings.fallback_provider),
            e
        )
    })?;
    info!("✅ Gemini Live ready (fallback: {})", settings.fallback_provider);
    Ok(())
}

pub async fn build_engine<R: Runtime>(app: &AppHandle<R>) -> Result<TranscriptionEngine, String> {
    let pool = pool(app);
    let api_key = resolve_api_key(&pool)
        .await?
        .ok_or("Gemini Live needs an API key. Add it in Settings → Transcription.")?;
    let (settings, _) = settings::load(&pool).await.map_err(|e| e.to_string())?;

    let fallback = match load_fallback(app, &settings.fallback_provider).await {
        Ok(f) => Some(f),
        Err(e) => {
            warn!("Gemini Live: local fallback unavailable: {}", e);
            None
        }
    };

    let status_app = app.clone();
    let on_status: StatusCallback = Arc::new(move |status: GeminiLiveStatus| {
        let _ = status_app.emit("gemini-live-status", &status);
    });

    info!(
        "🌐 Initializing Gemini Live transcription (language mode: {}, vocabulary: {} terms, mode: {})",
        settings.language_mode,
        settings.custom_vocabulary.len(),
        settings.transcription_mode
    );
    Ok(TranscriptionEngine::Provider(Arc::new(GeminiLiveProvider::new(
        api_key,
        settings.setup_options(),
        fallback,
        on_status,
    ))))
}

/// Called from the stop path: free the local fallback model.
pub async fn unload_fallback_engine<R: Runtime>(app: &AppHandle<R>) {
    let fallback_provider = settings::load(&pool(app))
        .await
        .map(|(s, _)| s.fallback_provider)
        .unwrap_or_else(|_| "parakeet".into());
    let unloaded = if fallback_provider == "localWhisper" {
        let engine = crate::whisper_engine::commands::WHISPER_ENGINE.lock().unwrap().as_ref().cloned();
        match engine {
            Some(e) => e.unload_model().await,
            None => false,
        }
    } else {
        let engine = crate::parakeet_engine::commands::PARAKEET_ENGINE.lock().unwrap().as_ref().cloned();
        match engine {
            Some(e) => e.unload_model().await,
            None => false,
        }
    };
    info!("Gemini Live: fallback {} model unloaded: {}", fallback_provider, unloaded);
}
