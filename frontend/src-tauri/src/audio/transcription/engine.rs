// audio/transcription/engine.rs
//
// TranscriptionEngine enum and model initialization/validation logic.

use super::provider::TranscriptionProvider;
use log::{info, warn};
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

/// Load the configured Whisper model at startup so the first recording does not stall.
///
/// The model is otherwise loaded during pre-record validation, where a ~3 GB file reaching
/// the GPU shows up as several seconds of dead time after the user presses record. Doing
/// it here costs nothing the user waits on. Skipped when Whisper is not the selected
/// provider, so a Parakeet or Nemotron user does not pay for a model they will not use.
pub async fn preload_configured_whisper_model<R: Runtime>(app: &AppHandle<R>) {
    // This runs from `setup`, where the database state may not be registered yet.
    // `State::state()` panics in that case, and a panic inside a spawned task disappears
    // without a trace - which is exactly how the first version of this failed silently.
    let mut attempts = 0;
    let config = loop {
        if app.try_state::<crate::state::AppState>().is_some() {
            match crate::api::api::api_get_transcript_config(
                app.clone(),
                app.state(),
                None,
            )
            .await
            {
                Ok(Some(config)) => break config,
                Ok(None) => return,
                Err(e) => {
                    warn!("Could not read the transcript config for pre-loading: {}", e);
                    return;
                }
            }
        }

        attempts += 1;
        if attempts > 20 {
            warn!("App state never became available; skipping Whisper pre-load");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    };

    if config.provider != "localWhisper" {
        return;
    }

    info!("⏳ Pre-loading Whisper model '{}' so recording starts instantly", config.model);
    match crate::whisper_engine::commands::whisper_validate_model_ready_with_config(app).await {
        Ok(model) => {
            info!("✅ Whisper model '{}' ready before the first recording", model);
            warm_up_whisper().await;
        }
        Err(e) => warn!("Could not pre-load the Whisper model: {}", e),
    }
}

/// Run one throwaway inference so the first real segment is not the one paying for GPU
/// kernel setup. Measured at roughly half a second on an RTX 5000 Ada - small next to the
/// model load, but it lands on the user's very first sentence.
async fn warm_up_whisper() {
    let engine = {
        let guard = match crate::whisper_engine::commands::WHISPER_ENGINE.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        guard.as_ref().cloned()
    };

    let engine = match engine {
        Some(engine) => engine,
        None => return,
    };

    // A second of silence: enough to walk the whole encode/decode path, and whatever text
    // comes back is discarded.
    let silence = vec![0.0f32; 16_000];
    match engine.transcribe_audio_with_confidence(silence, Some("en".to_string())).await {
        Ok(_) => info!("✅ Whisper GPU kernels warmed up"),
        Err(e) => warn!("Whisper warm-up pass failed (harmless): {}", e),
    }
}

/// Resolve where the Nemotron ONNX files for `model_name` live, falling back to the
/// default model directory when the stored configuration carries no model name.
fn nemotron_model_dir<R: Runtime>(
    app: &AppHandle<R>,
    model_name: &str,
) -> Result<std::path::PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve the app data directory: {}", e))?;

    let model_name = if model_name.trim().is_empty() {
        super::nemotron_provider::DEFAULT_NEMOTRON_MODEL
    } else {
        model_name
    };

    Ok(super::nemotron_provider::NemotronProvider::model_dir_for(
        &app_data_dir,
        model_name,
    ))
}

// ============================================================================
// TRANSCRIPTION ENGINE ENUM
// ============================================================================

// Transcription engine abstraction to support multiple providers
pub enum TranscriptionEngine {
    Whisper(Arc<crate::whisper_engine::WhisperEngine>),  // Direct access (backward compat)
    Parakeet(Arc<crate::parakeet_engine::ParakeetEngine>), // Direct access (backward compat)
    Provider(Arc<dyn TranscriptionProvider>),  // Trait-based (preferred for new code)
}

impl TranscriptionEngine {
    /// Check if the engine has a model loaded
    pub async fn is_model_loaded(&self) -> bool {
        match self {
            Self::Whisper(engine) => engine.is_model_loaded().await,
            Self::Parakeet(engine) => engine.is_model_loaded().await,
            Self::Provider(provider) => provider.is_model_loaded().await,
        }
    }

    /// Get the current model name
    pub async fn get_current_model(&self) -> Option<String> {
        match self {
            Self::Whisper(engine) => engine.get_current_model().await,
            Self::Parakeet(engine) => engine.get_current_model().await,
            Self::Provider(provider) => provider.get_current_model().await,
        }
    }

    /// Get the provider name for logging
    pub fn provider_name(&self) -> &str {
        match self {
            Self::Whisper(_) => "Whisper (direct)",
            Self::Parakeet(_) => "Parakeet (direct)",
            Self::Provider(provider) => provider.provider_name(),
        }
    }

