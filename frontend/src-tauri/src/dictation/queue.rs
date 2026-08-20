// dictation/queue.rs
//
// A small bounded, drop-oldest-on-overflow queue used to hand transcribed
// segments from the (synchronous, hot-path) transcription worker over to the
// dedicated AT-SPI injector task, without ever blocking the worker.
//
// This exists instead of a plain `tokio::sync::mpsc` channel because Fix 4 of
// the live-dictation plan requires that a full queue drop the OLDEST queued
// segment (not reject the newest), which a bounded mpsc channel cannot do:
// `Sender::try_send` on a full channel simply fails, it cannot evict the
// receiver's queued head. `push` is synchronous and non-blocking so it is
// safe to call directly from worker.rs's serial transcription loop.

use parking_lot::Mutex;
use std::collections::VecDeque;
use tokio::sync::Notify;

pub struct DictationQueue {
    inner: Mutex<QueueState>,
    capacity: usize,
    notify: Notify,
}

struct QueueState {
    items: VecDeque<String>,
    closed: bool,
}

impl DictationQueue {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "DictationQueue capacity must be non-zero");
        Self {
            inner: Mutex::new(QueueState {
                items: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            capacity,
            notify: Notify::new(),
        }
    }

    /// Push a segment onto the queue. Non-blocking. If the queue is at
    /// capacity, the oldest queued segment is dropped (and logged) to make
    /// room, rather than blocking or rejecting the new segment.
    pub fn push(&self, text: String) {
        let mut state = self.inner.lock();
        if state.closed {
            return;
        }
        if state.items.len() >= self.capacity {
            if let Some(dropped) = state.items.pop_front() {
                log::warn!(
                    "Dictation queue full (capacity {}); dropping oldest queued segment: {:?}",
                    self.capacity,
                    dropped
                );
            }
        }
        state.items.push_back(text);
        drop(state);
        self.notify.notify_one();
    }

    /// Wait for and remove the oldest queued segment. Returns `None` once the
    /// queue has been closed and drained.
    pub async fn pop(&self) -> Option<String> {
        loop {
            {
                let mut state = self.inner.lock();
                if let Some(text) = state.items.pop_front() {
                    return Some(text);
                }
                if state.closed {
                    return None;
                }
            }
            self.notify.notified().await;
        }
    }

    /// Discard any queued segments without processing them (used when
    /// (re)starting dictation so stale segments from a prior session are not
    /// injected).
    pub fn clear(&self) {
        self.inner.lock().items.clear();
    }

    /// Mark the queue closed and wake any waiting consumer; `pop` will drain
    /// remaining items then return `None`.
    pub fn close(&self) {
        self.inner.lock().closed = true;
        self.notify.notify_waiters();
    }

    /// Reopen a previously closed queue for a new dictation session.
    pub fn reopen(&self) {
        self.inner.lock().closed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_preserves_fifo_order() {
        let queue = DictationQueue::new(4);
        queue.push("one".to_string());
        queue.push("two".to_string());
        queue.push("three".to_string());

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert_eq!(queue.pop().await.as_deref(), Some("one"));
            assert_eq!(queue.pop().await.as_deref(), Some("two"));
            assert_eq!(queue.pop().await.as_deref(), Some("three"));
        });
    }

    #[test]
    fn overflow_drops_oldest_not_newest() {
        let queue = DictationQueue::new(2);
        queue.push("a".to_string());
        queue.push("b".to_string());
        // Queue is now full at capacity 2 with ["a", "b"]; pushing "c" must
        // evict "a" (the oldest), keeping "b" and "c".
        queue.push("c".to_string());

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert_eq!(queue.pop().await.as_deref(), Some("b"));
            assert_eq!(queue.pop().await.as_deref(), Some("c"));
        });
    }

    #[test]
    fn close_drains_then_returns_none() {
        let queue = DictationQueue::new(4);
        queue.push("only".to_string());
        queue.close();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            // Draining a segment queued before close() still succeeds.
            assert_eq!(queue.pop().await.as_deref(), Some("only"));
            // Once drained, a closed queue yields None instead of hanging.
            assert_eq!(queue.pop().await, None);
        });
    }

    #[test]
    fn push_after_close_is_ignored() {
        let queue = DictationQueue::new(4);
        queue.close();
        queue.push("dropped".to_string());

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert_eq!(queue.pop().await, None);
        });
    }

    #[test]
    fn reopen_allows_a_new_session() {
        let queue = DictationQueue::new(4);
        queue.push("first-session".to_string());
        queue.close();
        queue.clear();
        queue.reopen();
        queue.push("second-session".to_string());

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert_eq!(queue.pop().await.as_deref(), Some("second-session"));
        });
    }
}
