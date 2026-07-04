//! Apple Speech engine implementation using SFSpeechRecognizer.
//!
//! ObjC types from objc2 are not Send/Sync. We use two techniques:
//!
//! 1. Actor thread: the recognizer is wrapped in a `SendPtr` raw-pointer newtype
//!    that is `unsafe impl Send`. A dedicated OS thread owns it exclusively;
//!    no other code ever touches the pointer.
//!
//! 2. `request_authorization`: SFSpeechRecognizer::requestAuthorization is a class
//!    method that must be called on the main thread (Apple requirement). We call it
//!    synchronously in a blocking task and signal back via a std channel.

use anyhow::{anyhow, Result};
use log::{info, warn};
use std::sync::Arc;

use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_foundation::NSString;
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognizer,
    SFSpeechRecognizerAuthorizationStatus,
};
use objc2_avf_audio::{AVAudioFormat, AVAudioPCMBuffer};

// ---------------------------------------------------------------------------
// Raw-pointer wrapper that is Send (single owner, never aliased across threads)
// ---------------------------------------------------------------------------

struct SendPtr(*mut SFSpeechRecognizer);
unsafe impl Send for SendPtr {}

impl SendPtr {
    fn as_ref(&self) -> &SFSpeechRecognizer {
        // SAFETY: pointer is always valid; created from Retained and never freed
        // while the actor thread is alive.
        unsafe { &*self.0 }
    }
}

// ---------------------------------------------------------------------------
// Actor messages
// ---------------------------------------------------------------------------

type TranscribeResult = Result<(String, Option<f32>, bool)>;

enum ActorMsg {
    Transcribe {
        audio: Vec<f32>,
        reply: std::sync::mpsc::SyncSender<TranscribeResult>,
    },
    IsAvailable {
        reply: std::sync::mpsc::SyncSender<bool>,
    },
}

// ---------------------------------------------------------------------------
// Public engine struct — Send + Sync (all ObjC state lives on actor thread)
// ---------------------------------------------------------------------------

/// Apple Speech transcription engine.
///
/// Pins SFSpeechRecognizer to a private OS thread via an actor channel.
pub struct AppleSpeechEngine {
    tx: std::sync::mpsc::Sender<ActorMsg>,
    locale_name: Arc<std::sync::Mutex<String>>,
}

unsafe impl Send for AppleSpeechEngine {}
unsafe impl Sync for AppleSpeechEngine {}

impl AppleSpeechEngine {
    pub fn new() -> Result<Self> {
        let recognizer = unsafe { SFSpeechRecognizer::new() };
        let locale_name = unsafe { recognizer.locale().localeIdentifier().to_string() };
        info!("Apple Speech engine created — locale: {}", locale_name);
        Self::from_recognizer(recognizer, locale_name)
    }

    pub fn with_locale(locale_id: &str) -> Result<Self> {
        let ns_id = NSString::from_str(locale_id);
        let locale = objc2_foundation::NSLocale::localeWithLocaleIdentifier(&ns_id);
        let recognizer = unsafe {
            SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &locale)
        }
        .ok_or_else(|| anyhow!("Failed to create SFSpeechRecognizer with locale '{}'", locale_id))?;
        info!("Apple Speech engine created with locale '{}'", locale_id);
        Self::from_recognizer(recognizer, locale_id.to_string())
    }

    fn from_recognizer(recognizer: Retained<SFSpeechRecognizer>, locale: String) -> Result<Self> {
        // Leak the Retained into a raw pointer so we can send it across the thread boundary.
        // The actor thread owns it for its lifetime; it is freed when the channel closes.
        let raw = Retained::into_raw(recognizer) as *mut SFSpeechRecognizer;
        let ptr = SendPtr(raw);

        let (tx, rx) = std::sync::mpsc::channel::<ActorMsg>();

        std::thread::Builder::new()
            .name("apple-speech-actor".into())
            .spawn(move || {
                // actor_loop consumes ptr; ObjC object released here on the actor thread.
                actor_loop(ptr, rx);
            })?;

        Ok(Self {
            tx,
            locale_name: Arc::new(std::sync::Mutex::new(locale)),
        })
    }

    /// Request speech recognition authorization. Must be called before transcribing.
    pub async fn request_authorization() -> Result<()> {
        // RcBlock is !Send, so we spawn a std thread that owns both block and channel.
        let (result_tx, result_rx) = std::sync::mpsc::sync_channel::<bool>(1);
        std::thread::Builder::new()
            .name("apple-speech-auth".into())
            .spawn(move || {
                let tx = result_tx.clone();
                let block = RcBlock::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
                    let authorized = status == SFSpeechRecognizerAuthorizationStatus::Authorized;
                    let _ = tx.send(authorized);
                });
                unsafe { SFSpeechRecognizer::requestAuthorization(&block) };
                // block dropped here on this thread after the callback fires
            })
            .map_err(|e| anyhow!("Failed to spawn auth thread: {}", e))?;

        let authorized = tokio::task::spawn_blocking(move || {
            result_rx
                .recv_timeout(std::time::Duration::from_secs(60))
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false);

        if authorized {
            info!("Speech recognition authorized");
            Ok(())
        } else {
            warn!("Speech recognition not authorized by user");
            Err(anyhow!(
                "Speech recognition not authorized. Enable in System Settings > Privacy & Security > Speech Recognition."
            ))
        }
    }

    /// Transcribe 16kHz mono f32 audio. Returns (text, confidence, is_partial).
    pub async fn transcribe_audio(&self, audio: Vec<f32>) -> Result<(String, Option<f32>, bool)> {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        self.tx
            .send(ActorMsg::Transcribe { audio, reply: reply_tx })
            .map_err(|_| anyhow!("Apple Speech actor thread died"))?;
        tokio::task::spawn_blocking(move || {
            reply_rx
                .recv_timeout(std::time::Duration::from_secs(30))
                .map_err(|_| anyhow!("Speech recognition timed out after 30 seconds"))?
        })
        .await
        .map_err(|e| anyhow!("spawn_blocking panicked: {}", e))?
    }

    pub async fn is_model_loaded(&self) -> bool {
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        if self.tx.send(ActorMsg::IsAvailable { reply: reply_tx }).is_err() {
            return false;
        }
        tokio::task::spawn_blocking(move || reply_rx.recv().unwrap_or(false))
            .await
            .unwrap_or(false)
    }

    pub async fn get_current_model(&self) -> Option<String> {
        let name = self.locale_name.lock().unwrap().clone();
        if name.is_empty() { None } else { Some(format!("apple-speech-{}", name)) }
    }
}