    /// Whether this engine decodes a continuous stream one step at a time.
    ///
    /// The pipeline sends both VAD segments and stream steps without knowing which
    /// engine is running - it starts before the engine finishes loading, so it
    /// cannot know. This is what the worker uses to pick a lane and discard the
    /// other. Whisper and Parakeet answer `false`: both need whole utterances.
    pub fn supports_streaming(&self) -> bool {
        match self {
            Self::Whisper(_) | Self::Parakeet(_) => false,
            Self::Provider(provider) => provider.supports_streaming(),
        }
    }

    /// Decode one streaming step. Only meaningful when [`Self::supports_streaming`].
    pub async fn transcribe_step(
        &self,
        audio: Vec<f32>,
    ) -> std::result::Result<String, super::provider::TranscriptionError> {
        match self {
            Self::Provider(provider) => provider.transcribe_step(audio).await,
            _ => Err(super::provider::TranscriptionError::EngineFailed(format!(
                "{} does not decode streams a step at a time",
                self.provider_name()
            ))),
        }
    }

    /// Forget decoder state carried over from a previous recording.
    pub async fn reset_stream(
        &self,
    ) -> std::result::Result<(), super::provider::TranscriptionError> {
        match self {
            Self::Provider(provider) => provider.reset_stream().await,
            _ => Ok(()),
        }
    }
}

/// The language Nemotron is told to expect, as a BCP-47-ish code.
///
/// Defaulting to English rather than letting the model decide is a deliberate accuracy
/// choice: the multilingual model reserves a prompt slot for "work it out yourself", and
/// naming the language removes a guess it can get wrong. `en` and `en-US` reach the same
/// slot. A user who has set a language preference overrides this.
fn nemotron_language() -> String {
    match crate::get_language_preference_internal() {
        Some(lang) if !lang.trim().is_empty() && lang != "auto" => lang,
        _ => "en-US".to_string(),
    }
}

// ============================================================================
// MODEL VALIDATION AND INITIALIZATION
// ============================================================================

