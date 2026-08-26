use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
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

pub const PROVIDER: &str = "deepgram";
const LISTEN_V1_REST_URL: &str = "https://api.deepgram.com/v1/listen";
const REALTIME_V1_WS_URL: &str = "wss://api.deepgram.com/v1/listen";
const REALTIME_V2_WS_URL: &str = "wss://api.deepgram.com/v2/listen";
const PCM_SAMPLE_RATE: u32 = 16_000;

static REALTIME_SEQUENCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static REALTIME_SPEECH_DETECTED_EMITTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub struct DeepgramClient {
    api_key: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeepgramWord {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub speaker: Option<usize>,
    #[serde(default)]
    pub punctuated_word: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramAlternative {
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub words: Vec<DeepgramWord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramChannel {
    #[serde(default)]
    pub alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramLiveResponse {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub turn_index: Option<usize>,
    #[serde(default)]
    pub channel: Option<DeepgramChannel>,
    #[serde(default)]
    pub is_final: Option<bool>,
    #[serde(default)]
    pub speech_final: Option<bool>,
    #[serde(default)]
    pub audio_window_start: Option<f64>,
    #[serde(default)]
    pub audio_window_end: Option<f64>,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub duration: Option<f64>,
    // Top-level fields for Flux TurnInfo format
    #[serde(default)]
    pub transcript: Option<String>,
    #[serde(default)]
    pub words: Vec<DeepgramWord>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub err_code: Option<String>,
    #[serde(default)]
    pub err_msg: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramPrerecordedResponse {
    #[serde(default)]
    pub results: Option<DeepgramPrerecordedResults>,
    #[serde(default)]
    pub err_code: Option<String>,
    #[serde(default)]
    pub err_msg: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepgramPrerecordedResults {
    #[serde(default)]
    pub channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Clone)]
pub struct DeepgramTranscript {
    pub text: String,
    pub words: Vec<DeepgramWord>,
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
    speaker: Option<usize>,
}

#[derive(Debug, Clone)]
struct FluxTurnState {
    turn_index: usize,
    words: Vec<DeepgramWord>,
    transcript: Option<String>,
    start_sec: Option<f64>,
    end_sec: Option<f64>,
    is_flushed: bool,
}

struct RealtimeSegmentBuffer<R: Runtime> {
    app: AppHandle<R>,
    tokens: Vec<MappedRealtimeToken>,
}

pub fn build_realtime_ws_url(model: &str, language: Option<&str>) -> String {
    let is_flux = model.to_lowercase().contains("flux");
    if is_flux {
        let mut params = vec![
            format!("model={}", model),
            "encoding=linear16".to_string(),
            format!("sample_rate={}", PCM_SAMPLE_RATE),
        ];

        if let Some(lang) = language {
            let trimmed = lang.trim();
            if !trimmed.is_empty()
                && trimmed != "auto"
                && trimmed != "auto-translate"
                && trimmed != "detect"
            {
                params.push(format!("language_hint={}", trimmed));
            }
        }

        format!("{}?{}", REALTIME_V2_WS_URL, params.join("&"))
    } else {
        let mut params = vec![
            format!("model={}", model),
            "encoding=linear16".to_string(),
            format!("sample_rate={}", PCM_SAMPLE_RATE),
            "channels=1".to_string(),
            "punctuate=true".to_string(),
            "interim_results=true".to_string(),
            "smart_format=true".to_string(),
        ];

        match language {
            Some(lang)
                if !lang.trim().is_empty()
                    && lang != "auto"
                    && lang != "auto-translate"
                    && lang != "detect" =>
            {
                params.push(format!("language={}", lang.trim()));
            }
            _ => {
                params.push("language=multi".to_string());
            }
        }

        format!("{}?{}", REALTIME_V1_WS_URL, params.join("&"))
    }
}

pub fn build_prerecorded_url(model: &str, language: Option<&str>) -> String {
    let effective_model = if model.to_lowercase().contains("flux") {
        crate::config::DEFAULT_DEEPGRAM_ASYNC_MODEL
    } else {
        model
    };

    let mut params = vec![
        format!("model={}", effective_model),
        "smart_format=true".to_string(),
        "diarize=true".to_string(),
        "punctuate=true".to_string(),
    ];

    match language {
        Some(lang)
            if !lang.trim().is_empty()
                && lang != "auto"
                && lang != "auto-translate"
                && lang != "detect" =>
        {
            params.push(format!("language={}", lang.trim()));
        }
        _ => {
            params.push("detect_language=true".to_string());
        }
    }

    format!("{}?{}", LISTEN_V1_REST_URL, params.join("&"))
}

impl DeepgramClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    pub async fn transcribe_file_with_progress<F, C>(
        &self,
        path: &Path,
        model: Option<String>,
        language: Option<String>,
        _client_reference_id: &str,
        mut progress: F,
        mut should_cancel: C,
    ) -> Result<DeepgramTranscript>
    where
        F: FnMut(u32, &str),
        C: FnMut() -> bool,
    {
        ensure_not_cancelled(&mut should_cancel)?;
        progress(10, "Reading audio file for Deepgram transcription");

        let file = tokio::fs::File::open(path)
            .await
            .with_context(|| format!("Failed to open audio file: {}", path.display()))?;
        let metadata = file
            .metadata()
            .await
            .with_context(|| format!("Failed to get audio metadata: {}", path.display()))?;
        let stream = FramedRead::new(file, BytesCodec::new())
            .map(|result| result.map(|bytes| bytes.freeze()));
        let body = reqwest::Body::wrap_stream(stream);

        let model_name = model
            .filter(|m| !m.trim().is_empty() && !m.to_lowercase().contains("flux"))
            .unwrap_or_else(|| crate::config::DEFAULT_DEEPGRAM_ASYNC_MODEL.to_string());

        let url = build_prerecorded_url(&model_name, language.as_deref());
        info!("Sending Deepgram transcription request to {}", url);

        progress(25, "Uploading audio to Deepgram");
        let content_type = "application/octet-stream";

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", content_type)
            .header("Content-Length", metadata.len().to_string())
            .body(body)
            .send()
            .await
            .context("Failed to send transcription request to Deepgram")?;

        ensure_not_cancelled(&mut should_cancel)?;
        progress(65, "Processing Deepgram response");

        let response = ensure_success(response, "transcribe audio").await?;
        let payload: DeepgramPrerecordedResponse = response
            .json()
            .await
            .context("Failed to parse Deepgram response")?;

        if let Some(msg) = payload.err_msg.or(payload.err_code) {
            return Err(anyhow!("Deepgram transcription error: {}", msg));
        }

        let (text, words) = if let Some(results) = payload.results {
            if let Some(channel) = results.channels.first() {
                if let Some(alt) = channel.alternatives.first() {
                    (alt.transcript.clone(), alt.words.clone())
                } else {
                    (String::new(), Vec::new())
                }
            } else {
                (String::new(), Vec::new())
            }
        } else {
            (String::new(), Vec::new())
        };

        progress(90, "Deepgram transcription completed");
        Ok(DeepgramTranscript { text, words })
    }
}

pub async fn configured_api_key<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    let stored_key =
        SettingsRepository::get_transcript_api_key(app_state.db_manager.pool(), PROVIDER)
            .await
            .context("Failed to read Deepgram API key from settings")?;

    let key = stored_key
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("DEEPGRAM_API_KEY").ok())
        .unwrap_or_default();

    if key.trim().is_empty() {
        return Err(anyhow!(
            "Deepgram API key is not configured. Add it in Transcription settings or set DEEPGRAM_API_KEY."
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

pub async fn client_from_config<R: Runtime>(app: &AppHandle<R>) -> Result<DeepgramClient> {
    Ok(DeepgramClient::new(configured_api_key(app).await?))
}

pub fn deepgram_words_to_transcript_tuples(words: &[DeepgramWord]) -> Vec<(String, f64, f64)> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut current_start_ms = 0.0;
    let mut current_end_ms = 0.0;
    let mut current_speaker: Option<usize> = None;

    for word in words {
        let display_word = word.punctuated_word.as_deref().unwrap_or(&word.word);
        if display_word.trim().is_empty() {
            continue;
        }

        let word_start_ms = word.start.max(0.0) * 1000.0;
        let word_end_ms = (word.end * 1000.0).max(word_start_ms);
        let speaker_changed = current_speaker.is_some() && word.speaker != current_speaker;
        let long_gap = !current_text.is_empty() && word_start_ms - current_end_ms > 1500.0;
        let long_segment =
            word_start_ms - current_start_ms > 30_000.0 && ends_sentence(&current_text);

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
            current_start_ms = word_start_ms;
            current_speaker = word.speaker;
        } else {
            current_text.push(' ');
        }

        current_text.push_str(display_word.trim());
        current_end_ms = word_end_ms;
    }

    push_transcript_tuple(
        &mut segments,
        current_text,
        current_start_ms,
        current_end_ms,
    );
    segments
}

pub fn deepgram_words_to_transcript_segments(words: &[DeepgramWord]) -> Vec<TranscriptSegment> {
    super::common::create_transcript_segments(&deepgram_words_to_transcript_tuples(words))
}

pub fn transcript_to_segments(
    transcript: &DeepgramTranscript,
    fallback_duration_seconds: f64,
) -> Vec<TranscriptSegment> {
    let mut segments = deepgram_words_to_transcript_segments(&transcript.words);
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

    let model =
        match crate::api::api::api_get_transcript_config(app.clone(), app.clone().state(), None)
            .await
        {
            Ok(Some(config)) if config.provider == PROVIDER && !config.model.trim().is_empty() => {
                config.model
            }
            _ => crate::config::DEFAULT_DEEPGRAM_REALTIME_MODEL.to_string(),
        };

    let ws_url = build_realtime_ws_url(&model, language.as_deref());
    info!(
        "Connecting to Deepgram realtime transcription at {}",
        ws_url
    );

    let mut request = ws_url
        .clone()
        .into_client_request()
        .map_err(|e| format!("Failed to build Deepgram WebSocket request: {}", e))?;
    let auth_header = HeaderValue::from_str(&format!("Token {}", api_key))
        .map_err(|e| format!("Failed to build Deepgram auth header: {}", e))?;
    request.headers_mut().insert("Authorization", auth_header);

    let (ws_stream, _) = connect_async(request)
        .await
        .map_err(|e| format!("Failed to connect to Deepgram realtime API: {}", e))?;
    let (mut write, mut read) = ws_stream.split();

    let time_map = Arc::new(Mutex::new(Vec::<AudioTimeMapEntry>::new()));
    let stream_cursor_ms = Arc::new(Mutex::new(0.0f64));
    let shutdown = Arc::new(AtomicBool::new(false));

    let writer_time_map = time_map.clone();
    let writer_cursor = stream_cursor_ms.clone();
    let writer_shutdown = shutdown.clone();
    let writer_handle = tokio::spawn(async move {
        let mut receiver = transcription_receiver;
        let mut ping_interval = tokio::time::interval(Duration::from_secs(10));
        let mut shutdown_interval = tokio::time::interval(Duration::from_millis(250));

        loop {
            tokio::select! {
                _ = shutdown_interval.tick() => {
                    if writer_shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                }
                _ = ping_interval.tick() => {
                    let keepalive = serde_json::json!({ "type": "KeepAlive" });
                    if let Err(e) = write.send(Message::Text(keepalive.to_string())).await {
                        return Err(anyhow!("Failed to send KeepAlive to Deepgram realtime API: {}", e));
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
                        .map_err(|e| anyhow!("Failed to send audio to Deepgram realtime API: {}", e))?;
                }
            }
        }

        let close_stream = serde_json::json!({ "type": "CloseStream" });
        let _ = write.send(Message::Text(close_stream.to_string())).await;
        let _ = write.send(Message::Binary(Vec::new())).await;
        Ok::<(), anyhow::Error>(())
    });

    let reader_time_map = time_map.clone();
    let reader_shutdown = shutdown.clone();
    let reader_app = app.clone();
    let reader_handle = tokio::spawn(async move {
        let mut buffer = RealtimeSegmentBuffer::new(reader_app.clone());
        let mut current_flux_turn: Option<FluxTurnState> = None;

        while let Some(message) = read.next().await {
            let message = match message {
                Ok(message) => message,
                Err(e) => {
                    emit_transcript_preview(&reader_app, String::new(), 0.0, 0.0, 0.0);
                    reader_shutdown.store(true, Ordering::SeqCst);
                    return Err(anyhow!("Deepgram realtime WebSocket error: {}", e));
                }
            };

            match message {
                Message::Text(text) => {
                    let response: DeepgramLiveResponse = match serde_json::from_str(&text) {
                        Ok(response) => response,
                        Err(e) => {
                            warn!(
                                "Failed to parse Deepgram realtime response: {} ({})",
                                e, text
                            );
                            continue;
                        }
                    };

                    if let Some(message) = response.err_msg.or(response.error).or(response.err_code)
                    {
                        emit_transcript_preview(&reader_app, String::new(), 0.0, 0.0, 0.0);
                        reader_shutdown.store(true, Ordering::SeqCst);
                        return Err(anyhow!("Deepgram realtime error: {}", message));
                    }

                    // Flux TurnInfo v2 protocol vs Nova Results v1 protocol
                    let is_flux_turn = response.r#type.as_deref() == Some("TurnInfo")
                        || response.event.is_some()
                        || response.turn_index.is_some()
                        || (!response.words.is_empty() && response.channel.is_none())
                        || (response.transcript.is_some() && response.channel.is_none());

                    if is_flux_turn {
                        let turn_idx = response.turn_index.unwrap_or_else(|| {
                            current_flux_turn
                                .as_ref()
                                .map(|t| t.turn_index)
                                .unwrap_or(0)
                        });

                        let entries = reader_time_map.lock().await.clone();

                        if let Some(prev) = current_flux_turn.as_mut() {
                            if prev.turn_index != turn_idx {
                                if !prev.is_flushed {
                                    flush_flux_turn(&reader_app, &entries, prev);
                                    prev.is_flushed = true;
                                }
                                *prev = FluxTurnState {
                                    turn_index: turn_idx,
                                    words: Vec::new(),
                                    transcript: None,
                                    start_sec: None,
                                    end_sec: None,
                                    is_flushed: false,
                                };
                            }
                        } else {
                            current_flux_turn = Some(FluxTurnState {
                                turn_index: turn_idx,
                                words: Vec::new(),
                                transcript: None,
                                start_sec: None,
                                end_sec: None,
                                is_flushed: false,
                            });
                        }

                        let turn = current_flux_turn.as_mut().unwrap();

                        if !response.words.is_empty() {
                            turn.words = response.words;
                        }
                        if let Some(ref text) = response.transcript {
                            if !text.is_empty() {
                                turn.transcript = Some(text.clone());
                            }
                        }
                        if response.audio_window_start.is_some() || response.start.is_some() {
                            let s = response.audio_window_start.or(response.start);
                            if turn.start_sec.is_none()
                                || s.unwrap_or(0.0) < turn.start_sec.unwrap_or(0.0)
                            {
                                turn.start_sec = s;
                            }
                        }
                        if response.audio_window_end.is_some() || response.end.is_some() {
                            turn.end_sec = response.audio_window_end.or(response.end);
                        }

                        let is_eot = response.event.as_deref() == Some("EndOfTurn");
                        if is_eot && !turn.is_flushed {
                            flush_flux_turn(&reader_app, &entries, turn);
                            turn.is_flushed = true;
                        } else {
                            emit_flux_turn_preview(&reader_app, &entries, turn);
                        }
                        continue;
                    }

                    // Standard Deepgram v1 Nova Results protocol
                    let Some(channel) = response.channel else {
                        continue;
                    };
                    let Some(alternative) = channel.alternatives.first() else {
                        continue;
                    };

                    let entries = reader_time_map.lock().await.clone();
                    let tokens = if !alternative.words.is_empty() {
                        alternative
                            .words
                            .iter()
                            .map(|word| {
                                let display_word =
                                    word.punctuated_word.as_deref().unwrap_or(&word.word);
                                let word_start_stream_ms = word.start.max(0.0) * 1000.0;
                                let word_end_stream_ms =
                                    (word.end * 1000.0).max(word_start_stream_ms);
                                let start_ms =
                                    map_stream_ms_to_original(&entries, word_start_stream_ms);
                                let end_ms =
                                    map_stream_ms_to_original(&entries, word_end_stream_ms)
                                        .max(start_ms);
                                MappedRealtimeToken {
                                    text: display_word.to_string(),
                                    start_ms,
                                    end_ms,
                                    confidence: word.confidence.or(alternative.confidence),
                                    speaker: word.speaker,
                                }
                            })
                            .collect::<Vec<_>>()
                    } else if !alternative.transcript.trim().is_empty() {
                        let stream_start_ms = response.start.unwrap_or(0.0) * 1000.0;
                        let stream_end_ms =
                            stream_start_ms + (response.duration.unwrap_or(0.0) * 1000.0);
                        let start_ms = map_stream_ms_to_original(&entries, stream_start_ms);
                        let end_ms =
                            map_stream_ms_to_original(&entries, stream_end_ms).max(start_ms);
                        vec![MappedRealtimeToken {
                            text: alternative.transcript.clone(),
                            start_ms,
                            end_ms,
                            confidence: alternative.confidence,
                            speaker: None,
                        }]
                    } else {
                        Vec::new()
                    };

                    if response.is_final.unwrap_or(false) {
                        buffer.ingest(tokens);
                        if response.speech_final.unwrap_or(false) {
                            buffer.flush();
                        }
                        buffer.emit_preview(&[]);
                    } else {
                        buffer.emit_preview(&tokens);
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        if let Some(ref mut turn) = current_flux_turn {
            if !turn.is_flushed {
                let entries = reader_time_map.lock().await.clone();
                flush_flux_turn(&reader_app, &entries, turn);
                turn.is_flushed = true;
            }
        }
        buffer.flush();
        emit_transcript_preview(&reader_app, String::new(), 0.0, 0.0, 0.0);
        reader_shutdown.store(true, Ordering::SeqCst);
        Ok::<(), anyhow::Error>(())
    });

    let writer_result = writer_handle
        .await
        .map_err(|e| format!("Deepgram realtime writer task failed: {}", e))?;
    if let Err(e) = writer_result {
        shutdown.store(true, Ordering::SeqCst);
        emit_transcription_error(&app, e.to_string());
        return Err(e.to_string());
    }

    let mut reader_handle = reader_handle;
    match tokio::time::timeout(Duration::from_secs(30), &mut reader_handle).await {
        Ok(join_result) => {
            let reader_result =
                join_result.map_err(|e| format!("Deepgram realtime reader task failed: {}", e))?;
            if let Err(e) = reader_result {
                emit_transcription_error(&app, e.to_string());
                return Err(e.to_string());
            }
        }
        Err(_) => {
            reader_handle.abort();
            warn!("Timed out waiting for Deepgram realtime final response");
        }
    }

    info!("Deepgram realtime transcription task completed");
    Ok(())
}

fn emit_flux_turn_preview<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[AudioTimeMapEntry],
    turn: &FluxTurnState,
) {
    let text = flux_turn_preview_text(turn);

    let Some(text) = text else {
        return;
    };

    let stream_start_ms = turn
        .words
        .first()
        .map(|word| word.start.max(0.0) * 1000.0)
        .or(turn.start_sec.map(|start| start.max(0.0) * 1000.0))
        .unwrap_or(0.0);
    let stream_end_ms = turn
        .words
        .last()
        .map(|word| word.end.max(0.0) * 1000.0)
        .or(turn.end_sec.map(|end| end.max(0.0) * 1000.0))
        .unwrap_or(stream_start_ms);
    let start_ms = map_stream_ms_to_original(entries, stream_start_ms);
    let end_ms = map_stream_ms_to_original(entries, stream_end_ms).max(start_ms);
    let confidence = average_confidence(turn.words.iter().filter_map(|word| word.confidence));

    emit_speech_detected(app);
    emit_transcript_preview(app, text, start_ms, end_ms, confidence);
}

fn flux_turn_preview_text(turn: &FluxTurnState) -> Option<String> {
    turn.transcript
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| {
            let joined = turn
                .words
                .iter()
                .map(|word| word.punctuated_word.as_deref().unwrap_or(&word.word).trim())
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (!joined.is_empty()).then_some(joined)
        })
}

fn flush_flux_turn<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[AudioTimeMapEntry],
    turn: &FluxTurnState,
) {
    if !turn.words.is_empty() {
        let tuples = deepgram_words_to_transcript_tuples(&turn.words);
        let confidence = average_confidence(turn.words.iter().filter_map(|w| w.confidence));
        for (text, stream_start_ms, stream_end_ms) in tuples {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let start_ms = map_stream_ms_to_original(entries, stream_start_ms);
            let end_ms = map_stream_ms_to_original(entries, stream_end_ms).max(start_ms);
            emit_transcript_update(
                app,
                trimmed.to_string(),
                start_ms,
                end_ms,
                confidence,
                false,
            );
        }
    } else if let Some(ref text) = turn.transcript {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let stream_start_ms = turn.start_sec.unwrap_or(0.0).max(0.0) * 1000.0;
            let stream_end_ms = turn
                .end_sec
                .map(|e| e * 1000.0)
                .unwrap_or(stream_start_ms + 1000.0);
            let start_ms = map_stream_ms_to_original(entries, stream_start_ms);
            let end_ms = map_stream_ms_to_original(entries, stream_end_ms).max(start_ms);
            emit_transcript_update(app, trimmed.to_string(), start_ms, end_ms, 0.85, false);
        }
    }
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
            if token.text.trim().is_empty() {
                continue;
            }

            let gap = self
                .tokens
                .last()
                .map(|last| token.start_ms - last.end_ms > 1500.0)
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

            // Avoid micro-chunking: only flush on sentence boundary if we have at least 4 tokens
            // or if the accumulated segment duration is >= 4.0 seconds.
            let duration_ms = self.tokens.last().map(|l| l.end_ms).unwrap_or(0.0)
                - self.tokens.first().map(|f| f.start_ms).unwrap_or(0.0);
            if (self.tokens.len() >= 4 || duration_ms >= 4000.0)
                && self
                    .tokens
                    .last()
                    .map(|last| ends_sentence(&last.text))
                    .unwrap_or(false)
            {
                self.flush();
            }
        }
    }

    fn emit_preview(&self, interim_tokens: &[MappedRealtimeToken]) {
        let tokens = self
            .tokens
            .iter()
            .chain(interim_tokens.iter())
            .collect::<Vec<_>>();

        if tokens.is_empty() {
            emit_transcript_preview(&self.app, String::new(), 0.0, 0.0, 0.0);
            return;
        }

        let text = mapped_preview_text(&tokens);
        if text.is_empty() {
            return;
        }

        let start_ms = tokens.first().map(|token| token.start_ms).unwrap_or(0.0);
        let end_ms = tokens
            .last()
            .map(|token| token.end_ms)
            .unwrap_or(start_ms)
            .max(start_ms);
        let confidence = average_confidence(tokens.iter().filter_map(|token| token.confidence));

        emit_speech_detected(&self.app);
        emit_transcript_preview(&self.app, text, start_ms, end_ms, confidence);
    }

    fn flush(&mut self) {
        if self.tokens.is_empty() {
            return;
        }

        let mut text = String::new();
        for token in &self.tokens {
            if !text.is_empty() && !token.text.starts_with(|c: char| c.is_ascii_punctuation()) {
                text.push(' ');
            }
            text.push_str(token.text.trim());
        }
        let text = text.trim().to_string();

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

fn mapped_preview_text(tokens: &[&MappedRealtimeToken]) -> String {
    tokens
        .iter()
        .map(|token| token.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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
        error!("Failed to emit Deepgram transcript update: {}", e);
    }
}

fn emit_transcript_preview<R: Runtime>(
    app: &AppHandle<R>,
    text: String,
    start_ms: f64,
    end_ms: f64,
    confidence: f32,
) {
    let audio_start_time = start_ms / 1000.0;
    let audio_end_time = end_ms.max(start_ms) / 1000.0;
    let update = TranscriptUpdate {
        text,
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        source: "Audio".to_string(),
        sequence_id: REALTIME_SEQUENCE_COUNTER.load(Ordering::SeqCst),
        chunk_start_time: audio_start_time,
        is_partial: true,
        confidence,
        audio_start_time,
        audio_end_time,
        duration: (audio_end_time - audio_start_time).max(0.0),
    };

    if let Err(e) = app.emit("transcript-preview", &update) {
        error!("Failed to emit Deepgram transcript preview: {}", e);
    }
}

fn emit_transcription_error<R: Runtime>(app: &AppHandle<R>, error: String) {
    let _ = app.emit(
        "transcription-error",
        serde_json::json!({
            "error": error,
            "userMessage": "Deepgram transcription failed. Check your API key and network connection.",
            "actionable": true
        }),
    );
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
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<empty response>".to_string());
    Err(anyhow!(
        "Deepgram {} failed (HTTP {}): {}",
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
    fn test_build_realtime_ws_url_nova() {
        let url = build_realtime_ws_url("nova-3", Some("en"));
        assert!(url.starts_with(REALTIME_V1_WS_URL));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("language=en"));
        assert!(url.contains("channels=1"));
        assert!(url.contains("smart_format=true"));

        let auto_url = build_realtime_ws_url("nova-3", None);
        assert!(auto_url.contains("language=multi"));
    }

    #[test]
    fn test_build_realtime_ws_url_flux() {
        let url = build_realtime_ws_url("flux-general-multi", Some("en"));
        assert!(url.starts_with(REALTIME_V2_WS_URL));
        assert!(url.contains("model=flux-general-multi"));
        assert!(url.contains("language_hint=en"));
        assert!(!url.contains("channels="));
        assert!(!url.contains("punctuate="));

        let auto_url = build_realtime_ws_url("flux-general-multi", None);
        assert!(!auto_url.contains("language_hint"));
        assert!(!auto_url.contains("channels="));
    }

    #[test]
    fn test_build_prerecorded_url() {
        let url = build_prerecorded_url("nova-3", Some("en"));
        assert!(url.starts_with(LISTEN_V1_REST_URL));
        assert!(url.contains("model=nova-3"));
        assert!(url.contains("language=en"));

        let auto_url = build_prerecorded_url("nova-3", Some("auto"));
        assert!(auto_url.contains("detect_language=true"));

        let flux_url = build_prerecorded_url("flux-general-multi", Some("en"));
        assert!(flux_url.contains("model=nova-3"));
    }

    #[test]
    fn test_flux_turn_info_parsing() {
        let json_flux = r#"{
            "type": "TurnInfo",
            "event": "Update",
            "transcript": "Hello from Flux",
            "words": [
                {
                    "word": "Hello",
                    "start": 0.1,
                    "end": 0.5,
                    "confidence": 0.98,
                    "speaker": 0,
                    "punctuated_word": "Hello"
                },
                {
                    "word": "from",
                    "start": 0.6,
                    "end": 0.8,
                    "confidence": 0.99,
                    "speaker": 0,
                    "punctuated_word": "from"
                },
                {
                    "word": "Flux",
                    "start": 0.85,
                    "end": 1.2,
                    "confidence": 0.97,
                    "speaker": 0,
                    "punctuated_word": "Flux."
                }
            ]
        }"#;

        let response: DeepgramLiveResponse = serde_json::from_str(json_flux).unwrap();
        assert_eq!(response.r#type.as_deref(), Some("TurnInfo"));
        assert_eq!(response.words.len(), 3);
        let tuples = deepgram_words_to_transcript_tuples(&response.words);
        assert_eq!(tuples.len(), 1);
        assert_eq!(tuples[0].0, "Hello from Flux.");
    }

    #[test]
    fn test_flux_preview_uses_latest_turn_transcript() {
        let turn = FluxTurnState {
            turn_index: 2,
            words: Vec::new(),
            transcript: Some("  live Flux preview  ".to_string()),
            start_sec: Some(1.0),
            end_sec: Some(2.0),
            is_flushed: false,
        };

        assert_eq!(
            flux_turn_preview_text(&turn).as_deref(),
            Some("live Flux preview")
        );
    }

    #[test]
    fn test_nova_preview_combines_stable_and_interim_tokens() {
        let stable = MappedRealtimeToken {
            text: "stable".to_string(),
            start_ms: 0.0,
            end_ms: 200.0,
            confidence: Some(0.9),
            speaker: None,
        };
        let interim = MappedRealtimeToken {
            text: "interim".to_string(),
            start_ms: 200.0,
            end_ms: 400.0,
            confidence: Some(0.8),
            speaker: None,
        };

        assert_eq!(mapped_preview_text(&[&stable, &interim]), "stable interim");
    }

    #[test]
    fn test_deepgram_words_to_transcript_tuples() {
        let words = vec![
            DeepgramWord {
                word: "Hello".to_string(),
                start: 0.0,
                end: 0.5,
                confidence: Some(0.95),
                speaker: Some(0),
                punctuated_word: Some("Hello.".to_string()),
            },
            DeepgramWord {
                word: "How".to_string(),
                start: 2.5,
                end: 2.8,
                confidence: Some(0.9),
                speaker: Some(0),
                punctuated_word: Some("How".to_string()),
            },
            DeepgramWord {
                word: "are".to_string(),
                start: 2.9,
                end: 3.1,
                confidence: Some(0.9),
                speaker: Some(0),
                punctuated_word: Some("are".to_string()),
            },
            DeepgramWord {
                word: "you".to_string(),
                start: 3.2,
                end: 3.5,
                confidence: Some(0.95),
                speaker: Some(0),
                punctuated_word: Some("you?".to_string()),
            },
        ];

        let segments = deepgram_words_to_transcript_tuples(&words);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].0, "Hello.");
        assert_eq!(segments[1].0, "How are you?");
    }

    #[test]
    fn test_map_stream_ms_to_original() {
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
                original_start_ms: 10000.0,
                original_end_ms: 11000.0,
            },
        ];

        assert_eq!(map_stream_ms_to_original(&entries, 500.0), 5500.0);
        assert_eq!(map_stream_ms_to_original(&entries, 1500.0), 10500.0);
    }
}
