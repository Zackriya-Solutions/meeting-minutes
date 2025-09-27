//! Compatibility layer between legacy and modern audio systems
//! 
//! This module provides a bridge that allows seamless switching between
//! the old audio system and the new modern system.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::{ModernAudioSystem, AudioConfig};
use crate::audio::recording_saver::RecordingSaver;
use crate::audio::recording_state::{AudioChunk, RecordingState};

/// Bridge between legacy and modern audio systems
pub struct LegacyBridge {
    legacy_saver: Option<RecordingSaver>,
    modern_system: Option<ModernAudioSystem>,
    mode: AudioMode,
}

/// Audio system mode
#[derive(Debug, Clone)]
pub enum AudioMode {
    /// Use the legacy audio system
    Legacy,
    /// Use the modern audio system
    Modern,
    /// Run both systems in parallel for comparison
    Hybrid,
}

impl LegacyBridge {
    /// Create a new bridge with the specified mode
    pub fn new(mode: AudioMode) -> Self {
        Self {
            legacy_saver: None,
            modern_system: None,
            mode,
        }
    }

    /// Initialize the bridge based on the current mode
    pub async fn initialize(&mut self) -> Result<()> {
        match self.mode {
            AudioMode::Legacy => {
                self.legacy_saver = Some(RecordingSaver::new());
                log::info!("Initialized legacy audio system");
            }
            AudioMode::Modern => {
                self.modern_system = Some(ModernAudioSystem::new());
                if let Some(ref mut system) = self.modern_system {
                    system.initialize().await?;
                }
                log::info!("Initialized modern audio system");
            }
            AudioMode::Hybrid => {
                self.legacy_saver = Some(RecordingSaver::new());
                self.modern_system = Some(ModernAudioSystem::new());
                if let Some(ref mut system) = self.modern_system {
                    system.initialize().await?;
                }
                log::info!("Initialized hybrid audio system (both legacy and modern)");
            }
        }
        Ok(())
    }

    /// Start recording using the appropriate system
    pub async fn start_recording<R: tauri::Runtime>(
        &mut self,
        app: &tauri::AppHandle<R>,
    ) -> Result<mpsc::UnboundedSender<AudioChunk>> {
        match self.mode {
            AudioMode::Legacy => {
                if let Some(ref mut saver) = self.legacy_saver {
                    let sender = saver.start_accumulation();
                    log::info!("Started recording with legacy system");
                    Ok(sender)
                } else {
                    Err(anyhow::anyhow!("Legacy saver not initialized"))
                }
            }
            AudioMode::Modern => {
                if let Some(ref mut system) = self.modern_system {
                    system.start_recording().await?;
                    // TODO: Return a sender for the modern system
                    // This will be implemented when we create the modern recorder
                    Err(anyhow::anyhow!("Modern system sender not yet implemented"))
                } else {
                    Err(anyhow::anyhow!("Modern system not initialized"))
                }
            }
            AudioMode::Hybrid => {
                // Start both systems
                let legacy_sender = if let Some(ref mut saver) = self.legacy_saver {
                    let sender = saver.start_accumulation();
                    log::info!("Started recording with legacy system");
                    Some(sender)
                } else {
                    None
                };

                if let Some(ref mut system) = self.modern_system {
                    system.start_recording().await?;
                    log::info!("Started recording with modern system");
                }

                // For hybrid mode, we'll use the legacy sender for now
                // In the future, we'll create a multiplexer that sends to both
                legacy_sender.ok_or_else(|| anyhow::anyhow!("Failed to start legacy recording"))
            }
        }
    }

    /// Stop recording and return the file path(s)
    pub async fn stop_recording<R: tauri::Runtime>(
        &mut self,
        app: &tauri::AppHandle<R>,
    ) -> Result<Option<String>> {
        match self.mode {
            AudioMode::Legacy => {
                if let Some(ref mut saver) = self.legacy_saver {
                    let result = saver.stop_and_save(app).await;
                    log::info!("Stopped recording with legacy system");
                    result.map_err(|e| anyhow::anyhow!("Legacy recording failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Legacy saver not initialized"))
                }
            }
            AudioMode::Modern => {
                if let Some(ref mut system) = self.modern_system {
                    let result = system.stop_recording().await;
                    log::info!("Stopped recording with modern system");
                    result.map_err(|e| anyhow::anyhow!("Modern recording failed: {}", e))
                } else {
                    Err(anyhow::anyhow!("Modern system not initialized"))
                }
            }
            AudioMode::Hybrid => {
                // Stop both systems and return the modern system result
                let legacy_result = if let Some(ref mut saver) = self.legacy_saver {
                    saver.stop_and_save(app).await.ok()
                } else {
                    None
                };

                let modern_result = if let Some(ref mut system) = self.modern_system {
                    system.stop_recording().await.ok()
                } else {
                    None
                };

                log::info!("Stopped recording with both systems");
                
                // For now, return the modern result if available, otherwise legacy
                Ok(modern_result.or(legacy_result).flatten())
            }
        }
    }

    /// Get the current mode
    pub fn mode(&self) -> &AudioMode {
        &self.mode
    }

    /// Switch to a different mode
    pub async fn switch_mode(&mut self, new_mode: AudioMode) -> Result<()> {
        log::info!("Switching audio mode from {:?} to {:?}", self.mode, new_mode);
        self.mode = new_mode;
        self.initialize().await
    }

    /// Get audio quality metrics (only available in modern mode)
    pub fn get_quality_metrics(&self) -> Option<AudioQualityMetrics> {
        match self.mode {
            AudioMode::Modern | AudioMode::Hybrid => {
                // TODO: Implement quality metrics
                Some(AudioQualityMetrics::default())
            }
            AudioMode::Legacy => None,
        }
    }
}

/// Audio quality metrics for monitoring
#[derive(Debug, Clone, Default)]
pub struct AudioQualityMetrics {
    /// Sync accuracy in milliseconds
    pub sync_accuracy_ms: f64,
    /// Peak level (0.0 to 1.0)
    pub peak_level: f32,
    /// RMS level (0.0 to 1.0)
    pub rms_level: f32,
    /// LUFS level for EBU R128 compliance
    pub lufs_level: f64,
    /// True peak level
    pub true_peak_level: f32,
    /// Number of clipping events
    pub clipping_events: u32,
}

impl Default for LegacyBridge {
    fn default() -> Self {
        Self::new(AudioMode::Legacy)
    }
}

/// Feature flag helper functions
pub mod feature_flags {
    /// Check if legacy audio is enabled
    pub fn is_legacy_enabled() -> bool {
        cfg!(feature = "legacy-audio")
    }

    /// Check if modern audio is enabled
    pub fn is_modern_enabled() -> bool {
        cfg!(feature = "modern-audio")
    }

    /// Check if hybrid mode is enabled
    pub fn is_hybrid_enabled() -> bool {
        cfg!(feature = "hybrid-mode")
    }

    /// Get the default audio mode based on feature flags
    pub fn default_mode() -> super::AudioMode {
        if is_hybrid_enabled() {
            super::AudioMode::Hybrid
        } else if is_modern_enabled() {
            super::AudioMode::Modern
        } else {
            super::AudioMode::Legacy
        }
    }
}
