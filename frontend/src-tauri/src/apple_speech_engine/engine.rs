//! Apple Speech engine implementation using SFSpeechRecognizer.
//!
//! Wraps macOS native speech recognition behind the same Arc<RwLock<>> pattern
//! used by WhisperEngine and ParakeetEngine.

use anyhow::{anyhow, Result};
use log::{info, warn};
use std::sync::Arc;
use tokio::sync::RwLock;

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognizer,
    SFSpeechRecognizerAuthorizationStatus,
};
use objc2_avf_audio::{AVAudioFormat, AVAudioPCMBuffer};

/// Apple Speech transcription engine.
///
/// Wraps SFSpeechRecognizer for on-device speech recognition on macOS.
/// Thread-safe via Arc<RwLock<>> — matches WhisperEngine/ParakeetEngine pattern.
pub struct AppleSpeechEngine {
    recognizer: Arc<RwLock<Option<Retained<SFSpeechRecognizer>>>>,
    locale_name: Arc<RwLock<String>>,
    available: Arc<RwLock<bool>>,
}

impl AppleSpeechEngine {
    /// Create a new AppleSpeechEngine with the system default locale.
    pub fn new() -> Result<Self> {
        let recognizer = unsafe { SFSpeechRecognizer::new() };
        let available = unsafe { recognizer.isAvailable() };
        let locale_name = unsafe {
            let locale = recognizer.locale();
            locale.localeIdentifier().to_string()
        };

        info!(
            "🍎 Apple Speech engine created — available: {}, locale: {}",
            available, locale_name
        );

        Ok(Self {
            recognizer: Arc::new(RwLock::new(Some(recognizer))),
            locale_name: Arc::new(RwLock::new(locale_name)),
            available: Arc::new(RwLock::new(available)),
        })
    }

    /// Create engine with a specific locale (e.g. "en-US", "es-ES").
    pub fn with_locale(locale_id: &str) -> Result<Self> {
        let ns_locale_id = NSString::from_str(locale_id);
        let locale = objc2_foundation::NSLocale::localeWithLocaleIdentifier(&ns_locale_id);
        let recognizer = unsafe { SFSpeechRecognizer::initWithLocale(
            SFSpeechRecognizer::alloc(),
            &locale,
        ) }
        .ok_or_else(|| anyhow!("Failed to create SFSpeechRecognizer with locale '{}'", locale_id))?;

        let available = unsafe { recognizer.isAvailable() };

        info!(
            "🍎 Apple Speech engine created with locale '{}' — available: {}",
            locale_id, available
        );

        Ok(Self {
            recognizer: Arc::new(RwLock::new(Some(recognizer))),
            locale_name: Arc::new(RwLock::new(locale_id.to_string())),
            available: Arc::new(RwLock::new(available)),
        })
    }

    /// Request speech recognition authorization from the user.
    pub async fn request_authorization() -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Mutex::new(Some(tx));

        let block = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
            let authorized = status == SFSpeechRecognizerAuthorizationStatus::Authorized;
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(authorized);
            }
        });

        unsafe { SFSpeechRecognizer::requestAuthorization(&block) };

        match rx.await {
            Ok(true) => {
                info!("🍎 Speech recognition authorized");
                Ok(())
            }
            Ok(false) => {
                warn!("🍎 Speech recognition not authorized by user");
                Err(anyhow!("Speech recognition not authorized. Please enable in System Settings > Privacy & Security > Speech Recognition."))
            }
            Err(_) => Err(anyhow!("Failed to receive authorization response")),
        }
    }

    /// Transcribe audio samples to text.
    ///
    /// Audio must be 16kHz mono f32 samples (same format as other engines).
    /// Returns (text, optional_confidence, is_partial).
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<(String, Option<f32>, bool)> {
        let recognizer_guard = self.recognizer.read().await;
        let recognizer = recognizer_guard
            .as_ref()
            .ok_or_else(|| anyhow!("Apple Speech recognizer not initialized"))?;

        if !unsafe { recognizer.isAvailable() } {
            return Err(anyhow!("Apple Speech recognizer is not available"));
        }

        // Create recognition request
        let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
        unsafe { request.setShouldReportPartialResults(false) };

        // Enable on-device recognition if supported
        if unsafe { recognizer.supportsOnDeviceRecognition() } {
            unsafe { request.setRequiresOnDeviceRecognition(true) };
        }

        // Create audio format: 16kHz, 1 channel, float32 interleaved
        let format = unsafe {
            AVAudioFormat::initStandardFormatWithSampleRate_channels(
                AVAudioFormat::alloc(),
                16000.0,
                1,
            )
        }
        .ok_or_else(|| anyhow!("Failed to create AVAudioFormat (16kHz mono)"))?;

        // Create PCM buffer and copy audio data
        let frame_count = audio.len() as u32;
        let buffer = unsafe {
            AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &format,
                frame_count,
            )
        }
        .ok_or_else(|| anyhow!("Failed to create AVAudioPCMBuffer"))?;

        // Copy f32 samples into the buffer
        unsafe {
            let float_data = buffer.floatChannelData();
            if float_data.is_null() {
                return Err(anyhow!("AVAudioPCMBuffer floatChannelData is null"));
            }
            let channel_ptr = (*float_data).as_ptr();
            std::ptr::copy_nonoverlapping(audio.as_ptr(), channel_ptr, audio.len());
            buffer.setFrameLength(frame_count);
        }

        // Append audio and signal end
        unsafe {
            request.appendAudioPCMBuffer(&buffer);
            request.endAudio();
        }

        // Run recognition task with result handler
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(String, Option<f32>, bool)>>();
        let tx = std::sync::Mutex::new(Some(tx));

        let block = RcBlock::new(
            move |result: *mut SFSpeechRecognitionResult,
                  error: *mut objc2_foundation::NSError| {
                let Some(tx) = tx.lock().unwrap().take() else {
                    return; // Already sent result
                };

                if !error.is_null() {
                    let err = unsafe { &*error };
                    let description = err.localizedDescription().to_string();
                    let _ = tx.send(Err(anyhow!("Speech recognition error: {}", description)));
                    return;
                }

                if result.is_null() {
                    let _ = tx.send(Err(anyhow!("Speech recognition returned null result")));
                    return;
                }

                let result = unsafe { &*result };
                let is_final = unsafe { result.isFinal() };

                if is_final {
                    let transcription = unsafe { result.bestTranscription() };
                    let text = unsafe { transcription.formattedString() }.to_string();

                    // Average confidence across segments
                    let segments = unsafe { transcription.segments() };
                    let confidence = if segments.len() > 0 {
                        let sum: f32 = segments
                            .to_vec()
                            .iter()
                            .map(|s| unsafe { s.confidence() } as f32)
                            .sum();
                        Some(sum / segments.len() as f32)
                    } else {
                        None
                    };

                    let _ = tx.send(Ok((text, confidence, false)));
                }
            },
        );

        let _task = unsafe {
            recognizer.recognitionTaskWithRequest_resultHandler(&request, &block)
        };

        // Wait for result with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow!("Recognition task channel closed unexpectedly")),
            Err(_) => Err(anyhow!("Speech recognition timed out after 30 seconds")),
        }
    }

    /// Check if the recognizer is available and ready.
    pub async fn is_model_loaded(&self) -> bool {
        *self.available.read().await
    }

    /// Get the current model/locale name.
    pub async fn get_current_model(&self) -> Option<String> {
        let name = self.locale_name.read().await.clone();
        if name == "unavailable" {
            None
        } else {
            Some(format!("apple-speech-{}", name))
        }
    }
}
