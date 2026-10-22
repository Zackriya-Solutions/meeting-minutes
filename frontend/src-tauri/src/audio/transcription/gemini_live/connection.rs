// audio/transcription/gemini_live/connection.rs
//
// One Gemini Live WebSocket session. `transcribe_segment` streams a VAD speech segment as
// paced 100 ms PCM frames, sends `audioStreamEnd`, and collects the finalized text.
//
// Pacing matters: a segment sent in one burst produced no transcription at all, while
// 4x real time transcribed correctly, so frames are paced at PACE_FACTOR.

use super::pcm;
use super::protocol::{self, ServerEvent, SetupOptions};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use log::{debug, info, warn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{timeout, timeout_at, Duration, Instant};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait after `audioStreamEnd` for the server to report speech. Segments the
/// server considers non-speech get no reply at all.
const NO_SPEECH_WAIT: Duration = Duration::from_millis(1500);
/// After `generationComplete`, wait briefly in case another utterance from the same
/// segment is still being finalized.
const TURN_DONE_GRACE: Duration = Duration::from_millis(250);

pub const PACE_FACTOR: f64 = 4.0;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub enum LiveError {
    /// Retrying will not help (invalid or missing API key, rejected setup).
    Auth(String),
    /// Network errors, timeouts, server-side closes.
    Transient(String),
}

impl std::fmt::Display for LiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth(m) => write!(f, "Gemini rejected the request: {}", m),
            Self::Transient(m) => write!(f, "Gemini connection problem: {}", m),
        }
    }
}

/// `during_setup`: auth problems are only reported while the session is being set up.
/// Mid-session the server also uses 1008, e.g. when closing ~50 s after goAway, and that
/// must be retried rather than treated as a bad key.
fn classify_close(code: Option<u16>, reason: &str, during_setup: bool) -> LiveError {
    let msg = match code {
        Some(c) => format!("connection closed ({}): {}", c, reason),
        None => format!("connection closed: {}", reason),
    };
    if during_setup && protocol::is_permanent_close(code) {
        LiveError::Auth(msg)
    } else {
        LiveError::Transient(msg)
    }
}

pub struct LiveConnection {
    sink: Option<SplitSink<WsStream, Message>>,
    events: mpsc::UnboundedReceiver<ServerEvent>,
    reader: JoinHandle<()>,
    opened_at: Instant,
    go_away: bool,
    closed: bool,
    /// Finals that arrived after their segment's wait ended.
    carry: Vec<String>,
}

/// Per-segment bookkeeping while waiting for results.
#[derive(Default)]
struct SegmentWait {
    texts: Vec<String>,
    speech_seen: bool,
    turn_done_at: Option<Instant>,
}

impl LiveConnection {
    pub async fn connect(api_key: &str, setup: &SetupOptions) -> Result<Self, LiveError> {
        Self::connect_to(protocol::WS_URL, api_key, setup).await
    }

    /// Connect to a specific endpoint (tests point this at a local mock server).
    pub async fn connect_to(endpoint: &str, api_key: &str, setup: &SetupOptions) -> Result<Self, LiveError> {
        let url = url::Url::parse(endpoint).map_err(|e| LiveError::Transient(e.to_string()))?;
        let host = url.host_str().unwrap_or_default().to_string();
        let port = url.port_or_known_default().unwrap_or(443);
        let mut request = endpoint
            .into_client_request()
            .map_err(|e| LiveError::Transient(e.to_string()))?;
        // Header auth keeps the key out of the URL (and out of any logged URL).
        let key = HeaderValue::from_str(api_key)
            .map_err(|_| LiveError::Auth("API key contains invalid characters".into()))?;
        request.headers_mut().insert("x-goog-api-key", key);

        let ws = timeout(CONNECT_TIMEOUT, async {
            let tcp = open_tcp(&host, port).await?;
            tokio_tungstenite::client_async_tls(request, tcp)
                .await
                .map_err(handshake_error)
        })
        .await
        .map_err(|_| LiveError::Transient("timed out connecting to Gemini".into()))??
        .0;

        let (sink, mut stream) = ws.split();
        let (tx, events) = mpsc::unbounded_channel();
        let reader = tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                let text = match msg {
                    Ok(Message::Text(t)) => t,
                    // The Live API sends JSON in binary frames too.
                    Ok(Message::Binary(b)) => String::from_utf8_lossy(&b).into_owned(),
                    Ok(Message::Close(frame)) => {
                        let (code, reason) = frame
                            .map(|f| (Some(u16::from(f.code)), f.reason.into_owned()))
                            .unwrap_or((None, String::new()));
                        let _ = tx.send(ServerEvent::Closed { code, reason });
                        return;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        let _ = tx.send(ServerEvent::Closed { code: None, reason: e.to_string() });
                        return;
                    }
                };
                for event in protocol::parse_server_message(&text) {
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
            let _ = tx.send(ServerEvent::Closed { code: None, reason: "stream ended".into() });
        });

        let mut conn = Self {
            sink: Some(sink),
            events,
            reader,
            opened_at: Instant::now(),
            go_away: false,
            closed: false,
            carry: Vec::new(),
        };
        conn.send(protocol::setup_message(setup)).await?;

