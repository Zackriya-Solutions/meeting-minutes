// audio/transcription/gemini_live/tests.rs
//
// Provider tests against a local mock of the Gemini Live WebSocket, which behaves like
// the real server as observed in live tests (setupComplete, voiceActivity, finals +
// generationComplete per audioStreamEnd, goAway, auth closes with 1007).
//
// `real_api_hinglish_clip` talks to the real API and is ignored by default:
//   GEMINI_API_KEY=... MEETILY_GEMINI_TEST_PCM=clip_16k_s16le.pcm \
//     cargo test --lib gemini_live::tests::real_api -- --ignored --nocapture

use super::provider::{GeminiLiveProvider, GeminiLiveStatus, StatusCallback};
use super::protocol::SetupOptions;
use crate::audio::transcription::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::Message;

const KEY: &str = "test-key";

#[derive(Clone, Copy, Default)]
struct ConnPlan {
    reject_auth: bool,
    /// Drop the socket (no close frame) after this many audio frames.
    drop_after_frames: Option<usize>,
    /// Send goAway after this many completed segments.
    go_away_after_segments: Option<usize>,
    /// Never report speech or send finals (non-speech audio).
    silent: bool,
}

#[derive(Default)]
struct Stats {
    connections: AtomicUsize,
    /// Samples received per segment, per connection.
    segments: Mutex<Vec<Vec<usize>>>,
}

type Plan = Arc<dyn Fn(usize) -> ConnPlan + Send + Sync>;
type Reply = Arc<dyn Fn(usize, usize) -> String + Send + Sync>;

async fn start_mock(plan: Plan, reply: Reply) -> (String, Arc<Stats>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let stats = Arc::new(Stats::default());
    let server_stats = stats.clone();
    tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let idx = server_stats.connections.fetch_add(1, Ordering::SeqCst);
            server_stats.segments.lock().unwrap().push(Vec::new());
            tokio::spawn(serve(tcp, idx, plan(idx), reply.clone(), server_stats.clone()));
        }
    });
    (format!("ws://127.0.0.1:{}/ws", port), stats)
}

