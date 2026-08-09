// audio/transcription/voxtral_realtime.rs
//
// Streaming transcription over a vLLM/OpenAI-style `/v1/realtime` websocket, the
// worked example being Mistral's `Voxtral-Mini-4B-Realtime-2602` served by vLLM.
//
// ## Wire protocol — VERIFIED against a live vLLM/Voxtral endpoint (2026-07-12)
//
// ```text
//  connect ws(s)://{host}/v1/realtime            (wss:// adds Authorization: Bearer)
//  → client: {"type":"session.update","model":…}            (model REQUIRED, flat)
//  → client: {"type":"input_audio_buffer.commit"}           (REQUIRED — opens the buffer)
//  ← server: {"type":"session.created","id":…}              (ignored; may arrive late)
//  → client: {"type":"input_audio_buffer.append","audio":"<b64 PCM16>"}  (repeated)
//  → client: {"type":"input_audio_buffer.commit","final":true}          (on finish)
//  ← server: {"type":"transcription.delta","delta":…}  → Partial (delta is INCREMENTAL)
//  ← server: {"type":"transcription.done","text":…}    → Final
//  ← server: {"type":"error","error":…}                → Error
// ```
//
// Audio is 16 kHz mono PCM16-LE (`i16 = f32 * i16::MAX`), base64-encoded.
//
// Findings that shaped this (all probed against the running server):
// - The **leading `commit` is required.** Omit it and the server ingests every
//   append but emits no delta and no `done` — the session silently hangs.
// - **`session.update` accepts only `model`.** It is validated (a bad name →
//   `model_not_found`) and must be **top-level**. Unknown fields are silently
//   ignored, so `language` / delay are *not* part of this contract: Voxtral
//   auto-detects language, and the transcription delay is a **server-side** knob
//   (the model's `tekken.json`, 80–1200 ms in 80 ms steps, Mistral recommends
//   480), not per-session. `CustomTranscriptionConfig::delay_ms` is therefore
//   persisted for UX/forward-compat but intentionally **not** sent here.
// - `transcription.delta` is **incremental** (one token per frame, often empty),
//   not cumulative — this client accumulates and emits the running text.

