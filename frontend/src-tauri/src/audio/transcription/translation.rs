use anyhow::{anyhow, Result};
use log::{error, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

pub const DEFAULT_TARGET_LANGUAGE: &str = "pt-BR";
pub const DEFAULT_LIBRETRANSLATE_ENDPOINT: &str = "";
pub const DEFAULT_LIBRETRANSLATE_API_KEY: &str = "";
pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "aya-expanse:latest";
const TRANSLATION_QUEUE_CAPACITY: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TranslationEngineKind {
    Libretranslate,
    Ollama,
}

impl Default for TranslationEngineKind {
    fn default() -> Self {
        Self::Ollama
    }
}

impl TranslationEngineKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "libretranslate" => Ok(Self::Libretranslate),
            "ollama" => Ok(Self::Ollama),
            other => Err(anyhow!(
                "Unsupported translation engine '{}'; choose libretranslate or ollama",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettings {
    pub enabled: bool,
    pub engine: TranslationEngineKind,
    pub target_language: String,
    pub libretranslate_endpoint: String,
    pub libretranslate_api_key: String,
    pub ollama_endpoint: String,
    pub ollama_model: String,
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: TranslationEngineKind::default(),
            target_language: DEFAULT_TARGET_LANGUAGE.to_string(),
            libretranslate_endpoint: DEFAULT_LIBRETRANSLATE_ENDPOINT.to_string(),
            libretranslate_api_key: DEFAULT_LIBRETRANSLATE_API_KEY.to_string(),
            ollama_endpoint: DEFAULT_OLLAMA_ENDPOINT.to_string(),
            ollama_model: DEFAULT_OLLAMA_MODEL.to_string(),
        }
    }
}

#[derive(Debug, Deserialize, sqlx::FromRow)]
struct TranslationSettingsRow {
    #[sqlx(rename = "translationEnabled")]
    translation_enabled: i64,
    #[sqlx(rename = "translationEngine")]
    translation_engine: String,
    #[sqlx(rename = "translationTargetLanguage")]
    translation_target_language: String,
    #[sqlx(rename = "translationLibreTranslateEndpoint")]
    translation_libretranslate_endpoint: String,
    #[sqlx(rename = "translationLibreTranslateApiKey")]
    translation_libretranslate_api_key: String,
    #[sqlx(rename = "translationOllamaEndpoint")]
    translation_ollama_endpoint: String,
    #[sqlx(rename = "translationOllamaModel")]
    translation_ollama_model: String,
}

pub async fn load_settings(pool: &SqlitePool) -> Result<TranslationSettings> {
    let row = sqlx::query_as::<_, TranslationSettingsRow>(
        "SELECT translationEnabled, translationEngine, translationTargetLanguage,
                translationLibreTranslateEndpoint, translationLibreTranslateApiKey,
                translationOllamaEndpoint, translationOllamaModel
         FROM translation_settings LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(TranslationSettings::default());
    };

    Ok(TranslationSettings {
        enabled: row.translation_enabled != 0,
        engine: TranslationEngineKind::parse(&row.translation_engine)?,
        target_language: if row.translation_target_language.trim().is_empty() {
            DEFAULT_TARGET_LANGUAGE.to_string()
        } else {
            row.translation_target_language
        },
        libretranslate_endpoint: if row.translation_libretranslate_endpoint.trim().is_empty() {
            DEFAULT_LIBRETRANSLATE_ENDPOINT.to_string()
        } else {
            row.translation_libretranslate_endpoint
        },
        libretranslate_api_key: row.translation_libretranslate_api_key,
        ollama_endpoint: if row.translation_ollama_endpoint.trim().is_empty() {
            DEFAULT_OLLAMA_ENDPOINT.to_string()
        } else {
            row.translation_ollama_endpoint
        },
        ollama_model: if row.translation_ollama_model.trim().is_empty() {
            DEFAULT_OLLAMA_MODEL.to_string()
        } else {
            row.translation_ollama_model
        },
    })
}

pub async fn save_settings(pool: &SqlitePool, settings: &TranslationSettings) -> Result<()> {
    let engine = serde_json::to_value(&settings.engine)?
        .as_str()
        .unwrap_or("ollama")
        .to_string();
    sqlx::query(
        "INSERT INTO translation_settings
            (id, translationEnabled, translationEngine, translationTargetLanguage,
             translationLibreTranslateEndpoint, translationLibreTranslateApiKey,
             translationOllamaEndpoint, translationOllamaModel)
         VALUES ('1', ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             translationEnabled = excluded.translationEnabled,
             translationEngine = excluded.translationEngine,
             translationTargetLanguage = excluded.translationTargetLanguage,
             translationLibreTranslateEndpoint = excluded.translationLibreTranslateEndpoint,
             translationLibreTranslateApiKey = excluded.translationLibreTranslateApiKey,
             translationOllamaEndpoint = excluded.translationOllamaEndpoint,
             translationOllamaModel = excluded.translationOllamaModel",
    )
    .bind(if settings.enabled { 1_i64 } else { 0_i64 })
    .bind(engine)
    .bind(&settings.target_language)
    .bind(&settings.libretranslate_endpoint)
    .bind(&settings.libretranslate_api_key)
    .bind(&settings.ollama_endpoint)
    .bind(&settings.ollama_model)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationJob {
    pub sequence_id: u64,
    pub text: String,
    pub target_language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationRoute {
    Disabled,
    NotFinal,
    Queued,
    Full,
}

/// Bounded, non-blocking queue used by the live worker and unit-tested independently.
pub struct TranslationQueue {
    sender: Mutex<Option<mpsc::Sender<TranslationJob>>>,
    receiver: Mutex<Option<mpsc::Receiver<TranslationJob>>>,
}

impl TranslationQueue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender: Mutex::new(Some(sender)),
            receiver: Mutex::new(Some(receiver)),
        }
    }

    pub fn route(&self, enabled: bool, is_final: bool, job: TranslationJob) -> TranslationRoute {
        if !enabled {
            return TranslationRoute::Disabled;
        }
        if !is_final {
            return TranslationRoute::NotFinal;
        }
        let Ok(sender) = self.sender.lock() else {
            return TranslationRoute::Full;
        };
        let Some(sender) = sender.as_ref() else {
            return TranslationRoute::Full;
        };
        match sender.try_send(job) {
            Ok(()) => TranslationRoute::Queued,
            Err(mpsc::error::TrySendError::Full(_)) => TranslationRoute::Full,
            Err(mpsc::error::TrySendError::Closed(_)) => TranslationRoute::Full,
        }
    }

    fn take_receiver(&self) -> Option<mpsc::Receiver<TranslationJob>> {
        self.receiver.lock().ok()?.take()
    }

    fn close(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslationUpdate {
    pub sequence_id: u64,
    pub translation: Option<String>,
    pub target_language: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OllamaTranslationRequest {
    pub model: String,
    pub messages: Vec<OllamaMessage>,
    pub stream: bool,
}

#[derive(Debug, Serialize)]
pub struct OllamaMessage {
    pub role: String,
    pub content: String,
}

impl OllamaTranslationRequest {
    pub fn new(model: &str, target_language: &str, text: &str) -> Self {
        Self {
            model: model.to_string(),
            messages: vec![
                OllamaMessage {
                    role: "system".to_string(),
                    content: format!(
                        "Translate the user's text from English to {}. Return only the translation, with no explanation.",
                        target_language
                    ),
                },
                OllamaMessage {
                    role: "user".to_string(),
                    content: text.to_string(),
                },
            ],
            stream: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OllamaTranslationResponse {
    message: Option<OllamaResponseMessage>,
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

pub fn parse_ollama_response(value: &serde_json::Value) -> Result<String> {
    let response: OllamaTranslationResponse = serde_json::from_value(value.clone())?;
    response
        .message
        .map(|message| message.content)
        .or(response.response)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow!("Ollama returned an empty translation"))
}

#[derive(Debug, Serialize)]
pub struct LibreTranslateRequest {
    pub q: String,
    pub source: String,
    pub target: String,
    pub format: String,
    pub alternatives: u32,
    pub api_key: String,
}

impl LibreTranslateRequest {
    pub fn new(text: &str, target: &str, api_key: &str) -> Self {
        Self {
            q: text.to_string(),
            source: "auto".to_string(),
            target: target.to_string(),
            format: "text".to_string(),
            alternatives: 3,
            api_key: api_key.to_string(),
        }
    }
}

pub fn parse_libretranslate_response(value: &serde_json::Value) -> Result<String> {
    let text = value
        .get("translatedText")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| anyhow!("LibreTranslate returned an empty translation"))?;
    Ok(text.to_string())
}

fn libretranslate_language_code(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

#[derive(Clone)]
enum TranslationAdapter {
    Libretranslate { endpoint: String, api_key: String },
    Ollama { endpoint: String, model: String },
}

impl TranslationAdapter {
    async fn translate(
        &self,
        client: &Client,
        text: &str,
        target_language: &str,
    ) -> Result<String> {
        match self {
            Self::Libretranslate { endpoint, api_key } => {
                let request = LibreTranslateRequest::new(
                    text,
                    libretranslate_language_code(target_language),
                    api_key,
                );
                let response = client
                    .post(format!("{}/translate", endpoint.trim_end_matches('/')))
                    .json(&request)
                    .send()
                    .await?;
                let status = response.status();
                let body: serde_json::Value = response.json().await?;
                if !status.is_success() {
                    return Err(anyhow!(
                        "LibreTranslate request failed ({}): {}",
                        status,
                        body.get("error")
                            .and_then(|value| value.as_str())
                            .unwrap_or("unknown error")
                    ));
                }
                parse_libretranslate_response(&body)
            }
            Self::Ollama { endpoint, model } => {
                let request = OllamaTranslationRequest::new(model, target_language, text);
                let response = client
                    .post(format!("{}/api/chat", endpoint.trim_end_matches('/')))
                    .json(&request)
                    .send()
                    .await?;
                let status = response.status();
                let body: serde_json::Value = response.json().await?;
                if !status.is_success() {
                    return Err(anyhow!("Ollama request failed ({}): {}", status, body));
                }
                parse_ollama_response(&body)
            }
        }
    }
}

pub struct TranslationSession {
    enabled: std::sync::atomic::AtomicBool,
    target_language: RwLock<String>,
    queue: TranslationQueue,
    adapter: TranslationAdapter,
    client: Client,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl TranslationSession {
    async fn from_settings<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Self>> {
        let settings =
            load_settings(app.state::<crate::state::AppState>().db_manager.pool()).await?;
        let engine = settings.engine.clone();
        if matches!(engine, TranslationEngineKind::Libretranslate)
            && settings.libretranslate_endpoint.trim().is_empty()
        {
            return Err(anyhow!("LibreTranslate endpoint is not configured"));
        }
        let adapter = match engine {
            TranslationEngineKind::Libretranslate => TranslationAdapter::Libretranslate {
                endpoint: settings.libretranslate_endpoint,
                api_key: settings.libretranslate_api_key,
            },
            TranslationEngineKind::Ollama => TranslationAdapter::Ollama {
                endpoint: settings.ollama_endpoint,
                model: settings.ollama_model,
            },
        };
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        let queue = TranslationQueue::new(TRANSLATION_QUEUE_CAPACITY);
        let receiver = queue
            .take_receiver()
            .ok_or_else(|| anyhow!("translation queue receiver already taken"))?;
        let session = Arc::new(Self {
            enabled: std::sync::atomic::AtomicBool::new(settings.enabled),
            target_language: RwLock::new(settings.target_language),
            queue,
            adapter,
            client,
            worker: Mutex::new(None),
        });
        let worker_session = session.clone();
        let worker_app = app.clone();
        let handle = tokio::spawn(async move {
            process_translation_queue(worker_app, worker_session, receiver).await;
        });
        *session.worker.lock().unwrap() = Some(handle);
        Ok(session)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled
            .store(enabled, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn target_language(&self) -> String {
        self.target_language.read().await.clone()
    }

    pub async fn set_target_language(&self, target_language: String) -> Result<()> {
        if target_language.trim().is_empty() {
            return Err(anyhow!("target language cannot be empty"));
        }
        *self.target_language.write().await = target_language;
        Ok(())
    }

    pub fn enqueue(&self, job: TranslationJob) -> TranslationRoute {
        self.queue.route(self.is_enabled(), true, job)
    }

    async fn close_and_wait(&self, timeout_duration: Duration) {
        self.queue.close();
        let handle = self.worker.lock().ok().and_then(|mut worker| worker.take());
        if let Some(mut handle) = handle {
            if tokio::time::timeout(timeout_duration, &mut handle)
                .await
                .is_err()
            {
                warn!(
                    "Translation drain exceeded {:?}; aborting pending work",
                    timeout_duration
                );
                handle.abort();
                let _ = handle.await;
            }
        }
    }
}

async fn process_translation_queue<R: Runtime>(
    app: AppHandle<R>,
    session: Arc<TranslationSession>,
    mut receiver: mpsc::Receiver<TranslationJob>,
) {
    while let Some(job) = receiver.recv().await {
        if !session.is_enabled() {
            let _ = app.emit(
                "translation-update",
                TranslationUpdate {
                    sequence_id: job.sequence_id,
                    translation: None,
                    target_language: job.target_language,
                    status: "disabled".to_string(),
                    error: None,
                },
            );
            continue;
        }
        let result = session
            .adapter
            .translate(&session.client, &job.text, &job.target_language)
            .await;
        let update = match result {
            Ok(translation) => TranslationUpdate {
                sequence_id: job.sequence_id,
                translation: Some(translation),
                target_language: job.target_language,
                status: "ready".to_string(),
                error: None,
            },
            Err(error) => {
                warn!(
                    "Translation failed for sequence {}: {}",
                    job.sequence_id, error
                );
                TranslationUpdate {
                    sequence_id: job.sequence_id,
                    translation: None,
                    target_language: job.target_language,
                    status: "error".to_string(),
                    error: Some(error.to_string()),
                }
            }
        };
        if let Err(error) = app.emit("translation-update", update) {
            error!("Failed to emit translation update: {}", error);
        }
    }
}

static ACTIVE_SESSION: LazyLock<Mutex<Option<Arc<TranslationSession>>>> =
    LazyLock::new(|| Mutex::new(None));

pub async fn start_active_session<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<Arc<TranslationSession>> {
    finish_active_session().await;
    let session = TranslationSession::from_settings(app).await?;
    *ACTIVE_SESSION
        .lock()
        .map_err(|_| anyhow!("translation session lock poisoned"))? = Some(session.clone());
    Ok(session)
}

pub fn active_session() -> Option<Arc<TranslationSession>> {
    ACTIVE_SESSION.lock().ok()?.clone()
}

pub fn set_active_enabled(enabled: bool) -> Result<()> {
    active_session()
        .ok_or_else(|| anyhow!("translation is not active"))?
        .set_enabled(enabled);
    Ok(())
}

pub async fn set_active_target(target_language: String) -> Result<()> {
    active_session()
        .ok_or_else(|| anyhow!("translation is not active"))?
        .set_target_language(target_language)
        .await
}

#[tauri::command]
pub async fn set_translation_enabled(enabled: bool) -> Result<(), String> {
    set_active_enabled(enabled).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_translation_target(target_language: String) -> Result<(), String> {
    set_active_target(target_language)
        .await
        .map_err(|error| error.to_string())
}

pub async fn finish_active_session() {
    let session = ACTIVE_SESSION
        .lock()
        .ok()
        .and_then(|mut guard| guard.take());
    if let Some(session) = session {
        session.close_and_wait(Duration::from_secs(15)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn routing_requires_enabled_final_segment_and_bounded_capacity() {
        let queue = TranslationQueue::new(1);
        let job = TranslationJob {
            sequence_id: 1,
            text: "hello".to_string(),
            target_language: "pt-BR".to_string(),
        };

        assert_eq!(
            queue.route(false, true, job.clone()),
            TranslationRoute::Disabled
        );
        assert_eq!(
            queue.route(true, false, job.clone()),
            TranslationRoute::NotFinal
        );
        assert_eq!(
            queue.route(true, true, job.clone()),
            TranslationRoute::Queued
        );
        assert_eq!(queue.route(true, true, job), TranslationRoute::Full);
    }

    #[test]
    fn ollama_adapter_uses_non_streaming_chat_and_extracts_message_content() {
        let request = OllamaTranslationRequest::new("aya-expanse:latest", "pt-BR", "Hello world");
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "model": "aya-expanse:latest",
                "messages": [
                    {"role": "system", "content": "Translate the user's text from English to pt-BR. Return only the translation, with no explanation."},
                    {"role": "user", "content": "Hello world"}
                ],
                "stream": false
            })
        );
        assert_eq!(
            parse_ollama_response(
                &json!({"message": {"role": "assistant", "content": "Olá mundo"}})
            )
            .unwrap(),
            "Olá mundo"
        );
    }

    #[test]
    fn libretranslate_adapter_uses_translate_contract_and_extracts_translated_text() {
        let request = LibreTranslateRequest::new("Hello world", "pt", "secret");
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({
                "q": "Hello world",
                "source": "auto",
                "target": "pt",
                "format": "text",
                "alternatives": 3,
                "api_key": "secret"
            })
        );
        assert_eq!(
            parse_libretranslate_response(&json!({"translatedText": "Olá mundo"})).unwrap(),
            "Olá mundo"
        );
    }

    #[tokio::test]
    async fn translation_settings_round_trip_keeps_summary_ollama_endpoint_isolated() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO settings (id, provider, model, whisperModel, ollamaEndpoint)
             VALUES ('1', 'ollama', 'summary-model', 'large-v3', 'http://summary:11434')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let settings = TranslationSettings {
            enabled: true,
            engine: TranslationEngineKind::Ollama,
            target_language: "pt-BR".to_string(),
            libretranslate_endpoint: String::new(),
            libretranslate_api_key: "secret".to_string(),
            ollama_endpoint: "http://translation:11434".to_string(),
            ollama_model: "aya-expanse:latest".to_string(),
        };
        save_settings(&pool, &settings).await.unwrap();

        let summary_endpoint: String =
            sqlx::query_scalar("SELECT ollamaEndpoint FROM settings WHERE id = '1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(summary_endpoint, "http://summary:11434");
        let loaded = load_settings(&pool).await.unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.ollama_endpoint, "http://translation:11434");
        assert_eq!(loaded.ollama_model, "aya-expanse:latest");
    }

    #[tokio::test]
    async fn saving_translation_settings_does_not_create_summary_defaults() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        save_settings(&pool, &TranslationSettings::default())
            .await
            .unwrap();

        let summary_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        let translation_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM translation_settings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(summary_rows, 0);
        assert_eq!(translation_rows, 1);
    }

    async fn mock_json_endpoint(
        response: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 1024];
            let content_length = loop {
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break 0;
                }
                request.extend_from_slice(&chunk[..count]);
                if let Some(header_end) =
                    request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|value| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        break length;
                    }
                }
            };
            let response_body = response.as_bytes();
            let response_headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                response_body.len()
            );
            socket.write_all(response_headers.as_bytes()).await.unwrap();
            socket.write_all(response_body).await.unwrap();
            let body_start = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap()
                + 4;
            format!(
                "{}:{}",
                String::from_utf8_lossy(&request[..body_start]),
                String::from_utf8_lossy(&request[body_start..body_start + content_length])
            )
        });
        (endpoint, handle)
    }

    #[tokio::test]
    async fn mocked_engine_flow_hits_provider_endpoint_and_returns_translation() {
        let (endpoint, server) = mock_json_endpoint(r#"{"translatedText":"Olá mundo"}"#).await;
        let client = Client::new();
        let adapter = TranslationAdapter::Libretranslate {
            endpoint: endpoint.clone(),
            api_key: "secret".to_string(),
        };
        let translated = adapter
            .translate(&client, "Hello world", "pt-BR")
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert!(request.contains("POST /translate"));
        assert!(request.contains("\"source\":\"auto\""));
        assert!(request.contains("\"target\":\"pt\""));
        assert!(request.contains("\"alternatives\":3"));
        assert!(request.contains("\"api_key\":\"secret\""));
        assert_eq!(translated, "Olá mundo");

        let (endpoint, server) =
            mock_json_endpoint(r#"{"message":{"role":"assistant","content":"Olá mundo"}}"#).await;
        let adapter = TranslationAdapter::Ollama {
            endpoint,
            model: "aya-expanse:latest".to_string(),
        };
        let translated = adapter
            .translate(&client, "Hello world", "pt-BR")
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert!(request.contains("POST /api/chat"));
        assert!(request.contains("\"stream\":false"));
        assert_eq!(translated, "Olá mundo");
    }
}
