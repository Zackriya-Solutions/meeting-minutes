//! Microphone echo cancellation, using the captured computer audio as the
//! reference, the way Granola and Hyprnote do it.
//!
//! With built-in speakers the microphone also hears the call. Without this,
//! the far side is in the mix twice (from the system audio, then about 100 ms
//! later through the mic), which sounds like an echo in the recording and
//! reaches transcription twice. The microphone stays raw (no Apple voice
//! processing), so other apps are not ducked.
//!
//! The engine is DTLN-aec (`neural_echo`), a neural canceller that runs at
//! 16 kHz. Above 7 kHz the raw microphone is kept, scaled by how much the model
//! attenuated the band below it, so the saved voice keeps its full bandwidth
//! while echo-only moments stay silent. It is left on with headphones: with no
//! echo in the mic the model leaves the voice as it is (measured), and the
//! output route can change mid-call.
//!
//! The model only removes echo that arrives after the reference it is given,
//! so both tracks leave the canceller `LEAD_MS` later than they entered it:
//! the mic is held back, which lets the reference be fed ahead of its echo
//! even when the system window trails the echo, while the two tracks stay
//! aligned for the mixer.

use log::{info, warn};
use realfft::RealFftPlanner;
use std::collections::VecDeque;

use super::neural_echo::{self, NeuralEcho};

/// Delay of both output tracks relative to the input windows.
const LEAD_MS: usize = 750;
/// Log reduction about every 10 s of audio.
const STATS_EVERY_MS: usize = 10_000;
/// The model's sample rate is a third of the pipeline's.
const SAMPLE_RATE: u32 = 48_000;

pub struct MicEchoCanceller {
    path: NeuralPath,
    sys_delay: VecDeque<f32>,
    /// Raw mic, delayed so the band split's output lines up with the model's.
    raw_delay: VecDeque<f32>,
    split: BandSplit,
    /// High-band gain at the end of the last block.
    high_gain: f32,
    stats: Stats,
    /// Audio fed since the last `drain`.
    pending: bool,
}

impl MicEchoCanceller {
    /// Needs the pipeline's 48 kHz rate; fails if the model cannot load.
    pub fn new(sample_rate: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(sample_rate == SAMPLE_RATE, "needs {SAMPLE_RATE} Hz, pipeline runs at {sample_rate} Hz");
        let lead = SAMPLE_RATE as usize * LEAD_MS / 1000;
        let path = NeuralPath::new(lead)?;
        info!("🔇 Mic echo cancellation on: neural DTLN-aec (computer audio as reference)");
        Ok(Self {
            path,
            sys_delay: std::iter::repeat(0.0).take(lead).collect(),
            raw_delay: std::iter::repeat(0.0).take(lead - BandSplit::DELAY).collect(),
            split: BandSplit::new(),
            high_gain: 1.0,
            stats: Stats::new(),
            pending: false,
        })
    }

    /// Takes time-aligned mic/system windows and returns them `LEAD_MS` later,
    /// with the speaker playback removed from the mic. Both stay aligned, so
    /// the mixer (and so the VAD and `audio.mp4`) sees one timeline.
    pub fn process(&mut self, mic: &mut Vec<f32>, sys: &mut Vec<f32>) {
        let n = mic.len().min(sys.len());
        mic.truncate(n);
        sys.truncate(n);
        self.pending |= n > 0;
        let low = self.path.process(mic, sys);
        self.raw_delay.extend(mic.iter());
        let early: Vec<f32> = self.raw_delay.drain(..n).collect();
        let (raw, raw_low) = self.split.process(&early);

        // Output = cleaned low band + the raw high band, scaled by the power
        // ratio the model left in the band below (ramped per 8 ms block). The
        // power ratio, not its root: measured, it removes as much echo as
        // dropping the high band, keeps the voice's full bandwidth, and only
        // lets some echo above 7 kHz through while the user talks over it.
        let mut cleaned = Vec::with_capacity(n);
        for ((low, raw), raw_low) in low.chunks(384).zip(raw.chunks(384)).zip(raw_low.chunks(384)) {
            let (out_e, in_e) = (energy(low), energy(raw_low));
            let target = if in_e > 1e-9 * low.len() as f64 { (out_e / in_e).min(1.0) as f32 } else { 1.0 };
            let start = self.high_gain;
            for (i, ((&l, &r), &rl)) in low.iter().zip(raw).zip(raw_low).enumerate() {
                let g = start + (target - start) * (i + 1) as f32 / low.len() as f32;
                cleaned.push(l + g * (r - rl));
            }
            self.high_gain = target;
        }

        self.sys_delay.extend(sys.iter());
        let sys_out: Vec<f32> = self.sys_delay.drain(..n).collect();
        self.stats.add(&raw, &cleaned, &sys_out, self.path.lag_ms());
        *mic = cleaned;
        *sys = sys_out;
    }

