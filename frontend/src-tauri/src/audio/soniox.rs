use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
};
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::api::TranscriptSegment;
use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

use super::recording_state::AudioChunk;
use super::transcription::TranscriptUpdate;

pub const PROVIDER: &str = "soniox";
const FILES_URL: &str = "https://api.soniox.com/v1/files";
const TRANSCRIPTIONS_URL: &str = "https://api.soniox.com/v1/transcriptions";
const REALTIME_WS_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
const PCM_SAMPLE_RATE: u32 = 16_000;
const MAX_POLL_ATTEMPTS: usize = 3_600; // 2 hours at 2s intervals

static REALTIME_SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static REALTIME_SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub struct SonioxClient {
    api_key: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SonioxToken {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub start_ms: Option<f64>,
    #[serde(default)]
    pub end_ms: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub is_final: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SonioxTranscript {
    pub text: String,
    pub tokens: Vec<SonioxToken>,
}

#[derive(Debug, Deserialize)]
struct UploadFileResponse {
    #[serde(alias = "id")]
    file_id: String,
}

#[derive(Debug, Serialize)]
struct CreateTranscriptionRequest<'a> {
    model: &'a str,
    file_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language_hints: Option<Vec<String>>,
    enable_language_identification: bool,
    enable_speaker_diarization: bool,
    client_reference_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct CreateTranscriptionResponse {
    #[serde(alias = "transcription_id")]
    id: String,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionStatusResponse {
    status: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptResponse {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    tokens: Vec<SonioxToken>,
}

#[derive(Debug, Deserialize)]
struct RealtimeResponse {
    #[serde(default)]
    tokens: Vec<SonioxToken>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AudioTimeMapEntry {
    pub stream_start_ms: f64,
    pub stream_end_ms: f64,
    pub original_start_ms: f64,
    pub original_end_ms: f64,
}

#[derive(Debug, Clone)]
struct MappedRealtimeToken {
    text: String,
    start_ms: f64,
    end_ms: f64,
    confidence: Option<f32>,
    speaker: Option<String>,
}

struct RealtimeSegmentBuffer<R: Runtime> {
    app: AppHandle<R>,
    tokens: Vec<MappedRealtimeToken>,
}

impl SonioxClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    pub async fn transcribe_file_with_progress<F, C>(
        &self,
        path: &Path,
        language: Option<String>,
        client_reference_id: &str,
        mut progress: F,
        mut should_cancel: C,
    ) -> Result<SonioxTranscript>
    where
        F: FnMut(u32, &str),
        C: FnMut() -> bool,
    {
        ensure_not_cancelled(&mut should_cancel)?;
        progress(5, "Uploading audio to Soniox...");
        let file_id = self.upload_file(path).await?;

        ensure_not_cancelled(&mut should_cancel)?;
        progress(20, "Creating Soniox transcription...");
        let transcription_id = self
            .create_transcription(&file_id, language, client_reference_id)
            .await?;

        progress(30, "Waiting for Soniox transcription...");
        self.poll_until_complete(&transcription_id, &mut progress, &mut should_cancel)
            .await?;

        ensure_not_cancelled(&mut should_cancel)?;
        progress(90, "Fetching Soniox transcript...");
        self.get_transcript(&transcription_id).await
    }

    async fn upload_file(&self, path: &Path) -> Result<String> {
        let file = tokio::fs::File::open(path)
            .await
            .with_context(|| format!("Failed to open audio file: {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio")
            .to_string();
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = reqwest::Body::wrap_stream(stream);
        let part = Part::stream(body)
            .file_name(filename)
            .mime_str("application/octet-stream")?;
        let form = Form::new().part("file", part);

        let response = self
            .http
            .post(FILES_URL)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .context("Failed to upload audio to Soniox")?;

        let response = ensure_success(response, "upload file").await?;
        let payload: UploadFileResponse = response
            .json()
            .await
            .context("Failed to parse Soniox upload response")?;

        Ok(payload.file_id)
    }

    async fn create_transcription(
        &self,
        file_id: &str,
        language: Option<String>,
        client_reference_id: &str,
    ) -> Result<String> {
        let language_hints = language_hints(language);
        let body = CreateTranscriptionRequest {
            model: crate::config::DEFAULT_SONIOX_ASYNC_MODEL,
            file_id,
            language_hints: if language_hints.is_empty() {
                None
            } else {
                Some(language_hints)
            },
            enable_language_identification: true,
            enable_speaker_diarization: false,
            client_reference_id,
        };

        let response = self
            .http
            .post(TRANSCRIPTIONS_URL)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("Failed to create Soniox transcription")?;

        let response = ensure_success(response, "create transcription").await?;
        let payload: CreateTranscriptionResponse = response
            .json()
            .await
            .context("Failed to parse Soniox create-transcription response")?;

        if let Some(status) = payload.status.as_deref() {
            info!(
                "Soniox transcription {} created with status {}",
                payload.id, status
            );
        }

        Ok(payload.id)
    }

    async fn poll_until_complete<F, C>(
        &self,
        transcription_id: &str,
        progress: &mut F,
        should_cancel: &mut C,
    ) -> Result<()>
    where
        F: FnMut(u32, &str),
        C: FnMut() -> bool,
    {
        for attempt in 0..MAX_POLL_ATTEMPTS {
            ensure_not_cancelled(should_cancel)?;

            let response = self
                .http
                .get(format!("{}/{}", TRANSCRIPTIONS_URL, transcription_id))
                .bearer_auth(&self.api_key)
                .send()
                .await
                .context("Failed to poll Soniox transcription")?;
            let response = ensure_success(response, "poll transcription").await?;
            let status: TranscriptionStatusResponse = response
                .json()
                .await
                .context("Failed to parse Soniox transcription status")?;

            match status.status.as_str() {
                "completed" => {
                    progress(85, "Soniox transcription completed");
                    return Ok(());
                }
                "error" | "failed" => {
                    let message = status
                        .error_message
                        .or(status.error)
                        .unwrap_or_else(|| "Unknown Soniox transcription error".to_string());
                    return Err(anyhow!("Soniox transcription failed: {}", message));
                }
                "queued" | "processing" => {
                    let poll_progress = 30 + ((attempt as f32 / 90.0).min(1.0) * 50.0) as u32;
                    progress(poll_progress, "Soniox transcription is processing...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                other => {
                    warn!("Unknown Soniox transcription status: {}", other);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        Err(anyhow!("Timed out waiting for Soniox transcription"))
    }

    async fn get_transcript(&self, transcription_id: &str) -> Result<SonioxTranscript> {
        let response = self
            .http
            .get(format!(
                "{}/{}/transcript",
                TRANSCRIPTIONS_URL, transcription_id
            ))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .context("Failed to fetch Soniox transcript")?;
        let response = ensure_success(response, "fetch transcript").await?;
        let payload: TranscriptResponse = response
            .json()
            .await
            .context("Failed to parse Soniox transcript response")?;

        let text = payload.text.unwrap_or_else(|| {
            payload
                .tokens
                .iter()
                .map(|token| token.text.as_str())
                .collect()
        });

        Ok(SonioxTranscript {
            text,
            tokens: payload.tokens,
        })
    }
}

pub async fn configured_api_key<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let stored_key =
        SettingsRepository::get_transcript_api_key(app_state.db_manager.pool(), PROVIDER)
            .await
            .context("Failed to read Soniox API key from settings")?;

    let key = stored_key
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("SONIOX_API_KEY").ok())
        .unwrap_or_default();

    if key.trim().is_empty() {
        return Err(anyhow!(
            "Soniox API key is not configured. Add it in Transcription settings or set SONIOX_API_KEY."
        ));
    }

    Ok(key)
}

pub async fn validate_configured_api_key<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    configured_api_key(app)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn client_from_config<R: Runtime>(app: &AppHandle<R>) -> Result<SonioxClient> {
    Ok(SonioxClient::new(configured_api_key(app).await?))
}

pub fn soniox_tokens_to_transcript_tuples(tokens: &[SonioxToken]) -> Vec<(String, f64, f64)> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut current_start_ms = 0.0;
    let mut current_end_ms = 0.0;
    let mut current_speaker: Option<String> = None;

    for token in tokens {
        if token.text.is_empty() {
            continue;
        }

        let token_start_ms = token.start_ms.unwrap_or(current_end_ms);
        let token_end_ms = token.end_ms.unwrap_or(token_start_ms).max(token_start_ms);
        let speaker_changed = current_speaker.is_some() && token.speaker != current_speaker;
        let long_gap = !current_text.is_empty() && token_start_ms - current_end_ms > 1500.0;
        let long_segment =
            token_start_ms - current_start_ms > 30_000.0 && ends_sentence(&current_text);

        if !current_text.is_empty() && (speaker_changed || long_gap || long_segment) {
            push_transcript_tuple(
                &mut segments,
                std::mem::take(&mut current_text),
                current_start_ms,
                current_end_ms,
            );
            current_speaker = None;
        }

        if current_text.is_empty() {
            current_start_ms = token_start_ms;
            current_speaker = token.speaker.clone();
        }

        current_text.push_str(&token.text);
        current_end_ms = token_end_ms;
    }

    push_transcript_tuple(
        &mut segments,
        current_text,
        current_start_ms,
        current_end_ms,
    );
    segments
}

pub fn soniox_tokens_to_transcript_segments(tokens: &[SonioxToken]) -> Vec<TranscriptSegment> {
    super::common::create_transcript_segments(&soniox_tokens_to_transcript_tuples(tokens))
}

pub fn transcript_to_segments(
    transcript: &SonioxTranscript,
    fallback_duration_seconds: f64,
) -> Vec<TranscriptSegment> {
    let mut segments = soniox_tokens_to_transcript_segments(&transcript.tokens);
    if segments.is_empty() && !transcript.text.trim().is_empty() {
        segments.push(TranscriptSegment {
            id: format!("transcript-{}", uuid::Uuid::new_v4()),
            text: transcript.text.trim().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(fallback_duration_seconds),
            duration: Some(fallback_duration_seconds),
        });
    }
    segments
}

pub fn map_stream_ms_to_original(entries: &[AudioTimeMapEntry], stream_ms: f64) -> f64 {
    if entries.is_empty() {
        return stream_ms;
    }

    if let Some(entry) = entries
        .iter()
        .find(|entry| stream_ms >= entry.stream_start_ms && stream_ms <= entry.stream_end_ms)
    {
        let stream_duration = (entry.stream_end_ms - entry.stream_start_ms).max(1.0);
        let original_duration = (entry.original_end_ms - entry.original_start_ms).max(0.0);
        let offset_ratio = ((stream_ms - entry.stream_start_ms) / stream_duration).clamp(0.0, 1.0);
        return entry.original_start_ms + original_duration * offset_ratio;
    }

    if stream_ms < entries[0].stream_start_ms {
        return entries[0].original_start_ms;
    }

    let last = entries.last().expect("entries is not empty");
    last.original_end_ms + (stream_ms - last.stream_end_ms).max(0.0)
}

pub async fn run_realtime_transcription<R: Runtime>(
    app: AppHandle<R>,
    transcription_receiver: mpsc::UnboundedReceiver<AudioChunk>,
) -> Result<(), String> {
    REALTIME_SPEECH_DETECTED_EMITTED.store(false, Ordering::SeqCst);

    let api_key = configured_api_key(&app).await.map_err(|e| e.to_string())?;
    let language = crate::get_language_preference_internal();
    let language_hints = language_hints(language);

    let mut request = REALTIME_WS_URL
        .into_client_request()
        .map_err(|e| format!("Failed to build Soniox WebSocket request: {}", e))?;
    let auth_header = HeaderValue::from_str(&format!("Bearer {}", api_key))
        .map_err(|e| format!("Failed to build Soniox auth header: {}", e))?;
    request.headers_mut().insert("Authorization", auth_header);

    info!("Connecting to Soniox realtime transcription");
    let (ws_stream, _) = connect_async(request)
        .await
        .map_err(|e| format!("Failed to connect to Soniox realtime API: {}", e))?;
    let (mut write, mut read) = ws_stream.split();

    let config = serde_json::json!({
        "api_key": api_key,
        "model": crate::config::DEFAULT_SONIOX_REALTIME_MODEL,
        "audio_format": "pcm_s16le",
        "sample_rate": PCM_SAMPLE_RATE,
        "num_channels": 1,
        "enable_language_identification": true,
        "enable_endpoint_detection": true,
        "language_hints": language_hints,
    });

    write
        .send(Message::Text(config.to_string()))
        .await
        .map_err(|e| format!("Failed to send Soniox realtime config: {}", e))?;

    let time_map = Arc::new(Mutex::new(Vec::<AudioTimeMapEntry>::new()));
    let stream_cursor_ms = Arc::new(Mutex::new(0.0f64));
    let shutdown = Arc::new(AtomicBool::new(false));

    let writer_time_map = time_map.clone();
    let writer_cursor = stream_cursor_ms.clone();
    let writer_shutdown = shutdown.clone();
    let writer_handle = tokio::spawn(async move {
        let mut receiver = transcription_receiver;
        let mut ping_interval = tokio::time::interval(Duration::from_secs(20));
        let mut shutdown_interval = tokio::time::interval(Duration::from_millis(250));

        loop {
            tokio::select! {
                _ = shutdown_interval.tick() => {
                    if writer_shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(Vec::new())).await {
                        return Err(anyhow!("Failed to ping Soniox realtime API: {}", e));
                    }
                }
                maybe_chunk = receiver.recv() => {
                    let Some(chunk) = maybe_chunk else {
                        break;
                    };

                    if chunk.data.is_empty() {
                        continue;
                    }

                    let original_start_ms = chunk.timestamp * 1000.0;
                    let original_duration_ms =
                        chunk.data.len() as f64 / chunk.sample_rate as f64 * 1000.0;
                    let original_end_ms = original_start_ms + original_duration_ms;

                    let pcm_samples = if chunk.sample_rate != PCM_SAMPLE_RATE {
                        super::audio_processing::resample_audio(&chunk.data, chunk.sample_rate, PCM_SAMPLE_RATE)
                    } else {
                        chunk.data
                    };

                    if pcm_samples.is_empty() {
                        continue;
                    }

                    let duration_ms = pcm_samples.len() as f64 / PCM_SAMPLE_RATE as f64 * 1000.0;

                    {
                        let mut cursor = writer_cursor.lock().await;
                        let entry = AudioTimeMapEntry {
                            stream_start_ms: *cursor,
                            stream_end_ms: *cursor + duration_ms,
                            original_start_ms,
                            original_end_ms,
                        };
                        *cursor += duration_ms;
                        writer_time_map.lock().await.push(entry);
                    }

                    let bytes = f32_to_pcm16le(&pcm_samples);
                    write
                        .send(Message::Binary(bytes))
                        .await
                        .map_err(|e| anyhow!("Failed to send audio to Soniox realtime API: {}", e))?;
                }
            }
        }

        let _ = write.send(Message::Binary(Vec::new())).await;
        Ok::<(), anyhow::Error>(())
    });

    let reader_time_map = time_map.clone();
    let reader_shutdown = shutdown.clone();
    let reader_app = app.clone();
    let reader_handle = tokio::spawn(async move {
        let mut buffer = RealtimeSegmentBuffer::new(reader_app);

        while let Some(message) = read.next().await {
            let message = match message {
                Ok(message) => message,
                Err(e) => {
                    reader_shutdown.store(true, Ordering::SeqCst);
                    return Err(anyhow!("Soniox realtime WebSocket error: {}", e));
                }
            };

            match message {
                Message::Text(text) => {
                    let response: RealtimeResponse = match serde_json::from_str(&text) {
                        Ok(response) => response,
                        Err(e) => {
                            warn!("Failed to parse Soniox realtime response: {} ({})", e, text);
                            continue;
                        }
                    };

                    if let Some(message) = response
                        .error_message
                        .or(response.error)
                        .or(response.error_code)
                    {
                        reader_shutdown.store(true, Ordering::SeqCst);
                        return Err(anyhow!("Soniox realtime error: {}", message));
                    }

                    let entries = reader_time_map.lock().await.clone();
                    let final_tokens = response
                        .tokens
                        .into_iter()
                        .filter(|token| token.is_final.unwrap_or(false))
                        .map(|token| {
                            let start_ms = map_stream_ms_to_original(
                                &entries,
                                token.start_ms.unwrap_or_default(),
                            );
                            let end_ms = map_stream_ms_to_original(
                                &entries,
                                token.end_ms.unwrap_or(token.start_ms.unwrap_or_default()),
                            )
                            .max(start_ms);
                            MappedRealtimeToken {
                                text: token.text,
                                start_ms,
                                end_ms,
                                confidence: token.confidence,
                                speaker: token.speaker,
                            }
                        })
                        .collect::<Vec<_>>();

                    buffer.ingest(final_tokens);
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        buffer.flush();
        reader_shutdown.store(true, Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    });

    let writer_result = writer_handle
        .await
        .map_err(|e| format!("Soniox realtime writer task failed: {}", e))?;
    if let Err(e) = writer_result {
        shutdown.store(true, Ordering::SeqCst);
        emit_transcription_error(&app, e.to_string());
        return Err(e.to_string());
    }

    let mut reader_handle = reader_handle;
    match tokio::time::timeout(Duration::from_secs(30), &mut reader_handle).await {
        Ok(join_result) => {
            let reader_result =
                join_result.map_err(|e| format!("Soniox realtime reader task failed: {}", e))?;
            if let Err(e) = reader_result {
                emit_transcription_error(&app, e.to_string());
                return Err(e.to_string());
            }
        }
        Err(_) => {
            reader_handle.abort();
            warn!("Timed out waiting for Soniox realtime final response");
        }
    }

    info!("Soniox realtime transcription task completed");
    Ok(())
}

impl<R: Runtime> RealtimeSegmentBuffer<R> {
    fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            tokens: Vec::new(),
        }
    }

    fn ingest(&mut self, tokens: Vec<MappedRealtimeToken>) {
        for token in tokens {
            if token.text.is_empty() {
                continue;
            }

            let gap = self
                .tokens
                .last()
                .map(|last| token.start_ms - last.end_ms > 1200.0)
                .unwrap_or(false);
            let speaker_changed = self
                .tokens
                .last()
                .map(|last| last.speaker != token.speaker)
                .unwrap_or(false);

            if !self.tokens.is_empty() && (gap || speaker_changed) {
                self.flush();
            }

            self.tokens.push(token);

            if self
                .tokens
                .last()
                .map(|last| ends_sentence(&last.text))
                .unwrap_or(false)
            {
                self.flush();
            }
        }
    }

    fn flush(&mut self) {
        if self.tokens.is_empty() {
            return;
        }

        let text = self
            .tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<String>()
            .trim()
            .to_string();
        if text.is_empty() {
            self.tokens.clear();
            return;
        }

        emit_speech_detected(&self.app);

        let start_ms = self
            .tokens
            .first()
            .map(|token| token.start_ms)
            .unwrap_or(0.0);
        let end_ms = self
            .tokens
            .last()
            .map(|token| token.end_ms)
            .unwrap_or(start_ms);
        let confidence =
            average_confidence(self.tokens.iter().filter_map(|token| token.confidence));
        emit_transcript_update(&self.app, text, start_ms, end_ms, confidence, false);
        self.tokens.clear();
    }
}

fn emit_speech_detected<R: Runtime>(app: &AppHandle<R>) {
    if REALTIME_SPEECH_DETECTED_EMITTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let _ = app.emit(
            "speech-detected",
            serde_json::json!({ "message": "Speech activity detected" }),
        );
    }
}

fn emit_transcript_update<R: Runtime>(
    app: &AppHandle<R>,
    text: String,
    start_ms: f64,
    end_ms: f64,
    confidence: f32,
    is_partial: bool,
) {
    let sequence_id = REALTIME_SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let audio_start_time = start_ms / 1000.0;
    let audio_end_time = end_ms.max(start_ms) / 1000.0;
    let duration = (audio_end_time - audio_start_time).max(0.0);

    let update = TranscriptUpdate {
        text,
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        source: "Audio".to_string(),
        sequence_id,
        chunk_start_time: audio_start_time,
        is_partial,
        confidence,
        audio_start_time,
        audio_end_time,
        duration,
    };

    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit Soniox transcript update: {}", e);
    }
}

fn emit_transcription_error<R: Runtime>(app: &AppHandle<R>, error: String) {
    let _ = app.emit(
        "transcription-error",
        serde_json::json!({
            "error": error,
            "userMessage": "Soniox transcription failed. Check your API key and network connection.",
            "actionable": true
        }),
    );
}

fn language_hints(language: Option<String>) -> Vec<String> {
    language
        .into_iter()
        .map(|language| language.trim().to_string())
        .filter(|language| {
            !language.is_empty()
                && language != "auto"
                && language != "auto-translate"
                && language != "detect"
        })
        .collect()
}

fn f32_to_pcm16le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

async fn ensure_success(response: reqwest::Response, action: &str) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(anyhow!(
        "Soniox {} failed (HTTP {}): {}",
        action,
        status,
        truncate_for_error(&body)
    ))
}

fn ensure_not_cancelled<C>(should_cancel: &mut C) -> Result<()>
where
    C: FnMut() -> bool,
{
    if should_cancel() {
        Err(anyhow!("Operation cancelled"))
    } else {
        Ok(())
    }
}

fn push_transcript_tuple(
    segments: &mut Vec<(String, f64, f64)>,
    text: String,
    start_ms: f64,
    end_ms: f64,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }

    segments.push((trimmed.to_string(), start_ms, end_ms.max(start_ms)));
}

fn average_confidence(values: impl Iterator<Item = f32>) -> f32 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values {
        sum += value;
        count += 1;
    }

    if count == 0 {
        0.85
    } else {
        sum / count as f32
    }
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .map(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .unwrap_or(false)
}

fn truncate_for_error(text: &str) -> String {
    const MAX_LEN: usize = 500;
    if text.len() <= MAX_LEN {
        return text.to_string();
    }

    let mut end = MAX_LEN;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_tokens_on_large_gaps() {
        let tokens = vec![
            SonioxToken {
                text: "Hello".to_string(),
                start_ms: Some(0.0),
                end_ms: Some(400.0),
                confidence: Some(0.9),
                speaker: None,
                is_final: Some(true),
            },
            SonioxToken {
                text: " world.".to_string(),
                start_ms: Some(450.0),
                end_ms: Some(900.0),
                confidence: Some(0.9),
                speaker: None,
                is_final: Some(true),
            },
            SonioxToken {
                text: "Later".to_string(),
                start_ms: Some(3000.0),
                end_ms: Some(3400.0),
                confidence: Some(0.8),
                speaker: None,
                is_final: Some(true),
            },
        ];

        let segments = soniox_tokens_to_transcript_tuples(&tokens);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0], ("Hello world.".to_string(), 0.0, 900.0));
        assert_eq!(segments[1], ("Later".to_string(), 3000.0, 3400.0));
    }

    #[test]
    fn maps_stream_time_to_original_chunk_time() {
        let entries = vec![
            AudioTimeMapEntry {
                stream_start_ms: 0.0,
                stream_end_ms: 1000.0,
                original_start_ms: 5000.0,
                original_end_ms: 6000.0,
            },
            AudioTimeMapEntry {
                stream_start_ms: 1000.0,
                stream_end_ms: 2000.0,
                original_start_ms: 9000.0,
                original_end_ms: 10_000.0,
            },
        ];

        assert_eq!(map_stream_ms_to_original(&entries, 500.0), 5500.0);
        assert_eq!(map_stream_ms_to_original(&entries, 1500.0), 9500.0);
    }
}
