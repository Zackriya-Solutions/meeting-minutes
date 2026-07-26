use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use silero_rs::{VadConfig, VadSession, VadTransition};
use std::collections::VecDeque;
use std::time::Duration;

/// How the segmenter decided this audio was worth transcribing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentOrigin {
    /// Silero reported a speech region.
    Vad,
    /// Silero reported nothing for a long stretch that still carried energy well
    /// above the room's noise floor, so the audio was recovered rather than lost.
    Recovered,
}

/// Represents a complete speech segment detected by VAD
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    pub start_timestamp_ms: f64,
    pub end_timestamp_ms: f64,
    pub confidence: f32,
    pub origin: SegmentOrigin,
}

/// Longest span of clean audio kept for slicing segments out of.
const HISTORY_MAX_SECONDS: f64 = 60.0;
/// Unclaimed audio is handed over once it reaches this much.
///
/// Bounds worst-case loss: before the ledger existed, a stretch Silero never
/// flagged was gone for good, however long it ran.
const UNCLAIMED_EMIT_SECONDS: f64 = 6.0;
/// Never recover a span shorter than this; below it there is nothing to transcribe.
const UNCLAIMED_MIN_SECONDS: f64 = 1.0;
/// RMS a span must clear before it is worth recovering.
///
/// Measured on a real recording: the room's noise floor sat at 0.003-0.007 RMS
/// while speech Silero missed ran 0.045-0.077, so this sits well clear of both.
const RECOVER_ABSOLUTE_RMS: f32 = 0.012;
/// ...and it must also stand this far above the tracked noise floor, so a noisy
/// room does not turn every silence into a recovered segment.
const RECOVER_NOISE_RATIO: f32 = 3.0;
/// Pitch strength a frame needs before it counts as speech worth recovering.
///
/// Sits between the two populations measured on a real recording: speech frames
/// scored 0.26-0.73, room noise 0.04-0.13.
const VOICING_THRESHOLD: f32 = 0.20;
/// RMS the detector copy is driven towards. Silero's probabilities collapse on
/// quiet input; the transcriber still receives the untouched audio.
const DETECTOR_TARGET_RMS: f32 = 0.08;
/// Ceiling on the detector gain, so a silent room is not amplified into noise.
const DETECTOR_MAX_GAIN: f32 = 8.0;

/// Processes audio in 30ms chunks but returns complete speech segments
pub struct ContinuousVadProcessor {
    session: VadSession,
    chunk_size: usize,
    sample_rate: u32,
    /// Stateful 48k->16k resampler. Rebuilding one per call loses filter state
    /// at every boundary, which the capture path already learned the hard way.
    resampler: Option<SincFixedIn<f32>>,
    resampler_chunk: usize,
    resampler_input: Vec<f32>,
    buffer: Vec<f32>,
    speech_segments: VecDeque<SpeechSegment>,
    current_speech: Vec<f32>,
    in_speech: bool,
    processed_samples: usize,
    speech_start_sample: usize,

    /// Untouched 16 kHz audio, so segments handed to the transcriber never carry
    /// the detector's gain. `history_start_sample` is the absolute index of
    /// `history[0]`.
    history: Vec<f32>,
    history_start_sample: usize,
    /// Absolute 16 kHz index every emitted segment has accounted for.
    covered_through_sample: usize,
    /// Slowly tracked room noise level, used to judge unclaimed spans.
    noise_floor: f32,
    /// Smoothed detector gain, so the level presented to Silero does not jump.
    detector_gain: f32,

    /// Resampled 16 kHz audio not yet handed to a streaming transcriber.
    ///
    /// The stream has to be resampled exactly once. Resampling it a second time
    /// elsewhere would mean a second filter with its own state, and resampling each
    /// 560 ms step independently would put a boundary artefact at every step. This
    /// carries the output of the resampler that already ran.
    resampled_out: Vec<f32>,

    // State tracking for smart logging
    last_logged_state: bool,
}