    /// The last `LEAD_MS` of both tracks, for the final flush; `None` when
    /// nothing was fed since the last drain (stop sends several flushes).
    pub fn drain(&mut self) -> Option<(Vec<f32>, Vec<f32>)> {
        if !self.pending {
            return None;
        }
        let mut mic = vec![0.0; self.sys_delay.len()];
        let mut sys = vec![0.0; self.sys_delay.len()];
        self.process(&mut mic, &mut sys);
        self.pending = false;
        Some((mic, sys))
    }
}

/// Before/after energy while computer audio plays, logged about every 10 s.
struct Stats {
    pre: f64,
    post: f64,
    samples: usize,
}

impl Stats {
    fn new() -> Self {
        Self { pre: 0.0, post: 0.0, samples: 0 }
    }

    fn add(&mut self, raw: &[f32], cleaned: &[f32], reference: &[f32], lag_ms: i64) {
        // Only windows with computer audio playing say anything about echo.
        if energy(reference) > 1e-6 * reference.len() as f64 {
            self.pre += energy(raw);
            self.post += energy(cleaned);
        }
        self.samples += raw.len();
        if self.samples >= SAMPLE_RATE as usize * STATS_EVERY_MS / 1000 {
            if self.pre > 0.0 {
                info!(
                    "🔇 Mic echo cancellation: mic {:.1} dB → {:.1} dB while computer audio played ({:.1} dB removed); echo lag {lag_ms} ms",
                    db(self.pre), db(self.post), db(self.pre) - db(self.post),
                );
            }
            *self = Self::new();
        }
    }
}

fn energy(s: &[f32]) -> f64 {
    s.iter().map(|&v| (v as f64) * (v as f64)).sum()
}

fn db(e: f64) -> f64 {
    10.0 * (e + 1e-12).log10()
}

// ---------------------------------------------------------------------------
// Neural path (48 kHz in/out, DTLN-aec at 16 kHz)
// ---------------------------------------------------------------------------

/// Echo lag assumed until the estimator locks (measured ~107 ms between the
/// system window and its echo in the mic on a MacBook Pro's built-in speakers).
const DEFAULT_LAG_MS: i64 = 100;
/// The model works best with the reference 5-40 ms ahead of its echo
/// (measured plateau); aim for the middle so estimator error stays inside it.
const REFERENCE_LEAD_MS: i64 = 20;
/// The mic waits this long before the model sees it. The lag estimator runs
/// on the incoming audio, so when the echo delay changes (a new playback
/// stream), the new lag is known before the model reaches that audio and is
/// applied from where the change happened: no echo leaks while re-locking.
/// Locking takes up to ~0.63 s after a change (see `LagEstimator::observe`).
const MIC_HOLD_MS: i64 = 700;
/// Shortest echo lag searched and supported (echo before its reference, as
/// the system window can trail the mic).
const MIN_LAG_MS: i64 = -80;
/// Longest echo lag searched and supported.
const MAX_LAG_MS: i64 = 500;
/// Smaller estimate changes are measurement jitter.
const LAG_HYSTERESIS_MS: i64 = 1;

