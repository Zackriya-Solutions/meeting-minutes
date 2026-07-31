//! Mic + system audio mixing.
//!
//! `AudioMixerRingBuffer` and `ProfessionalAudioMixer` are copied from the main
//! app's `pipeline.rs`: fixed 600 ms windows, zero-padding whichever stream lags,
//! and a plain `clamp(mic + system, ±1.0)` mix. `LinearResampler` brings each
//! stream to a common rate before mixing (the app resamples to 48 kHz upstream).

use std::collections::VecDeque;

use log::{error, warn};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    Mic,
    System,
}

/// Accumulates mic + system samples (at a common rate) and hands out aligned
/// fixed-size windows for mixing.
pub struct AudioMixerRingBuffer {
    mic_buffer: VecDeque<f32>,
    system_buffer: VecDeque<f32>,
    window_size_samples: usize,
    max_buffer_size: usize,
}

impl AudioMixerRingBuffer {
    pub fn new(sample_rate: u32) -> Self {
        let window_ms = 600.0;
        let window_size_samples = (sample_rate as f32 * window_ms / 1000.0) as usize;
        // Generous safety cap: system audio (CoreAudio) can be jittery.
        let max_buffer_size = window_size_samples * 8;
        Self {
            mic_buffer: VecDeque::with_capacity(max_buffer_size),
            system_buffer: VecDeque::with_capacity(max_buffer_size),
            window_size_samples,
            max_buffer_size,
        }
    }

    pub fn add_samples(&mut self, track: Track, samples: &[f32]) {
        match track {
            Track::Mic => self.mic_buffer.extend(samples.iter().copied()),
            Track::System => self.system_buffer.extend(samples.iter().copied()),
        }

        if self.mic_buffer.len() > self.max_buffer_size {
            warn!(
                "mic buffer overflow: {} > {} samples, dropping oldest",
                self.mic_buffer.len(),
                self.max_buffer_size
            );
        }
        if self.system_buffer.len() > self.max_buffer_size {
            error!(
                "system buffer overflow: {} > {} samples, dropping oldest",
                self.system_buffer.len(),
                self.max_buffer_size
            );
        }
        while self.mic_buffer.len() > self.max_buffer_size {
            self.mic_buffer.pop_front();
        }
        while self.system_buffer.len() > self.max_buffer_size {
            self.system_buffer.pop_front();
        }
    }

    pub fn can_mix(&self) -> bool {
        self.mic_buffer.len() >= self.window_size_samples
            || self.system_buffer.len() >= self.window_size_samples
    }

    /// Drain one aligned window from each buffer, zero-padding the short one.
    pub fn extract_window(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if !self.can_mix() {
            return None;
        }
        let mic_window = drain_window(&mut self.mic_buffer, self.window_size_samples);
        let sys_window = drain_window(&mut self.system_buffer, self.window_size_samples);
        Some((mic_window, sys_window))
    }

    /// Drain whatever remains (a final sub-window), zero-padding the shorter side.
    /// Used once when capture stops so the tail isn't dropped.
    pub fn flush(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if self.mic_buffer.is_empty() && self.system_buffer.is_empty() {
            return None;
        }
        let len = self.mic_buffer.len().max(self.system_buffer.len());
        let mut mic: Vec<f32> = self.mic_buffer.drain(..).collect();
        let mut sys: Vec<f32> = self.system_buffer.drain(..).collect();
        mic.resize(len, 0.0);
        sys.resize(len, 0.0);
        Some((mic, sys))
    }
}

fn drain_window(buffer: &mut VecDeque<f32>, window: usize) -> Vec<f32> {
    if buffer.len() >= window {
        buffer.drain(0..window).collect()
    } else if !buffer.is_empty() {
        let available: Vec<f32> = buffer.drain(..).collect();
        let mut padded = Vec::with_capacity(window);
        padded.extend_from_slice(&available);
        padded.resize(window, 0.0); // zero-pad (silence) — inaudible, no artifacts
        padded
    } else {
        vec![0.0; window]
    }
}

/// Plain sum with proportional clip protection — matches the app's shipping mixer.
pub struct ProfessionalAudioMixer;

impl ProfessionalAudioMixer {
    pub fn new() -> Self {
        Self
    }