use base64::Engine as _;
use futures_util::{Sink, SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_util::sync::CancellationToken;

use super::provider::TranscriptionError;
use super::streaming_provider::{
    StreamSession, StreamTranscriptEvent, StreamingTranscriptionProvider,
};
use super::CustomTranscriptionConfig;
use async_trait::async_trait;
use log::{info, warn};

/// Raw PCM16 bytes per `input_audio_buffer.append` frame (~128 ms @ 16 kHz).
const FRAME_BYTES: usize = 4096;
/// Sample rate the provider is fed at, used to turn the configured session limit
/// into a sample budget. Set by the streaming worker's resampler.
const SAMPLE_RATE: u64 = 16_000;
/// How long a planned rollover waits for the outgoing session's final transcript.
/// Deliberately short: live audio is buffering the whole time, and whatever the
/// old session already transcribed is emitted regardless.
const ROLLOVER_FINISH_SECS: u64 = 5;
/// How long to wait for the terminal `transcription.done` after end-of-audio
/// before giving up, so a stalled server can't hang teardown forever.
const FINISH_TIMEOUT_SECS: u64 = 30;
/// How long `test_connection` waits for the handshake to settle.
const TEST_TIMEOUT_SECS: u64 = 10;
/// Reconnect attempts allowed per session (total, not per drop) before giving up.
/// A server restart mid-meeting should recover; a server that is gone for good, or
/// one that flaps endlessly, must stop and tell the user rather than retry forever.
const MAX_RECONNECT_ATTEMPTS: u32 = 3;
/// Backoff before the first reconnect attempt; doubles for each further attempt.
const RECONNECT_BACKOFF_MS: u64 = 500;
/// How long to keep reading a dropped socket for transcript frames that arrived
/// but hadn't been consumed yet, before writing the session off.
const DROP_DRAIN_MS: u64 = 50;

/// Streaming provider backed by a `/v1/realtime` websocket.
pub struct VoxtralRealtimeProvider {
    config: CustomTranscriptionConfig,
}

impl VoxtralRealtimeProvider {
    pub fn new(config: CustomTranscriptionConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl StreamingTranscriptionProvider for VoxtralRealtimeProvider {
    async fn start_session(
        &self,
        _language: Option<String>,
        events: mpsc::UnboundedSender<StreamTranscriptEvent>,
    ) -> Result<StreamSession, TranscriptionError> {
        // Connect eagerly so a bad endpoint / unreachable server fails loudly at
        // record-start rather than silently swallowing audio.
        let stream = connect_and_handshake(&self.config).await?;

        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            run_session(config, stream, audio_rx, events, worker_cancel).await;
        });

        Ok(StreamSession { audio_tx, cancel })
    }

    async fn test_connection(&self) -> Result<(), TranscriptionError> {
        let mut stream = connect_and_handshake(&self.config).await?;

        // Read until we either see a server error (fail — e.g. model_not_found) or
        // the handshake clearly settled. Absence of an error within the window is
        // treated as success: some servers stay quiet until audio arrives.
        let deadline = tokio::time::Duration::from_secs(TEST_TIMEOUT_SECS);
        loop {
            match tokio::time::timeout(deadline, stream.next()).await {
                Ok(Some(Ok(Message::Text(t)))) => {
                    match serde_json::from_str::<ServerEvent>(t.as_str()) {
                        Ok(ServerEvent::Error { error }) => {
                            let _ = stream.close(None).await;
                            return Err(TranscriptionError::EngineFailed(error));
                        }
                        // Any non-error frame means the socket + handshake work.
                        Ok(_) => break,
                        Err(_) => break,
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                    return Err(TranscriptionError::EngineFailed(
                        "server closed the connection during handshake".to_string(),
                    ));
                }
                Ok(Some(Ok(_))) => continue, // ping/pong/binary
                Ok(Some(Err(e))) => {
                    return Err(TranscriptionError::EngineFailed(format!("ws: {e}")));
                }
                // Quiet server — connection + handshake succeeded, good enough.
                Err(_) => break,
            }
        }
        let _ = stream.close(None).await;
        Ok(())
    }

    fn provider_name(&self) -> &'static str {
        "Voxtral Realtime (streaming)"
    }
}

// === Session worker ========================================================

async fn run_session(
    config: CustomTranscriptionConfig,
    stream: WsStream,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<f32>>,
    events: mpsc::UnboundedSender<StreamTranscriptEvent>,
    cancel: CancellationToken,
) {
    let mut current = stream;
    let mut reconnects_used: u32 = 0;
    // Audio budget for one server session. Exceeding whatever the backend can hold
    // is silent — it simply stops transcribing — so the client bounds it instead.
    let limit_samples = config
        .session_limit_seconds()
        .map(|secs| secs as u64 * SAMPLE_RATE);

    loop {
        let (mut write, mut read) = current.split();
        let mut cumulative = String::new();
        let mut pending: Vec<u8> = Vec::new();
        let mut samples_sent: u64 = 0;

        // Streaming phase: forward audio and surface segments until the audio
        // channel closes (clean end-of-recording), the session hits its audio
        // budget, it is cancelled, or the socket drops.
        let phase = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Phase::Cancelled,
                pcm = audio_rx.recv() => match pcm {
                    Some(s) => {
                        samples_sent += s.len() as u64;
                        append_pcm16(&mut pending, &s);
                        if let Err(e) = flush_frames(&mut write, &mut pending, false).await {
                            // A failed send means the socket is gone, not that the
                            // audio was bad — recoverable like any other drop.
                            warn!("Voxtral realtime: send failed ({e}); connection considered dropped");
                            break Phase::Dropped;
                        }
                        if limit_samples.is_some_and(|limit| samples_sent >= limit) {
                            break Phase::LimitReached;
                        }
                    }
                    None => break Phase::EndOfAudio, // audio_tx dropped → end of recording
                },
                msg = read.next() => match apply(msg, &events, &mut cumulative) {
                    Flow::Continue | Flow::Terminal => {}
                    // A mid-stream server error usually means this session is
                    // finished even though the socket is still open (a full context
                    // is the common cause). Ignoring it is how a recording ends up
                    // silently untranscribed from that point on, so treat it like a
                    // drop and start a fresh session.
                    Flow::Failed | Flow::Closed => break Phase::Dropped,
                },
            }
        };

        if let Phase::EndOfAudio = phase {
            // Drain any audio buffered before the channel closed, flush the tail,
            // then close the buffer and read until the final transcript arrives.
            while let Ok(s) = audio_rx.try_recv() {
                append_pcm16(&mut pending, &s);
            }
            let _ = flush_frames(&mut write, &mut pending, true).await;
            let _ =
                send_json_split(&mut write, &ClientEvent::Commit { final_flag: Some(true) }).await;

            let deadline = tokio::time::Duration::from_secs(FINISH_TIMEOUT_SECS);
            loop {
                match tokio::time::timeout(deadline, read.next()).await {
                    Ok(msg) => match apply(msg, &events, &mut cumulative) {
                        Flow::Continue => {}
                        Flow::Terminal | Flow::Failed | Flow::Closed => break,
                    },
                    Err(_) => {
                        let _ = events.send(StreamTranscriptEvent::Error {
                            message: "realtime finish timed out".to_string(),
                            fatal: false,
                        });
                        break;
                    }
                }
            }
        }

        if let Phase::LimitReached = phase {
            // Close the outgoing session the same way end-of-recording does, so the
            // server transcribes the audio it is still holding instead of dropping
            // it, then take its final transcript. Audio recorded during this
            // handover keeps queueing in `audio_rx` (unbounded) and is sent to the
            // replacement session — a rollover costs latency, never words.
            let _ = flush_frames(&mut write, &mut pending, true).await;
            let _ =
                send_json_split(&mut write, &ClientEvent::Commit { final_flag: Some(true) }).await;

            let deadline = tokio::time::Duration::from_secs(ROLLOVER_FINISH_SECS);
            loop {
                match tokio::time::timeout(deadline, read.next()).await {
                    Ok(msg) => match apply(msg, &events, &mut cumulative) {
                        Flow::Continue => {}
                        Flow::Terminal | Flow::Failed | Flow::Closed => break,
                    },
                    // A server that won't finalize can't be allowed to stall the
                    // handover; the replacement session is opened regardless.
                    Err(_) => break,
                }
            }
        }

        let _ = write.close().await;

        match phase {
            Phase::EndOfAudio | Phase::Cancelled => break,
            Phase::LimitReached => {
                // Close out this session's transcript. The replacement server starts
                // counting from zero, so the worker must stop treating its offset
                // into the old cumulative text as valid — a `Final` says exactly
                // that. (`apply` already emitted one if `transcription.done` came
                // back in time, leaving `cumulative` empty.)
                if !cumulative.trim().is_empty() {
                    let _ = events.send(StreamTranscriptEvent::Final {
                        text: std::mem::take(&mut cumulative),
                        confidence: None,
                    });
                }
                match connect_and_handshake(&config).await {
                    Ok(stream) => {
                        info!(
                            "Voxtral realtime: rolled over to a fresh session after {}s of audio",
                            samples_sent / SAMPLE_RATE
                        );
                        // The endpoint just proved it is healthy, so earlier drops
                        // shouldn't count against a meeting that may run for hours.
                        reconnects_used = 0;
                        current = stream;
                    }
                    // The rollover was planned, but the server didn't answer — that
                    // is an outage like any other, so hand it to the retry path.
                    Err(e) => {
                        warn!("Voxtral realtime: rollover connect failed: {e}");
                        match reconnect(&config, &events, &cancel, &mut reconnects_used).await {
                            Some(stream) => {
                                // Backoff means minutes of audio may have queued;
                                // sending it all would leave the live transcript
                                // permanently trailing the speaker.
                                while audio_rx.try_recv().is_ok() {}
                                current = stream;
                            }
                            None => break,
                        }
                    }
                }
            }
            Phase::Dropped => {
                // A drop is often noticed on a failed *send*, which can win the
                // select! race against a *read* whose transcript frames already
                // arrived. Drain what the socket still holds before writing the
                // session off, so the last words spoken before the outage aren't
                // silently dropped along with the connection.
                loop {
                    let drain = tokio::time::Duration::from_millis(DROP_DRAIN_MS);
                    match tokio::time::timeout(drain, read.next()).await {
                        Ok(Some(msg)) => match apply(Some(msg), &events, &mut cumulative) {
                            Flow::Continue | Flow::Terminal => {}
                            Flow::Failed | Flow::Closed => break,
                        },
                        // Timed out, or the stream is exhausted — nothing left.
                        _ => break,
                    }
                }

                // Close out whatever this (now dead) session had transcribed. The
                // replacement server starts a fresh transcript from scratch, so the
                // worker has to stop treating its offset into the old cumulative
                // text as valid — a `Final` is exactly that signal.
                let _ = events.send(StreamTranscriptEvent::Final {
                    text: std::mem::take(&mut cumulative),
                    confidence: None,
                });
                match reconnect(&config, &events, &cancel, &mut reconnects_used).await {
                    Some(stream) => {
                        // Discard audio buffered during the outage: the new session
                        // can't transcribe it, and replaying it would only push the
                        // live transcript permanently behind the speaker.
                        while audio_rx.try_recv().is_ok() {}
                        current = stream;
                    }
                    None => break,
                }
            }
        }
    }

    info!("Voxtral realtime session ended");
}