struct NeuralPath {
    model: NeuralEcho,
    down_mic: Decimator,
    down_ref: Decimator,
    up: Interpolator,
    /// 16 kHz mic not yet processed; its front is absolute index `mic_next`.
    mic16: VecDeque<f32>,
    mic_next: i64,
    /// 16 kHz reference history; `ref_base` is the absolute index of its front.
    ref16: VecDeque<f32>,
    ref_base: i64,
    lag: LagEstimator,
    /// Echo lag in use, in 16 kHz samples.
    lag_in_use: i64,
    /// Lag changes waiting for the model to reach the mic index they start at.
    lag_changes: VecDeque<(i64, i64)>,
    /// Cleaned 48 kHz mic awaiting output.
    out48: VecDeque<f32>,
    #[cfg(test)]
    missing_reference: usize,
}

impl NeuralPath {
    fn new(lead48: usize) -> anyhow::Result<Self> {
        let model = NeuralEcho::new(neural_echo::MODEL_1, neural_echo::MODEL_2, neural_echo::MODEL_STATE_SIZE)?;
        // Content delay of the chain: both FIRs plus the model's overlap-add.
        // The mic hold delays *when* samples come out, not their position, so
        // the output queue's prefill must also cover it.
        let chain = 2 * FIR_DELAY_48 + 3 * neural_echo::LATENCY;
        let hold = 3 * (ms16(MIC_HOLD_MS) as usize + neural_echo::HOP);
        anyhow::ensure!(chain + hold <= lead48, "LEAD_MS too short for the neural chain");
        Ok(Self {
            model,
            down_mic: Decimator::new(),
            down_ref: Decimator::new(),
            up: Interpolator::new(),
            mic16: VecDeque::new(),
            mic_next: 0,
            ref16: VecDeque::new(),
            ref_base: 0,
            lag: LagEstimator::new(),
            lag_in_use: ms16(DEFAULT_LAG_MS),
            lag_changes: VecDeque::new(),
            out48: std::iter::repeat(0.0).take(lead48 - chain).collect(),
            #[cfg(test)]
            missing_reference: 0,
        })
    }

    fn lag_ms(&self) -> i64 {
        self.lag_in_use * 1000 / 16_000
    }

    fn process(&mut self, mic48: &[f32], sys48: &[f32]) -> Vec<f32> {
        let mic16 = self.down_mic.process(mic48);
        let ref16 = self.down_ref.process(sys48);
        if let Some((lag, from)) = self.lag.observe(&mic16, &ref16) {
            let current = self.lag_changes.back().map_or(self.lag_in_use, |&(_, l)| l);
            if (lag - current).abs() >= ms16(LAG_HYSTERESIS_MS) {
                info!("🔇 Echo lag locked at {} ms", lag * 1000 / 16_000);
                // Applied when the model reaches `from`. The model keeps its
                // state: resetting it on a new lag leaked more echo after a
                // delay jump and hurt the voice after a false lock (measured
                // in real_recordings_report).
                self.lag_changes.push_back((from, lag));
            }
        }
        self.mic16.extend(mic16);
        self.ref16.extend(ref16);

        // Each hop of mic pairs with the reference `lag - REFERENCE_LEAD_MS`
        // earlier, which has arrived once the mic has waited `MIC_HOLD_MS`.
        let hop = neural_echo::HOP;
        let hold = ms16(MIC_HOLD_MS) as usize;
        let mut cleaned16 = Vec::new();
        while self.mic16.len() >= hold + hop {
            while let Some(&(from, lag)) = self.lag_changes.front() {
                if from > self.mic_next {
                    break;
                }
                self.lag_in_use = lag;
                self.lag_changes.pop_front();
            }
            let ref_start = self.mic_next - self.lag_in_use + ms16(REFERENCE_LEAD_MS);
            let reference: Vec<f32> = (ref_start..ref_start + hop as i64)
                .map(|i| {
                    let k = i - self.ref_base;
                    #[cfg(test)]
                    if k >= self.ref16.len() as i64 { self.missing_reference += 1; }
                    if k >= 0 && (k as usize) < self.ref16.len() { self.ref16[k as usize] } else { 0.0 }
                })
                .collect();
            let mic: Vec<f32> = self.mic16.drain(..hop).collect();
            cleaned16.extend(self.model.process(&mic, &reference));
            self.mic_next += hop as i64;
        }
        // Keep only the reference history a future hop can still need.
        let keep_from = self.mic_next - ms16(MAX_LAG_MS);
        while self.ref_base < keep_from && !self.ref16.is_empty() {
            self.ref16.pop_front();
            self.ref_base += 1;
        }

        self.out48.extend(self.up.process(&cleaned16));
        let n = mic48.len();
        if self.out48.len() < n {
            warn!("Neural echo cancellation fell behind; padding {} samples", n - self.out48.len());
            self.out48.resize(n, 0.0);
        }
        self.out48.drain(..n).collect()
    }
}

