use anyhow::{anyhow, Result};
use silero_rs::{VadConfig, VadSession, VadTransition};
use log::{debug, info, warn};
use std::collections::VecDeque;
use std::time::Duration;

const VAD_SAMPLE_RATE: u32 = 16000;
const SPEECH_HISTORY_DURATION_MS: usize = 1_000;
pub const LIVE_MAX_SEGMENT_DURATION_MS: u32 = 8_000;

/// Represents a complete speech segment detected by VAD
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub samples: Vec<f32>,
    pub start_timestamp_ms: f64,
    pub end_timestamp_ms: f64,
    pub confidence: f32,
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
    max_segment_samples: Option<usize>,
    speech_history: VecDeque<f32>,
    speech_history_capacity: usize,
    last_emitted_end_sample: usize,
    preserve_realtime_audio: bool,
    // State tracking for smart logging
    last_logged_state: bool,
}

impl ContinuousVadProcessor {
    pub fn new(input_sample_rate: u32, redemption_time_ms: u32) -> Result<Self> {
        Self::new_with_max_segment_duration(input_sample_rate, redemption_time_ms, None)
    }

    pub fn new_realtime(input_sample_rate: u32, redemption_time_ms: u32) -> Result<Self> {
        let mut processor = Self::new_with_max_segment_duration(
            input_sample_rate,
            redemption_time_ms,
            Some(LIVE_MAX_SEGMENT_DURATION_MS),
        )?;
        processor.preserve_realtime_audio = true;
        Ok(processor)
    }