/// Why the streaming phase stopped.
enum Phase {
    /// `audio_tx` was dropped — clean end of recording, finalize the transcript.
    EndOfAudio,
    /// The session used up its audio budget — finalize it and continue the
    /// recording on a fresh session.
    LimitReached,
    /// The session was hard-cancelled; leave without finalizing.
    Cancelled,
    /// The socket closed or failed mid-recording — recoverable via reconnect.
    Dropped,
}

/// Re-establish a dropped session, with backoff, until it succeeds or the
/// per-session attempt budget runs out.
///
/// Emits a warning per attempt and, on exhaustion, a **fatal** error — a websocket
/// that dies mid-meeting must never leave the user with a silently empty transcript.
async fn reconnect(
    config: &CustomTranscriptionConfig,
    events: &mpsc::UnboundedSender<StreamTranscriptEvent>,
    cancel: &CancellationToken,
    used: &mut u32,
) -> Option<WsStream> {
    while *used < MAX_RECONNECT_ATTEMPTS {
        *used += 1;
        let backoff =
            tokio::time::Duration::from_millis(RECONNECT_BACKOFF_MS << (*used - 1).min(6));
        let _ = events.send(StreamTranscriptEvent::Error {
            message: format!(
                "Realtime transcription connection lost — reconnecting (attempt {}/{})",
                *used, MAX_RECONNECT_ATTEMPTS
            ),
            fatal: false,
        });

        tokio::select! {
            _ = cancel.cancelled() => return None,
            _ = tokio::time::sleep(backoff) => {}
        }

        match connect_and_handshake(config).await {
            Ok(stream) => {
                info!("Voxtral realtime: reconnected after {} attempt(s)", *used);
                return Some(stream);
            }
            Err(e) => warn!("Voxtral realtime: reconnect attempt {} failed: {}", *used, e),
        }
    }

    let _ = events.send(StreamTranscriptEvent::Error {
        message: format!(
            "Lost the connection to the realtime transcription endpoint and could not \
             reconnect after {MAX_RECONNECT_ATTEMPTS} attempts. Transcription has stopped \
             for the rest of this recording — audio is still being recorded and can be \
             transcribed afterwards."
        ),
        fatal: true,
    });
    None
}

/// What a handled inbound message means for the read loop.
enum Flow {
    Continue,
    /// The server finished an utterance (`transcription.done`); the session itself
    /// is still usable.
    Terminal,
    /// The server reported an error on this session. The socket may well still be
    /// open, but the session should be considered spent and replaced.
    Failed,
    Closed,
}