fn ms16(ms: i64) -> i64 {
    ms * 16
}

// ---------------------------------------------------------------------------
// 48 kHz <-> 16 kHz with linear-phase FIRs of known, integer delay
// ---------------------------------------------------------------------------

const FIR_TAPS: usize = 97;
/// Group delay of the FIR, in 48 kHz samples (16 samples at 16 kHz).
const FIR_DELAY_48: usize = (FIR_TAPS - 1) / 2;

/// Blackman-windowed sinc low-pass at 7 kHz (48 kHz rate), unity DC gain.
fn fir() -> [f32; FIR_TAPS] {
    let fc = 7_000.0 / 48_000.0;
    let mid = FIR_DELAY_48 as f64;
    let mut h = [0f32; FIR_TAPS];
    let mut sum = 0.0;
    for (i, tap) in h.iter_mut().enumerate() {
        let x = i as f64 - mid;
        let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (FIR_TAPS - 1) as f64).cos()
            + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (FIR_TAPS - 1) as f64).cos();
        *tap = (sinc * w) as f32;
        sum += sinc * w;
    }
    h.iter_mut().for_each(|t| *t /= sum as f32);
    h
}

/// FIR filter state: the last `FIR_TAPS` inputs, newest last.
struct Fir {
    h: [f32; FIR_TAPS],
    history: VecDeque<f32>,
}

impl Fir {
    fn new() -> Self {
        Self { h: fir(), history: std::iter::repeat(0.0).take(FIR_TAPS).collect() }
    }

    fn push(&mut self, x: f32) {
        self.history.pop_front();
        self.history.push_back(x);
    }

    fn output(&self) -> f32 {
        self.history.iter().rev().zip(&self.h).map(|(a, b)| a * b).sum()
    }
}

/// 48 kHz → 16 kHz. Output k is the filtered input at 3k, so 16 kHz sample m
/// sits at 48 kHz sample 3m (plus the FIR delay).
struct Decimator {
    fir: Fir,
    phase: usize,
}

impl Decimator {
    fn new() -> Self {
        Self { fir: Fir::new(), phase: 2 }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(input.len() / 3 + 1);
        for &x in input {
            self.fir.push(x);
            self.phase = (self.phase + 1) % 3;
            if self.phase == 0 {
                out.push(self.fir.output());
            }
        }
        out
    }
}

/// 16 kHz → 48 kHz (zero-stuffing and the same low-pass, gain 3).
struct Interpolator {
    fir: Fir,
}

impl Interpolator {
    fn new() -> Self {
        Self { fir: Fir::new() }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(input.len() * 3);
        for &x in input {
            for stuffed in [3.0 * x, 0.0, 0.0] {
                self.fir.push(stuffed);
                out.push(self.fir.output());
            }
        }
        out
    }
}

/// Splits the raw 48 kHz mic into the part the model's round trip keeps (the
/// low-pass applied twice) and the input on the same timeline, so their
/// difference is exactly the high band the model drops.
struct BandSplit {
    first: Fir,
    second: Fir,
}

impl BandSplit {
    /// Delay of both outputs.
    const DELAY: usize = 2 * FIR_DELAY_48;

