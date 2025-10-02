use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use log::{Level, Record};

/// Async logger for performance-critical audio processing
/// Buffers log messages and writes them asynchronously to avoid blocking audio threads
pub struct AsyncLogger {
    sender: mpsc::UnboundedSender<LogMessage>,
    _handle: JoinHandle<()>,
}

#[derive(Debug)]
struct LogMessage {
    level: Level,
    target: String,
    message: String,
    #[allow(dead_code)]
    timestamp: std::time::Instant,
}

impl AsyncLogger {
    /// Create a new async logger with specified buffer size
    pub fn new(buffer_size: usize) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<LogMessage>();

        // Spawn background task to process log messages
        let handle = tokio::spawn(async move {
            let mut buffered_messages = Vec::with_capacity(buffer_size);
            let mut last_flush = std::time::Instant::now();

            while let Some(message) = receiver.recv().await {
                buffered_messages.push(message);

                // Flush buffer when full or after timeout (100ms)
                if buffered_messages.len() >= buffer_size ||
                   last_flush.elapsed().as_millis() >= 100 {
                    Self::flush_messages(&mut buffered_messages);
                    last_flush = std::time::Instant::now();
                }
            }

            // Flush any remaining messages on shutdown
            if !buffered_messages.is_empty() {
                Self::flush_messages(&mut buffered_messages);
            }
        });

        Self {
            sender,
            _handle: handle,
        }
    }

    /// Log a message asynchronously (non-blocking)
    pub fn log(&self, level: Level, target: &str, message: String) {
        let log_msg = LogMessage {
            level,
            target: target.to_string(),
            message,
            timestamp: std::time::Instant::now(),
        };

        // Non-blocking send - if channel is full, drop the message to avoid blocking
        let _ = self.sender.send(log_msg);
    }

    /// Flush buffered messages to the actual logger
    fn flush_messages(messages: &mut Vec<LogMessage>) {
        for msg in messages.drain(..) {
            // Use the standard log crate to actually write the message
            log::logger().log(&Record::builder()
                .args(format_args!("{}", msg.message))
                .level(msg.level)
                .target(&msg.target)
                .build());
        }
    }
}

/// Thread-safe async logger instance for audio components
static ASYNC_LOGGER: once_cell::sync::OnceCell<Arc<AsyncLogger>> = once_cell::sync::OnceCell::new();

/// Initialize the global async logger (only if tokio runtime is available)
pub fn init_async_logger() {
    // Only initialize if we're in a tokio runtime context
    if tokio::runtime::Handle::try_current().is_ok() {
        let logger = AsyncLogger::new(1000); // Buffer up to 1000 messages
        ASYNC_LOGGER.set(Arc::new(logger)).ok();
    }
}

/// Get the global async logger instance (lazy initialization)
pub fn get_async_logger() -> Option<Arc<AsyncLogger>> {
    // Lazy initialization - only create logger when first needed and tokio runtime is available
    if ASYNC_LOGGER.get().is_none() && tokio::runtime::Handle::try_current().is_ok() {
        let logger = AsyncLogger::new(1000);
        let _ = ASYNC_LOGGER.set(Arc::new(logger));
    }
    ASYNC_LOGGER.get().cloned()
}

/// Macro for async debug logging in performance-critical paths
#[macro_export]
macro_rules! async_debug {
    ($($arg:tt)*) => {
        if let Some(logger) = $crate::audio::async_logger::get_async_logger() {
            logger.log(log::Level::Debug, module_path!(), format!($($arg)*));
        }
    };
}

/// Macro for async info logging that doesn't block
#[macro_export]
macro_rules! async_info {
    ($($arg:tt)*) => {
        if let Some(logger) = $crate::audio::async_logger::get_async_logger() {
            logger.log(log::Level::Info, module_path!(), format!($($arg)*));
        }
    };
}

/// Macro for async warning logging
#[macro_export]
macro_rules! async_warn {
    ($($arg:tt)*) => {
        if let Some(logger) = $crate::audio::async_logger::get_async_logger() {
            logger.log(log::Level::Warn, module_path!(), format!($($arg)*));
        }
    };
}