/// Handle one inbound WS message: emit mapped events and report how the read loop
/// should proceed. Tolerant of unknown / non-text frames.
fn apply(
    msg: Option<Result<Message, WsError>>,
    events: &mpsc::UnboundedSender<StreamTranscriptEvent>,
    cumulative: &mut String,
) -> Flow {
    let msg = match msg {
        Some(Ok(m)) => m,
        Some(Err(e)) => {
            // Not fatal on its own: the caller treats a closed socket as a
            // reconnectable drop, and only gives up once retries are exhausted.
            let _ = events.send(StreamTranscriptEvent::Error {
                message: format!("ws: {e}"),
                fatal: false,
            });
            return Flow::Closed;
        }
        None => return Flow::Closed,
    };
    let payload = match msg {
        Message::Text(t) => t.as_str().to_string(),
        Message::Close(_) => return Flow::Closed,
        _ => return Flow::Continue, // ping/pong/binary
    };
    match serde_json::from_str::<ServerEvent>(&payload) {
        Ok(ServerEvent::Delta { delta }) => {
            if !delta.is_empty() {
                cumulative.push_str(&delta);
                let _ = events.send(StreamTranscriptEvent::Partial {
                    text: cumulative.clone(),
                });
            }
            Flow::Continue
        }
        Ok(ServerEvent::Done { text }) => {
            // Emit the delta-accumulated `cumulative` VERBATIM (same string the
            // partials carried) so the worker's byte offset into it stays valid at
            // end-of-stream. Sending a trimmed/normalized `done.text` here would be
            // a *different* string and make the worker re-segment the whole
            // transcript. `done.text` is the same content in practice; it's used
            // only when we somehow received no deltas.
            let final_text = if cumulative.trim().is_empty() {
                text.trim().to_string()
            } else {
                cumulative.clone()
            };
            if !final_text.trim().is_empty() {
                let _ = events.send(StreamTranscriptEvent::Final {
                    text: final_text,
                    confidence: None,
                });
            }
            cumulative.clear();
            Flow::Terminal
        }
        Ok(ServerEvent::Error { error }) => {
            let _ = events.send(StreamTranscriptEvent::Error { message: error, fatal: false });
            Flow::Failed
        }
        Ok(ServerEvent::SessionCreated) | Ok(ServerEvent::Other) => Flow::Continue,
        // Tolerate an unparseable shape rather than tearing the session down.
        Err(_) => Flow::Continue,
    }
}

// === Connection ============================================================

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Connect and perform the full session handshake: `session.update` (which
/// validates the model server-side) followed by the required leading commit that
/// opens the audio buffer. Shared by record-start, "Test Connection", and
/// reconnects so all three go through the identical sequence.
async fn connect_and_handshake(
    config: &CustomTranscriptionConfig,
) -> Result<WsStream, TranscriptionError> {
    let mut stream = connect(config).await?;
    send_json(&mut stream, &ClientEvent::SessionUpdate { model: config.model.clone() })
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("session.update: {e}")))?;
    send_json(&mut stream, &ClientEvent::Commit { final_flag: None })
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("open buffer: {e}")))?;
    Ok(stream)
}

async fn connect(config: &CustomTranscriptionConfig) -> Result<WsStream, TranscriptionError> {
    let url = normalize_realtime_url(&config.endpoint);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| TranscriptionError::EngineFailed(format!("bad endpoint '{url}': {e}")))?;
    if let Some(key) = config.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        if let Ok(val) = format!("Bearer {key}").parse() {
            request.headers_mut().insert(AUTHORIZATION, val);
        }
    }
    // Name the endpoint in the error: a handshake failure here is usually the
    // server (a proxy 502 while the speech backend boots), not the client, and an
    // error that says only "502" sends you hunting the wrong side.
    let (stream, _resp) = connect_async(request)
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("connect to '{url}': {e}")))?;
    warn!("Voxtral realtime connected: {url}");
    Ok(stream)
}

/// Derive the websocket URL from a user-entered endpoint: map `http(s)` → `ws(s)`
/// and append the default `/v1/realtime` path when the user gave only a host.
fn normalize_realtime_url(endpoint: &str) -> String {
    let mut url = endpoint.trim().to_string();
    if let Some(rest) = url.strip_prefix("https://") {
        url = format!("wss://{rest}");
    } else if let Some(rest) = url.strip_prefix("http://") {
        url = format!("ws://{rest}");
    }
    let after_scheme = url.splitn(2, "://").nth(1).unwrap_or("");
    let path = after_scheme.splitn(2, '/').nth(1).unwrap_or("");
    if path.is_empty() {
        url = format!("{}/v1/realtime", url.trim_end_matches('/'));
    }
    url
}

// === Endpoint capability probe =============================================

/// Voxtral's audio encoder emits one token per 80 ms frame, so a second of audio
/// costs 12.5 context tokens.
const AUDIO_TOKENS_PER_SEC: f32 = 12.5;
/// The transcript the model writes shares that same context. Roughly 2.5 words a
/// second of natural speech, a little over a token per word.
const TEXT_TOKENS_PER_SEC: f32 = 3.5;
/// Fraction of the context a session is allowed to fill. The token rates above
/// are estimates, and the cost of stopping short is one extra rollover — the cost
/// of overshooting is a session that stops transcribing.
const CONTEXT_HEADROOM: f32 = 0.85;

/// What an endpoint reports about how much audio one session can hold.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedSessionLimit {
    /// Context window the server reports for the model, in tokens; `None` when the
    /// endpoint doesn't announce one.
    pub max_model_len: Option<u32>,
    /// Session length derived from that context, in seconds.
    pub recommended_seconds: Option<u32>,
    /// Plain-language account of what was found, for the settings UI.
    pub detail: String,
}

