//! Professional audio mixing
//! 
//! This module provides dynamic audio mixing capabilities based on real-time
//! analysis, replacing the fixed 60%/40% mixing ratio.

use anyhow::Result;
use std::collections::VecDeque;

/// Professional audio mixer with dynamic level analysis
pub struct AudioMixer {
    rms_analyzer: RmsAnalyzer,
    ducking_processor: DuckingProcessor,
    crossfade_processor: CrossfadeProcessor,
    mixing_mode: MixingMode,
    history_buffer: VecDeque<f32>,
    history_size: usize,
}

/// Audio mixing modes
#[derive(Debug, Clone)]
pub enum MixingMode {
    /// Fixed ratio mixing (legacy behavior)
    Fixed { mic_ratio: f32, system_ratio: f32 },
    /// Dynamic mixing based on audio levels
    Dynamic,
    /// Professional ducking and crossfading
    Professional,
}

/// RMS analyzer for real-time audio level detection
struct RmsAnalyzer {
    window_size: usize,
    buffer: VecDeque<f32>,
    sum_squares: f32,
}

/// Ducking processor for automatic level adjustment
struct DuckingProcessor {
    threshold: f32,
    attack_time: f32,
    release_time: f32,
    current_gain: f32,
    target_gain: f32,
}

/// Crossfade processor for smooth transitions
struct CrossfadeProcessor {
    fade_length: usize,
    fade_buffer: VecDeque<f32>,
}

impl AudioMixer {
    /// Create a new professional audio mixer
    pub fn new(mixing_mode: MixingMode) -> Self {
        Self {
            rms_analyzer: RmsAnalyzer::new(1024), // 1024 sample window
            ducking_processor: DuckingProcessor::new(0.1, 0.01, 0.1), // 10% threshold, 10ms attack, 100ms release
            crossfade_processor: CrossfadeProcessor::new(256), // 256 sample crossfade
            mixing_mode,
            history_buffer: VecDeque::with_capacity(2048),
            history_size: 2048,
        }
    }

    /// Mix microphone and system audio with professional processing
    pub fn mix(&mut self, mic: &[f32], system: &[f32]) -> Vec<f32> {
        let max_len = mic.len().max(system.len());
        let mut mixed = Vec::with_capacity(max_len);

        match self.mixing_mode {
            MixingMode::Fixed { mic_ratio, system_ratio } => {
                // Fixed ratio mixing (legacy behavior)
                for i in 0..max_len {
                    let mic_sample = if i < mic.len() { mic[i] } else { 0.0 };
                    let system_sample = if i < system.len() { system[i] } else { 0.0 };
                    mixed.push(mic_sample * mic_ratio + system_sample * system_ratio);
                }
            }
            MixingMode::Dynamic => {
                // Dynamic mixing based on real-time analysis
                let mic_rms = self.rms_analyzer.analyze(mic);
                let system_rms = self.rms_analyzer.analyze(system);
                
                let (mic_ratio, system_ratio) = self.calculate_dynamic_ratios(mic_rms, system_rms);
                
                for i in 0..max_len {
                    let mic_sample = if i < mic.len() { mic[i] } else { 0.0 };
                    let system_sample = if i < system.len() { system[i] } else { 0.0 };
                    mixed.push(mic_sample * mic_ratio + system_sample * system_ratio);
                }
            }
            MixingMode::Professional => {
                // Professional mixing with ducking and crossfading
                for i in 0..max_len {
                    let mic_sample = if i < mic.len() { mic[i] } else { 0.0 };
                    let system_sample = if i < system.len() { system[i] } else { 0.0 };
                    
                    // Apply ducking
                    let ducked_mic = self.ducking_processor.process(mic_sample, system_sample);
                    
                    // Apply crossfade
                    let crossfaded = self.crossfade_processor.process(ducked_mic, system_sample);
                    
                    mixed.push(crossfaded);
                }
            }
        }

        // Update history buffer for analysis
        for &sample in &mixed {
            self.history_buffer.push_back(sample);
            if self.history_buffer.len() > self.history_size {
                self.history_buffer.pop_front();
            }
        }

        mixed
    }