impl ContinuousVadProcessor {
    pub fn new(input_sample_rate: u32, redemption_time_ms: u32) -> Result<Self> {
        // Silero VAD MUST use 16kHz - this is hardcoded requirement
        const VAD_SAMPLE_RATE: u32 = 16000;

        // Use STRICT settings to prevent silence from reaching Whisper
        let mut config = VadConfig::default();
        config.sample_rate = VAD_SAMPLE_RATE as usize;

        // CONTINUOUS SPEECH FIX: Tuned for capturing complete 5+ second utterances
        // Previous: 0.55/0.40 with 400ms redemption was fragmenting speech into 40ms segments
        // New: More lenient thresholds + longer redemption for continuous speech
        config.positive_speech_threshold = 0.50;  // Silero default - good for continuous speech
        config.negative_speech_threshold = 0.35;  // Silero default - allows natural pauses

        // CRITICAL FIX: Removed redemption_time capping to support long continuous speech
        // Previous: capped at 400ms, causing VAD to fragment 5-second speech into 40ms segments
        // New: Use full redemption_time from pipeline (2000ms) to bridge natural pauses
        config.redemption_time = Duration::from_millis(redemption_time_ms as u64);
        config.pre_speech_pad = Duration::from_millis(300);   // Pre-speech padding for context
        config.post_speech_pad = Duration::from_millis(400);  // Increased: more context at end

        // CRITICAL FIX: Increased min_speech_time to prevent tiny 40ms fragments
        // Previous: 100ms allowed too-short segments that Whisper rejects
        // New: 250ms ensures segments are substantial enough for Whisper (>100ms requirement)
        config.min_speech_time = Duration::from_millis(250);  // Prevent tiny fragments

        debug!("Creating VAD session with: sample_rate={}Hz, redemption={}ms, min_speech={}ms, input_rate={}Hz",
               VAD_SAMPLE_RATE, redemption_time_ms, 250, input_sample_rate);

        let session = VadSession::new(config)
            .map_err(|e| anyhow!("Failed to create VAD session: {:?}", e))?;

        // VAD uses 30ms chunks at 16kHz (480 samples)
        let vad_chunk_size = (VAD_SAMPLE_RATE as f32 * 0.03) as usize; // 480 samples

        // A proper band-limited resampler. The moving average this replaces was
        // only ~-14 dB at the 8 kHz Nyquist, so everything above it folded back
        // into the speech band as noise and depressed Silero's probabilities.
        const RESAMPLER_CHUNK: usize = 1024;
        let resampler = if input_sample_rate == VAD_SAMPLE_RATE {
            None
        } else {
            let params = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            match SincFixedIn::<f32>::new(
                VAD_SAMPLE_RATE as f64 / input_sample_rate as f64,
                1.0,
                params,
                RESAMPLER_CHUNK,
                1,
            ) {
                Ok(r) => Some(r),
                Err(e) => {
                    // Losing anti-aliasing hurts accuracy but is better than
                    // losing audio, so fall back to the naive path.
                    warn!("Failed to build VAD resampler ({}), falling back to decimation", e);
                    None
                }
            }
        };

        info!("VAD processor created: input={}Hz, vad={}Hz, chunk_size={} samples, band-limited resampler={}",
              input_sample_rate, VAD_SAMPLE_RATE, vad_chunk_size, resampler.is_some());

        Ok(Self {
            session,
            chunk_size: vad_chunk_size,
            sample_rate: input_sample_rate, // Store input rate for resampling ratio in resample_to_16k()
            resampler,
            resampler_chunk: RESAMPLER_CHUNK,
            resampler_input: Vec::with_capacity(RESAMPLER_CHUNK * 2),
            buffer: Vec::with_capacity(vad_chunk_size * 2),
            speech_segments: VecDeque::new(),
            current_speech: Vec::new(),
            in_speech: false,
            processed_samples: 0,
            speech_start_sample: 0,
            history: Vec::new(),
            history_start_sample: 0,
            covered_through_sample: 0,
            noise_floor: 0.005,
            detector_gain: 1.0,
            resampled_out: Vec::new(),
            last_logged_state: false,
        })
    }

    /// Absolute 16 kHz sample index for a session timestamp.
    fn ms_to_sample(ms: f64) -> usize {
        (ms.max(0.0) * 16.0) as usize
    }

    /// Untouched audio for an absolute 16 kHz range, as far as history still holds it.
    fn slice_clean(&self, start_sample: usize, end_sample: usize) -> Vec<f32> {
        let from = start_sample.saturating_sub(self.history_start_sample);
        let to = end_sample.saturating_sub(self.history_start_sample);
        let from = from.min(self.history.len());
        let to = to.min(self.history.len());
        if to <= from {
            return Vec::new();
        }
        self.history[from..to].to_vec()
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Drops history that no longer backs any pending decision.
    fn prune_history(&mut self) {
        let max_samples = (HISTORY_MAX_SECONDS * 16000.0) as usize;
        if self.history.len() <= max_samples {
            return;
        }

        // Keep everything the ledger and any in-flight utterance still need.
        let keep_from = self
            .covered_through_sample
            .min(self.speech_start_sample)
            .max(self.processed_samples.saturating_sub(max_samples));
        let drop_to = keep_from.saturating_sub(self.history_start_sample);
        if drop_to == 0 {
            return;
        }

        self.history.drain(..drop_to.min(self.history.len()));
        self.history_start_sample += drop_to;
    }

    /// Hands over audio Silero never flagged, once enough of it has piled up and
    /// it clearly carries more than room noise.
    ///
    /// This is not a maximum segment length: it only fires where VAD produced
    /// *nothing*, so the alternative is not a cleaner cut, it is silence.
    fn recover_unclaimed(&mut self) -> Option<SpeechSegment> {
        if self.in_speech {
            return None;
        }

        let unclaimed = self.processed_samples.saturating_sub(self.covered_through_sample);
        if (unclaimed as f64 / 16000.0) < UNCLAIMED_EMIT_SECONDS {
            return None;
        }

        let start = self.covered_through_sample;
        let end = self.processed_samples;
        let samples = self.slice_clean(start, end);

        // Nothing left in history to recover: give up on the span rather than
        // repeatedly re-testing it.
        if (samples.len() as f64 / 16000.0) < UNCLAIMED_MIN_SECONDS {
            self.covered_through_sample = end;
            return None;
        }

        self.covered_through_sample = end;

        // Trim to the part that actually carries energy. Handing the model a
        // block that is half silence invites hallucinated filler, which is worse
        // than the gap it was meant to fix.
        let Some((from, to)) = self.speech_span(&samples) else {
            debug!(
                "VAD ledger: {:.1}s unclaimed but nothing above the floor ({:.4}) - treated as silence",
                samples.len() as f64 / 16000.0,
                self.noise_floor
            );
            return None;
        };

        let trimmed = &samples[from..to];
        if (trimmed.len() as f64 / 16000.0) < UNCLAIMED_MIN_SECONDS {
            return None;
        }

        info!(
            "🛟 VAD ledger: recovering {:.1}s Silero did not flag (trimmed from {:.1}s, {:.4} RMS vs {:.4} floor)",
            trimmed.len() as f64 / 16000.0,
            samples.len() as f64 / 16000.0,
            Self::rms(trimmed),
            self.noise_floor
        );

        Some(SpeechSegment {
            start_timestamp_ms: (start + from) as f64 / 16.0,
            end_timestamp_ms: (start + to) as f64 / 16.0,
            samples: trimmed.to_vec(),
            confidence: 0.5,
            origin: SegmentOrigin::Recovered,
        })
    }

    /// Strength of the strongest pitch period in a frame, 0.0 to 1.0.
    ///
    /// Voiced speech repeats at its pitch, so its autocorrelation has a clear
    /// peak somewhere in the 80-400 Hz range; broadband room noise has none.
    /// Measured on a real recording, speech frames scored 0.26-0.73 while noise
    /// frames scored 0.04-0.13, which is what [`VOICING_THRESHOLD`] sits between.
    fn voicing(frame: &[f32]) -> f32 {
        const MIN_LAG: usize = 40; // 400 Hz at 16 kHz
        const MAX_LAG: usize = 200; // 80 Hz at 16 kHz

        if frame.len() <= MAX_LAG {
            return 0.0;
        }

        let mean = frame.iter().sum::<f32>() / frame.len() as f32;
        let centred: Vec<f32> = frame.iter().map(|s| s - mean).collect();

        let energy: f32 = centred.iter().map(|s| s * s).sum();
        if energy < 1e-12 {
            return 0.0;
        }

        let mut best = 0.0f32;
        for lag in MIN_LAG..MAX_LAG.min(centred.len() - 1) {
            let correlation: f32 = centred[lag..]
                .iter()
                .zip(centred.iter())
                .map(|(a, b)| a * b)
                .sum();
            best = best.max(correlation / energy);
        }

        best
    }

    /// Narrows a span to the first and last 100 ms frame that looks like speech,
    /// keeping a short margin so word onsets survive the trim.
    ///
    /// Energy alone cannot do this: on the recording used for tuning, speech at
    /// 0.0145 RMS and room noise at 0.0118 RMS were indistinguishable by level,
    /// and only the pitch test told them apart.
    fn speech_span(&self, samples: &[f32]) -> Option<(usize, usize)> {
        const FRAME: usize = 1600; // 100 ms at 16 kHz
        const MARGIN_FRAMES: usize = 3; // 300 ms of context on each side

        let gate = RECOVER_ABSOLUTE_RMS.max(self.noise_floor * RECOVER_NOISE_RATIO);

        let speechy: Vec<usize> = samples
            .chunks(FRAME)
            .enumerate()
            .filter(|(_, frame)| {
                Self::rms(frame) >= gate && Self::voicing(frame) >= VOICING_THRESHOLD
            })
            .map(|(i, _)| i)
            .collect();

        let first = *speechy.first()?;
        let last = *speechy.last()?;

        let from = first.saturating_sub(MARGIN_FRAMES) * FRAME;
        let to = ((last + 1 + MARGIN_FRAMES) * FRAME).min(samples.len());

        Some((from, to))
    }

    /// Process incoming audio samples and return any complete speech segments
    /// Handles resampling from input sample rate to 16kHz for VAD processing
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<SpeechSegment>> {
        // Resample to 16kHz if needed
        let resampled_audio = if self.sample_rate == 16000 {
            samples.to_vec()
        } else {
            self.resample_to_16k(samples)?
        };

        self.history.extend_from_slice(&resampled_audio);
        self.buffer.extend_from_slice(&resampled_audio);
        self.resampled_out.extend_from_slice(&resampled_audio);
        let mut completed_segments = Vec::new();

        // Process complete 30ms chunks (480 samples at 16kHz)
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            self.process_chunk(&chunk)?;

            // Extract any completed speech segments
            while let Some(segment) = self.speech_segments.pop_front() {
                completed_segments.push(segment);
            }
        }

        if let Some(recovered) = self.recover_unclaimed() {
            completed_segments.push(recovered);
        }

        self.prune_history();

        Ok(completed_segments)
    }