    fn new_with_max_segment_duration(
        input_sample_rate: u32,
        redemption_time_ms: u32,
        max_segment_duration_ms: Option<u32>,
    ) -> Result<Self> {
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

        let max_segment_samples = max_segment_duration_ms
            .map(|duration_ms| duration_ms as usize * VAD_SAMPLE_RATE as usize / 1_000);
        let speech_history_capacity =
            SPEECH_HISTORY_DURATION_MS * VAD_SAMPLE_RATE as usize / 1_000;

        info!(
            "VAD processor created: input={}Hz, vad={}Hz, chunk_size={} samples, max_segment={}ms",
            input_sample_rate,
            VAD_SAMPLE_RATE,
            vad_chunk_size,
            max_segment_duration_ms
                .map(|duration| duration.to_string())
                .unwrap_or_else(|| "unbounded".to_string())
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
            max_segment_samples,
            speech_history: VecDeque::with_capacity(speech_history_capacity),
            speech_history_capacity,
            last_emitted_end_sample: 0,
            preserve_realtime_audio: false,
            // Initialize state tracking
            last_logged_state: false,
        })
    }

    /// Process incoming audio samples and return any complete speech segments
    /// Handles resampling from input sample rate to 16kHz for VAD processing
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<SpeechSegment>> {
        // Resample to 16kHz if needed
        let mut resampled_audio = if self.sample_rate == 16000 {
            samples.to_vec()
        } else {
            self.resample_to_16k(samples)?
        };

        let sanitized_samples = Self::sanitize_samples(&mut resampled_audio);
        if sanitized_samples > 0 {
            warn!(
                "VAD: sanitized {} non-finite or out-of-range audio samples",
                sanitized_samples
            );
        }

        self.buffer.extend_from_slice(&resampled_audio);

        // Silero can reject valid system/media speech even when ASR recognizes it
        // clearly. Live transcription must preserve continuous coverage, so use
        // fixed-size streaming windows and leave speech filtering to the ASR. The
        // VAD path remains active for offline/batch processing.
        if self.preserve_realtime_audio {
            return Ok(self.emit_lossless_realtime_segments());
        }

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

    /// Improved resampling from input sample rate to 16kHz with anti-aliasing
    /// Uses linear interpolation and basic low-pass filtering for better quality
    fn resample_to_16k(&self, samples: &[f32]) -> Result<Vec<f32>> {
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

        if self.preserve_realtime_audio {
            return Ok(self.flush_lossless_realtime_segment().into_iter().collect());
        }

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
            let end_sample = self.speech_start_sample + self.current_speech.len();
            let start_ms = self.samples_to_ms(self.speech_start_sample);
            let end_ms = self.samples_to_ms(end_sample);

            debug!("VAD flush: Force-ending speech - start={}ms, end={}ms, duration={}ms, samples={}",
                  start_ms, end_ms, end_ms - start_ms, self.current_speech.len());

            let segment = SpeechSegment {
                samples: self.current_speech.clone(),
                start_timestamp_ms: start_ms,
                end_timestamp_ms: end_ms,
                confidence: 0.8, // Estimated confidence for forced end
            };

            self.speech_segments.push_back(segment);
            self.last_emitted_end_sample = end_sample;
            self.current_speech.clear();
            self.in_speech = false;
        }

        // Extract all remaining segments
        while let Some(segment) = self.speech_segments.pop_front() {
            completed_segments.push(segment);
        }

        Ok(completed_segments)
    }

    fn sanitize_samples(samples: &mut [f32]) -> usize {
        let mut sanitized = 0;
        for sample in samples {
            let replacement = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            if replacement != *sample {
                *sample = replacement;
                sanitized += 1;
            }
        }
        sanitized
    }

    fn emit_lossless_realtime_segments(&mut self) -> Vec<SpeechSegment> {
        let max_segment_samples = self
            .max_segment_samples
            .expect("Realtime audio preservation requires a segment duration");
        let mut segments = Vec::new();

        while self.buffer.len() >= max_segment_samples {
            let start_sample = self.processed_samples;
            let end_sample = start_sample + max_segment_samples;
            let samples = self.buffer.drain(..max_segment_samples).collect();

            segments.push(SpeechSegment {
                samples,
                start_timestamp_ms: self.samples_to_ms(start_sample),
                end_timestamp_ms: self.samples_to_ms(end_sample),
                confidence: 1.0,
            });
            self.processed_samples = end_sample;
            self.last_emitted_end_sample = end_sample;
        }

        segments
    }

    fn flush_lossless_realtime_segment(&mut self) -> Option<SpeechSegment> {
        if self.buffer.is_empty() {
            return None;
        }

        let start_sample = self.processed_samples;
        let samples = std::mem::take(&mut self.buffer);
        let end_sample = start_sample + samples.len();
        self.processed_samples = end_sample;
        self.last_emitted_end_sample = end_sample;

        Some(SpeechSegment {
            samples,
            start_timestamp_ms: self.samples_to_ms(start_sample),
            end_timestamp_ms: self.samples_to_ms(end_sample),
            confidence: 1.0,
        })
    }

    fn process_chunk(&mut self, chunk: &[f32]) -> Result<()> {
        // Track accumulated speech buffer size to detect memory issues
        let current_speech_size = self.current_speech.len();
        if current_speech_size > 1_000_000 {
            // More than ~62 seconds of accumulated speech at 16kHz
            warn!("VAD: Accumulated speech buffer is large: {} samples ({:.1}s) - possible memory issue",
                  current_speech_size, current_speech_size as f64 / 16000.0);
        }

        let was_in_speech = self.in_speech;
        let transitions = self
            .session
            .process(chunk)
            .map_err(|e| anyhow!("VAD processing failed: {}", e))?;
        let mut speech_end_timestamp_ms = None;

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
                    self.begin_speech(timestamp_ms);
                }
                VadTransition::SpeechEnd {
                    start_timestamp_ms,
                    end_timestamp_ms,
                    samples: _,
                } => {
                    // Only log if we were previously in speech state
                    if self.last_logged_state {
                        debug!("VAD: Speech ended at {}ms (duration: {}ms)", end_timestamp_ms, end_timestamp_ms - start_timestamp_ms);
                        self.last_logged_state = false;
                    }
                    self.in_speech = false;
                    speech_end_timestamp_ms = Some(end_timestamp_ms);
                }
            }
        }

        // Include the transition frame in the active segment. SpeechEnd is emitted
        // after the configured post-speech padding has elapsed.
        if self.in_speech || was_in_speech || speech_end_timestamp_ms.is_some() {
            self.current_speech.extend_from_slice(chunk);
        }

        self.processed_samples += chunk.len();
        self.remember_history(chunk);

        if let Some(end_timestamp_ms) = speech_end_timestamp_ms {
            self.finish_speech(end_timestamp_ms);
        } else if self.in_speech {
            self.emit_realtime_segment_if_due();
        }

        Ok(())
    }

    fn begin_speech(&mut self, timestamp_ms: usize) {
        let requested_start = timestamp_ms * VAD_SAMPLE_RATE as usize / 1_000;
        let history_start = self.processed_samples.saturating_sub(self.speech_history.len());
        let actual_start = requested_start
            .max(self.last_emitted_end_sample)
            .max(history_start)
            .min(self.processed_samples);
        let history_offset = actual_start - history_start;

        self.speech_start_sample = actual_start;
        self.current_speech.clear();
        self.current_speech.extend(
            self.speech_history
                .iter()
                .skip(history_offset)
                .copied(),
        );
    }

    fn finish_speech(&mut self, end_timestamp_ms: usize) {
        let requested_end = end_timestamp_ms * VAD_SAMPLE_RATE as usize / 1_000;
        let requested_len = requested_end.saturating_sub(self.speech_start_sample);
        if requested_len < self.current_speech.len() {
            self.current_speech.truncate(requested_len);
        }

        if !self.current_speech.is_empty() {
            let end_sample = self.speech_start_sample + self.current_speech.len();
            let segment = SpeechSegment {
                samples: std::mem::take(&mut self.current_speech),
                start_timestamp_ms: self.samples_to_ms(self.speech_start_sample),
                end_timestamp_ms: self.samples_to_ms(end_sample),
                confidence: 0.9,
            };

            info!(
                "VAD: Completed speech segment: {:.1}ms duration, {} samples",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len()
            );

            self.last_emitted_end_sample = end_sample;
            self.speech_segments.push_back(segment);
        }

        self.current_speech.clear();
    }

    fn emit_realtime_segment_if_due(&mut self) {
        let Some(max_segment_samples) = self.max_segment_samples else {
            return;
        };
        if self.current_speech.len() < max_segment_samples {
            return;
        }

        let end_sample = self.speech_start_sample + max_segment_samples;
        let segment = SpeechSegment {
            samples: self.current_speech.drain(..max_segment_samples).collect(),
            start_timestamp_ms: self.samples_to_ms(self.speech_start_sample),
            end_timestamp_ms: self.samples_to_ms(end_sample),
            confidence: 0.9,
        };

        info!(
            "VAD: Emitting realtime segment at hard limit: {:.1}ms, {} samples",
            segment.end_timestamp_ms - segment.start_timestamp_ms,
            segment.samples.len()
        );

        self.speech_segments.push_back(segment);
        self.last_emitted_end_sample = end_sample;
        self.speech_start_sample = end_sample;

        // Keep Silero's active utterance intact until it emits SpeechEnd. Removing
        // audio here leaves its internal Speech state pointing at a deleted start
        // timestamp, which can panic on the next sufficiently long silence and
        // terminate the whole live audio pipeline. `current_speech` is our output
        // buffer, so it can still be split independently without touching Silero.
    }

    fn remember_history(&mut self, chunk: &[f32]) {
        self.speech_history.extend(chunk.iter().copied());
        let excess = self
            .speech_history
            .len()
            .saturating_sub(self.speech_history_capacity);
        if excess > 0 {
            self.speech_history.drain(..excess);
        }
    }

    fn samples_to_ms(&self, samples: usize) -> f64 {
        samples as f64 * 1_000.0 / VAD_SAMPLE_RATE as f64
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

    #[test]
    fn test_realtime_processing_preserves_every_input_sample() {
        let mut processor = ContinuousVadProcessor::new_realtime(VAD_SAMPLE_RATE, 400)
            .expect("Failed to create realtime processor");
        let max_samples =
            LIVE_MAX_SEGMENT_DURATION_MS as usize * VAD_SAMPLE_RATE as usize / 1_000;
        let audio: Vec<f32> = (0..(max_samples * 2 + 1_234))
            .map(|index| (index % 2_000) as f32 / 2_000.0 - 0.5)
            .collect();
        let mut segments = Vec::new();

        for chunk in audio.chunks(777) {
            segments.extend(processor.process_audio(chunk).expect("Processing failed"));
        }
        segments.extend(processor.flush().expect("Flush failed"));

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].start_timestamp_ms, 0.0);
        assert_eq!(segments[0].end_timestamp_ms, 8_000.0);
        assert_eq!(segments[1].start_timestamp_ms, 8_000.0);
        assert_eq!(segments[1].end_timestamp_ms, 16_000.0);
        assert_eq!(segments[2].start_timestamp_ms, 16_000.0);
        assert_eq!(segments[2].samples.len(), 1_234);

        let reconstructed: Vec<f32> = segments
            .into_iter()
            .flat_map(|segment| segment.samples)
            .collect();
        assert_eq!(reconstructed, audio);
    }

    #[test]
    fn test_realtime_processing_sanitizes_samples_for_asr() {
        let mut processor = ContinuousVadProcessor::new_realtime(VAD_SAMPLE_RATE, 400)
            .expect("Failed to create realtime processor");
        let mut audio = vec![0.0; 800];
        audio[..5].copy_from_slice(&[
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1.5,
            -1.5,
        ]);

        let mut segments = processor.process_audio(&audio).expect("Processing failed");
        segments.extend(processor.flush().expect("Flush failed"));

        assert_eq!(segments.len(), 1);
        assert_eq!(&segments[0].samples[..5], &[0.0, 0.0, 0.0, 1.0, -1.0]);
        assert!(segments[0]
            .samples
            .iter()
            .all(|sample| sample.is_finite() && (-1.0..=1.0).contains(sample)));
    }

    #[test]
    fn test_realtime_vad_enforces_eight_second_segment_limit() {
        let mut processor = ContinuousVadProcessor::new_with_max_segment_duration(
            VAD_SAMPLE_RATE,
            400,
            Some(LIVE_MAX_SEGMENT_DURATION_MS),
        )
        .expect("Failed to create realtime VAD processor");
        let max_samples = LIVE_MAX_SEGMENT_DURATION_MS as usize * VAD_SAMPLE_RATE as usize / 1_000;
        let start_sample = 2 * VAD_SAMPLE_RATE as usize;

        processor.in_speech = true;
        processor.speech_start_sample = start_sample;
        processor.processed_samples = start_sample + max_samples + processor.chunk_size;
        processor.current_speech = vec![0.25; max_samples + processor.chunk_size];
        processor.emit_realtime_segment_if_due();

        let segment = processor
            .speech_segments
            .pop_front()
            .expect("Expected the hard limit to emit a segment");
        assert_eq!(segment.samples.len(), max_samples);
        assert_eq!(segment.start_timestamp_ms, 2_000.0);
        assert_eq!(
            segment.end_timestamp_ms - segment.start_timestamp_ms,
            LIVE_MAX_SEGMENT_DURATION_MS as f64
        );
        assert_eq!(
            processor.last_emitted_end_sample,
            start_sample + max_samples
        );
        assert!(processor.in_speech);
        assert_eq!(processor.current_speech.len(), processor.chunk_size);
        assert_eq!(processor.speech_start_sample, start_sample + max_samples);
    }

    #[test]
    fn test_realtime_vad_keeps_segmenting_twelve_minutes_of_continuous_speech() {
        let mut processor = ContinuousVadProcessor::new_with_max_segment_duration(
            VAD_SAMPLE_RATE,
            400,
            Some(LIVE_MAX_SEGMENT_DURATION_MS),
        )
        .expect("Failed to create realtime VAD processor");
        let max_samples = LIVE_MAX_SEGMENT_DURATION_MS as usize * VAD_SAMPLE_RATE as usize / 1_000;
        let expected_segments = 12 * 60 * 1_000 / LIVE_MAX_SEGMENT_DURATION_MS as usize;

        processor.in_speech = true;
        for index in 0..expected_segments {
            processor
                .current_speech
                .extend(std::iter::repeat(0.25).take(max_samples));
            processor.emit_realtime_segment_if_due();

            let segment = processor
                .speech_segments
                .pop_front()
                .expect("Expected the next realtime segment");
            assert_eq!(segment.samples.len(), max_samples);
            assert_eq!(
                segment.start_timestamp_ms,
                index as f64 * LIVE_MAX_SEGMENT_DURATION_MS as f64
            );
            assert_eq!(
                segment.end_timestamp_ms,
                (index + 1) as f64 * LIVE_MAX_SEGMENT_DURATION_MS as f64
            );
        }

        assert_eq!(
            processor.last_emitted_end_sample,
            expected_segments * max_samples
        );
        assert!(processor.in_speech);
        assert!(processor.current_speech.is_empty());
        assert!(processor.speech_segments.is_empty());
    }

    #[test]
    fn test_realtime_forced_segment_preserves_vad_session_buffer() {
        let mut processor = ContinuousVadProcessor::new_realtime(VAD_SAMPLE_RATE, 400)
            .expect("Failed to create realtime VAD processor");
        processor
            .session
            .process(&vec![0.0; 9 * VAD_SAMPLE_RATE as usize])
            .expect("Failed to populate the VAD session buffer");

        let max_samples = LIVE_MAX_SEGMENT_DURATION_MS as usize * VAD_SAMPLE_RATE as usize / 1_000;
        processor.in_speech = true;
        processor.current_speech = vec![0.25; max_samples];
        processor.emit_realtime_segment_if_due();

        let (buffer_start, _) = processor.session.current_buffer_range();
        assert_eq!(
            buffer_start,
            Duration::ZERO,
            "A forced output segment must not delete audio owned by active VAD state"
        );
    }

    #[test]
    fn test_realtime_vad_does_not_repeat_audio_after_forced_segment() {
        let mut processor = ContinuousVadProcessor::new_realtime(VAD_SAMPLE_RATE, 400)
            .expect("Failed to create realtime VAD processor");
        processor.processed_samples = 10 * VAD_SAMPLE_RATE as usize;
        processor.speech_history =
            VecDeque::from(vec![0.25; processor.speech_history_capacity]);
        processor.last_emitted_end_sample =
            processor.processed_samples - (VAD_SAMPLE_RATE as usize / 5);

        processor.begin_speech(9_000);

        assert_eq!(
            processor.speech_start_sample,
            processor.last_emitted_end_sample
        );
        assert_eq!(
            processor.current_speech.len(),
            VAD_SAMPLE_RATE as usize / 5
        );
    }
}
