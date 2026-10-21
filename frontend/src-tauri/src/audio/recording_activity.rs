//! Shared recording lifecycle policy, kept independent of audio/runtime dependencies.
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn active(starting: &AtomicBool, recording: &AtomicBool, stopping: &AtomicBool) -> bool {
    // Read in handoff order: ACTIVE is set before STARTING clears, and
    // STOPPING is set before ACTIVE clears at the end of capture shutdown.
    starting.load(Ordering::SeqCst)
        || recording.load(Ordering::SeqCst)
        || stopping.load(Ordering::SeqCst)
}

pub(crate) struct StartingGuard<'a>(&'a AtomicBool);
impl<'a> StartingGuard<'a> {
    pub(crate) fn acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self(flag))
    }
}
impl Drop for StartingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

// Both lifecycle phases need exclusive ownership and release-on-error semantics.
pub(crate) type StoppingGuard<'a> = StartingGuard<'a>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_stop_cannot_release_first_stops_quit_protection() {
        let starting = AtomicBool::new(false);
        let recording = AtomicBool::new(false);
        let stopping = AtomicBool::new(false);
        let first = StoppingGuard::acquire(&stopping).unwrap();
        let second = StoppingGuard::acquire(&stopping);
        let duplicate_was_rejected = second.is_none();
        drop(second);
        assert!(
            active(&starting, &recording, &stopping),
            "first stop still saving after second returns"
        );
        assert!(duplicate_was_rejected);
        drop(first);
        assert!(!active(&starting, &recording, &stopping));
    }

    #[test]
    fn starting_and_stop_tail_protect_the_same_exit_guard_as_capture() {
        for (starting, recording, stopping, expected) in [
            (false, false, false, false),
            (true, false, false, true),
            (false, true, false, true),
            (false, false, true, true),
            (true, true, false, true),
            (false, true, true, true),
        ] {
            assert_eq!(
                active(
                    &AtomicBool::new(starting),
                    &AtomicBool::new(recording),
                    &AtomicBool::new(stopping)
                ),
                expected
            );
        }
    }

    #[test]
    fn failed_or_unwinding_start_releases_guard_and_reentrant_start_is_rejected() {
        let starting = AtomicBool::new(false);
        let first = StartingGuard::acquire(&starting).unwrap();
        assert!(starting.load(Ordering::SeqCst));
        assert!(StartingGuard::acquire(&starting).is_none());
        drop(first);
        assert!(!starting.load(Ordering::SeqCst));
        let panic = std::panic::catch_unwind(|| {
            let _guard = StartingGuard::acquire(&starting).unwrap();
            panic!("simulated capture/model failure");
        });
        assert!(panic.is_err());
        assert!(!starting.load(Ordering::SeqCst));
        assert!(StartingGuard::acquire(&starting).is_some());
    }

    #[test]
    fn successful_start_hands_protection_to_capture_then_stop_tail() {
        let starting = AtomicBool::new(false);
        let recording = AtomicBool::new(false);
        let stopping = AtomicBool::new(false);
        let guard = StartingGuard::acquire(&starting).unwrap();
        assert!(active(&starting, &recording, &stopping));
        recording.store(true, Ordering::SeqCst);
        drop(guard);
        assert!(active(&starting, &recording, &stopping));
        stopping.store(true, Ordering::SeqCst);
        recording.store(false, Ordering::SeqCst);
        assert!(active(&starting, &recording, &stopping));
        stopping.store(false, Ordering::SeqCst);
        assert!(!active(&starting, &recording, &stopping));
    }
}
