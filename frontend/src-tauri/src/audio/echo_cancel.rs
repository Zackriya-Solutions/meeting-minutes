// audio/echo_cancel.rs
//
// Removes the speakers from the microphone.
//
// When someone joins a meeting without headphones, whatever the remote participants say
// comes out of the speakers and straight back into the microphone a few milliseconds
// later. Measured on a real recording: a second copy of the far-end signal arriving
// about 16.7 ms later at 73% relative strength, which comb-filters the band that
// distinguishes one vowel from another.
//
// The system audio is an exact copy of what the speakers were asked to play, so the echo
// is a filtered version of a signal we already have. That makes it removable: estimate
// the filter, apply it to the reference, subtract the result.

/// Normalised least-mean-squares echo canceller.
///
/// Learns the path from "what we played" to "what the microphone heard" and subtracts
/// its prediction. Normalised rather than plain LMS because speech level swings by tens
/// of decibels within a sentence, and a fixed step size that converges on a loud vowel
/// diverges on a quiet one.
pub struct EchoCanceller {
    /// The estimated room response, one weight per sample of delay.
    weights: Vec<f32>,
    /// The most recent reference samples, newest last. Same length as `weights`.
    history: Vec<f32>,
    /// Running energy of `history`, kept incrementally so each sample costs one pass
    /// over the taps rather than two.
    energy: f32,
    step: f32,
}

/// How much of the room to model: 100 ms at 16 kHz.
///
/// The direct path plus early reflections is what carries the energy; later reverb is
/// far quieter and modelling it costs taps without removing much. Too short and the
/// filter cannot represent the room at all; too long and it adapts slowly and picks up
/// noise in the tail.
const TAPS: usize = 1_600;

/// Reference energy below which nothing is learned.
///
/// With no sound coming out of the speakers there is no echo to model, and adapting
/// against silence just walks the weights into noise.
const SILENCE_ENERGY: f32 = 1e-6;

/// How often to reconsider whether someone in the room is talking: 10 ms at 16 kHz.
///
/// Short enough that a word does not slip past before adaptation stops, long enough that
/// the decision is not made on a handful of samples.
const DECISION_BLOCK: usize = 160;

/// How much louder than the reference the microphone may be before the extra is taken to
/// be a person in the room rather than echo.
///
/// Grounded in physics rather than tuning: the echo path is passive - a speaker, air, a
/// room - and cannot amplify. A microphone louder than what was played is carrying
/// something the speakers did not produce. Set at parity, so the filter stops learning
/// the moment the room contributes more than the echo possibly could.
///
/// Being wrong in the cautious direction only slows adaptation; being wrong the other
/// way teaches the filter that a person's voice is echo and it starts erasing them.
const NEAR_END_MARGIN: f32 = 1.0;

impl EchoCanceller {
    pub fn new() -> Self {
        Self {
            weights: vec![0.0; TAPS],
            history: vec![0.0; TAPS],
            energy: 0.0,
            step: 0.3,
        }
    }

    /// Subtract the predicted echo of `reference` from `microphone`.
    ///
    /// Both must be 16 kHz and sample-aligned - they come from the same mixer window,
    /// which is what guarantees that. The returned signal is the microphone with the
    /// speakers removed; what remains is whoever is actually in the room.
    ///
    /// Returns the microphone unchanged if the two lengths disagree, because a
    /// misaligned reference would have the canceller subtract the right sound at the
    /// wrong time, which is worse than not cancelling at all.
    pub fn cancel(&mut self, microphone: &[f32], reference: &[f32]) -> Vec<f32> {
        if microphone.len() != reference.len() {
            return microphone.to_vec();
        }

        let mut out = Vec::with_capacity(microphone.len());

        // Decide about near-end speech in short blocks rather than once per call, so a
        // caller handing over a long buffer still gets the protection.
        for block in 0..microphone.len().div_ceil(DECISION_BLOCK) {
            let from = block * DECISION_BLOCK;
            let to = (from + DECISION_BLOCK).min(microphone.len());
            let step = self.step_for(&microphone[from..to], &reference[from..to]);
            self.cancel_block(&microphone[from..to], &reference[from..to], step, &mut out);
        }

        out
    }

    /// How fast to learn from this block, or zero to stop learning from it.
    ///
    /// The echo can never be louder than what was played, so a microphone that is much
    /// louder than the reference is carrying something the speakers cannot explain -
    /// someone in the room. Adapting then teaches the filter that this person's voice is
    /// echo, and it starts subtracting them.
    ///
    /// Deliberately judged on peaks rather than on how well the filter is doing, so it
    /// works from the very first block, before anything has converged.
    fn step_for(&self, microphone: &[f32], reference: &[f32]) -> f32 {
        let peak = |samples: &[f32]| samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak(microphone) > NEAR_END_MARGIN * peak(reference) {
            0.0
        } else {
            self.step
        }
    }