    /// Resamples the input rate down to 16 kHz for Silero.
    ///
    /// Uses a band-limited sinc resampler kept alive across calls; the fallback
    /// below only runs if that could not be constructed.
    fn resample_to_16k(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.sample_rate == 16000 {
            return Ok(samples.to_vec());
        }

        if self.resampler.is_some() {
            self.resampler_input.extend_from_slice(samples);
            let mut out = Vec::new();
            let chunk = self.resampler_chunk;

            while self.resampler_input.len() >= chunk {
                let input: Vec<f32> = self.resampler_input.drain(..chunk).collect();
                let resampler = self
                    .resampler
                    .as_mut()
                    .expect("resampler presence checked above");
                match resampler.process(&[input], None) {
                    Ok(mut waves) if !waves.is_empty() => out.append(&mut waves[0]),
                    Ok(_) => {}
                    Err(e) => return Err(anyhow!("VAD resampling failed: {}", e)),
                }
            }

            return Ok(out);
        }

        self.resample_to_16k_fallback(samples)
    }

    /// Naive decimation kept only as a fallback; its anti-aliasing is poor.
    fn resample_to_16k_fallback(&self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.sample_rate == 16000 {
            return Ok(samples.to_vec());
        }

        // Calculate downsampling ratio
        let ratio = self.sample_rate as f64 / 16000.0;
        let output_len = (samples.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);

        // Apply simple low-pass filter before downsampling to reduce aliasing
        let cutoff_freq = 0.4; // Normalized frequency (0.4 * Nyquist)
        let mut filtered_samples = Vec::with_capacity(samples.len());
        
        // Simple moving average filter (basic low-pass)
        let filter_size = (self.sample_rate as f64 / (cutoff_freq * self.sample_rate as f64)) as usize;
        let filter_size = std::cmp::max(1, std::cmp::min(filter_size, 5)); // Limit filter size
        
        for i in 0..samples.len() {
            let start = if i >= filter_size { i - filter_size } else { 0 };
            let end = std::cmp::min(i + filter_size + 1, samples.len());
            let sum: f32 = samples[start..end].iter().sum();
            filtered_samples.push(sum / (end - start) as f32);
        }

        // Linear interpolation downsampling
        for i in 0..output_len {
            let source_pos = i as f64 * ratio;
            let source_index = source_pos as usize;
            let fraction = source_pos - source_index as f64;
            
            if source_index + 1 < filtered_samples.len() {
                // Linear interpolation
                let sample1 = filtered_samples[source_index];
                let sample2 = filtered_samples[source_index + 1];
                let interpolated = sample1 + (sample2 - sample1) * fraction as f32;
                resampled.push(interpolated);
            } else if source_index < filtered_samples.len() {
                resampled.push(filtered_samples[source_index]);
            }
        }

        debug!("Resampled from {} samples ({}Hz) to {} samples (16kHz) with anti-aliasing",
               samples.len(), self.sample_rate, resampled.len());

        Ok(resampled)
    }

