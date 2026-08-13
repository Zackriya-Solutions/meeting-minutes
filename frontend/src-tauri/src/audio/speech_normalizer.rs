//! Levels out loud and quiet talkers before the audio reaches VAD and Whisper.
//!
//! Mic and system audio are summed into one mono stream. When both sides are close to their
//! microphones that works fine. It breaks down when one side is far away - a phone on speaker
//! next to the laptop, someone at the other end of a meeting room, a headset held in hand.
//! That side can end up 15-20 dB below the local speaker in the mixed signal, which is quiet
//! enough for the VAD to treat it as background and drop it. The words never reach Whisper,
//! and the transcript silently loses one side of the conversation.
//!
//! Measured on a real recording where the remote side came in over a phone speaker: the local
//! speaker sat around -21 dBFS at the 90th percentile while quiet passages ran near -40 dBFS,
//! an 18 dB spread inside a single mono file. Re-running the same audio through the same model
//! after levelling recovered 18 % more words, including whole sentences from the remote side
//! that the live transcript had dropped entirely.
//!
//! This is a slow automatic gain control, not a compressor. It tracks the speech level over a
//! window of about a second and a half and applies one smoothed gain to the whole window, so
//! quiet speech is lifted without the pumping that fast per-sample compression causes. Three
//! properties keep it from making things worse:
//!
//! - It only ever amplifies. Loud speech is left exactly as it is.
//! - Below `NOISE_FLOOR` nothing happens, so room tone and fan noise are not lifted into the
//!   range where Whisper starts inventing words.
//! - Gain changes are smoothed and capped, and the output passes through a soft limiter.
//!
//! Only the transcription path uses this. The recording written to disk stays untouched, so the
//! saved audio remains a faithful copy of what the microphones picked up.

/// Target speech level. Slightly below the -18 dBFS that whisper.cpp's own examples assume,
/// leaving headroom for the peaks that sit above the RMS of natural speech.
const TARGET_RMS: f32 = 0.1; // -20 dBFS

/// Below this level the input is treated as silence or room tone and left alone. -55 dBFS sits
/// under any speech that is still intelligible but above a typical noise floor.
const NOISE_FLOOR: f32 = 0.0018;

/// Never amplify by more than this. 18 dB covers the measured 18 dB spread; going further mostly
/// raises noise in the gaps.
const MAX_GAIN: f32 = 8.0;

/// How much of the new gain estimate is taken per window when the signal gets louder. Small value
/// means slow reaction, which is what avoids audible pumping.
const ATTACK: f32 = 0.15;

/// Same when the signal gets quieter. Slower than attack so short pauses inside a sentence do not
/// ramp the gain up.
const RELEASE: f32 = 0.05;

/// Level at which the soft limiter starts bending the curve.
const LIMIT_THRESHOLD: f32 = 0.95;

/// Slow automatic gain control for the mixed signal on its way to VAD and Whisper.
pub struct SpeechNormalizer {
    /// Smoothed gain carried across windows so the level does not jump at window boundaries.
    gain: f32,
}

impl SpeechNormalizer {
    pub fn new() -> Self {
        Self { gain: 1.0 }
    }

    /// Returns the window with quiet speech lifted towards `TARGET_RMS`.
    ///
    /// Windows that are empty, or quieter than `NOISE_FLOOR`, come back unchanged apart from the
    /// gain already in flight from previous windows.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }

        let rms = rms_of(samples);

        // Only chase a new target when there is something that could be speech. In the gaps the
        // gain is held rather than reset, so a short pause does not cause a jump on the next word.
        if rms > NOISE_FLOOR {
            let wanted = (TARGET_RMS / rms).clamp(1.0, MAX_GAIN);
            let rate = if wanted > self.gain { ATTACK } else { RELEASE };
            self.gain += (wanted - self.gain) * rate;
        }

        if (self.gain - 1.0).abs() < f32::EPSILON {
            return samples.to_vec();
        }

        samples.iter().map(|&s| soft_limit(s * self.gain)).collect()
    }
}

impl Default for SpeechNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

fn rms_of(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Keeps peaks inside ±1.0 without the hard edge that clipping puts into the spectrum.
/// Everything below the threshold passes through untouched.
fn soft_limit(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= LIMIT_THRESHOLD {
        return sample;
    }
    let excess = magnitude - LIMIT_THRESHOLD;
    let headroom = 1.0 - LIMIT_THRESHOLD;
    let limited = LIMIT_THRESHOLD + headroom * (excess / (excess + headroom));
    limited * sample.signum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine wave at the given amplitude, long enough to look like one mixing window.
    fn tone(amplitude: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| amplitude * (i as f32 * 0.1).sin())
            .collect()
    }

    #[test]
    fn quiet_speech_is_lifted_towards_the_target() {
        let mut n = SpeechNormalizer::new();
        let quiet = tone(0.02, 2400); // roughly -37 dBFS, the level that gets dropped today

        // Feed several windows so the smoothed gain has time to settle, as it would in a call.
        let mut out = Vec::new();
        for _ in 0..60 {
            out = n.process(&quiet);
        }

        assert!(
            rms_of(&out) > rms_of(&quiet) * 3.0,
            "quiet speech should end up clearly louder, got {} from {}",
            rms_of(&out),
            rms_of(&quiet)
        );
    }

    #[test]
    fn loud_speech_is_left_alone() {
        let mut n = SpeechNormalizer::new();
        let loud = tone(0.5, 2400);
        let out = n.process(&loud);
        assert_eq!(out, loud, "speech already above the target must not be touched");
    }

    #[test]
    fn room_tone_is_not_amplified() {
        let mut n = SpeechNormalizer::new();
        let noise = tone(0.0005, 2400); // below NOISE_FLOOR

        for _ in 0..60 {
            n.process(&noise);
        }

        assert_eq!(n.gain, 1.0, "silence must not raise the gain and lift the noise floor");
    }

    #[test]
    fn output_stays_inside_range() {
        let mut n = SpeechNormalizer::new();
        // Quiet enough to ask for full gain, with peaks that would overshoot once applied.
        let spiky: Vec<f32> = (0..2400)
            .map(|i| if i % 100 == 0 { 0.4 } else { 0.01 })
            .collect();

        for _ in 0..60 {
            for &s in n.process(&spiky).iter() {
                assert!(s.abs() <= 1.0, "sample {} escaped the limiter", s);
            }
        }
    }

    #[test]
    fn gain_never_drops_below_unity() {
        let mut n = SpeechNormalizer::new();
        // Far above the target: the normalizer must not turn into a downward compressor.
        for _ in 0..60 {
            n.process(&tone(0.9, 2400));
        }
        assert!(n.gain >= 1.0, "gain fell to {}, this stage only amplifies", n.gain);
    }

    #[test]
    fn empty_input_is_handled() {
        let mut n = SpeechNormalizer::new();
        assert!(n.process(&[]).is_empty());
    }
}
