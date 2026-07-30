use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use silero_rs::{VadConfig, VadSession, VadTransition};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// When true, VAD sessions created afterwards use more sensitive thresholds so quiet,
/// narrowband inputs (e.g. Bluetooth headset mics in HFP mode at 16/24 kHz — AirPods as a
/// microphone) still register as speech instead of being skipped. Fed from the
/// `vad.high_sensitivity` app setting at recording start.
static VAD_HIGH_SENSITIVITY: AtomicBool = AtomicBool::new(false);

/// Set the high-sensitivity VAD profile. Takes effect for VAD sessions created after this
/// call (i.e. the next recording), so set it before starting the audio pipeline.
pub fn set_high_sensitivity(on: bool) {
    VAD_HIGH_SENSITIVITY.store(on, Ordering::SeqCst);
}

/// Represents a complete speech segment detected by VAD
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    pub start_timestamp_ms: f64,
    pub end_timestamp_ms: f64,
    pub confidence: f32,
}

fn absolute_vad_timestamp_to_sample(timestamp_ms: usize) -> usize {
    timestamp_ms * 16000 / 1000
}

/// Processes audio in 30ms chunks but returns complete speech segments
pub struct ContinuousVadProcessor {
    session: VadSession,
    chunk_size: usize,
    sample_rate: u32,
    buffer: Vec<f32>,
    speech_segments: VecDeque<SpeechSegment>,
    current_speech: Vec<f32>,
    in_speech: bool,
    processed_samples: usize,
    speech_start_sample: usize,
    // State tracking for smart logging
    last_logged_state: bool,
    last_large_buffer_warning_samples: usize,
    // Persistent sinc resampler (input rate → 16 kHz) with an input carry buffer, same
    // pattern as the capture-side resampler in pipeline.rs. The VAD's segment samples
    // feed the ASR engine directly, so resampling quality here is transcription quality:
    // the previous moving-average + linear-interpolation path aliased, costing a measured
    // ~0.6 pp of matched words on a reference meeting. `None` when input is already 16 kHz.
    resampler: Option<SincFixedIn<f32>>,
    resampler_input_buffer: Vec<f32>,
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
        //
        // High-sensitivity profile: Bluetooth/HFP mics (16/24 kHz) deliver weak, narrowband
        // audio that sits below Silero's default probability, so whole phrases get skipped.
        // Lower thresholds + shorter min-speech recover them (at the cost of a few more false
        // triggers, which the empty-transcript filter downstream drops anyway).
        let high_sensitivity = VAD_HIGH_SENSITIVITY.load(Ordering::SeqCst);
        let (positive_threshold, negative_threshold, min_speech_ms) = if high_sensitivity {
            (0.35_f32, 0.20_f32, 200_u64)
        } else {
            (0.50_f32, 0.35_f32, 250_u64)
        };
        config.positive_speech_threshold = positive_threshold; // above this prob = speech
        config.negative_speech_threshold = negative_threshold; // below this = silence (allows pauses)

        // CRITICAL FIX: Removed redemption_time capping to support long continuous speech
        // Previous: capped at 400ms, causing VAD to fragment 5-second speech into 40ms segments
        // New: Use full redemption_time from pipeline (2000ms) to bridge natural pauses
        config.redemption_time = Duration::from_millis(redemption_time_ms as u64);
        config.pre_speech_pad = Duration::from_millis(300); // Pre-speech padding for context
        config.post_speech_pad = Duration::from_millis(400); // Increased: more context at end

        // CRITICAL FIX: Increased min_speech_time to prevent tiny 40ms fragments
        // Previous: 100ms allowed too-short segments that Whisper rejects
        // New: 250ms ensures segments are substantial enough for Whisper (>100ms requirement)
        config.min_speech_time = Duration::from_millis(min_speech_ms); // Prevent tiny fragments

        debug!("Creating VAD session with: sample_rate={}Hz, redemption={}ms, min_speech={}ms, input_rate={}Hz, high_sensitivity={}",
               VAD_SAMPLE_RATE, redemption_time_ms, min_speech_ms, input_sample_rate, high_sensitivity);

        let session = VadSession::new(config)
            .map_err(|e| anyhow!("Failed to create VAD session: {:?}", e))?;

        // VAD uses 30ms chunks at 16kHz (480 samples)
        let vad_chunk_size = (VAD_SAMPLE_RATE as f32 * 0.03) as usize; // 480 samples

