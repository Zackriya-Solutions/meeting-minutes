// dictation/manager.rs
//
// Orchestrates the live dictation feature: owns the queue that worker.rs
// feeds (Fix 4: decoupled from the serial transcription critical path via a
// channel + separate task, never called inline), starts/stops the AT-SPI
// injector task on Linux, and implements the clipboard+notification fallback
// (Fix 6: never silently destroys the user's existing clipboard contents).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::RwLock;

use super::queue::DictationQueue;
use super::types::DictationState;

#[cfg(target_os = "linux")]
use super::atspi_injector;

/// Default queue capacity: how many not-yet-injected segments may be queued
/// before the oldest is dropped (see `DictationQueue`). Segments are short
/// (a few seconds of speech each), so a small capacity is enough headroom
/// for a brief AT-SPI hiccup without ever meaningfully lagging real time.
const QUEUE_CAPACITY: usize = 8;

/// How long the fallback clipboard write is held before the user's prior
/// clipboard contents are restored (Fix 6).
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum DictationError {
    #[error("live dictation is not supported on this platform")]
    UnsupportedPlatform,
    #[error("failed to connect to the AT-SPI accessibility bus: {0}")]
    ConnectionFailed(String),
}

/// The queue + active flag shared between worker.rs's hot path and the
/// manager. Always present in app-managed state (even when dictation is
/// off), so worker.rs's check is a single atomic load in the common case.
pub struct DictationBridge {
    pub queue: DictationQueue,
    pub active: AtomicBool,
}

impl DictationBridge {
    pub fn new() -> Self {
        Self {
            queue: DictationQueue::new(QUEUE_CAPACITY),
            active: AtomicBool::new(false),
        }
    }
}

pub type DictationBridgeState = Arc<DictationBridge>;

/// Shared handle used by the injector task to route a failed segment to the
/// clipboard fallback without needing to lock the whole manager.
#[derive(Clone)]
struct FallbackHandler<R: Runtime> {
    app_handle: AppHandle<R>,
    state: Arc<RwLock<DictationState>>,
}

impl<R: Runtime> FallbackHandler<R> {
    async fn handle(&self, text: String, reason: &str) {
        *self.state.write().await = DictationState::InjectFailedFallback;

        let clipboard = self.app_handle.clipboard();

        // Fix 6: read and hold the user's existing clipboard content before
        // overwriting it, so it can be restored afterwards.
        let prior_clipboard = match clipboard.read_text() {
            Ok(prior) => Some(prior),
            Err(e) => {
                log::debug!("Dictation fallback: no readable prior clipboard content: {}", e);
                None
            }
        };

        match clipboard.write_text(text) {
            Ok(()) => {
                log::info!("Dictation fallback: copied segment to clipboard ({})", reason);
            }
            Err(e) => {
                log::error!("Dictation fallback: failed to write clipboard: {}", e);
                return;
            }
        }

        let restore_secs = CLIPBOARD_RESTORE_DELAY.as_secs();
        let body = format!(
            "Couldn't type into the focused field ({reason}). The dictated text was copied to \
             your clipboard instead -- paste it now. Your previous clipboard content will be \
             restored in {restore_secs} seconds."
        );
        if let Err(e) = self
            .app_handle
            .notification()
            .builder()
            .title("Live Dictation")
            .body(&body)
            .show()
        {
            log::warn!("Dictation fallback: failed to show notification: {}", e);
        }

        let app_for_restore = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(CLIPBOARD_RESTORE_DELAY).await;
            let clipboard = app_for_restore.clipboard();
            let restore_result = match prior_clipboard {
                Some(prior_text) => clipboard.write_text(prior_text),
                None => clipboard.clear(),
            };
            if let Err(e) = restore_result {
                log::warn!("Dictation fallback: failed to restore prior clipboard: {}", e);
            }
        });
    }
}

/// Orchestrates the dictation feature's lifecycle. Cheap to clone (all state
/// is `Arc`-backed); the clone held by the injector task can call back into
/// `state` without contending with `start()`/`stop()` on the original held
/// in app-managed state.
#[derive(Clone)]
pub struct DictationManager<R: Runtime> {
    app_handle: AppHandle<R>,
    bridge: DictationBridgeState,
    state: Arc<RwLock<DictationState>>,
    #[cfg(target_os = "linux")]
    tasks: Arc<parking_lot::Mutex<Vec<tauri::async_runtime::JoinHandle<()>>>>,
}

impl<R: Runtime> DictationManager<R> {
    pub fn new(app_handle: AppHandle<R>, bridge: DictationBridgeState) -> Self {
        Self {
            app_handle,
            bridge,
            state: Arc::new(RwLock::new(DictationState::Idle)),
            #[cfg(target_os = "linux")]
            tasks: Arc::new(parking_lot::Mutex::new(Vec::new())),
        }
    }

    pub async fn state(&self) -> DictationState {
        *self.state.read().await
    }

    pub fn is_active(&self) -> bool {
        self.bridge.active.load(Ordering::SeqCst)
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn start(&self) -> Result<(), DictationError> {
        Err(DictationError::UnsupportedPlatform)
    }

    #[cfg(target_os = "linux")]
    pub async fn start(&self) -> Result<(), DictationError> {
        if self.is_active() {
            return Ok(());
        }

        let connection = atspi_injector::connect()
            .await
            .map_err(|e| DictationError::ConnectionFailed(e.to_string()))?;

        let cache = atspi_injector::FocusCache::new();
        let focus_task = atspi_injector::spawn_focus_tracker(connection.clone(), cache.clone());

        self.bridge.queue.reopen();
        self.bridge.queue.clear();

        let bridge = self.bridge.clone();
        let fallback = FallbackHandler {
            app_handle: self.app_handle.clone(),
            state: self.state.clone(),
        };
        let injector_connection = connection.clone();
        let injector_cache = cache.clone();
        let injector_task = tauri::async_runtime::spawn(async move {
            loop {
                let Some(text) = bridge.queue.pop().await else {
                    break;
                };
                match atspi_injector::inject_segment(&injector_connection, &injector_cache, &text)
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        log::warn!("Dictation: injection failed, using clipboard fallback: {}", e);
                        fallback.handle(text, &e.to_string()).await;
                    }
                }
            }
        });

        *self.tasks.lock() = vec![focus_task, injector_task];
        self.bridge.active.store(true, Ordering::SeqCst);
        *self.state.write().await = DictationState::Listening;

        log::info!("Dictation: started");
        Ok(())
    }

    pub async fn stop(&self) {
        if !self.is_active() {
            return;
        }

        self.bridge.active.store(false, Ordering::SeqCst);
        self.bridge.queue.close();

        #[cfg(target_os = "linux")]
        {
            let tasks: Vec<_> = self.tasks.lock().drain(..).collect();
            for task in tasks {
                task.abort();
            }
        }

        *self.state.write().await = DictationState::Idle;
        log::info!("Dictation: stopped");
    }
}

pub type DictationManagerState<R> = Arc<DictationManager<R>>;