/// Validate that transcription models (Whisper or Parakeet) are ready before starting recording
pub async fn validate_transcription_model_ready<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    // Check transcript configuration to determine which engine to validate
    let config = match crate::api::api::api_get_transcript_config(
        app.clone(),
        app.clone().state(),
        None,
    )
    .await
    {
        Ok(Some(config)) => {
            info!(
                "📝 Found transcript config - provider: {}, model: {}",
                config.provider, config.model
            );
            config
        }
        Ok(None) => {
            info!("📝 No transcript config found, defaulting to parakeet");
            crate::api::api::TranscriptConfig {
                provider: "parakeet".to_string(),
                model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                api_key: None,
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to get transcript config: {}, defaulting to parakeet", e);
            crate::api::api::TranscriptConfig {
                provider: "parakeet".to_string(),
                model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                api_key: None,
            }
        }
    };

    // Validate based on provider
    match config.provider.as_str() {
        "localWhisper" => {
            info!("🔍 Validating Whisper model...");
            // Ensure whisper engine is initialized first
            if let Err(init_error) = crate::whisper_engine::commands::whisper_init().await {
                warn!("❌ Failed to initialize Whisper engine: {}", init_error);
                return Err(format!(
                    "Failed to initialize speech recognition: {}",
                    init_error
                ));
            }

            // Call the whisper validation command with config support
            match crate::whisper_engine::commands::whisper_validate_model_ready_with_config(app).await {
                Ok(model_name) => {
                    info!("✅ Whisper model validation successful: {} is ready", model_name);
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ Whisper model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        "parakeet" => {
            info!("🔍 Validating Parakeet model...");
            // Ensure parakeet engine is initialized first
            if let Err(init_error) = crate::parakeet_engine::commands::parakeet_init().await {
                warn!("❌ Failed to initialize Parakeet engine: {}", init_error);
                return Err(format!(
                    "Failed to initialize Parakeet speech recognition: {}",
                    init_error
                ));
            }

            // Use the validation command that includes auto-discovery and loading
            // This matches the Whisper behavior for consistency
            match crate::parakeet_engine::commands::parakeet_validate_model_ready_with_config(app).await {
                Ok(model_name) => {
                    info!("✅ Parakeet model validation successful: {} is ready", model_name);
                    Ok(())
                }
                Err(e) => {
                    warn!("❌ Parakeet model validation failed: {}", e);
                    Err(e)
                }
            }
        }
        "nemotron" => {
            info!("🔍 Validating Nemotron model...");
            let model_dir = nemotron_model_dir(app, &config.model)?;
            if super::nemotron_provider::NemotronProvider::model_is_installed(&model_dir) {
                info!("✅ Nemotron model found at {}", model_dir.display());
                Ok(())
            } else {
                warn!("❌ Nemotron model files missing from {}", model_dir.display());
                Err(format!(
                    "Nemotron model files are missing from {}. Download the model before recording.",
                    model_dir.display()
                ))
            }
        }
        other => {
            warn!("❌ Unsupported transcription provider for local recording: {}", other);
            Err(format!(
                "Provider '{}' is not supported for local transcription. Please select 'localWhisper', 'parakeet' or 'nemotron'.",
                other
            ))
        }
    }
}

/// Get or initialize the appropriate transcription engine based on provider configuration
pub async fn get_or_init_transcription_engine<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<TranscriptionEngine, String> {
    // Get provider configuration from API
    let config = match crate::api::api::api_get_transcript_config(
        app.clone(),
        app.clone().state(),
        None,
    )
    .await
    {
        Ok(Some(config)) => {
            info!(
                "📝 Transcript config - provider: {}, model: {}",
                config.provider, config.model
            );
            config
        }
        Ok(None) => {
            info!("📝 No transcript config found, defaulting to parakeet");
            crate::api::api::TranscriptConfig {
                provider: "parakeet".to_string(),
                model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                api_key: None,
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to get transcript config: {}, defaulting to parakeet", e);
            crate::api::api::TranscriptConfig {
                provider: "parakeet".to_string(),
                model: crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
                api_key: None,
            }
        }
    };

    // Initialize the appropriate engine based on provider
    match config.provider.as_str() {
        "parakeet" => {
            info!("🦜 Initializing Parakeet transcription engine");

            // Get Parakeet engine
            let engine = {
                let guard = crate::parakeet_engine::commands::PARAKEET_ENGINE
                    .lock()
                    .unwrap();
                guard.as_ref().cloned()
            };

            match engine {
                Some(engine) => {
                    // Check if model is loaded
                    if engine.is_model_loaded().await {
                        let model_name = engine.get_current_model().await
                            .unwrap_or_else(|| "unknown".to_string());
                        info!("✅ Parakeet model '{}' already loaded", model_name);
                        Ok(TranscriptionEngine::Parakeet(engine))
                    } else {
                        Err("Parakeet engine initialized but no model loaded. This should not happen after validation.".to_string())
                    }
                }
                None => {
                    Err("Parakeet engine not initialized. This should not happen after validation.".to_string())
                }
            }
        }
        "nemotron" => {
            info!("🧠 Initializing Nemotron transcription provider");
            let model_dir = nemotron_model_dir(app, &config.model)?;
            let language = nemotron_language();
            info!("🧠 Nemotron will transcribe as '{}'", language);
            let provider =
                super::nemotron_provider::NemotronProvider::new(model_dir, Some(language));

            // Start the sidecar now. The worker skips every chunk unless the provider
            // already reports a loaded model, so this cannot be deferred to first use.
            provider
                .ensure_started()
                .await
                .map_err(|e| format!("Failed to start the Nemotron sidecar: {}", e))?;

            Ok(TranscriptionEngine::Provider(Arc::new(provider)))
        }
        "localWhisper" | _ => {
            info!("🎤 Initializing Whisper transcription engine");
            let whisper_engine = get_or_init_whisper(app).await?;
            Ok(TranscriptionEngine::Whisper(whisper_engine))
        }
    }
}

/// Get or initialize transcription engine using API configuration
/// Returns Whisper engine if provider is localWhisper, otherwise returns error for non-Whisper providers
pub async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<crate::whisper_engine::WhisperEngine>, String> {
    // Check if engine already exists and has a model loaded
    let existing_engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard.as_ref().cloned()
    };

    if let Some(engine) = existing_engine {
        // Check if a model is already loaded
        if engine.is_model_loaded().await {
            let current_model = engine
                .get_current_model()
                .await
                .unwrap_or_else(|| "unknown".to_string());

            // NEW: Check if loaded model matches saved config
            let configured_model = match crate::api::api::api_get_transcript_config(
                app.clone(),
                app.clone().state(),
                None,
            )
            .await
            {
                Ok(Some(config)) => {
                    info!(
                        "📝 Saved transcript config - provider: {}, model: {}",
                        config.provider, config.model
                    );
                    if config.provider == "localWhisper" && !config.model.is_empty() {
                        Some(config.model)
                    } else {
                        None
                    }
                }
                Ok(None) => {
                    info!("📝 No transcript config found in database");
                    None
                }
                Err(e) => {
                    warn!("⚠️ Failed to get transcript config: {}", e);
                    None
                }
            };

            // If loaded model matches config, reuse it
            if let Some(ref expected_model) = configured_model {
                if current_model == *expected_model {
                    info!(
                        "✅ Loaded model '{}' matches saved config, reusing",
                        current_model
                    );
                    return Ok(engine);
                } else {
                    info!(
                        "🔄 Loaded model '{}' doesn't match saved config '{}', reloading correct model...",
                        current_model, expected_model
                    );
                    // Unload the incorrect model
                    engine.unload_model().await;
                    info!("📉 Unloaded incorrect model '{}'", current_model);
                    // Continue to model loading logic below
                }
            } else {
                // No specific config saved, accept currently loaded model
                info!(
                    "✅ No specific model configured, using currently loaded model: '{}'",
                    current_model
                );
                return Ok(engine);
            }
        } else {
            info!("🔄 Whisper engine exists but no model loaded, will load model from config");
        }
    }

    // Initialize new engine if needed
    info!("Initializing Whisper engine");

    // First ensure the engine is initialized
    if let Err(e) = crate::whisper_engine::commands::whisper_init().await {
        return Err(format!("Failed to initialize Whisper engine: {}", e));
    }

    // Get the engine reference
    let engine = {
        let engine_guard = crate::whisper_engine::commands::WHISPER_ENGINE
            .lock()
            .unwrap();
        engine_guard
            .as_ref()
            .cloned()
            .ok_or("Failed to get initialized engine")?
    };

    // Get model configuration from API
    let model_to_load =
        match crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
            .await
        {
            Ok(Some(config)) => {
                info!(
                    "Got transcript config from API - provider: {}, model: {}",
                    config.provider, config.model
                );
                if config.provider == "localWhisper" {
                    info!("Using model from API config: {}", config.model);
                    config.model
                } else {
                    // Non-Whisper provider (e.g., parakeet) - this function shouldn't be called
                    return Err(format!(
                        "Cannot initialize Whisper engine: Config uses '{}' provider. This is a bug in the transcription task initialization.",
                        config.provider
                    ));
                }
            }
            Ok(None) => {
                info!("No transcript config found in API, falling back to 'small'");
                "small".to_string()
            }
            Err(e) => {
                warn!(
                    "Failed to get transcript config from API: {}, falling back to 'small'",
                    e
                );
                "small".to_string()
            }
        };

    info!("Selected model to load: {}", model_to_load);

    // Discover available models to check if the desired model is downloaded
    let models = engine
        .discover_models()
        .await
        .map_err(|e| format!("Failed to discover models: {}", e))?;

    info!("Discovered {} models", models.len());
    for model in &models {
        info!(
            "Model: {} - Status: {:?} - Path: {}",
            model.name,
            model.status,
            model.path.display()
        );
    }

    // Check if the desired model is available
    let model_info = models.iter().find(|model| model.name == model_to_load);

    if model_info.is_none() {
        info!(
            "Model '{}' not found in discovered models. Available models: {:?}",
            model_to_load,
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    match model_info {
        Some(model) => {
            match model.status {
                crate::whisper_engine::ModelStatus::Available => {
                    info!("Loading model: {}", model_to_load);
                    engine
                        .load_model(&model_to_load)
                        .await
                        .map_err(|e| format!("Failed to load model '{}': {}", model_to_load, e))?;
                    info!("✅ Model '{}' loaded successfully", model_to_load);
                }
                crate::whisper_engine::ModelStatus::Missing => {
                    return Err(format!(
                        "Model '{}' is not downloaded. Please download it first from the settings.",
                        model_to_load
                    ));
                }
                crate::whisper_engine::ModelStatus::Downloading { progress } => {
                    return Err(format!("Model '{}' is currently downloading ({}%). Please wait for it to complete.", model_to_load, progress));
                }
                crate::whisper_engine::ModelStatus::Error(ref err) => {
                    return Err(format!("Model '{}' has an error: {}. Please check the model or try downloading it again.", model_to_load, err));
                }
                crate::whisper_engine::ModelStatus::Corrupted { .. } => {
                    return Err(format!("Model '{}' is corrupted. Please delete it and download again from the settings.", model_to_load));
                }
            }
        }
        None => {
            // Check if we have any available models and try to load the first one
            let available_models: Vec<_> = models
                .iter()
                .filter(|m| matches!(m.status, crate::whisper_engine::ModelStatus::Available))
                .collect();

            if let Some(fallback_model) = available_models.first() {
                warn!(
                    "Model '{}' not found, falling back to available model: '{}'",
                    model_to_load, fallback_model.name
                );
                engine.load_model(&fallback_model.name).await.map_err(|e| {
                    format!(
                        "Failed to load fallback model '{}': {}",
                        fallback_model.name, e
                    )
                })?;
                info!(
                    "✅ Fallback model '{}' loaded successfully",
                    fallback_model.name
                );
            } else {
                return Err(format!("Model '{}' is not supported and no other models are available. Please download a model from the settings.", model_to_load));
            }
        }
    }

    Ok(engine)
}