// ---------------------------------------------------------------------------
// Actor loop — runs on dedicated thread, exclusively owns the recognizer
// ---------------------------------------------------------------------------

fn actor_loop(ptr: SendPtr, rx: std::sync::mpsc::Receiver<ActorMsg>) {
    for msg in rx {
        match msg {
            ActorMsg::IsAvailable { reply } => {
                let available = unsafe { ptr.as_ref().isAvailable() };
                let _ = reply.send(available);
            }
            ActorMsg::Transcribe { audio, reply } => {
                let result = transcribe_sync(ptr.as_ref(), audio);
                let _ = reply.send(result);
            }
        }
    }
    // Release the ObjC object on this thread when channel closes.
    // SAFETY: ptr.0 was created from Retained::into_raw in from_recognizer.
    unsafe { drop(Retained::from_raw(ptr.0)) };
}

fn transcribe_sync(recognizer: &SFSpeechRecognizer, audio: Vec<f32>) -> TranscribeResult {
    if !unsafe { recognizer.isAvailable() } {
        return Err(anyhow!("Apple Speech recognizer is not available"));
    }

    let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
    unsafe { request.setShouldReportPartialResults(false) };
    if unsafe { recognizer.supportsOnDeviceRecognition() } {
        unsafe { request.setRequiresOnDeviceRecognition(true) };
    }

    let format = unsafe {
        AVAudioFormat::initStandardFormatWithSampleRate_channels(AVAudioFormat::alloc(), 16000.0, 1)
    }
    .ok_or_else(|| anyhow!("Failed to create AVAudioFormat (16kHz mono)"))?;

    let frame_count = audio.len() as u32;
    let buffer = unsafe {
        AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &format, frame_count)
    }
    .ok_or_else(|| anyhow!("Failed to create AVAudioPCMBuffer"))?;

    unsafe {
        let float_data = buffer.floatChannelData();
        if float_data.is_null() {
            return Err(anyhow!("AVAudioPCMBuffer floatChannelData is null"));
        }
        let channel_ptr = (*float_data).as_ptr();
        std::ptr::copy_nonoverlapping(audio.as_ptr(), channel_ptr, audio.len());
        buffer.setFrameLength(frame_count);
        request.appendAudioPCMBuffer(&buffer);
        request.endAudio();
    }

    let (tx, rx) = std::sync::mpsc::channel::<TranscribeResult>();
    let tx = std::sync::Mutex::new(Some(tx));

    let block = RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, error: *mut objc2_foundation::NSError| {
            let Some(tx) = tx.lock().unwrap().take() else { return };
            if !error.is_null() {
                let desc = unsafe { &*error }.localizedDescription().to_string();
                let _ = tx.send(Err(anyhow!("Speech recognition error: {}", desc)));
                return;
            }
            if result.is_null() {
                let _ = tx.send(Err(anyhow!("Speech recognition returned null result")));
                return;
            }
            let result = unsafe { &*result };
            if unsafe { result.isFinal() } {
                let transcription = unsafe { result.bestTranscription() };
                let text = unsafe { transcription.formattedString() }.to_string();
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

    let _task = unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &block) };

    rx.recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| anyhow!("Speech recognition timed out after 30 seconds"))?
}
