//! Batched event sink for the Traction analytics hub (Memento module).
//!
//! Every event that passes through `AnalyticsClient::track_event` is also
//! queued here and flushed in batches to the Memento stats module ingest
//! (`stats/server.py` in this repo). The sink lives inside `AnalyticsClient`,
//! so it inherits the app's analytics opt-in: no consent — no client — no
//! events. Dev builds talk to a locally running module; release builds go to
//! the production hub.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

const PROD_INGEST_URL: &str = "https://stats.multitool.works/p/memento/events";
const DEV_INGEST_URL: &str = "http://127.0.0.1:9901/events";

const FLUSH_INTERVAL_SECS: u64 = 30;
const FLUSH_AT: usize = 25;
// Ingest rejects batches >500; beyond this we drop oldest (offline cap).
const MAX_QUEUE: usize = 500;

const SAFE_PROPERTY_KEYS: &[&str] = &[
    "accepted", "active_duration_seconds", "app_version", "architecture", "build_profile",
    "button", "category", "chunks_processed", "code", "copy_count_today", "copy_type",
    "date", "days_since_install", "device_category", "duration", "duration_seconds",
    "enabled", "error_category", "error_code", "error_type", "event_id", "feature",
    "feature_name", "file_size_bytes", "first_meeting_duration_seconds", "folder_type",
    "had_fatal_error", "has_microphone", "has_preferred_microphone",
    "has_preferred_system_audio", "has_system_audio", "import_kind", "is_auto_detect",
    "is_bluetooth", "language", "language_code", "latency_bucket", "location",
    "meeting_id_hash", "meetings_count", "memory_type", "microphone_device_type",
    "model_name", "model_provider", "new_model", "new_provider", "notifications_enabled",
    "old_model", "old_provider", "page", "pause_duration_seconds", "platform",
    "prompt_length", "provider", "release_channel", "result_count_bucket", "retryable",
    "scope", "segments_count", "session_duration", "session_id", "status", "success",
    "summary_model", "summary_provider", "system_audio_device_type",
    "time_since_recording_minutes", "total_duration_seconds", "transcript_length",
    "transcript_segments_count", "transcription_model", "transcription_provider", "workflow",
];

fn sanitize_properties(mut properties: HashMap<String, String>) -> HashMap<String, String> {
    properties.retain(|key, _| SAFE_PROPERTY_KEYS.contains(&key.as_str()));
    properties
}

#[derive(Serialize, Clone)]
struct TractionEvent {
    ts: f64,
    device_id: String,
    name: String,
    properties: HashMap<String, String>,
}

pub struct TractionSink {
    endpoint: String,
    token: String,
    http: reqwest::Client,
    queue: Arc<Mutex<Vec<TractionEvent>>>,
    kick: Arc<Notify>,
}