    /// Flush any remaining audio and return final speech segments
    pub fn flush(&mut self) -> Result<Vec<SpeechSegment>> {
        debug!("VAD flush: in_speech={}, current_speech_len={}, buffer_len={}, speech_segments_queued={}",
              self.in_speech, self.current_speech.len(), self.buffer.len(), self.speech_segments.len());

        let mut completed_segments = Vec::new();

        // Process any remaining buffered audio
        if !self.buffer.is_empty() {
            let remaining = self.buffer.clone();
            self.buffer.clear();

            // Pad to chunk size if needed
            let mut padded_chunk = remaining;
            if padded_chunk.len() < self.chunk_size {
                padded_chunk.resize(self.chunk_size, 0.0);
            }

            self.process_chunk(&padded_chunk)?;
        }

        // Force end any ongoing speech
        if self.in_speech && !self.current_speech.is_empty() {
            // processed_samples and speech_start_sample always count 16kHz samples (post-resampling)
            let start_ms = (self.speech_start_sample as f64 / 16000.0) * 1000.0;
            let end_ms = (self.processed_samples as f64 / 16000.0) * 1000.0;

            debug!("VAD flush: Force-ending speech - start={}ms, end={}ms, duration={}ms, samples={}",
                  start_ms, end_ms, end_ms - start_ms, self.current_speech.len());

            let segment = SpeechSegment {
                samples: self.current_speech.clone(),
                start_timestamp_ms: start_ms,
                end_timestamp_ms: end_ms,
                confidence: 0.8, // Estimated confidence for forced end
                origin: SegmentOrigin::Vad,
            };

            self.speech_segments.push_back(segment);
            self.covered_through_sample = self.processed_samples;
            self.current_speech.clear();
            self.in_speech = false;
        }

        // Hand over anything the ledger is still holding before shutting down,
        // so the tail of a recording is never silently discarded.
        let leftover = self.processed_samples.saturating_sub(self.covered_through_sample);
        if (leftover as f64 / 16000.0) >= UNCLAIMED_MIN_SECONDS {
            let start = self.covered_through_sample;
            let end = self.processed_samples;
            let samples = self.slice_clean(start, end);
            let level = Self::rms(&samples);
            if !samples.is_empty()
                && level >= RECOVER_ABSOLUTE_RMS
                && level >= self.noise_floor * RECOVER_NOISE_RATIO
            {
                info!(
                    "🛟 VAD flush: recovering {:.1}s of unclaimed audio",
                    samples.len() as f64 / 16000.0
                );
                self.speech_segments.push_back(SpeechSegment {
                    start_timestamp_ms: start as f64 / 16.0,
                    end_timestamp_ms: end as f64 / 16.0,
                    samples,
                    confidence: 0.5,
                    origin: SegmentOrigin::Recovered,
                });
            }
            self.covered_through_sample = end;
        }

        // Extract all remaining segments
        while let Some(segment) = self.speech_segments.pop_front() {
            completed_segments.push(segment);
        }

        Ok(completed_segments)
    }