        let deadline = Instant::now() + CONNECT_TIMEOUT;
        loop {
            match timeout_at(deadline, conn.events.recv()).await {
                Ok(Some(ServerEvent::SetupComplete)) => break,
                Ok(Some(ServerEvent::Closed { code, reason })) => return Err(classify_close(code, &reason, true)),
                Ok(Some(_)) => continue,
                Ok(None) => return Err(LiveError::Transient("connection dropped during setup".into())),
                Err(_) => return Err(LiveError::Transient("timed out waiting for setupComplete".into())),
            }
        }
        info!("🌐 Gemini Live session opened (model {})", protocol::MODEL);
        Ok(conn)
    }

    pub fn age(&self) -> Duration {
        self.opened_at.elapsed()
    }

    /// Process events queued since the last segment, then report whether the session can
    /// take another segment (false after goAway or once the socket closed).
    pub fn check_usable(&mut self) -> bool {
        let mut wait = SegmentWait::default();
        if self.drain_pending(&mut wait).is_err() {
            self.closed = true;
        }
        self.carry.append(&mut wait.texts);
        !self.closed && !self.go_away
    }

    /// Late text not yet returned by `transcribe_segment` (used when retiring a session).
    pub fn take_carry(&mut self) -> Vec<String> {
        std::mem::take(&mut self.carry)
    }

    async fn send(&mut self, text: String) -> Result<(), LiveError> {
        let sink = self
            .sink
            .as_mut()
            .ok_or_else(|| LiveError::Transient("connection already closed".into()))?;
        sink.send(Message::Text(text)).await.map_err(|e| {
            self.closed = true;
            LiveError::Transient(e.to_string())
        })
    }

    fn handle(&mut self, event: ServerEvent, wait: &mut SegmentWait) -> Result<(), LiveError> {
        match event {
            ServerEvent::Final(text) => {
                wait.speech_seen = true;
                wait.turn_done_at = None;
                if !text.trim().is_empty() {
                    wait.texts.push(text.trim().to_string());
                }
            }
            ServerEvent::ActivityStart => {
                wait.speech_seen = true;
                wait.turn_done_at = None;
            }
            ServerEvent::TurnDone => wait.turn_done_at = Some(Instant::now()),
            ServerEvent::GoAway { time_left } => {
                info!("🌐 Gemini Live goAway received (time left: {:?}); will rotate before next segment", time_left);
                self.go_away = true;
            }
            ServerEvent::Closed { code, reason } => {
                self.closed = true;
                return Err(classify_close(code, &reason, false));
            }
            ServerEvent::Interim(_) | ServerEvent::ActivityEnd | ServerEvent::SetupComplete => {}
        }
        Ok(())
    }

    fn drain_pending(&mut self, wait: &mut SegmentWait) -> Result<(), LiveError> {
        while let Ok(event) = self.events.try_recv() {
            self.handle(event, wait)?;
        }
        Ok(())
    }

    /// Stream one speech segment (16 kHz mono f32) and return its finalized text.
    /// Text that arrived late for the previous segment is included rather than lost.
    pub async fn transcribe_segment(&mut self, samples: &[f32], pace: f64) -> Result<String, LiveError> {
        let mut wait = SegmentWait { texts: std::mem::take(&mut self.carry), ..Default::default() };
        // Late finals from the previous segment.
        self.drain_pending(&mut wait)?;
        wait.speech_seen = false;
        wait.turn_done_at = None;

        let frame_period = Duration::from_secs_f64(0.1 / pace);
        let start = Instant::now();
        for (i, frame) in pcm::frames(samples).enumerate() {
            self.send(protocol::audio_message(&frame)).await?;
            tokio::time::sleep_until(start + frame_period * (i as u32 + 1)).await;
            self.drain_pending(&mut wait)?;
        }
        self.send(protocol::audio_stream_end_message()).await?;
        let ended = Instant::now();

        let segment_secs = pcm::duration_secs(samples);
        let hard_deadline = ended + Duration::from_secs_f64((8.0 + segment_secs * 0.5).min(30.0));
        loop {
            let soft = match (wait.turn_done_at, wait.speech_seen) {
                (Some(t), _) => t + TURN_DONE_GRACE,
                (None, false) => ended + NO_SPEECH_WAIT,
                (None, true) => hard_deadline,
            };
            match timeout_at(soft.min(hard_deadline), self.events.recv()).await {
                Ok(Some(event)) => self.handle(event, &mut wait)?,
                Ok(None) => {
                    self.closed = true;
                    return Err(LiveError::Transient("connection dropped".into()));
                }
                Err(_) => {
                    if wait.turn_done_at.is_some() || !wait.speech_seen {
                        break;
                    }
                    if wait.texts.is_empty() {
                        // Speech was detected but nothing came back: treat the session as
                        // stuck so the caller retries on a fresh connection.
                        self.closed = true;
                        return Err(LiveError::Transient(format!(
                            "no transcription {:.0}s after a {:.1}s segment",
                            hard_deadline.duration_since(ended).as_secs_f64(),
                            segment_secs
                        )));
                    }
                    warn!("Gemini Live: no generationComplete before deadline; using {} final(s) received", wait.texts.len());
                    break;
                }
            }
        }
        debug!(
            "Gemini Live segment {:.1}s -> {} final(s) in {:.2}s after stream end",
            segment_secs,
            wait.texts.len(),
            ended.elapsed().as_secs_f64()
        );
        Ok(wait.texts.join(" "))
    }

    /// Close politely in the background (used when rotating to a new session).
    pub fn close_in_background(mut self) {
        if let Some(mut sink) = self.sink.take() {
            tokio::spawn(async move {
                let _ = sink.send(Message::Close(None)).await;
                let _ = sink.close().await;
            });
        }
    }
}

