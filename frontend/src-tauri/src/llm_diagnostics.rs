//! Wave 28 / PR-45a: in-memory LLM failure diagnostics.
//!
//! Aggregates typed `PostprocessError.code` values emitted by
//! `llm_postprocess::map_llm_error` and the result of the settings
//! "Test connection" probe. The list is bounded (most recent 200
//! entries) and never persisted to SQLite; this matches the semantics
//! of `HotwordHitStatsPanel` and avoids leaking secrets or provider
//! messages at rest.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

pub const DIAGNOSTICS_CAP: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEntry {
    pub ts: u64,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticBucket {
    pub code: String,
    pub count: usize,
    pub last_message: String,
    pub last_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTestResult {
    pub ok: bool,
    pub latency_ms: u128,
    pub code: Option<String>,
    pub message: Option<String>,
    pub ts: u64,
}

impl LastTestResult {
    pub fn ok(latency_ms: u128) -> Self {
        Self { ok: true, latency_ms, code: None, message: None, ts: now_ts() }
    }
    pub fn failed(latency_ms: u128, code: &str, message: &str) -> Self {
        Self {
            ok: false,
            latency_ms,
            code: Some(code.to_string()),
            message: Some(message.to_string()),
            ts: now_ts(),
        }
    }
}

/// Snapshot returned to the frontend on every diagnostics refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub buckets: Vec<DiagnosticBucket>,
    pub last_test: Option<LastTestResult>,
}

#[derive(Default)]
pub struct LLMDiagnosticsState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    entries: VecDeque<DiagnosticEntry>,
    last_test: Option<LastTestResult>,
}

impl LLMDiagnosticsState {
    pub fn record_failure(&self, code: &str, message: &str) {
        if let Ok(mut g) = self.inner.lock() {
            if g.entries.len() >= DIAGNOSTICS_CAP {
                g.entries.pop_front();
            }
            g.entries.push_back(DiagnosticEntry {
                ts: now_ts(),
                code: code.to_string(),
                message: message.to_string(),
            });
        }
    }

    pub fn set_last_test(&self, result: LastTestResult) {
        if let Ok(mut g) = self.inner.lock() {
            g.last_test = Some(result);
        }
    }

    pub fn last_test(&self) -> Option<LastTestResult> {
        self.inner.lock().ok().and_then(|g| g.last_test.clone())
    }

    pub fn buckets(&self) -> Vec<DiagnosticBucket> {
        let mut agg: BTreeMap<String, DiagnosticBucket> = BTreeMap::new();
        if let Ok(g) = self.inner.lock() {
            for entry in g.entries.iter() {
                let bucket = agg.entry(entry.code.clone()).or_insert_with(|| DiagnosticBucket {
                    code: entry.code.clone(),
                    count: 0,
                    last_message: String::new(),
                    last_ts: 0,
                });
                bucket.count += 1;
                if entry.ts >= bucket.last_ts {
                    bucket.last_ts = entry.ts;
                    bucket.last_message = entry.message.clone();
                }
            }
        }
        let mut out: Vec<DiagnosticBucket> = agg.into_values().collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then(b.last_ts.cmp(&a.last_ts)));
        out
    }

    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.entries.clear();
            // last_test intentionally preserved so the user keeps seeing
            // the most recent manual probe result after a bulk clear.
        }
    }

    pub fn clear_all(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.entries.clear();
            g.last_test = None;
        }
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Frontend-driven panel refresh entry point. Cheap (Mutex snapshot).
#[tauri::command]
pub fn get_llm_diagnostics(
    state: tauri::State<'_, LLMDiagnosticsState>,
) -> DiagnosticsSnapshot {
    DiagnosticsSnapshot {
        buckets: state.buckets(),
        last_test: state.last_test(),
    }
}

/// Drop every recorded entry and the cached last-test result, then
/// notify the frontend so the panel can clear its row list.
#[tauri::command]
pub fn clear_llm_diagnostics(
    app: tauri::AppHandle,
    state: tauri::State<'_, LLMDiagnosticsState>,
) -> Result<DiagnosticsSnapshot, String> {
    state.clear_all();
    let snap = DiagnosticsSnapshot {
        buckets: state.buckets(),
        last_test: state.last_test(),
    };
    app.emit("llm-diagnostics-updated", &snap)
        .map_err(|e| format!("emit llm-diagnostics-updated failed: {e}"))?;
    Ok(snap)
}

#[cfg(test)]
mod tests;