    pub fn mix_window(&mut self, mic_window: &[f32], sys_window: &[f32]) -> Vec<f32> {
        let max_len = mic_window.len().max(sys_window.len());
        let mut mixed = Vec::with_capacity(max_len);
        for i in 0..max_len {
            let mic = mic_window.get(i).copied().unwrap_or(0.0);
            let sys = sys_window.get(i).copied().unwrap_or(0.0);
            let sum = mic + sys;
            // Scale down proportionally if the sum would clip (avoids hard-clip buzz).
            let sum_abs = sum.abs();
            mixed.push(if sum_abs > 1.0 { sum / sum_abs } else { sum });
        }
        mixed
    }
}

/// Streaming linear-interpolation resampler. Good enough for speech; used to bring
/// mic/system to the common mix rate. Passthrough when input == output rate.
pub struct LinearResampler {
    ratio: f64, // input_rate / output_rate
    buf: VecDeque<f32>,
    pos: f64, // read cursor within buf (input samples)
    passthrough: bool,
}

impl LinearResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let output_rate = output_rate.max(1);
        Self {
            ratio: input_rate as f64 / output_rate as f64,
            buf: VecDeque::new(),
            pos: 0.0,
            passthrough: input_rate == output_rate,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passthrough {
            return input.to_vec();
        }
        self.buf.extend(input.iter().copied());

        let mut out = Vec::new();
        // Need buf[i0] and buf[i0 + 1]; emit while both are available.
        while (self.pos as usize) + 1 < self.buf.len() {
            let i0 = self.pos.floor() as usize;
            let frac = (self.pos - i0 as f64) as f32;
            let s0 = self.buf[i0];
            let s1 = self.buf[i0 + 1];
            out.push(s0 * (1.0 - frac) + s1 * frac);
            self.pos += self.ratio;
        }

        // Drop fully-consumed samples, keep the tail for cross-chunk interpolation.
        let keep_from = self.pos.floor() as usize;
        if keep_from > 0 {
            let drain = keep_from.min(self.buf.len());
            self.buf.drain(0..drain);
            self.pos -= drain as f64;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_passthrough_is_identity() {
        let mut r = LinearResampler::new(48_000, 48_000);
        let input: Vec<f32> = (0..100).map(|i| i as f32).collect();
        assert_eq!(r.process(&input), input);
    }

    #[test]
    fn resampler_upsamples_roughly_by_ratio() {
        // 24k -> 48k should roughly double the sample count across streaming chunks.
        let mut r = LinearResampler::new(24_000, 48_000);
        let mut total = 0usize;
        for _ in 0..10 {
            let chunk = vec![0.5f32; 1000];
            total += r.process(&chunk).len();
        }
        // ~2x of 10_000 input samples, allowing for interpolation edge effects.
        assert!(total > 19_000 && total <= 20_000, "got {total}");
    }

    #[test]
    fn resampler_downsamples_roughly_by_ratio() {
        let mut r = LinearResampler::new(48_000, 24_000);
        let mut total = 0usize;
        for _ in 0..10 {
            total += r.process(&vec![0.1f32; 1000]).len();
        }
        assert!(total > 4_900 && total <= 5_000, "got {total}");
    }

    #[test]
    fn mixer_sums_and_clamps() {
        let mut m = ProfessionalAudioMixer::new();
        // In range: plain sum.
        assert!((m.mix_window(&[0.3], &[0.4])[0] - 0.7).abs() < 1e-5);
        // Over range: proportional clamp to +/-1.0.
        assert!((m.mix_window(&[0.8], &[0.8])[0] - 1.0).abs() < 1e-6);
        assert!((m.mix_window(&[-0.9], &[-0.9])[0] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn ring_zero_pads_missing_system() {
        let mut ring = AudioMixerRingBuffer::new(1000); // 600ms window = 600 samples
        ring.add_samples(Track::Mic, &vec![0.5; 600]);
        assert!(ring.can_mix());
        let (mic, sys) = ring.extract_window().unwrap();
        assert_eq!(mic.len(), 600);
        assert_eq!(sys.len(), 600);
        assert!(sys.iter().all(|&s| s == 0.0)); // no system data -> silence
    }
}
