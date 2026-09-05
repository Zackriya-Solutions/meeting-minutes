//! Lock-free storage for the live microphone level.
//!
//! The audio capture callback writes the latest RMS/peak here and the
//! floating recording indicator reads it on a timer to drive its waveform.

use std::sync::atomic::{AtomicU32, Ordering};

static MIC_RMS: AtomicU32 = AtomicU32::new(0);
static MIC_PEAK: AtomicU32 = AtomicU32::new(0);

/// Store the latest microphone level (called from the audio callback).
pub fn update(rms: f32, peak: f32) {
    MIC_RMS.store(rms.to_bits(), Ordering::Relaxed);
    MIC_PEAK.store(peak.to_bits(), Ordering::Relaxed);
}

/// Read the latest microphone level as `(rms, peak)`.
pub fn get() -> (f32, f32) {
    (
        f32::from_bits(MIC_RMS.load(Ordering::Relaxed)),
        f32::from_bits(MIC_PEAK.load(Ordering::Relaxed)),
    )
}

/// Reset levels to silence (called when recording stops).
pub fn reset() {
    update(0.0, 0.0);
}