    fn new() -> Self {
        Self { first: Fir::new(), second: Fir::new() }
    }

    /// Returns (input delayed by `DELAY`, its low band).
    fn process(&mut self, input: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let mut delayed = Vec::with_capacity(input.len());
        let mut low = Vec::with_capacity(input.len());
        for &x in input {
            self.first.push(x);
            self.second.push(self.first.output());
            low.push(self.second.output());
            // The oldest input in the first filter's history, FIR_TAPS - 1 =
            // `DELAY` samples ago.
            delayed.push(self.first.history[0]);
        }
        (delayed, low)
    }
}

// ---------------------------------------------------------------------------
// Echo lag estimation (GCC-PHAT on 16 kHz mic and reference)
// ---------------------------------------------------------------------------

// Half-second windows, re-estimated every quarter second: after a new
// playback stream moves the echo by 50 ms, cancellation is back within
// 0.5 s (1 s windows every 0.5 s took 1.5-2 s; see real_recordings_report).
const LAG_WINDOW: usize = 8_192; // ~0.5 s
const LAG_EVERY: usize = 4_000; // 0.25 s
/// Peak height over the correlation's spread needed to trust an estimate.
const LAG_MIN_CONFIDENCE: f32 = 8.0;

struct LagEstimator {
    mic: VecDeque<f32>,
    reference: VecDeque<f32>,
    since: usize,
    last: Option<i64>,
    /// Samples observed so far: the absolute 16 kHz index of the next one.
    seen: i64,
    /// The lag last returned.
    agreed: Option<i64>,
    /// Where the first estimate that differs from `agreed` was made.
    first_change: Option<i64>,
    planner: RealFftPlanner<f32>,
}

impl LagEstimator {
    fn new() -> Self {
        Self { mic: VecDeque::new(), reference: VecDeque::new(), since: 0, last: None, seen: 0, agreed: None, first_change: None, planner: RealFftPlanner::new() }
    }

    /// Feed new 16 kHz samples (same span for both). Returns a lag (16 kHz
    /// samples, echo after reference is positive) once two consecutive
    /// confident estimates agree within 2 ms, with the absolute sample index
    /// it most likely starts at. The first estimate that sees a new lag ends
    /// a 0.5 s window that is mostly past the change, so the change lies
    /// 0.25-0.5 s before it: the middle, 0.375 s, is returned. The agreement
    /// comes one step (0.25 s) later, so the start is ~0.63 s back.
    fn observe(&mut self, mic: &[f32], reference: &[f32]) -> Option<(i64, i64)> {
        // In LAG_EVERY steps, so the estimate rate does not depend on how
        // large the caller's windows are (the pipeline's are 600 ms).
        let mut agreed = None;
        for (mic, reference) in mic.chunks(LAG_EVERY).zip(reference.chunks(LAG_EVERY)) {
            agreed = self.observe_step(mic, reference).or(agreed);
        }
        agreed
    }

    fn observe_step(&mut self, mic: &[f32], reference: &[f32]) -> Option<(i64, i64)> {
        self.seen += mic.len() as i64;
        self.mic.extend(mic);
        self.reference.extend(reference);
        while self.mic.len() > LAG_WINDOW { self.mic.pop_front(); }
        while self.reference.len() > LAG_WINDOW { self.reference.pop_front(); }
        self.since += mic.len();
        if self.since < LAG_EVERY || self.mic.len() < LAG_WINDOW || self.reference.len() < LAG_WINDOW {
            return None;
        }
        self.since = 0;
        let estimate = self.estimate();
        let near = |a: i64, b: i64| (a - b).abs() <= ms16(2);
        match (estimate, self.agreed) {
            (Some(now), Some(agreed)) if near(now, agreed) => self.first_change = None,
            (Some(_), _) if self.first_change.is_none() => self.first_change = Some(self.seen),
            _ => {}
        }
        let lag = match (estimate, self.last) {
            (Some(now), Some(before)) if near(now, before) => Some((now + before) / 2),
            _ => None,
        };
        self.last = estimate;
        let lag = lag.filter(|&lag| self.agreed.map_or(true, |agreed| !near(lag, agreed)))?;
        let from = self.first_change.take().unwrap_or(self.seen) - ms16(375);
        self.agreed = Some(lag);
        Some((lag, from))
    }