/// Ask the endpoint how big its context is and turn that into a session length.
///
/// vLLM (and several other OpenAI-compatible servers) publish `max_model_len` per
/// model on `GET /v1/models`. Servers that don't are not an error: the caller
/// falls back to a value the user sets by hand, since only they know what their
/// backend can take.
pub async fn detect_session_limit(
    config: &CustomTranscriptionConfig,
) -> Result<DetectedSessionLimit, TranscriptionError> {
    let url = models_url(&config.endpoint);
    let mut request = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(TEST_TIMEOUT_SECS));
    if let Some(key) = config.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("GET {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(TranscriptionError::EngineFailed(format!(
            "GET {url} returned {}",
            response.status()
        )));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| TranscriptionError::EngineFailed(format!("GET {url}: {e}")))?;

    Ok(match context_length_for_model(&body, &config.model) {
        Some(tokens) => {
            let seconds = seconds_for_context(tokens);
            DetectedSessionLimit {
                max_model_len: Some(tokens),
                recommended_seconds: Some(seconds),
                detail: format!(
                    "The endpoint reports a {tokens}-token context for this model, \
                     which holds about {seconds}s of audio plus its transcript."
                ),
            }
        }
        None => DetectedSessionLimit {
            max_model_len: None,
            recommended_seconds: None,
            detail: format!(
                "Reached {url}, but it doesn't announce a context size. \
                 Set the session length by hand to match your backend."
            ),
        },
    })
}

/// Derive the `/v1/models` URL from the configured realtime endpoint: back to
/// http(s), and `…/v1/realtime` → `…/v1/models` (falling back to the host root for
/// endpoints with some other path layout).
fn models_url(endpoint: &str) -> String {
    let mut url = endpoint.trim().trim_end_matches('/').to_string();
    if let Some(rest) = url.strip_prefix("wss://") {
        url = format!("https://{rest}");
    } else if let Some(rest) = url.strip_prefix("ws://") {
        url = format!("http://{rest}");
    }
    if let Some(base) = url.strip_suffix("/v1/realtime") {
        return format!("{base}/v1/models");
    }
    // No recognizable realtime path — assume the API lives at the host root.
    let (scheme, rest) = url.split_once("://").unwrap_or(("http", url.as_str()));
    let authority = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}://{authority}/v1/models")
}

/// Pull the context length out of an OpenAI-style `/v1/models` payload, preferring
/// the entry matching `model`. Key names differ between servers, so several are
/// accepted.
fn context_length_for_model(body: &serde_json::Value, model: &str) -> Option<u32> {
    const KEYS: [&str; 3] = ["max_model_len", "max_context_length", "context_length"];
    let entries = body.get("data").and_then(|d| d.as_array())?;
    let context_of = |entry: &serde_json::Value| -> Option<u32> {
        KEYS.iter()
            .find_map(|k| entry.get(k).and_then(|v| v.as_u64()))
            .filter(|v| *v > 0)
            .map(|v| v.min(u32::MAX as u64) as u32)
    };
    entries
        .iter()
        .find(|e| e.get("id").and_then(|i| i.as_str()) == Some(model))
        .and_then(context_of)
        // A server hosting one model under a different id is still telling us
        // something useful; fall back to whatever entry announces a context.
        .or_else(|| entries.iter().find_map(context_of))
}

/// Seconds of audio that fit in a context window of `max_model_len` tokens.
fn seconds_for_context(max_model_len: u32) -> u32 {
    let usable = max_model_len as f32 * CONTEXT_HEADROOM;
    let seconds = usable / (AUDIO_TOKENS_PER_SEC + TEXT_TOKENS_PER_SEC);
    (seconds as u32).clamp(60, 3600)
}

// === Wire types ============================================================

/// Client → server messages. Internally tagged by `type`.
#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum ClientEvent {
    /// `model` is the only field `/v1/realtime` accepts here — required, validated,
    /// top-level. See the module note.
    #[serde(rename = "session.update")]
    SessionUpdate { model: String },
    #[serde(rename = "input_audio_buffer.append")]
    Append { audio: String },
    #[serde(rename = "input_audio_buffer.commit")]
    Commit {
        #[serde(rename = "final", skip_serializing_if = "Option::is_none")]
        final_flag: Option<bool>,
    },
}

/// Server → client messages. Unknown `type`s are tolerated (`Other`).
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
enum ServerEvent {
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "transcription.delta")]
    Delta {
        #[serde(default)]
        delta: String,
    },
    #[serde(rename = "transcription.done")]
    Done {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        error: String,
    },
    #[serde(other)]
    Other,
}

// === PCM framing + send helpers ============================================

/// Append 16 kHz mono f32 samples to `buf` as PCM16 little-endian.
fn append_pcm16(buf: &mut Vec<u8>, samples: &[f32]) {
    buf.reserve(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Send full `FRAME_BYTES` frames from `pending` as base64 `append` messages; when
/// `flush_all`, also send the trailing partial frame.
async fn flush_frames<S>(write: &mut S, pending: &mut Vec<u8>, flush_all: bool) -> Result<(), WsError>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    while pending.len() >= FRAME_BYTES {
        let frame: Vec<u8> = pending.drain(..FRAME_BYTES).collect();
        send_json_split(write, &ClientEvent::Append { audio: b64(&frame) }).await?;
    }
    if flush_all && !pending.is_empty() {
        let frame = std::mem::take(pending);
        send_json_split(write, &ClientEvent::Append { audio: b64(&frame) }).await?;
    }
    Ok(())
}

/// Serialize + send on a split sink half (used inside the worker).
async fn send_json_split<S>(write: &mut S, ev: &ClientEvent) -> Result<(), WsError>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    let txt = serde_json::to_string(ev).unwrap_or_default();
    write.send(Message::Text(txt.into())).await
}

