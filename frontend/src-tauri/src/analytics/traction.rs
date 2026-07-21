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
// Static per-project token: spam protection at the ingest edge, not a secret
// capable of reading anything back (the dashboard sits behind Basic Auth).
const PROD_INGEST_TOKEN: &str = "beb4c8ebb4c216000a536e33584571680bbad34b4987a07b";

const FLUSH_INTERVAL_SECS: u64 = 30;
const FLUSH_AT: usize = 25;
// Ingest rejects batches >500; beyond this we drop oldest (offline cap).
const MAX_QUEUE: usize = 500;

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
    pub fn new() -> Arc<Self> {
        let endpoint = std::env::var("MEMENTO_STATS_URL").unwrap_or_else(|_| {
            if cfg!(debug_assertions) { DEV_INGEST_URL } else { PROD_INGEST_URL }.to_string()
        });
        let token = std::env::var("MEMENTO_STATS_INGEST_TOKEN").unwrap_or_else(|_| {
            if cfg!(debug_assertions) { String::new() } else { PROD_INGEST_TOKEN.to_string() }
        });

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

        sink
    }

    pub async fn track(&self, device_id: &str, name: &str, properties: HashMap<String, String>) {
        let mut queue = self.queue.lock().await;
        queue.push(TractionEvent {
            ts: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            device_id: device_id.to_string(),
            name: name.to_string(),
            properties,
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

    /// e2e против локально запущенного модуля (stats/server.py, порт 9901,
    /// STATS_INGEST_TOKEN=devtoken). В обычном прогоне не бегает:
    ///   cargo test -p meetily traction -- --ignored
    #[tokio::test]
    #[ignore]
    async fn sink_delivers_batch_to_local_module() {
        std::env::set_var("MEMENTO_STATS_URL", "http://127.0.0.1:9901/events");
        std::env::set_var("MEMENTO_STATS_INGEST_TOKEN", "devtoken");
        let sink = TractionSink::new();
        let mut props = HashMap::new();
        props.insert("total_duration_seconds".to_string(), "42".to_string());
        sink.track("user_e2e_test", "meeting_ended", props).await;
        sink.flush().await;
        assert!(sink.queue.lock().await.is_empty(), "flush must drain the queue on success");
    }
}