impl TractionSink {
    pub fn new() -> Option<Arc<Self>> {
        let endpoint = std::env::var("MEMENTO_STATS_URL").unwrap_or_else(|_| {
            if cfg!(debug_assertions) { DEV_INGEST_URL } else { PROD_INGEST_URL }.to_string()
        });
        // Release builds require build-time injection. A client credential is
        // only ingest spam protection, but committing the live value prevents
        // rotation and makes abuse needlessly easy.
        let token = std::env::var("MEMENTO_STATS_INGEST_TOKEN").unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                String::new()
            } else {
                option_env!("MEMENTO_STATS_INGEST_TOKEN")
                    .unwrap_or("")
                    .to_string()
            }
        });
        if !cfg!(debug_assertions) && token.is_empty() {
            log::warn!("Traction analytics disabled: release ingest token was not injected");
            return None;
        }

        let sink = Arc::new(Self {
            endpoint,
            token,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            queue: Arc::new(Mutex::new(Vec::new())),
            kick: Arc::new(Notify::new()),
        });

        // Воркер держит только Weak (плюс клон Notify на время ожидания):
        // когда disable_analytics дропает AnalyticsClient и с ним синк,
        // upgrade() после пробуждения проваливается — воркер выходит, НЕ
        // отправляя накопленную очередь (согласие уже отозвано), и не
        // копится при повторных init_analytics.
        let worker = Arc::downgrade(&sink);
        let kick = Arc::clone(&sink.kick);
        tauri::async_runtime::spawn(async move {
            loop {
                let timer = tokio::time::sleep(std::time::Duration::from_secs(FLUSH_INTERVAL_SECS));
                tokio::select! {
                    _ = timer => {}
                    _ = kick.notified() => {}
                }
                let Some(sink) = worker.upgrade() else { break };
                sink.flush().await;
            }
        });

        Some(sink)
    }

    pub async fn track(&self, device_id: &str, name: &str, properties: HashMap<String, String>) {
        let mut queue = self.queue.lock().await;
        queue.push(TractionEvent {
            ts: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            device_id: device_id.to_string(),
            name: name.to_string(),
            properties: sanitize_properties(properties),
        });
        if queue.len() > MAX_QUEUE {
            let drop = queue.len() - MAX_QUEUE;
            queue.drain(..drop);
        }
        let ready = queue.len() >= FLUSH_AT;
        drop(queue);
        if ready {
            self.kick.notify_one();
        }
    }

    pub async fn flush(&self) {
        let batch: Vec<TractionEvent> = {
            let mut queue = self.queue.lock().await;
            std::mem::take(&mut *queue)
        };
        if batch.is_empty() {
            return;
        }

        let mut req = self
            .http
            .post(&self.endpoint)
            .json(&serde_json::json!({ "events": batch }));
        if !self.token.is_empty() {
            req = req.header("X-Ingest-Token", &self.token);
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {}
            outcome => {
                match outcome {
                    Ok(resp) => log::warn!("Traction ingest rejected batch: {}", resp.status()),
                    Err(e) => log::debug!("Traction ingest unreachable: {}", e),
                }
                // Return the batch to the queue so the next flush retries it.
                let mut queue = self.queue.lock().await;
                let mut restored = batch;
                restored.extend(queue.drain(..));
                if restored.len() > MAX_QUEUE {
                    let drop = restored.len() - MAX_QUEUE;
                    restored.drain(..drop);
                }
                *queue = restored;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sink_keeps_metric_fields_and_drops_unapproved_content() {
        let sink = TractionSink::new().expect("debug sink");
        let mut props = HashMap::new();
        props.insert("event_id".to_string(), "event-123".to_string());
        props.insert("duration_seconds".to_string(), "42".to_string());
        props.insert("meeting_title".to_string(), "Private roadmap".to_string());
        props.insert("error_message".to_string(), "secret transcript".to_string());

        sink.track("device", "meeting_ended", props).await;
        let queue = sink.queue.lock().await;
        assert_eq!(queue[0].properties.get("event_id"), Some(&"event-123".to_string()));
        assert_eq!(queue[0].properties.get("duration_seconds"), Some(&"42".to_string()));
        assert!(!queue[0].properties.contains_key("meeting_title"));
        assert!(!queue[0].properties.contains_key("error_message"));
    }

    /// e2e против локально запущенного модуля (stats/server.py, порт 9901,
    /// STATS_INGEST_TOKEN=devtoken). В обычном прогоне не бегает:
    ///   cargo test -p meetily traction -- --ignored
    #[tokio::test]
    #[ignore]
    async fn sink_delivers_batch_to_local_module() {
        std::env::set_var("MEMENTO_STATS_URL", "http://127.0.0.1:9901/events");
        std::env::set_var("MEMENTO_STATS_INGEST_TOKEN", "devtoken");
        let sink = TractionSink::new().expect("debug sink");
        let mut props = HashMap::new();
        props.insert("total_duration_seconds".to_string(), "42".to_string());
        sink.track("user_e2e_test", "meeting_ended", props).await;
        sink.flush().await;
        assert!(sink.queue.lock().await.is_empty(), "flush must drain the queue on success");
    }
}