        // Persistent anti-aliased resampler for non-16kHz input (parameters mirror
        // audio_processing::resample's downsampling profile). Falls back to the legacy
        // filter+interpolate path if construction fails, so recording never breaks.
        let resampler = if input_sample_rate != VAD_SAMPLE_RATE {
            let params = SincInterpolationParameters {
                sinc_len: 512,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Cubic,
                oversampling_factor: 512,
                window: WindowFunction::BlackmanHarris2,
            };
            match SincFixedIn::<f32>::new(
                VAD_SAMPLE_RATE as f64 / input_sample_rate as f64,
                2.0,
                params,
                1024,
                1, // mono
            ) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!("VAD: sinc resampler init failed ({e}); using legacy resampling");
                    None
                }
            }
        } else {
            None
        };

        info!(
            "VAD processor created: input={}Hz, vad={}Hz, chunk_size={} samples, sinc_resampler={}",
            input_sample_rate,
            VAD_SAMPLE_RATE,
            vad_chunk_size,
            input_sample_rate != VAD_SAMPLE_RATE && resampler.is_some()
        );

        Ok(Self {
            session,
            chunk_size: vad_chunk_size,
            sample_rate: input_sample_rate, // Store input rate for resampling ratio in resample_to_16k()
            buffer: Vec::with_capacity(vad_chunk_size * 2),
            speech_segments: VecDeque::new(),
            current_speech: Vec::new(),
            in_speech: false,
            processed_samples: 0,
            speech_start_sample: 0,
            // Initialize state tracking
            last_logged_state: false,
            last_large_buffer_warning_samples: 0,
            resampler,
            resampler_input_buffer: Vec::new(),
        })
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

        self.buffer.extend_from_slice(&resampled_audio);
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

        Ok(completed_segments)
    }

    /// Resample from the input sample rate to 16 kHz through the persistent sinc
    /// resampler. Input accumulates in a carry buffer and is processed in fixed 1024-frame
    /// chunks (rubato's FixedIn contract); the sub-chunk remainder carries to the next
    /// call and is drained by [`Self::flush`], so no audio is lost — output just trails
    /// input by under a chunk (~21 ms at 48 kHz).
    fn resample_to_16k(&mut self, samples: &[f32]) -> Result<Vec<f32>> {
        if self.sample_rate == 16000 {
            return Ok(samples.to_vec());
        }
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(Self::legacy_resample_to_16k(self.sample_rate, samples));
        };

        self.resampler_input_buffer.extend_from_slice(samples);
        let mut out = Vec::new();
        loop {
            let needed = resampler.input_frames_next();
            if self.resampler_input_buffer.len() < needed {
                break;
            }
            let chunk: Vec<f32> = self.resampler_input_buffer.drain(..needed).collect();
            let mut waves = resampler
                .process(&[chunk], None)
                .map_err(|e| anyhow!("VAD resampler: {e}"))?;
            if let Some(wave) = waves.pop() {
                out.extend_from_slice(&wave);
            }
        }
        // The sinc filter overshoots slightly beyond [-1, 1] near sharp transients
        // (Gibbs); the VAD session rejects out-of-range samples outright, so clamp —
        // same as the decoder does after its sinc resampler.
        for s in &mut out {
            *s = s.clamp(-1.0, 1.0);
        }
        Ok(out)
    }

    /// Drain the resampler's carry buffer at end of stream (final partial chunk).
    fn drain_resampler(&mut self) -> Result<Vec<f32>> {
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(Vec::new());
        };
        if self.resampler_input_buffer.is_empty() {
            return Ok(Vec::new());
        }
        let tail: Vec<f32> = std::mem::take(&mut self.resampler_input_buffer);
        let mut waves = resampler
            .process_partial(Some(&[tail]), None)
            .map_err(|e| anyhow!("VAD resampler flush: {e}"))?;
        let mut out = waves.pop().unwrap_or_default();
        for s in &mut out {
            *s = s.clamp(-1.0, 1.0);
        }
        Ok(out)
    }

    /// Legacy moving-average + linear-interpolation downsampler. Only used when sinc
    /// resampler construction failed — it aliases audibly and should not be the norm.
    fn legacy_resample_to_16k(sample_rate: u32, samples: &[f32]) -> Vec<f32> {
        let ratio = sample_rate as f64 / 16000.0;
        let output_len = (samples.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(output_len);

        let filter_size = 3usize;
        let mut filtered_samples = Vec::with_capacity(samples.len());
        for i in 0..samples.len() {
            let start = i.saturating_sub(filter_size);
            let end = std::cmp::min(i + filter_size + 1, samples.len());
            let sum: f32 = samples[start..end].iter().sum();
            filtered_samples.push(sum / (end - start) as f32);
        }

        for i in 0..output_len {
            let source_pos = i as f64 * ratio;
            let source_index = source_pos as usize;
            let fraction = source_pos - source_index as f64;
            if source_index + 1 < filtered_samples.len() {
                let sample1 = filtered_samples[source_index];
                let sample2 = filtered_samples[source_index + 1];
                resampled.push(sample1 + (sample2 - sample1) * fraction as f32);
            } else if source_index < filtered_samples.len() {
                resampled.push(filtered_samples[source_index]);
            }
        }
        resampled
    }

    /// Flush any remaining audio and return final speech segments
    pub fn flush(&mut self) -> Result<Vec<SpeechSegment>> {
        debug!("VAD flush: in_speech={}, current_speech_len={}, buffer_len={}, speech_segments_queued={}",
              self.in_speech, self.current_speech.len(), self.buffer.len(), self.speech_segments.len());

        let mut completed_segments = Vec::new();

        // Drain the resampler's carry buffer first so the final <chunk of input audio
        // reaches the VAD instead of being dropped.
        match self.drain_resampler() {
            Ok(tail) => self.buffer.extend_from_slice(&tail),
            Err(e) => warn!("VAD flush: resampler drain failed: {e}"),
        }

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

            debug!(
                "VAD flush: Force-ending speech - start={}ms, end={}ms, duration={}ms, samples={}",
                start_ms,
                end_ms,
                end_ms - start_ms,
                self.current_speech.len()
            );

            let segment = SpeechSegment {
                samples: self.current_speech.clone(),
                start_timestamp_ms: start_ms,
                end_timestamp_ms: end_ms,
                confidence: 0.8, // Estimated confidence for forced end
            };

            self.speech_segments.push_back(segment);
            self.current_speech.clear();
            self.last_large_buffer_warning_samples = 0;
            self.in_speech = false;
        }

        // Extract all remaining segments
        while let Some(segment) = self.speech_segments.pop_front() {
            completed_segments.push(segment);
        }

        Ok(completed_segments)
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<()> {
        // Track accumulated speech buffer size to detect memory issues
        let current_speech_size = self.current_speech.len();
        // Warn once after roughly one minute, then at most once per additional
        // minute. The old condition emitted on every 30ms VAD chunk and could
        // produce thousands of lines for a single continuous segment.
        if current_speech_size >= 1_000_000
            && current_speech_size.saturating_sub(self.last_large_buffer_warning_samples) >= 960_000
        {
            warn!("VAD: Accumulated speech buffer is large: {} samples ({:.1}s) - possible memory issue",
                  current_speech_size, current_speech_size as f64 / 16000.0);
            self.last_large_buffer_warning_samples = current_speech_size;
        }

        let transitions = self
            .session
            .process(chunk)
            .map_err(|e| anyhow!("VAD processing failed: {}", e))?;

        // Log transitions for debugging
        if !transitions.is_empty() {
            debug!(
                "VAD transitions at sample {}: {} transitions",
                self.processed_samples,
                transitions.len()
            );
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
                    // Silero timestamps are absolute for the whole session. Adding
                    // processed_samples here double-counts the chunk offset and makes
                    // a final flushed segment start after the audio has already ended.
                    self.speech_start_sample = absolute_vad_timestamp_to_sample(timestamp_ms);
                    self.current_speech.clear();
                    self.last_large_buffer_warning_samples = 0;
                }
                VadTransition::SpeechEnd {
                    start_timestamp_ms,
                    end_timestamp_ms,
                    samples,
                } => {
                    // Only log if we were previously in speech state
                    if self.last_logged_state {
                        debug!(
                            "VAD: Speech ended at {}ms (duration: {}ms)",
                            end_timestamp_ms,
                            end_timestamp_ms - start_timestamp_ms
                        );
                        self.last_logged_state = false;
                    }
                    self.in_speech = false;

                    // Use samples from VAD transition if available, otherwise use accumulated samples
                    let speech_samples = if !samples.is_empty() {
                        samples
                    } else {
                        self.current_speech.clone()
                    };

                    if !speech_samples.is_empty() {
                        let segment = SpeechSegment {
                            samples: speech_samples,
                            start_timestamp_ms: start_timestamp_ms as f64,
                            end_timestamp_ms: end_timestamp_ms as f64,
                            confidence: 0.9, // VAD confidence
                        };

                        info!(
                            "VAD: Completed speech segment: {:.1}ms duration, {} samples",
                            end_timestamp_ms - start_timestamp_ms,
                            segment.samples.len()
                        );

                        self.speech_segments.push_back(segment);
                    }

                    self.current_speech.clear();
                    self.last_large_buffer_warning_samples = 0;
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
    if result.len() < 1600 {
        // Less than 100ms at 16kHz
        let input_energy: f32 =
            samples_mono_16k.iter().map(|&x| x * x).sum::<f32>() / samples_mono_16k.len() as f32;
        let rms = input_energy.sqrt();
        let peak = samples_mono_16k
            .iter()
            .map(|&x| x.abs())
            .fold(0.0f32, f32::max);

        // BALANCED FIX: Lowered thresholds to preserve quiet speech while still filtering silence
        // Previous aggressive values (0.08/0.15) were discarding valid quiet speech
        // New values (0.03/0.08) are more balanced - catch quiet speech, reject pure silence
        if rms < 0.2 || peak < 0.20 {
            info!("-----VAD detected silence/noise (RMS: {:.6}, Peak: {:.6}), skipping to prevent hallucinations-----", rms, peak);
            return Ok(Vec::new());
        } else {
            info!(
                "VAD detected speech with sufficient energy (RMS: {:.6}, Peak: {:.6})",
                rms, peak
            );
            return Ok(samples_mono_16k.to_vec());
        }
    }

    debug!(
        "VAD: Processed {} samples, extracted {} speech samples from {} segments",
        samples_mono_16k.len(),
        result.len(),
        num_segments
    );

    Ok(result)
}

/// Simple convenience function to get speech chunks from audio
/// Uses the optimized ContinuousVadProcessor with configurable redemption time
pub fn get_speech_chunks(
    samples_mono_16k: &[f32],
    redemption_time_ms: u32,
) -> Result<Vec<SpeechSegment>> {
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
        info!(
            "VAD: Processing large file ({} samples = {:.1}s), will log progress...",
            total_samples,
            total_samples as f64 / 16000.0
        );

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
            debug!(
                "VAD: Chunk {}/{} processed in {:?}, found {} segments",
                chunk_count,
                total_chunks,
                elapsed,
                segments.len()
            );

            // Warn if chunk processing took too long (>1 second)
            if elapsed.as_secs() > 1 {
                warn!(
                    "VAD: Chunk {} took {:?} - possible performance issue",
                    chunk_count, elapsed
                );
            }

            all_segments.extend(segments);

            processed += chunk.len();
            let progress = ((processed * 100) / total_samples) as u32;

            // Call progress callback every 5%
            if progress >= last_progress + 5 {
                debug!(
                    "VAD: Progress {}% ({} segments found so far)",
                    progress,
                    all_segments.len()
                );

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

        info!(
            "VAD: Complete! Found {} speech segments",
            all_segments.len()
        );
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

    #[test]
    fn test_absolute_vad_timestamp_does_not_include_chunk_offset_twice() {
        assert_eq!(absolute_vad_timestamp_to_sample(184_830), 2_957_280);
    }

    #[test]
    fn sinc_resampler_output_stays_in_valid_sample_range() {
        // Full-scale square-ish input rings past ±1.0 through a sinc filter (Gibbs);
        // the VAD session hard-rejects out-of-range samples, so the resampler must
        // clamp. Regression: unclamped output made process_audio() error on loud
        // 48 kHz input, killing live transcription.
        let mut processor = ContinuousVadProcessor::new(48_000, 800).expect("processor");
        let loud: Vec<f32> = (0..48_000)
            .map(|i| if (i / 60) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let out = processor.resample_to_16k(&loud).expect("resample");
        assert!(!out.is_empty(), "one second of input must produce output");
        assert!(
            out.iter().all(|s| (-1.0..=1.0).contains(s)),
            "resampled samples must stay within [-1, 1]"
        );
        // And the full VAD path must accept it end-to-end.
        processor
            .process_audio(&loud)
            .expect("VAD must accept clamped audio");
    }

    /// Generate synthetic speech-like audio with alternating speech/silence
    fn generate_test_audio_with_speech(duration_seconds: f32, sample_rate: u32) -> Vec<f32> {
        let total_samples = (duration_seconds * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];

        // Create speech-like patterns: bursts of sine waves with varying amplitude
        // Speech every 10 seconds for 5 seconds
        let speech_interval = 10.0; // seconds between speech starts
        let speech_duration = 5.0; // seconds of speech

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
                samples[i] = amplitude
                    * (0.5 * (2.0 * std::f32::consts::PI * freq1 * time).sin()
                        + 0.3 * (2.0 * std::f32::consts::PI * freq2 * time).sin()
                        + 0.2 * (2.0 * std::f32::consts::PI * freq3 * time).sin());
            }
            // else: silence (already 0.0)
        }

        samples
    }

    #[test]
    fn test_vad_chunked_vs_single_processing() {
        // Generate 60 seconds of audio with speech patterns at 16kHz
        let audio = generate_test_audio_with_speech(60.0, 16000);
        println!(
            "Generated {} samples ({:.1}s)",
            audio.len(),
            audio.len() as f32 / 16000.0
        );

        // Process all at once (like small files)
        let segments_single = get_speech_chunks(&audio, 2000).expect("Single processing failed");
        println!("Single processing found {} segments", segments_single.len());

        // Process in chunks (like large files)
        let segments_chunked =
            get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
                println!("Chunked progress: {}%, {} segments", progress, segments);
                true // Don't cancel
            })
            .expect("Chunked processing failed");
        println!(
            "Chunked processing found {} segments",
            segments_chunked.len()
        );

        // Both should find the same number of segments (approximately)
        // Allow some variance due to chunk boundary effects
        let diff = (segments_single.len() as i32 - segments_chunked.len() as i32).abs();
        assert!(
            diff <= 1,
            "Chunked and single processing found different segment counts: {} vs {} (diff: {})",
            segments_single.len(),
            segments_chunked.len(),
            diff
        );
    }

    #[test]
    fn test_vad_large_file_progress() {
        // Generate 120 seconds (2 minutes) of audio - triggers large file threshold
        let audio = generate_test_audio_with_speech(120.0, 16000);
        let total_samples = audio.len();
        println!(
            "Generated {} samples ({:.1}s)",
            total_samples,
            total_samples as f32 / 16000.0
        );

        // This should trigger the large file path (>960,000 samples)
        assert!(
            total_samples > 960_000,
            "Audio should be large enough to trigger chunked processing"
        );

        let mut progress_updates = Vec::new();
        let segments = get_speech_chunks_with_progress(&audio, 2000, |progress, segments| {
            progress_updates.push((progress, segments));
            true // Don't cancel
        })
        .expect("Processing failed");

        println!(
            "Found {} segments with {} progress updates",
            segments.len(),
            progress_updates.len()
        );

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
        assert!(
            !progress_updates.is_empty(),
            "Expected progress updates for large file"
        );
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
        assert!(
            err_msg.contains("cancelled"),
            "Error should mention cancellation: {}",
            err_msg
        );
    }

    #[test]
    fn test_vad_continuous_processor_state_across_chunks() {
        // Test that VAD state is correctly maintained across chunk boundaries
        let mut processor =
            ContinuousVadProcessor::new(16000, 2000).expect("Failed to create processor");

        // Generate audio with a speech segment that spans a chunk boundary
        let chunk_size = 160_000; // 10 seconds
        let audio = generate_test_audio_with_speech(30.0, 16000); // 30 seconds

        // Process in 10-second chunks
        let mut all_segments = Vec::new();
        for (i, chunk) in audio.chunks(chunk_size).enumerate() {
            let segments = processor.process_audio(chunk).expect("Processing failed");
            println!(
                "Chunk {}: processed {} samples, found {} segments",
                i,
                chunk.len(),
                segments.len()
            );
            all_segments.extend(segments);
        }

        // Flush remaining
        let final_segments = processor.flush().expect("Flush failed");
        all_segments.extend(final_segments);

        println!("Total segments found: {}", all_segments.len());

        // Should find speech segments
        assert!(
            all_segments.len() >= 1,
            "Expected at least 1 speech segment"
        );
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
            assert!(
                duration_ms >= 200.0,
                "Segment {} too short: {:.0}ms",
                i,
                duration_ms
            );
        }
    }
}
