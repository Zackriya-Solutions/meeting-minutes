//! System-wide short-form dictation.
//!
//! The module owns the dictation lifecycle. Platform, transcription, cleanup,
//! delivery, persistence, and UI integrations attach as adapters around this
//! interface so meeting recording remains independent.

mod activation;
mod delivery;
mod session;
#[cfg(target_os = "windows")]
mod windows_delivery;

pub use activation::{ActivationEvent, HoldShortcut, KeyCode, ShortcutTracker};
pub use delivery::{deliver_text, ClipboardPort, DeliveryError, DeliveryReceipt, PastePort};
pub use session::{
    DictationFailure, DictationFailureCode, DictationPhase, DictationSession,
    DictationTransitionError,
};
#[cfg(target_os = "windows")]
pub use windows_delivery::{WindowsClipboard, WindowsPaste};
