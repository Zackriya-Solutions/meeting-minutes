// Live dictation module (issue #719): transcribe speech directly into
// whichever text field currently has OS focus, via the AT-SPI2 accessibility
// bus on Linux. See local://plan-live-dictation-v2.md for the design.

pub mod commands;
pub mod manager;
pub mod queue;
pub mod settings;
pub mod types;

// AT-SPI is Linux-only; gate the submodule declaration itself per repo
// convention (see `tray.rs`'s cfg-gating precedent), not with internal
// `#[cfg]` sprinkled through a shared file.
#[cfg(target_os = "linux")]
pub mod atspi_injector;

pub use manager::{DictationBridge, DictationBridgeState, DictationManager, DictationManagerState};
pub use types::{DictationSettings, DictationState};
