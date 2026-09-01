//! System-wide short-form dictation.
//!
//! The module owns the dictation lifecycle. Platform, transcription, cleanup,
//! delivery, persistence, and UI integrations attach as adapters around this
//! interface so meeting recording remains independent.

mod activation;
mod activation_bus;
pub mod commands;
mod coordinator;
mod delivery;
mod history;
mod session;
mod short_audio;
#[cfg(target_os = "windows")]
mod windows_delivery;

pub use activation::{ActivationEvent, HoldShortcut, KeyCode, ShortcutTracker};
pub use activation_bus::ActivationBus;
pub use coordinator::{position_overlay, start_coordinator};
pub use delivery::{deliver_text, ClipboardPort, DeliveryError, DeliveryReceipt, PastePort};
pub use session::{
    DictationFailure, DictationFailureCode, DictationPhase, DictationSession,
    DictationTransitionError,
};
pub use short_audio::{ShortAudioCapture, ShortAudioError};
#[cfg(target_os = "windows")]
pub use windows_delivery::{WindowsClipboard, WindowsPaste};