    fn estimate(&mut self) -> Option<i64> {
        let n = 2 * LAG_WINDOW;
        let fft = self.planner.plan_fft_forward(n);
        let ifft = self.planner.plan_fft_inverse(n);
        let mut a: Vec<f32> = self.mic.iter().copied().chain(std::iter::repeat(0.0)).take(n).collect();
        let mut b: Vec<f32> = self.reference.iter().copied().chain(std::iter::repeat(0.0)).take(n).collect();
        // Need real playback in the window (mean power above -60 dBFS).
        if energy(&b[..LAG_WINDOW]) < 1e-6 * LAG_WINDOW as f64 {
            return None;
        }
        let mut fa = fft.make_output_vec();
        let mut fb = fft.make_output_vec();
        fft.process(&mut a, &mut fa).ok()?;
        fft.process(&mut b, &mut fb).ok()?;
        let mut cross: Vec<_> = fa.iter().zip(&fb).map(|(x, y)| { let v = x * y.conj(); v / (v.norm() + 1e-9) }).collect();
        let mut corr = vec![0.0f32; n];
        ifft.process(&mut cross, &mut corr).ok()?;
        let at = |lag: i64| corr[lag.rem_euclid(n as i64) as usize];
        let range = ms16(MIN_LAG_MS)..=ms16(MAX_LAG_MS);
        let (best, peak) = range.clone().map(|l| (l, at(l))).max_by(|x, y| x.1.total_cmp(&y.1))?;
        let values: Vec<f32> = range.filter(|l| (l - best).abs() > ms16(5)).map(|l| at(l).abs()).collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread = (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt();
        ((peak - mean) / spread.max(1e-12) >= LAG_MIN_CONFIDENCE).then_some(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAD: usize = 48_000 * LEAD_MS / 1000;

    fn lcg(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 33) as f32 / (1u64 << 31) as f32) - 0.5
    }

    /// Speech-like test signal: noise in 250 ms bursts.
    fn bursts(n: usize, seed: u64) -> Vec<f32> {
        let mut s = seed;
        (0..n).map(|i| if (i / 12_000) % 2 == 0 { lcg(&mut s) * 0.5 } else { 0.0 }).collect()
    }

    fn delayed(x: &[f32], d: usize, gain: f32) -> Vec<f32> {
        (0..x.len()).map(|i| if i >= d { x[i - d] * gain } else { 0.0 }).collect()
    }

    /// Run windows of 600 ms through the canceller, like the pipeline does.
    /// Returns (cleaned mic, system).
    fn run(aec: &mut MicEchoCanceller, mic: &[f32], sys: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let (mut mo, mut so) = (Vec::new(), Vec::new());
        for (m, s) in mic.chunks(28_800).zip(sys.chunks(28_800)) {
            let (mut m, mut s) = (m.to_vec(), s.to_vec());
            aec.process(&mut m, &mut s);
            mo.extend(m);
            so.extend(s);
        }
        (mo, so)
    }

    fn best_lag(a: &[f32], b: &[f32], max: usize) -> usize {
        (0..max).max_by(|&x, &y| {
            let c = |d: usize| b.iter().zip(a.iter().skip(d)).map(|(p, q)| p * q).sum::<f32>();
            c(x).total_cmp(&c(y))
        }).unwrap()
    }

    fn e(s: &[f32]) -> f64 {
        s.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() + 1e-12
    }

    /// Scale-invariant signal-to-distortion ratio of `est` against `target`.
    fn si_sdr(est: &[f32], target: &[f32]) -> f64 {
        let dot: f64 = est.iter().zip(target).map(|(&a, &b)| a as f64 * b as f64).sum();
        let alpha = dot / e(target);
        let noise: f64 = est.iter().zip(target).map(|(&a, &b)| (a as f64 - alpha * b as f64).powi(2)).sum::<f64>() + 1e-12;
        10.0 * (alpha * alpha * e(target) / noise).log10()
    }

    #[test]
    fn resampling_round_trip_has_known_delay() {
        // Band-limited noise (a pure tone would match a later cycle too).
        let noise = bursts(48_000, 5);
        let input = Interpolator::new().process(&Decimator::new().process(&noise));
        let out = Interpolator::new().process(&Decimator::new().process(&input));
        let d = best_lag(&out, &input[..40_000], 200);
        assert_eq!(d, 2 * FIR_DELAY_48, "round trip delay");
        let ratio = e(&out[d..d + 40_000]) / e(&input[..40_000]);
        assert!((0.9..1.1).contains(&ratio), "gain {ratio}");
    }

    #[test]
    fn band_split_low_band_matches_the_round_trip() {
        let x = bursts(48_000, 11);
        let (delayed_x, low) = BandSplit::new().process(&x);
        assert_eq!(&delayed_x[BandSplit::DELAY..], &x[..x.len() - BandSplit::DELAY]);
        let trip = Interpolator::new().process(&Decimator::new().process(&x));
        // Same filter twice: only the round trip's resampling images differ.
        let r = 2_000..46_000;
        assert!(si_sdr(&trip[r.clone()], &low[r]) > 30.0);
    }

    #[test]
    fn lag_estimator_locks_on_long_delays() {
        // Bluetooth or AirPlay speakers: 300 and 450 ms.
        for delay in [4_800usize, 7_200] {
            let reference = bursts(16_000 * 6, 4);
            let mic = delayed(&reference, delay, 0.4);
            let mut est = LagEstimator::new();
            let mut locked = None;
            for (m, r) in mic.chunks(800).zip(reference.chunks(800)) {
                if let Some((l, _)) = est.observe(m, r) { locked = Some(l); }
            }
            let l = locked.expect("no lock");
            assert!((l - delay as i64).abs() <= 32, "delay {delay}: locked at {l}");
        }
    }

    #[test]
    fn lag_estimator_locks_on_the_echo_delay() {
        let reference = bursts(16_000 * 6, 3);
        let mic = delayed(&reference, 1_712, 0.4); // 107 ms
        let mut est = LagEstimator::new();
        let mut locked = None;
        for (m, r) in mic.chunks(800).zip(reference.chunks(800)) {
            if let Some((l, _)) = est.observe(m, r) { locked = Some(l); }
        }
        let l = locked.expect("no lock");
        assert!((l - 1_712).abs() <= 32, "locked at {l}");
    }

    #[test]
    fn reference_arrives_in_time_for_every_supported_lag() {
        for lag_ms in [MIN_LAG_MS, 0, 10, 107, MAX_LAG_MS] {
            let mut path = NeuralPath::new(LEAD).unwrap();
            path.lag_in_use = ms16(lag_ms);
            let x = bursts(48_000 * 2, 1);
            for w in x.chunks(2400) {
                path.lag.since = 0; // keep the lag fixed
                path.process(w, w);
            }
            assert_eq!(path.missing_reference, 0, "lag {lag_ms} ms");
        }
    }

    #[test]
    fn tracks_stay_aligned_and_delayed_by_lead() {
        let mut aec = MicEchoCanceller::new(48_000).unwrap();
        // A noise burst on the mic with silent speakers comes out `LEAD` later.
        let burst = bursts(2_400, 9);
        let mic: Vec<f32> = (0..96_000).map(|i| if (20_000..22_400).contains(&i) { burst[i - 20_000] } else { 0.0 }).collect();
        let sys: Vec<f32> = (0..96_000).map(|i| i as f32 * 1e-9).collect();
        let (mo, so) = run(&mut aec, &mic, &sys);
        assert_eq!(so[LEAD + 1], 1e-9, "system track delay");
        assert_eq!(best_lag(&mo, &mic, LEAD + 400), LEAD, "cleaned mic delay");
        let (mt, st) = aec.drain().expect("pending audio");
        assert_eq!((mt.len(), st.len()), (LEAD, LEAD));
        assert!(aec.drain().is_none(), "a second flush adds nothing");
    }

    /// Real built-in mic/speaker takes (48 kHz raw f32le files in
    /// $AEC_EVAL_DIR: voice48.f32, echo_mic48.f32, echo_ref48.f32), mixed so
    /// the clean voice is known.
    #[test]
    #[ignore]
    fn real_recordings_report() {
        let dir = std::env::var("AEC_EVAL_DIR").expect("AEC_EVAL_DIR");
        let read = |name: &str| -> Vec<f32> {
            std::fs::read(format!("{dir}/{name}.f32")).unwrap()
                .chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
        };
        let (voice, echo_mic, echo_ref) = (read("voice48"), read("echo_mic48"), read("echo_ref48"));
        let n = voice.len().min(echo_mic.len()).min(echo_ref.len());
        let mix: Vec<f32> = (0..n).map(|i| voice[i] + echo_mic[i]).collect();
        let fresh = || MicEchoCanceller::new(48_000).unwrap();
        let shifted = |s: &[f32], r: &std::ops::Range<usize>| s[r.start - LEAD..r.end - LEAD].to_vec();

        let (echo_out, _) = run(&mut fresh(), &echo_mic[..n], &echo_ref[..n]);
        // A new playback stream can reach the speakers with a different delay:
        // move the echo 50 ms later (and earlier) at 10 s and see how fast the
        // canceller recovers.
        for jump in [2_400i64, -2_400] {
            let at = 48_000 * 10;
            let mic: Vec<f32> = (0..n)
                .map(|i| if i < at { echo_mic[i] } else { echo_mic.get((i as i64 - jump) as usize).copied().unwrap_or(0.0) })
                .collect();
            let (out, _) = run(&mut fresh(), &mic, &echo_ref[..n]);
            let report: Vec<String> = (0..8)
                .map(|k| {
                    let w = at + 24_000 * k + LEAD..at + 24_000 * (k + 1) + LEAD;
                    format!("{:.0}", 10.0 * (e(&shifted(&mic, &w)) / e(&out[w])).log10())
                })
                .collect();
            println!("lag jump {:+} ms at 10 s, dB removed per 0.5 s after: {}", jump / 48, report.join(" "));
        }
        for (a, b) in [(0usize, 1usize), (1, 2), (2, 3), (3, 4)] {
            let w = 48_000 * a + LEAD..48_000 * b + LEAD;
            println!("  second {a}-{b}: echo removed {:.1} dB", 10.0 * (e(&shifted(&echo_mic, &w)) / e(&echo_out[w])).log10());
        }
        let r = 48_000 * 4..n; // lag lock + convergence
        println!("echo only: removed {:.1} dB", 10.0 * (e(&shifted(&echo_mic, &r)) / e(&echo_out[r.clone()])).log10());
        let (mix_out, _) = run(&mut fresh(), &mix, &echo_ref[..n]);
        println!("double talk: voice SI-SDR {:.1} dB", si_sdr(&mix_out[r.clone()], &shifted(&voice, &r)));
        let silent = vec![0.0; n];
        let high = |s: &[f32]| { let (d, l) = BandSplit::new().process(s); d.iter().zip(&l).map(|(a, b)| a - b).collect::<Vec<f32>>() };
        for (label, reference) in [("silent speakers", &silent[..]), ("playback, no echo (headphones)", &echo_ref[..n])] {
            let (out, _) = run(&mut fresh(), &voice[..n], reference);
            println!("voice only, {label}: SI-SDR {:.1} dB, energy above 7 kHz kept {:.0}%",
                si_sdr(&out[r.clone()], &shifted(&voice, &r)),
                100.0 * e(&high(&out[r.clone()])) / e(&high(&shifted(&voice, &r))));
        }
    }
}