    /// Speech accumulated since the current utterance began, while the speaker is still
    /// going. `None` once they pause, because the utterance is then delivered as a
    /// completed segment instead.
    /// Take the 16 kHz audio resampled since the last call.
    ///
    /// This is the whole mixed stream, speech and silence alike - deliberately not
    /// the VAD's opinion of it. VAD forwarded roughly two thirds of the audio, and a
    /// streaming model does not need the other third withheld: silence decodes to an
    /// empty piece on its own.
    pub fn drain_resampled_16k(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.resampled_out)
    }

    pub fn speech_in_progress(&self) -> Option<&[f32]> {
        if self.in_speech && !self.current_speech.is_empty() {
            Some(&self.current_speech)
        } else {
            None
        }
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<()> {
        // Track accumulated speech buffer size to detect memory issues
        let current_speech_size = self.current_speech.len();
        if current_speech_size > 1_000_000 {
            // More than ~62 seconds of accumulated speech at 16kHz
            warn!("VAD: Accumulated speech buffer is large: {} samples ({:.1}s) - possible memory issue",
                  current_speech_size, current_speech_size as f64 / 16000.0);
        }

        let level = Self::rms(chunk);

        // Track the quietest recent level as the room's floor: fall fast towards
        // a new quiet, rise slowly so a long utterance does not drag it up.
        if level < self.noise_floor {
            self.noise_floor = self.noise_floor * 0.9 + level * 0.1;
        } else {
            self.noise_floor = self.noise_floor * 0.999 + level * 0.001;
        }
        self.noise_floor = self.noise_floor.clamp(1e-5, 0.05);

        // Detector-only conditioning. The transcriber is served from `history`,
        // so none of this reaches the model.
        let wanted = if level > 1e-6 {
            (DETECTOR_TARGET_RMS / level).clamp(1.0, DETECTOR_MAX_GAIN)
        } else {
            1.0
        };
        self.detector_gain = self.detector_gain * 0.9 + wanted * 0.1;

        let detector_chunk: Vec<f32> = chunk
            .iter()
            .map(|s| {
                let v = s * self.detector_gain;
                // Silero rejects a whole frame containing a non-finite or
                // out-of-range sample, and that check only exists in debug
                // builds - so sanitise rather than trust the caller.
                if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect();

        let transitions = match self.session.process(&detector_chunk) {
            Ok(transitions) => transitions,
            Err(e) => {
                // Do not drop the audio: leaving `covered_through_sample` where
                // it is means the ledger will hand this span over instead.
                warn!("VAD rejected a frame ({}); leaving it to the ledger", e);
                self.processed_samples += chunk.len();
                if self.in_speech {
                    self.current_speech.extend_from_slice(chunk);
                }
                return Ok(());
            }
        };

        // Log transitions for debugging
        if !transitions.is_empty() {
            debug!("VAD transitions at sample {}: {} transitions", self.processed_samples, transitions.len());
        }

        // Handle VAD transitions
        for transition in transitions {
            match transition {
                VadTransition::SpeechStart { timestamp_ms } => {
                    // Only log if state changed
                    if !self.last_logged_state {
                        debug!("VAD: Speech started at {}ms", timestamp_ms);
                        self.last_logged_state = true;
                    }
                    self.in_speech = true;
                    // Silero reports session-absolute timestamps, so this is
                    // already the absolute 16 kHz index; adding the current
                    // position (as this once did) counted the offset twice.
                    self.speech_start_sample = Self::ms_to_sample(timestamp_ms as f64);
                    self.current_speech.clear();
                }
                VadTransition::SpeechEnd { start_timestamp_ms, end_timestamp_ms, samples } => {
                    // Only log if we were previously in speech state
                    if self.last_logged_state {
                        debug!("VAD: Speech ended at {}ms (duration: {}ms)", end_timestamp_ms, end_timestamp_ms - start_timestamp_ms);
                        self.last_logged_state = false;
                    }
                    self.in_speech = false;

                    // Slice the untouched audio rather than taking Silero's copy:
                    // the session was fed a gain-conditioned signal, and the
                    // transcriber must never see that.
                    let start_sample = Self::ms_to_sample(start_timestamp_ms as f64);
                    let end_sample = Self::ms_to_sample(end_timestamp_ms as f64);

                    // History is bounded, so an utterance longer than
                    // HISTORY_MAX_SECONDS no longer has its opening in there.
                    // Slicing anyway would quietly drop the first words, which is
                    // the exact failure this whole change exists to remove.
                    let history_covers_start = start_sample >= self.history_start_sample;
                    let mut speech_samples = if history_covers_start {
                        self.slice_clean(start_sample, end_sample)
                    } else {
                        Vec::new()
                    };

                    // Fall back to what was accumulated, then to Silero's own
                    // copy. Both hold the whole utterance.
                    if speech_samples.is_empty() {
                        speech_samples = if !self.current_speech.is_empty() {
                            self.current_speech.clone()
                        } else {
                            samples
                        };
                    }

                    if !speech_samples.is_empty() {
                        let segment = SpeechSegment {
                            samples: speech_samples,
                            start_timestamp_ms: start_timestamp_ms as f64,
                            end_timestamp_ms: end_timestamp_ms as f64,
                            confidence: 0.9, // VAD confidence
                            origin: SegmentOrigin::Vad,
                        };

                        info!("VAD: Completed speech segment: {:.1}ms duration, {} samples",
                              end_timestamp_ms - start_timestamp_ms, segment.samples.len());

                        self.speech_segments.push_back(segment);
                    }

                    // Everything up to here is now accounted for.
                    self.covered_through_sample = self.covered_through_sample.max(end_sample);
                    self.current_speech.clear();
                }
            }
        }

        // Accumulate speech if we're currently in a speech state
        if self.in_speech {
            self.current_speech.extend_from_slice(chunk);
        }

        self.processed_samples += chunk.len();
        Ok(())
    }
}

/// Legacy function for backward compatibility - now uses the optimized approach
pub fn extract_speech_16k(samples_mono_16k: &[f32]) -> Result<Vec<f32>> {
    let mut processor = ContinuousVadProcessor::new(16000, 400)?;

    // Process all audio
    let mut all_segments = processor.process_audio(samples_mono_16k)?;
    let final_segments = processor.flush()?;
    all_segments.extend(final_segments);

    // Concatenate all speech segments
    let mut result = Vec::new();
    let num_segments = all_segments.len();
    for segment in &all_segments {
        result.extend_from_slice(&segment.samples);
    }

    // Apply balanced energy filtering for very short segments
    if result.len() < 1600 { // Less than 100ms at 16kHz
        let input_energy: f32 = samples_mono_16k.iter().map(|&x| x * x).sum::<f32>() / samples_mono_16k.len() as f32;
        let rms = input_energy.sqrt();
        let peak = samples_mono_16k.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);

        // BALANCED FIX: Lowered thresholds to preserve quiet speech while still filtering silence
        // Previous aggressive values (0.08/0.15) were discarding valid quiet speech
        // New values (0.03/0.08) are more balanced - catch quiet speech, reject pure silence
        if rms < 0.2 || peak < 0.20 {
            info!("-----VAD detected silence/noise (RMS: {:.6}, Peak: {:.6}), skipping to prevent hallucinations-----", rms, peak);
            return Ok(Vec::new());
        } else {
            info!("VAD detected speech with sufficient energy (RMS: {:.6}, Peak: {:.6})", rms, peak);
            return Ok(samples_mono_16k.to_vec());
        }
    }

    debug!("VAD: Processed {} samples, extracted {} speech samples from {} segments",
           samples_mono_16k.len(), result.len(), num_segments);

    Ok(result)
}

/// Simple convenience function to get speech chunks from audio
/// Uses the optimized ContinuousVadProcessor with configurable redemption time
pub fn get_speech_chunks(samples_mono_16k: &[f32], redemption_time_ms: u32) -> Result<Vec<SpeechSegment>> {
    get_speech_chunks_with_progress(samples_mono_16k, redemption_time_ms, |_, _| true)
}

/// Get speech chunks with progress callback and cancellation support
/// The callback receives (progress_percent, segments_found) and returns false to cancel
pub fn get_speech_chunks_with_progress<F>(
    samples_mono_16k: &[f32],
    redemption_time_ms: u32,
    mut progress_callback: F,
) -> Result<Vec<SpeechSegment>>
where
    F: FnMut(u32, usize) -> bool,
{
    let mut processor = ContinuousVadProcessor::new(16000, redemption_time_ms)?;

    let total_samples = samples_mono_16k.len();

    // For large files (>1 minute at 16kHz = 960,000 samples), process in chunks with progress logging
    const LARGE_FILE_THRESHOLD: usize = 960_000;
    const CHUNK_SIZE: usize = 160_000; // 10 seconds at 16kHz

    let mut all_segments = Vec::new();

    if total_samples > LARGE_FILE_THRESHOLD {
        info!("VAD: Processing large file ({} samples = {:.1}s), will log progress...",
              total_samples, total_samples as f64 / 16000.0);

        let mut processed = 0;
        let mut last_progress = 0u32;
        let mut chunk_count = 0;
        let total_chunks = (total_samples + CHUNK_SIZE - 1) / CHUNK_SIZE;

        for chunk in samples_mono_16k.chunks(CHUNK_SIZE) {
            chunk_count += 1;

            let start_time = std::time::Instant::now();
            let segments = processor.process_audio(chunk)?;
            let elapsed = start_time.elapsed();

            // Debug log for chunk processing details
            debug!("VAD: Chunk {}/{} processed in {:?}, found {} segments",
                  chunk_count, total_chunks, elapsed, segments.len());

            // Warn if chunk processing took too long (>1 second)
            if elapsed.as_secs() > 1 {
                warn!("VAD: Chunk {} took {:?} - possible performance issue", chunk_count, elapsed);
            }

            all_segments.extend(segments);

            processed += chunk.len();
            let progress = ((processed * 100) / total_samples) as u32;

            // Call progress callback every 5%
            if progress >= last_progress + 5 {
                debug!("VAD: Progress {}% ({} segments found so far)", progress, all_segments.len());

                // Check for cancellation
                if !progress_callback(progress, all_segments.len()) {
                    info!("VAD: Cancelled by callback at {}%", progress);
                    return Err(anyhow!("VAD processing cancelled"));
                }

                last_progress = progress;
            }
        }

        let final_segments = processor.flush()?;
        all_segments.extend(final_segments);

        info!("VAD: Complete! Found {} speech segments", all_segments.len());
    } else {
        // Small file - process all at once
        all_segments = processor.process_audio(samples_mono_16k)?;
        let final_segments = processor.flush()?;
        all_segments.extend(final_segments);
    }

    Ok(all_segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generate synthetic speech-like audio with alternating speech/silence
    fn generate_test_audio_with_speech(duration_seconds: f32, sample_rate: u32) -> Vec<f32> {
        let total_samples = (duration_seconds * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];

        // Create speech-like patterns: bursts of sine waves with varying amplitude
        // Speech every 10 seconds for 5 seconds
        let speech_interval = 10.0; // seconds between speech starts
        let speech_duration = 5.0;  // seconds of speech

        for i in 0..total_samples {
            let time = i as f32 / sample_rate as f32;
            let cycle_time = time % speech_interval;

            // Speech occurs in the first `speech_duration` seconds of each cycle
            if cycle_time < speech_duration {
                // Generate speech-like signal: multiple frequencies with amplitude modulation
                let freq1 = 200.0 + (time * 50.0).sin() * 100.0; // Varying fundamental
                let freq2 = freq1 * 2.0; // Harmonic
                let freq3 = freq1 * 3.0; // Another harmonic

                let amplitude = 0.3 + 0.1 * (time * 5.0).sin(); // Amplitude modulation
                samples[i] = amplitude * (
                    0.5 * (2.0 * std::f32::consts::PI * freq1 * time).sin() +
                    0.3 * (2.0 * std::f32::consts::PI * freq2 * time).sin() +
                    0.2 * (2.0 * std::f32::consts::PI * freq3 * time).sin()
                );
            }
            // else: silence (already 0.0)
        }

        samples
    }

    // A maximum segment length was tried here so a long monologue would not leave the
    // screen blank until the speaker stopped. Measured with whisper large-v3 on a real
    // recording, capping at 6 s turned
    //   "Oh my God. $10,000 up on A&B here today. It's amazing. People are kicking up
    //    with their stocks. They just want to buy more and more..."
    // into "amazing" plus "more and more people feel like building with". Cutting an
    // utterance at an arbitrary point costs far more accuracy than the latency is worth,
    // so segments stay bounded by the speaker's own pauses.

    /// Deterministic pseudo-noise, so the tests do not depend on a rng crate.
    fn noise(len: usize, level: f32) -> Vec<f32> {
        let mut state: u32 = 0x1234_5678;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((state >> 8) as f32 / 8_388_608.0 - 1.0) * level
            })
            .collect()
    }

    /// A sawtooth has a clear pitch period and rich harmonics, which is the
    /// property the recovery gate keys on.
    fn pitched(len: usize, hz: f32, level: f32, sample_rate: f32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let phase = (i as f32 * hz / sample_rate).fract();
                (phase * 2.0 - 1.0) * level
            })
            .collect()
    }

    #[test]
    fn voicing_separates_pitched_audio_from_noise() {
        let frame = 1600;
        let speech = pitched(frame, 150.0, 0.08, 16000.0);
        let hiss = noise(frame, 0.08);

        let speech_score = ContinuousVadProcessor::voicing(&speech);
        let noise_score = ContinuousVadProcessor::voicing(&hiss);

        assert!(
            speech_score >= VOICING_THRESHOLD,
            "pitched audio should read as voiced, got {speech_score}"
        );
        assert!(
            noise_score < VOICING_THRESHOLD,
            "noise should not read as voiced, got {noise_score}"
        );
    }

    #[test]
    fn voicing_ignores_silence() {
        assert_eq!(ContinuousVadProcessor::voicing(&vec![0.0; 1600]), 0.0);
        // Too short to hold a pitch period at all.
        assert_eq!(ContinuousVadProcessor::voicing(&[0.1; 100]), 0.0);
    }

    /// The whole point of the ledger: audible, pitched content reaches the
    /// transcriber whether or not Silero decides to flag it.
    #[test]
    fn audible_speech_is_never_silently_dropped() {
        let mut processor = ContinuousVadProcessor::new(16000, 800).expect("vad");
        let mut segments = Vec::new();

        // 12 s, comfortably past the ledger's hand-over point.
        for _ in 0..120 {
            let chunk = pitched(1600, 150.0, 0.09, 16000.0);
            segments.extend(processor.process_audio(&chunk).expect("process"));
        }
        segments.extend(processor.flush().expect("flush"));

        let total: usize = segments.iter().map(|s| s.samples.len()).sum();
        assert!(
            !segments.is_empty(),
            "12s of audible pitched audio produced no segments at all"
        );
        assert!(
            total as f64 / 16000.0 >= 4.0,
            "expected most of the audio to survive, got {:.1}s",
            total as f64 / 16000.0
        );
    }

    #[test]
    fn quiet_room_noise_is_not_recovered() {
        let mut processor = ContinuousVadProcessor::new(16000, 800).expect("vad");
        let mut segments = Vec::new();

        // 15 s of the sort of hiss an open microphone produces in a quiet room.
        for _ in 0..150 {
            let chunk = noise(1600, 0.004);
            segments.extend(processor.process_audio(&chunk).expect("process"));
        }
        segments.extend(processor.flush().expect("flush"));

        assert!(
            segments.is_empty(),
            "room noise must not be turned into segments, got {} ({:?})",
            segments.len(),
            segments.iter().map(|s| s.origin).collect::<Vec<_>>()
        );
    }

    /// Silero rejects a frame holding a non-finite or out-of-range sample, and
    /// its guard only exists in debug builds. Sanitising before the call keeps
    /// the behaviour identical in both profiles.
    #[test]
    fn hostile_samples_do_not_break_processing() {
        let mut processor = ContinuousVadProcessor::new(16000, 800).expect("vad");

        let mut chunk = pitched(1600, 150.0, 0.09, 16000.0);
        chunk[10] = f32::NAN;
        chunk[20] = f32::INFINITY;
        chunk[30] = -12.5;
        chunk[40] = 7.0;

        let result = processor.process_audio(&chunk);
        assert!(result.is_ok(), "sanitised input should process: {result:?}");
    }

    /// Replays a real recording through the exact processor the live pipeline
    /// uses, so VAD coverage can be measured without a microphone.
    ///
    /// The pipeline feeds `process_audio` one 600 ms window of 48 kHz mixed
    /// audio at a time (`pipeline.rs`), and that same buffer is what gets saved
    /// to disk — so a decoded recording is a faithful replay of what VAD saw.
    ///
    /// ```text
    /// ffmpeg -i audio.mp4 -ac 1 -ar 48000 -f f32le case.raw
    /// MEETILY_VAD_CASE=case.raw cargo test --lib vad_coverage -- --nocapture --ignored
    /// ```
    #[test]
    #[ignore = "needs MEETILY_VAD_CASE=<path to f32le 48kHz mono raw>"]
    fn vad_coverage_on_a_real_recording() {
        let path = std::env::var("MEETILY_VAD_CASE")
            .expect("set MEETILY_VAD_CASE to a raw f32le 48kHz mono file");
        let bytes = std::fs::read(&path).expect("case file should be readable");
        let raw: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        // Decoding lossy audio overshoots full scale, which the live path never
        // does: the mixer scales any sum past +/-1.0 back onto it. Mirror that
        // here so the replay measures VAD, not the decoder.
        let out_of_range = raw.iter().filter(|s| !(-1.0..=1.0).contains(*s)).count();

        // Hypothesis knob: Silero's probability collapses on quiet input, so
        // sweep the level presented to VAD without touching anything else.
        let gain: f32 = std::env::var("MEETILY_VAD_GAIN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);

        let samples: Vec<f32> = raw.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect();
        let input_rms =
            (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        println!("gain={gain}x  input RMS={input_rms:.4}");
        println!(
            "clamped {} of {} samples ({:.4}%) that the decoder pushed past full scale",
            out_of_range,
            raw.len(),
            100.0 * out_of_range as f64 / raw.len() as f64
        );

        const SR: usize = 48_000;
        const WINDOW: usize = SR * 600 / 1000; // the pipeline's mixing window

        let redemption: u32 = std::env::var("MEETILY_VAD_REDEMPTION")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(800);

        let mut processor = ContinuousVadProcessor::new(SR as u32, redemption).expect("vad");
        let mut segments = Vec::new();

        for window in samples.chunks(WINDOW) {
            segments.extend(processor.process_audio(window).expect("process_audio"));
        }
        segments.extend(processor.flush().expect("flush"));

        let total_s = samples.len() as f64 / SR as f64;
        let speech_s: f64 = segments
            .iter()
            .map(|s| (s.end_timestamp_ms - s.start_timestamp_ms) / 1000.0)
            .sum();

        println!(
            "redemption={redemption}ms  file={total_s:.1}s  segments={}  speech={speech_s:.1}s  coverage={:.0}%",
            segments.len(),
            100.0 * speech_s / total_s
        );
        for s in segments.iter().take(40) {
            println!(
                "  {:8.2} -> {:8.2}  ({:5.2}s, {} samples)",
                s.start_timestamp_ms / 1000.0,
                s.end_timestamp_ms / 1000.0,
                (s.end_timestamp_ms - s.start_timestamp_ms) / 1000.0,
                s.samples.len()
            );
        }
    }

    #[test]
    fn test_vad_chunked_vs_single_processing() {
        // Generate 60 seconds of audio with speech patterns at 16kHz
        let audio = generate_test_audio_with_speech(60.0, 16000);
        println!("Generated {} samples ({:.1}s)", audio.len(), audio.len() as f32 / 16000.0);

        // Process all at once (like small files)
        let segments_single = get_speech_chunks(&audio, 2000).expect("Single processing failed");
        println!("Single processing found {} segments", segments_single.len());

        // Process in chunks (like large files)
        let segments_chunked = get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
            println!("Chunked progress: {}%, {} segments", progress, segments);
            true // Don't cancel
        }).expect("Chunked processing failed");
        println!("Chunked processing found {} segments", segments_chunked.len());

        // Both should find the same number of segments (approximately)
        // Allow some variance due to chunk boundary effects
        let diff = (segments_single.len() as i32 - segments_chunked.len() as i32).abs();
        assert!(diff <= 1,
            "Chunked and single processing found different segment counts: {} vs {} (diff: {})",
            segments_single.len(), segments_chunked.len(), diff);
    }

    #[test]
    fn test_vad_large_file_progress() {
        // Generate 120 seconds (2 minutes) of audio - triggers large file threshold
        let audio = generate_test_audio_with_speech(120.0, 16000);
        let total_samples = audio.len();
        println!("Generated {} samples ({:.1}s)", total_samples, total_samples as f32 / 16000.0);

        // This should trigger the large file path (>960,000 samples)
        assert!(total_samples > 960_000, "Audio should be large enough to trigger chunked processing");

        let mut progress_updates = Vec::new();
        let segments = get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
            progress_updates.push((progress, segments));
            true // Don't cancel
        }).expect("Processing failed");

        println!("Found {} segments with {} progress updates", segments.len(), progress_updates.len());

        // The synthetic signal is not real speech, so Silero may merge it into
        // one long segment. This test is specifically for the large-file path:
        // it must still emit speech and report monotonic progress through 100%.
        assert!(!segments.is_empty(), "Expected at least one speech segment");
        assert!(
            segments.iter().all(|segment| !segment.samples.is_empty()
                && segment.end_timestamp_ms > segment.start_timestamp_ms),
            "Expected all speech segments to contain audio with positive duration"
        );

        // Should have received progress updates
        assert!(!progress_updates.is_empty(), "Expected progress updates for large file");
        assert_eq!(
            progress_updates.last().map(|(progress, _)| *progress),
            Some(100),
            "Expected progress to reach 100%"
        );
        assert!(
            progress_updates
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0),
            "Expected progress updates to increase monotonically: {:?}",
            progress_updates
        );
    }

    #[test]
    fn test_vad_cancellation() {
        let audio = generate_test_audio_with_speech(120.0, 16000);

        // Cancel at 50%
        let result = get_speech_chunks_with_progress(&audio, 2000, |progress, _| {
            progress < 50 // Cancel when reaching 50%
        });

        // Should return error due to cancellation
        assert!(result.is_err(), "Expected cancellation error");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("cancelled"), "Error should mention cancellation: {}", err_msg);
    }

    #[test]
    fn test_vad_continuous_processor_state_across_chunks() {
        // Test that VAD state is correctly maintained across chunk boundaries
        let mut processor = ContinuousVadProcessor::new(16000, 2000).expect("Failed to create processor");

        // Generate audio with a speech segment that spans a chunk boundary
        let chunk_size = 160_000; // 10 seconds
        let audio = generate_test_audio_with_speech(30.0, 16000); // 30 seconds

        // Process in 10-second chunks
        let mut all_segments = Vec::new();
        for (i, chunk) in audio.chunks(chunk_size).enumerate() {
            let segments = processor.process_audio(chunk).expect("Processing failed");
            println!("Chunk {}: processed {} samples, found {} segments", i, chunk.len(), segments.len());
            all_segments.extend(segments);
        }

        // Flush remaining
        let final_segments = processor.flush().expect("Flush failed");
        all_segments.extend(final_segments);

        println!("Total segments found: {}", all_segments.len());

        // Should find speech segments
        assert!(all_segments.len() >= 1, "Expected at least 1 speech segment");
    }

    #[test]
    fn test_vad_400ms_vs_2000ms_segmentation() {
        // Demonstrates why 2000ms redemption is needed for batch processing:
        // 400ms creates excessive fragmentation, 2000ms bridges natural pauses.
        //
        // Audio pattern: 60s with 5s speech / 5s silence cycles
        // Natural pauses within speech (sentence gaps) are 500ms-1.5s
        let audio = generate_test_audio_with_speech(60.0, 16000);

        let segments_400 = get_speech_chunks(&audio, 400).expect("400ms processing failed");
        let segments_2000 = get_speech_chunks(&audio, 2000).expect("2000ms processing failed");

        println!(
            "400ms redemption: {} segments, 2000ms redemption: {} segments",
            segments_400.len(),
            segments_2000.len()
        );

        // 2000ms should produce fewer or equal segments (bridges more pauses)
        assert!(
            segments_2000.len() <= segments_400.len(),
            "2000ms redemption ({} segments) should not produce more segments than 400ms ({} segments)",
            segments_2000.len(),
            segments_400.len()
        );

        // Verify segments have reasonable durations with 2000ms
        for (i, seg) in segments_2000.iter().enumerate() {
            let duration_ms = seg.end_timestamp_ms - seg.start_timestamp_ms;
            println!("2000ms segment {}: {:.0}ms duration", i, duration_ms);
            // Each segment should be at least 250ms (min_speech_time)
            assert!(duration_ms >= 200.0, "Segment {} too short: {:.0}ms", i, duration_ms);
        }
    }
}