    /// Calculate dynamic mixing ratios based on RMS levels
    fn calculate_dynamic_ratios(&self, mic_rms: f32, system_rms: f32) -> (f32, f32) {
        if mic_rms == 0.0 && system_rms == 0.0 {
            return (0.5, 0.5); // Equal mix for silence
        }

        if mic_rms == 0.0 {
            return (0.0, 1.0); // Only system audio
        }

        if system_rms == 0.0 {
            return (1.0, 0.0); // Only mic audio
        }

        // Calculate ratios based on relative levels
        let total_level = mic_rms + system_rms;
        let mic_ratio = (mic_rms / total_level).max(0.1).min(0.9); // Keep between 10% and 90%
        let system_ratio = 1.0 - mic_ratio;

        (mic_ratio, system_ratio)
    }

    /// Get current mixing mode
    pub fn mixing_mode(&self) -> &MixingMode {
        &self.mixing_mode
    }

    /// Update mixing mode
    pub fn set_mixing_mode(&mut self, mode: MixingMode) {
        self.mixing_mode = mode;
    }

    /// Get audio level statistics
    pub fn get_level_stats(&self) -> AudioLevelStats {
        let history: Vec<f32> = self.history_buffer.iter().cloned().collect();
        let rms = if !history.is_empty() {
            (history.iter().map(|&x| x * x).sum::<f32>() / history.len() as f32).sqrt()
        } else {
            0.0
        };

        let peak = history.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        AudioLevelStats {
            rms,
            peak,
            samples_analyzed: history.len(),
        }
    }
}

impl RmsAnalyzer {
    fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: VecDeque::with_capacity(window_size),
            sum_squares: 0.0,
        }
    }

    fn analyze(&mut self, samples: &[f32]) -> f32 {
        for &sample in samples {
            self.buffer.push_back(sample);
            self.sum_squares += sample * sample;

            if self.buffer.len() > self.window_size {
                if let Some(old_sample) = self.buffer.pop_front() {
                    self.sum_squares -= old_sample * old_sample;
                }
            }
        }

        if self.buffer.is_empty() {
            0.0
        } else {
            (self.sum_squares / self.buffer.len() as f32).sqrt()
        }
    }
}

impl DuckingProcessor {
    fn new(threshold: f32, attack_time: f32, release_time: f32) -> Self {
        Self {
            threshold,
            attack_time,
            release_time,
            current_gain: 1.0,
            target_gain: 1.0,
        }
    }

    fn process(&mut self, mic_sample: f32, system_sample: f32) -> f32 {
        let system_level = system_sample.abs();
        
        // Calculate target gain based on system level
        if system_level > self.threshold {
            // Duck the mic when system audio is loud
            self.target_gain = 0.3; // Reduce mic to 30%
        } else {
            // Restore mic when system audio is quiet
            self.target_gain = 1.0;
        }

        // Smooth gain transitions
        let gain_diff = self.target_gain - self.current_gain;
        if gain_diff.abs() > 0.01 {
            let step_size = if gain_diff > 0.0 {
                self.attack_time
            } else {
                self.release_time
            };
            self.current_gain += gain_diff * step_size;
        }

        mic_sample * self.current_gain
    }
}

impl CrossfadeProcessor {
    fn new(fade_length: usize) -> Self {
        Self {
            fade_length,
            fade_buffer: VecDeque::with_capacity(fade_length),
        }
    }

    fn process(&mut self, mic_sample: f32, system_sample: f32) -> f32 {
        // Simple crossfade implementation
        // In a more sophisticated version, this would handle smooth transitions
        // between different audio sources
        
        // For now, use a simple weighted average
        let mic_weight = 0.6;
        let system_weight = 0.4;
        
        mic_sample * mic_weight + system_sample * system_weight
    }
}

/// Audio level statistics
#[derive(Debug, Clone)]
pub struct AudioLevelStats {
    pub rms: f32,
    pub peak: f32,
    pub samples_analyzed: usize,
}

impl Default for AudioMixer {
    fn default() -> Self {
        Self::new(MixingMode::Professional)
    }
}