/// Serialize + send on the whole stream (used before splitting, during handshake).
async fn send_json(stream: &mut WsStream, ev: &ClientEvent) -> Result<(), WsError> {
    let txt = serde_json::to_string(ev).unwrap_or_default();
    stream.send(Message::Text(txt.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn client_event_json_shapes() {
        assert_eq!(
            serde_json::to_string(&ClientEvent::SessionUpdate { model: "m".into() }).unwrap(),
            r#"{"type":"session.update","model":"m"}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientEvent::Commit { final_flag: Some(true) }).unwrap(),
            r#"{"type":"input_audio_buffer.commit","final":true}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientEvent::Commit { final_flag: None }).unwrap(),
            r#"{"type":"input_audio_buffer.commit"}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientEvent::Append { audio: "AAA=".into() }).unwrap(),
            r#"{"type":"input_audio_buffer.append","audio":"AAA="}"#
        );
    }

    #[test]
    fn pcm16_framing_is_little_endian() {
        // +1.0 → i16::MAX (0x7FFF), -1.0 → -i16::MAX (0x8001), 0.0 → 0.
        let mut buf = Vec::new();
        append_pcm16(&mut buf, &[0.0, 1.0, -1.0]);
        assert_eq!(buf, vec![0x00, 0x00, 0xFF, 0x7F, 0x01, 0x80]);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64(&buf))
            .unwrap();
        assert_eq!(decoded, buf);
    }

    #[test]
    fn server_event_parsing() {
        assert!(matches!(
            serde_json::from_str::<ServerEvent>(r#"{"type":"transcription.delta","delta":"hi"}"#),
            Ok(ServerEvent::Delta { delta }) if delta == "hi"
        ));
        assert!(matches!(
            serde_json::from_str::<ServerEvent>(r#"{"type":"transcription.done","text":"done"}"#),
            Ok(ServerEvent::Done { text }) if text == "done"
        ));
        assert!(matches!(
            serde_json::from_str::<ServerEvent>(r#"{"type":"session.created","id":"s1"}"#),
            Ok(ServerEvent::SessionCreated)
        ));
        assert!(matches!(
            serde_json::from_str::<ServerEvent>(r#"{"type":"something.else"}"#),
            Ok(ServerEvent::Other)
        ));
    }

    #[test]
    fn url_normalization() {
        // Host-only → default realtime path appended.
        assert_eq!(
            normalize_realtime_url("ws://localhost:8000"),
            "ws://localhost:8000/v1/realtime"
        );
        // Trailing slash treated as no path.
        assert_eq!(
            normalize_realtime_url("ws://localhost:8000/"),
            "ws://localhost:8000/v1/realtime"
        );
        // http(s) scheme mapped to ws(s).
        assert_eq!(
            normalize_realtime_url("https://asr.example.com"),
            "wss://asr.example.com/v1/realtime"
        );
        assert_eq!(
            normalize_realtime_url("http://box:9000"),
            "ws://box:9000/v1/realtime"
        );
        // Explicit path preserved.
        assert_eq!(
            normalize_realtime_url("ws://host/custom/realtime"),
            "ws://host/custom/realtime"
        );
    }

    /// End-to-end against a mock `/v1/realtime` server: the provider must send
    /// session.update + a leading commit + audio + a final commit, and turn the
    /// server's delta/done into Partial → Final events.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streams_partial_then_final() {
        use tokio::net::TcpListener;

        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            ws.send(Message::Text(r#"{"type":"session.created","id":"s1"}"#.into()))
                .await
                .unwrap();
            let mut saw_final_commit = false;
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    if t.as_str().contains(r#""final":true"#) {
                        saw_final_commit = true;
                        // Incremental deltas that accumulate to the full text, as a
                        // real server streams them.
                        ws.send(Message::Text(
                            r#"{"type":"transcription.delta","delta":"hallo "}"#.into(),
                        ))
                        .await
                        .unwrap();
                        ws.send(Message::Text(
                            r#"{"type":"transcription.delta","delta":"welt"}"#.into(),
                        ))
                        .await
                        .unwrap();
                        ws.send(Message::Text(
                            r#"{"type":"transcription.done","text":"hallo welt"}"#.into(),
                        ))
                        .await
                        .unwrap();
                        break;
                    }
                }
            }
            saw_final_commit
        });

        let provider = VoxtralRealtimeProvider::new(test_config(addr));
        let (tx, mut rx) = mpsc::unbounded_channel::<StreamTranscriptEvent>();
        let session = provider.start_session(None, tx).await.unwrap();
        session.audio_tx.send(vec![0.1f32; 1600]).unwrap();
        // Close the audio channel → triggers flush + final commit.
        drop(session.audio_tx);

        let mut saw_partial = false;
        let mut final_text = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamTranscriptEvent::Partial { .. } => saw_partial = true,
                StreamTranscriptEvent::Final { text, .. } => {
                    final_text = Some(text);
                    break;
                }
                StreamTranscriptEvent::Error { message, .. } => {
                    panic!("unexpected error: {message}")
                }
            }
        }

        assert!(server.await.unwrap(), "server saw the final commit");
        assert!(saw_partial, "expected a streaming Partial");
        assert_eq!(final_text.as_deref(), Some("hallo welt"));
    }

    fn test_config(addr: std::net::SocketAddr) -> CustomTranscriptionConfig {
        CustomTranscriptionConfig {
            endpoint: format!("ws://{addr}/v1/realtime"),
            api_key: None,
            model: "m".to_string(),
            protocol: "voxtral-realtime".to_string(),
            delay_ms: None,
            // Far beyond anything these tests feed, so only the rollover test
            // exercises the limit.
            max_session_seconds: None,
        }
    }

    /// Read client frames until the leading `input_audio_buffer.commit` that ends
    /// the handshake, so a mock server can't hang up before the client is ready.
    async fn await_handshake<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        while let Some(Ok(msg)) = ws.next().await {
            if let Message::Text(t) = msg {
                if t.as_str().contains("input_audio_buffer.commit") {
                    return;
                }
            }
        }
    }

    /// A server that drops the connection mid-recording must not kill transcription:
    /// the provider reconnects and keeps transcribing on the new socket.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reconnects_after_the_server_drops_mid_session() {
        use tokio::net::TcpListener;

        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // Connection 1: transcribe a little, then hang up mid-recording.
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            await_handshake(&mut ws).await;
            ws.send(Message::Text(
                r#"{"type":"transcription.delta","delta":"before drop"}"#.into(),
            ))
            .await
            .unwrap();
            let _ = ws.close(None).await;
            drop(ws);

            // Connection 2: the reconnect. Transcribe through to done.
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    if t.as_str().contains(r#""final":true"#) {
                        ws.send(Message::Text(
                            r#"{"type":"transcription.done","text":"after reconnect"}"#.into(),
                        ))
                        .await
                        .unwrap();
                        return true;
                    }
                }
            }
            false
        });

        let provider = VoxtralRealtimeProvider::new(test_config(addr));
        let (tx, mut rx) = mpsc::unbounded_channel::<StreamTranscriptEvent>();
        let session = provider.start_session(None, tx).await.unwrap();

        // Feed audio continuously (so the reconnected socket has something to
        // flush) until told to stop — dropping the sender ends the recording.
        let stop = Arc::new(AtomicBool::new(false));
        let pump_stop = stop.clone();
        let audio_tx = session.audio_tx;
        let pump = tokio::spawn(async move {
            while !pump_stop.load(Ordering::Relaxed) {
                if audio_tx.send(vec![0.1f32; 1600]).is_err() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
            drop(audio_tx); // end of recording → final commit on the live socket
        });

        let mut reconnect_notices = 0;
        let mut finals = Vec::new();
        let mut fatal = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamTranscriptEvent::Partial { .. } => {}
                StreamTranscriptEvent::Final { text, .. } => {
                    finals.push(text);
                    if finals.len() == 2 {
                        break;
                    }
                }
                StreamTranscriptEvent::Error { fatal: true, message } => {
                    fatal = Some(message);
                    break;
                }
                StreamTranscriptEvent::Error { message, .. } => {
                    if message.contains("reconnecting (attempt") {
                        reconnect_notices += 1;
                        // Reconnect is under way; stop the audio so the new session
                        // reaches its final commit and we can assert on the result.
                        stop.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        let _ = pump.await;

        assert!(reconnect_notices >= 1, "a dropped connection must surface a warning");
        assert_eq!(fatal, None, "a recoverable drop must not be reported as fatal");
        assert!(
            server.await.unwrap(),
            "the reconnected session must reach the final commit"
        );
        assert_eq!(
            finals.first().map(String::as_str),
            Some("before drop"),
            "text from the dead session is closed out before reconnecting"
        );
        assert_eq!(
            finals.get(1).map(String::as_str),
            Some("after reconnect"),
            "transcription continues on the reconnected socket"
        );
    }

    /// When the endpoint stays down, the provider must give up and say so loudly —
    /// the silent-failure case this guards against is a whole meeting recorded with
    /// an empty transcript and no indication anything went wrong.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gives_up_with_a_fatal_error_when_the_endpoint_stays_down() {
        use tokio::net::TcpListener;

        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        // The first connection completes the handshake and is then dropped; every
        // retry is refused outright, standing in for a server that stays down.
        let server = tokio::spawn(async move {
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            await_handshake(&mut ws).await;
            let _ = ws.close(None).await;
            drop(ws);
            while let Ok((sock, _)) = tcp.accept().await {
                drop(sock);
            }
        });

        let provider = VoxtralRealtimeProvider::new(test_config(addr));
        let (tx, mut rx) = mpsc::unbounded_channel::<StreamTranscriptEvent>();
        let session = provider.start_session(None, tx).await.unwrap();
        // Hold the recording open so teardown can't be mistaken for a clean stop.
        let _audio_tx = session.audio_tx;

        let mut reconnect_notices = 0;
        let mut fatal = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamTranscriptEvent::Error { fatal: true, message } => {
                    fatal = Some(message);
                    break;
                }
                StreamTranscriptEvent::Error { message, .. } => {
                    if message.contains("reconnecting (attempt") {
                        reconnect_notices += 1;
                    }
                }
                _ => {}
            }
        }

        let fatal = fatal.expect("exhausted reconnects must emit a FATAL error, not silence");
        assert!(
            fatal.contains("could not reconnect"),
            "fatal error should explain what happened: {fatal}"
        );
        assert!(
            fatal.contains("audio is still being recorded"),
            "fatal error should tell the user the recording itself is safe: {fatal}"
        );
        assert_eq!(
            reconnect_notices, MAX_RECONNECT_ATTEMPTS as usize,
            "one notice per reconnect attempt before giving up"
        );
        // The events channel closes once the worker exits — no zombie session.
        assert!(rx.recv().await.is_none(), "worker should exit after giving up");
        server.abort();
    }

    /// A long meeting must outlive whatever one server session can hold: at the
    /// configured audio budget the provider finalizes the session and continues on
    /// a fresh one, without the user seeing an error.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rolls_over_to_a_fresh_session_at_the_audio_limit() {
        use tokio::net::TcpListener;

        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // Session 1 ends when the client finalizes it at the audio limit.
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            await_handshake(&mut ws).await;
            let mut sessions = 0;
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    if t.as_str().contains(r#""final":true"#) {
                        sessions += 1;
                        ws.send(Message::Text(
                            r#"{"type":"transcription.done","text":"first session"}"#.into(),
                        ))
                        .await
                        .unwrap();
                        break;
                    }
                }
            }

            // Session 2: the rollover. It runs to the end of the recording.
            let (sock, _) = tcp.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            await_handshake(&mut ws).await;
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    if t.as_str().contains(r#""final":true"#) {
                        sessions += 1;
                        ws.send(Message::Text(
                            r#"{"type":"transcription.done","text":"second session"}"#.into(),
                        ))
                        .await
                        .unwrap();
                        break;
                    }
                }
            }
            sessions
        });

        // One second of audio per session, fed in 100 ms pushes.
        let mut config = test_config(addr);
        config.max_session_seconds = Some(1);
        let provider = VoxtralRealtimeProvider::new(config);
        let (tx, mut rx) = mpsc::unbounded_channel::<StreamTranscriptEvent>();
        let session = provider.start_session(None, tx).await.unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let pump_stop = stop.clone();
        let audio_tx = session.audio_tx;
        let pump = tokio::spawn(async move {
            while !pump_stop.load(Ordering::Relaxed) {
                if audio_tx.send(vec![0.1f32; 1600]).is_err() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
            drop(audio_tx); // end of recording → final commit on session 2
        });

        let mut finals = Vec::new();
        let mut warnings = Vec::new();
        while let Some(ev) = rx.recv().await {
            match ev {
                StreamTranscriptEvent::Partial { .. } => {}
                StreamTranscriptEvent::Final { text, .. } => {
                    finals.push(text);
                    // The first session closed; end the recording so the second one
                    // finalizes too and we can assert both landed.
                    stop.store(true, Ordering::Relaxed);
                    if finals.len() == 2 {
                        break;
                    }
                }
                StreamTranscriptEvent::Error { message, .. } => warnings.push(message),
            }
        }
        let _ = pump.await;

        assert_eq!(server.await.unwrap(), 2, "both sessions must be finalized");
        assert_eq!(
            finals,
            vec!["first session".to_string(), "second session".to_string()],
            "transcription continues across the rollover"
        );
        assert!(
            !warnings.iter().any(|w| w.contains("reconnecting")),
            "a planned rollover is not a connection failure: {warnings:?}"
        );
    }

    #[test]
    fn session_limit_defaults_and_can_be_disabled() {
        let mut config = CustomTranscriptionConfig {
            endpoint: "ws://host".into(),
            api_key: None,
            model: "m".into(),
            protocol: "voxtral-realtime".into(),
            delay_ms: None,
            max_session_seconds: None,
        };
        // Unset → the conservative default, so long meetings survive out of the box.
        assert_eq!(
            config.session_limit_seconds(),
            Some(crate::audio::transcription::DEFAULT_MAX_SESSION_SECONDS)
        );
        config.max_session_seconds = Some(900);
        assert_eq!(config.session_limit_seconds(), Some(900));
        // 0 is the escape hatch: one session for the whole recording.
        config.max_session_seconds = Some(0);
        assert_eq!(config.session_limit_seconds(), None);
    }

    #[test]
    fn config_without_the_limit_field_still_deserializes() {
        // Configs saved before this setting existed must keep loading.
        let stored = r#"{"endpoint":"ws://host","apiKey":null,"model":"m","protocol":"voxtral-realtime","delayMs":null}"#;
        let config: CustomTranscriptionConfig = serde_json::from_str(stored).unwrap();
        assert_eq!(config.max_session_seconds, None);
        assert_eq!(
            config.session_limit_seconds(),
            Some(crate::audio::transcription::DEFAULT_MAX_SESSION_SECONDS)
        );
    }

    #[test]
    fn models_url_derivation() {
        assert_eq!(
            models_url("wss://asr.example.com/v1/realtime"),
            "https://asr.example.com/v1/models"
        );
        assert_eq!(
            models_url("ws://localhost:8000/v1/realtime"),
            "http://localhost:8000/v1/models"
        );
        // Host-only endpoints (the realtime path is implied) and trailing slashes.
        assert_eq!(models_url("ws://localhost:8000/"), "http://localhost:8000/v1/models");
        assert_eq!(
            models_url("https://asr.example.com"),
            "https://asr.example.com/v1/models"
        );
        // An unusual path layout falls back to the host root rather than guessing.
        assert_eq!(models_url("ws://host/custom/socket"), "http://host/v1/models");
    }

    #[test]
    fn context_length_prefers_the_requested_model() {
        let body = serde_json::json!({
            "object": "list",
            "data": [
                { "id": "other", "max_model_len": 32768 },
                { "id": "wanted", "max_model_len": 8192 },
            ]
        });
        assert_eq!(context_length_for_model(&body, "wanted"), Some(8192));
        // Unknown id → fall back to whatever the server does announce.
        assert_eq!(context_length_for_model(&body, "missing"), Some(32768));
    }

    #[test]
    fn context_length_absent_is_not_an_error() {
        let body = serde_json::json!({ "data": [{ "id": "m", "object": "model" }] });
        assert_eq!(context_length_for_model(&body, "m"), None);
        // Alternate spellings used by non-vLLM servers.
        let alt = serde_json::json!({ "data": [{ "id": "m", "context_length": 4096 }] });
        assert_eq!(context_length_for_model(&alt, "m"), Some(4096));
    }

    #[test]
    fn seconds_for_context_leaves_headroom() {
        // 8192 tokens: ~655s of audio alone, less once the transcript and headroom
        // are accounted for — and safely under the ~8 min where such a server dies.
        let secs = seconds_for_context(8192);
        assert!((400..=500).contains(&secs), "unexpected: {secs}");
        assert!(seconds_for_context(32768) > seconds_for_context(8192));
        // Absurd values stay in a usable band.
        assert_eq!(seconds_for_context(128), 60);
        assert_eq!(seconds_for_context(10_000_000), 3600);
    }
}