    fn cancel_block(
        &mut self,
        microphone: &[f32],
        reference: &[f32],
        step: f32,
        out: &mut Vec<f32>,
    ) {
        for (&mic, &far) in microphone.iter().zip(reference.iter()) {
            // Slide the reference window forward one sample, keeping its energy current.
            let leaving = self.history[0];
            self.history.copy_within(1.., 0);
            self.history[TAPS - 1] = far;
            self.energy += far * far - leaving * leaving;

            let predicted: f32 = self
                .weights
                .iter()
                .zip(self.history.iter())
                .map(|(w, x)| w * x)
                .sum();

            // What is left after removing the speakers is both the near-end voice and
            // the canceller's own error, which is why this doubles as the output and as
            // the signal the filter learns from.
            let error = mic - predicted;
            out.push(error);

            if step > 0.0 && self.energy > SILENCE_ENERGY {
                let gain = step * error / (self.energy + 1e-9);
                for (w, x) in self.weights.iter_mut().zip(self.history.iter()) {
                    *w += gain * x;
                }
            }
        }
    }
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a signal that behaves like speech for the filter's purposes: it has to vary
    /// in level, because a constant tone lets any filter look good.
    fn speechlike(samples: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;
        (0..samples)
            .map(|i| {
                // A cheap deterministic noise source, shaped by a slow envelope so the
                // signal has loud and quiet passages the way a sentence does.
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (state >> 8) as f32 / 8_388_608.0 - 1.0;
                let envelope = 0.5 + 0.5 * (i as f32 / 4_000.0).sin();
                noise * envelope * 0.3
            })
            .collect()
    }

    /// Put the reference through a delay and a few reflections, the way a room does.
    fn through_a_room(reference: &[f32]) -> Vec<f32> {
        // 16.7 ms is what was measured on the real recording; the quieter taps stand in
        // for the reflections that arrive after the direct path.
        const PATH: [(usize, f32); 4] = [(267, 0.73), (410, 0.31), (690, 0.18), (1_100, 0.09)];

        let mut echo = vec![0.0f32; reference.len()];
        for (delay, gain) in PATH {
            for i in delay..reference.len() {
                echo[i] += reference[i - delay] * gain;
            }
        }
        echo
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32
    }

    /// Echo return loss enhancement: how much of the speakers is gone, in decibels.
    ///
    /// Measured over the second half only, because the first half is the filter learning
    /// the room and nobody claims cancellation before it has converged.
    #[test]
    fn it_removes_the_speakers_from_the_microphone() {
        let reference = speechlike(160_000, 7); // 10 s at 16 kHz
        let echo = through_a_room(&reference);

        // Nobody in the room is talking, so the microphone hears only the speakers.
        let mut canceller = EchoCanceller::new();
        let cleaned = canceller.cancel(&echo, &reference);

        let half = echo.len() / 2;
        let before = energy(&echo[half..]);
        let after = energy(&cleaned[half..]);
        let erle = 10.0 * (before / after.max(1e-12)).log10();

        println!("erle {erle:.1} dB");
        assert!(
            erle > 15.0,
            "only removed {erle:.1} dB of the speakers; the microphone still hears them"
        );
    }

    /// The near-end voice has to survive. A canceller that removes the echo *and* the
    /// person in the room has solved nothing.
    ///
    /// Shaped like a meeting rather than like a worst case: the far end talks alone for
    /// the first half, which is when the filter learns the room, and the person in the
    /// room joins in for the second half. Continuous double-talk from the first sample
    /// gives the filter nothing clean to converge on, and no echo canceller - including
    /// the ones in Teams and Zoom - handles that well.
    #[test]
    fn it_keeps_the_voice_of_whoever_is_in_the_room() {
        let reference = speechlike(160_000, 7);
        let echo = through_a_room(&reference);

        let half = reference.len() / 2;
        let near_end: Vec<f32> = speechlike(160_000, 99)
            .into_iter()
            .enumerate()
            .map(|(i, s)| if i < half { 0.0 } else { s })
            .collect();

        let microphone: Vec<f32> = near_end
            .iter()
            .zip(echo.iter())
            .map(|(near, echo)| near + echo)
            .collect();

        let mut canceller = EchoCanceller::new();
        let cleaned = canceller.cancel(&microphone, &reference);

        // Compare against the near-end signal that was actually there. Over the second
        // half - the double-talk half - what is left should resemble it far more than
        // the raw microphone did, which is the whole claim.
        let before: Vec<f32> = microphone[half..]
            .iter()
            .zip(near_end[half..].iter())
            .map(|(m, n)| m - n)
            .collect();
        let after: Vec<f32> = cleaned[half..]
            .iter()
            .zip(near_end[half..].iter())
            .map(|(c, n)| c - n)
            .collect();

        println!(
            "distance to the near-end voice: {:.6} before, {:.6} after",
            energy(&before),
            energy(&after)
        );
        assert!(
            energy(&after) < energy(&before) / 4.0,
            "cancelling left the room's own voice no closer to the truth"
        );
    }

    /// A reference that does not line up with the microphone would have the filter
    /// subtract the right sound at the wrong moment. Refusing is the safe answer.
    #[test]
    fn a_misaligned_reference_is_refused_rather_than_guessed_at() {
        let mut canceller = EchoCanceller::new();
        let microphone = vec![0.5f32; 100];

        let out = canceller.cancel(&microphone, &[0.5f32; 80]);

        assert_eq!(out, microphone, "the microphone must pass through untouched");
    }

    /// Silence out of the speakers means there is nothing to learn from.
    #[test]
    fn silence_does_not_move_the_filter() {
        let mut canceller = EchoCanceller::new();
        let microphone = speechlike(16_000, 3);

        let cleaned = canceller.cancel(&microphone, &vec![0.0f32; 16_000]);

        assert_eq!(
            cleaned, microphone,
            "with nothing playing, the microphone is already clean"
        );
    }
}