async fn serve(tcp: tokio::net::TcpStream, idx: usize, plan: ConnPlan, reply: Reply, stats: Arc<Stats>) {
    let check_key = |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
                     resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
        assert_eq!(req.headers().get("x-goog-api-key").unwrap(), KEY);
        Ok(resp)
    };
    let mut ws = tokio_tungstenite::accept_hdr_async(tcp, check_key).await.unwrap();
    let (mut frames, mut samples, mut segment, mut active) = (0usize, 0usize, 0usize, false);
    let text = |s: String| Message::Text(s);
    while let Some(Ok(msg)) = ws.next().await {
        let Message::Text(raw) = msg else { continue };
        let v: Value = serde_json::from_str(&raw).unwrap();
        if v.get("setup").is_some() {
            if plan.reject_auth {
                let _ = ws
                    .send(Message::Close(Some(CloseFrame {
                        code: CloseCode::from(1007),
                        reason: "API key not valid. Please pass a valid API key.".into(),
                    })))
                    .await;
                return;
            }
            ws.send(text(r#"{"setupComplete":{}}"#.into())).await.unwrap();
        } else if let Some(data) = v.pointer("/realtimeInput/audio/data").and_then(Value::as_str) {
            frames += 1;
            samples += base64::engine::general_purpose::STANDARD.decode(data).unwrap().len() / 2;
            if plan.drop_after_frames == Some(frames) {
                return; // abrupt drop
            }
            if !active && !plan.silent {
                active = true;
                ws.send(text(r#"{"serverContent":{},"voiceActivity":{"type":"ACTIVITY_START","audioOffset":"0.4s"}}"#.into()))
                    .await
                    .unwrap();
            }
        } else if v.pointer("/realtimeInput/audioStreamEnd").is_some() {
            stats.segments.lock().unwrap()[idx].push(samples);
            if !plan.silent {
                let final_msg = serde_json::json!({"serverContent": {"inputTranscription": {"text": reply(idx, segment)}}});
                ws.send(text(final_msg.to_string())).await.unwrap();
                ws.send(text(r#"{"serverContent":{"generationComplete":true}}"#.into())).await.unwrap();
                ws.send(text(r#"{"serverContent":{},"voiceActivity":{"type":"ACTIVITY_END","audioOffset":"1s"}}"#.into()))
                    .await
                    .unwrap();
            }
            segment += 1;
            samples = 0;
            active = false;
            if plan.go_away_after_segments == Some(segment) {
                ws.send(text(r#"{"goAway":{"timeLeft":"50s"}}"#.into())).await.unwrap();
            }
        }
    }
}

struct LocalMock {
    calls: AtomicUsize,
}

#[async_trait]
impl TranscriptionProvider for LocalMock {
    async fn transcribe(&self, _audio: Vec<f32>, _language: Option<String>) -> Result<TranscriptResult, TranscriptionError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TranscriptResult { text: "local".into(), confidence: None, is_partial: false })
    }
    async fn is_model_loaded(&self) -> bool {
        true
    }
    async fn get_current_model(&self) -> Option<String> {
        Some("mock".into())
    }
    fn provider_name(&self) -> &'static str {
        "MockLocal"
    }
}

struct Harness {
    provider: GeminiLiveProvider,
    stats: Arc<Stats>,
    local: Arc<LocalMock>,
    statuses: Arc<Mutex<Vec<&'static str>>>,
}

impl Harness {
    async fn new(plan: impl Fn(usize) -> ConnPlan + Send + Sync + 'static, reply: impl Fn(usize, usize) -> String + Send + Sync + 'static) -> Self {
        let (url, stats) = start_mock(Arc::new(plan), Arc::new(reply)).await;
        let local = Arc::new(LocalMock { calls: AtomicUsize::new(0) });
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let sink = statuses.clone();
        let on_status: StatusCallback = Arc::new(move |s: GeminiLiveStatus| sink.lock().unwrap().push(s.kind));
        let provider = GeminiLiveProvider::new(KEY.into(), SetupOptions::default(), Some(local.clone()), on_status)
            .with_endpoint(url);
        Self { provider, stats, local, statuses }
    }

    async fn say(&self, secs: f64) -> String {
        let audio: Vec<f32> = (0..(secs * 16000.0) as usize).map(|i| 0.3 * (i as f32 * 0.07).sin()).collect();
        self.provider.transcribe(audio, None).await.unwrap().text
    }

    fn connections(&self) -> usize {
        self.stats.connections.load(Ordering::SeqCst)
    }
}

fn segment_reply(conn: usize, seg: usize) -> String {
    format!("conn{} seg{}", conn, seg)
}

#[tokio::test]
async fn reuses_one_session_across_segments() {
    let h = Harness::new(|_| ConnPlan::default(), segment_reply).await;
    assert_eq!(h.say(0.6).await, "conn0 seg0");
    assert_eq!(h.say(0.6).await, "conn0 seg1");
    assert_eq!(h.connections(), 1);
    assert_eq!(h.local.calls.load(Ordering::SeqCst), 0);
    // 0.6 s of audio arrives intact as 16 kHz samples.
    assert_eq!(h.stats.segments.lock().unwrap()[0], vec![9600, 9600]);
}

#[tokio::test]
async fn bad_key_uses_local_engine_for_rest_of_recording() {
    let h = Harness::new(|_| ConnPlan { reject_auth: true, ..Default::default() }, segment_reply).await;
    assert_eq!(h.say(0.5).await, "local");
    assert_eq!(h.say(0.5).await, "local");
    assert_eq!(h.connections(), 1, "no reconnect attempts after an auth failure");
    assert_eq!(*h.statuses.lock().unwrap(), vec!["disabled"]);
}

#[tokio::test]
async fn dropped_segment_is_resent_on_new_session_without_duplicates() {
    let h = Harness::new(
        |idx| ConnPlan { drop_after_frames: (idx == 0).then_some(3), ..Default::default() },
        segment_reply,
    )
    .await;
    assert_eq!(h.say(0.8).await, "conn1 seg0");
    assert_eq!(h.connections(), 2);
    assert_eq!(h.local.calls.load(Ordering::SeqCst), 0);
    // The retry carried the whole segment.
    assert_eq!(h.stats.segments.lock().unwrap()[1], vec![12800]);
}

#[tokio::test]
async fn go_away_rotates_session_with_overlap_and_dedup() {
    let h = Harness::new(
        |idx| ConnPlan { go_away_after_segments: (idx == 0).then_some(1), ..Default::default() },
        |conn, seg| match (conn, seg) {
            (0, 0) => "alpha beta gamma delta".into(),
            (1, 0) => "gamma delta".into(),               // priming result, discarded
            (1, 1) => "Gamma delta, epsilon zeta".into(), // overlaps the previous text
            _ => "later".into(),
        },
    )
    .await;
    assert_eq!(h.say(2.5).await, "alpha beta gamma delta");
    assert_eq!(h.say(1.0).await, "epsilon zeta");
    assert_eq!(h.connections(), 2);
    let segments = h.stats.segments.lock().unwrap();
    assert_eq!(segments[1], vec![32000, 16000], "new session primed with last 2 s, then the segment");
    drop(segments);
    assert_eq!(h.say(0.5).await, "later");
    assert!(h.statuses.lock().unwrap().is_empty());
}

#[tokio::test]
async fn repeated_failures_pause_gemini_and_use_local() {
    let h = Harness::new(|_| ConnPlan { drop_after_frames: Some(1), ..Default::default() }, segment_reply).await;
    for _ in 0..3 {
        assert_eq!(h.say(0.5).await, "local");
    }
    assert_eq!(*h.statuses.lock().unwrap(), vec!["fallback"]);
    let before = h.connections();
    assert_eq!(h.say(0.5).await, "local");
    assert_eq!(h.connections(), before, "Gemini is paused during the cooldown");
    assert_eq!(h.local.calls.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn non_speech_segment_returns_empty_quickly() {
    let h = Harness::new(|_| ConnPlan { silent: true, ..Default::default() }, segment_reply).await;
    let started = std::time::Instant::now();
    assert_eq!(h.say(1.0).await, "");
    assert!(started.elapsed().as_secs_f64() < 3.0, "took {:?}", started.elapsed());
    assert_eq!(h.local.calls.load(Ordering::SeqCst), 0);
}

/// Split s16le PCM into speech segments on 300 ms pauses, like the app's VAD output.
fn split_on_silence(pcm: &[i16]) -> Vec<Vec<f32>> {
    let frame = 480;
    let x: Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
    let rms: Vec<f32> = x.chunks(frame).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let peak = rms.iter().cloned().fold(0.0, f32::max);
    let (mut segments, mut start, mut quiet_run) = (Vec::new(), 0usize, 0usize);
    for (i, &r) in rms.iter().enumerate() {
        quiet_run = if r < 0.02 * peak { quiet_run + 1 } else { 0 };
        if quiet_run == 10 {
            let end = (i - 9) * frame;
            if end > start + 8000 {
                segments.push(x[start..end].to_vec());
            }
            start = (i + 1) * frame;
        } else if quiet_run > 10 {
            start = (i + 1) * frame;
        }
    }
    if x.len() > start + 8000 {
        segments.push(x[start..].to_vec());
    }
    segments
}

#[tokio::test]
#[ignore = "needs GEMINI_API_KEY and MEETILY_GEMINI_TEST_PCM"]
async fn real_api_hinglish_clip() {
    let key = std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY");
    let bytes = std::fs::read(std::env::var("MEETILY_GEMINI_TEST_PCM").expect("MEETILY_GEMINI_TEST_PCM")).unwrap();
    let pcm: Vec<i16> = bytes.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let settings = super::settings::GeminiLiveSettings {
        custom_vocabulary: vec!["Meetily".into(), "Kubernetes".into(), "BigQuery".into()],
        ..Default::default()
    };
    let on_status: StatusCallback = Arc::new(|s: GeminiLiveStatus| println!("status: {:?}", s));
    let provider = GeminiLiveProvider::new(key, settings.setup_options(), None, on_status);
    let mut all = Vec::new();
    for (i, segment) in split_on_silence(&pcm).into_iter().enumerate() {
        let started = std::time::Instant::now();
        let secs = segment.len() as f64 / 16000.0;
        let text = provider.transcribe(segment, None).await.expect("Gemini transcription").text;
        println!("segment {} ({:.1}s audio, {:.2}s wall): {}", i, secs, started.elapsed().as_secs_f64(), text);
        all.push(text);
    }
    let joined = all.join(" ");
    assert!(joined.contains("Kubernetes"), "{}", joined);
    assert!(joined.contains("Meetily"), "{}", joined);
}