impl Drop for LiveConnection {
    fn drop(&mut self) {
        self.reader.abort();
        if let Some(mut sink) = self.sink.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = sink.send(Message::Close(None)).await;
                    let _ = sink.close().await;
                });
            }
        }
    }
}

fn handshake_error(e: tokio_tungstenite::tungstenite::Error) -> LiveError {
    use tokio_tungstenite::tungstenite::Error;
    match e {
        Error::Http(response) => {
            let status = response.status().as_u16();
            let body = response
                .body()
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).chars().take(200).collect::<String>())
                .unwrap_or_default();
            let msg = format!("HTTP {} during WebSocket handshake {}", status, body);
            if status == 400 || status == 401 || status == 403 {
                LiveError::Auth(msg)
            } else {
                LiveError::Transient(msg)
            }
        }
        other => LiveError::Transient(other.to_string()),
    }
}

/// HTTPS proxy from the standard environment variables, honouring NO_PROXY.
fn proxy_for(host: &str) -> Option<url::Url> {
    let var = |names: &[&str]| names.iter().find_map(|n| std::env::var(n).ok().filter(|v| !v.is_empty()));
    let proxy = var(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"])?;
    if let Some(no_proxy) = var(&["NO_PROXY", "no_proxy"]) {
        let excluded = no_proxy.split(',').map(str::trim).any(|entry| {
            let entry = entry.trim_start_matches("*.").trim_start_matches('.');
            entry == "*" || (!entry.is_empty() && (host == entry || host.ends_with(&format!(".{}", entry))))
        });
        if excluded {
            return None;
        }
    }
    let proxy = if proxy.contains("://") { proxy } else { format!("http://{}", proxy) };
    url::Url::parse(&proxy).ok()
}

async fn open_tcp(host: &str, port: u16) -> Result<TcpStream, LiveError> {
    let io = |e: std::io::Error| LiveError::Transient(e.to_string());
    let Some(proxy) = proxy_for(host) else {
        return TcpStream::connect((host, port)).await.map_err(io);
    };
    let proxy_host = proxy.host_str().unwrap_or_default().to_string();
    let proxy_port = proxy.port_or_known_default().unwrap_or(8080);
    debug!("Gemini Live: connecting through HTTPS proxy {}:{}", proxy_host, proxy_port);
    let mut stream = TcpStream::connect((proxy_host.as_str(), proxy_port)).await.map_err(io)?;

    let mut request = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    if !proxy.username().is_empty() {
        use base64::Engine;
        let credentials = format!("{}:{}", proxy.username(), proxy.password().unwrap_or(""));
        request.push_str(&format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD.encode(credentials)
        ));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.map_err(io)?;

    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.map_err(io)? == 0 || response.len() > 16 * 1024 {
            return Err(LiveError::Transient("proxy closed the CONNECT request".into()));
        }
        response.push(byte[0]);
    }
    let status_line = String::from_utf8_lossy(&response);
    let status_line = status_line.lines().next().unwrap_or_default();
    if status_line.split_whitespace().nth(1) != Some("200") {
        return Err(LiveError::Transient(format!("proxy refused CONNECT: {}", status_line)));
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_codes_only_permanent_during_setup() {
        assert!(matches!(classify_close(Some(1007), "API key not valid", true), LiveError::Auth(_)));
        assert!(matches!(classify_close(Some(1008), "unregistered caller", true), LiveError::Auth(_)));
        // The server closes with 1008 ~50 s after goAway; that is a normal session end.
        assert!(matches!(classify_close(Some(1008), "failed to close after GoAway", false), LiveError::Transient(_)));
        assert!(matches!(classify_close(Some(1011), "internal", true), LiveError::Transient(_)));
    }

    #[test]
    fn no_proxy_excludes_host() {
        // Env vars are process-wide; keep all proxy assertions in this single test.
        std::env::set_var("HTTPS_PROXY", "http://proxy.local:3128");
        std::env::set_var("NO_PROXY", "localhost,.googleapis.com");
        assert!(proxy_for("generativelanguage.googleapis.com").is_none());
        std::env::set_var("NO_PROXY", "localhost");
        let p = proxy_for("generativelanguage.googleapis.com").unwrap();
        assert_eq!((p.host_str(), p.port()), (Some("proxy.local"), Some(3128)));
        std::env::remove_var("HTTPS_PROXY");
        std::env::remove_var("NO_PROXY");
    }
}
