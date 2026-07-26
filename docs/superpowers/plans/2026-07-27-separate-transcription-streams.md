# Separate Transcription Streams Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transcribe the microphone and the system audio as two independent streams instead of decoding their sum, so the remote participants' audio is never contaminated by what the microphone picks up out of the room.

**Architecture:** The mixer keeps producing one blended track for the saved recording, which is the right thing for a human listening back. The transcription path stops consuming that blend: the pipeline resamples each source to 16 kHz on its own, fills a separate 560 ms step accumulator per source, and tags each step with the device it came from. The provider keeps one decoder per source, because a streaming decoder's state is a running memory of one voice channel and merging two would corrupt both. Each committed segment carries its source, which is also the first useful layer of "who said what": the microphone is the room, the system audio is everyone dialling in.

**Tech Stack:** Rust, Tauri 2.x, `rubato` (band-limited resampling), `parakeet-rs` via the `nemotron-helper` sidecar, Python for the offline benchmark.

## Global Constraints

- A streaming step is exactly `STREAM_STEP_SAMPLES` = 8960 samples (560 ms at 16 kHz). Never send more per call: the encoder advances one step per call and buries the excess with no error. Measured cost of getting this wrong: 62.6% WER against 5.9%.
- Streaming pieces are concatenated verbatim. A leading space marks the start of a word; inserting or dropping one splits or fuses words.
- The saved recording must keep using the existing mixed output. This plan changes what is *transcribed*, not what is *recorded*.
- Whisper and Parakeet must keep working unchanged. They cannot stream and continue to consume VAD-segmented mixed audio.
- Every task ends green: `cargo test --lib --features cuda` passes.

---

## Why: the measurement that justifies this

| audio | WER | first word | notes |
|---|---|---|---|
| clean source, 60 s | **5.9%** | 1.1 s | `tests/fixtures/watchshop_60s_16k.f32` |
| the same content captured through the app and mixed | **62.1%** | 6.2 s | `tests/fixtures/device_60s_16k.f32` |

A 5-minute recording of that capture produced text for only 224.6 s of its 294.6 s — **24% of the audio decoded to nothing**.

The cause is in `pipeline.rs` `mix_window`: it sums microphone and system audio with no echo cancellation (`sum = mic + sys_scaled` where `sys_scaled = sys * 1.0`, despite comments describing ducking that no longer exists). When the speakers are playing, the microphone hears them, so the sum contains the same signal twice — once directly and once about 16.7 ms later, coloured by the room. Measured against the clean source, the capture has **+5.6 dB at 80-300 Hz** and **-4.1 dB at 300-1000 Hz**: room rumble amplified by the microphone's -23 LUFS loudness normalisation, and the first-formant band — the band that distinguishes vowels — suppressed by the comb filtering that summing a delayed copy produces.

Not fixed by this plan, and worth stating plainly: the **microphone** stream still contains room acoustics and still hears the speakers. This plan makes the system-audio stream clean and stops it dragging the microphone's problems into the remote participants' words. Making the microphone stream clean needs acoustic echo cancellation, which is separate work. A user wearing headphones has no acoustic path at all and gets both streams clean today.

---

## File Structure

| File | Responsibility after this change |
|---|---|
| `src/audio/transcription/provider.rs` | Trait gains a source argument on the streaming methods; `STREAM_STEP_SAMPLES` unchanged. |
| `src/audio/transcription/nemotron_provider.rs` | Holds one sidecar **per source** instead of one overall. |
| `src/audio/transcription/engine.rs` | Passes the source through to the provider. |
| `src/audio/recording_state.rs` | `AudioChunk::stream_step` records the real source instead of hardcoding microphone. |
| `src/audio/pipeline.rs` | Owns two lanes, each with its own resampler and accumulator; the mixer still feeds the recorder. |
| `src/audio/transcription/worker.rs` | One in-progress segment per source; stamps `TranscriptUpdate.source`. |
| `tests/streaming_bench.py` | Unchanged. Still the fastest way to score one stream. |

---

### Task 1: Give the streaming provider methods a source

**Files:**
- Modify: `src/audio/transcription/provider.rs`
- Modify: `src/audio/transcription/engine.rs`
- Modify: `src/audio/transcription/nemotron_provider.rs`
- Modify: `src/audio/recording_state.rs` (derive `Eq, Hash` on `DeviceType`)
- Modify: `src/audio/transcription/worker.rs` (call sites only)

**Interfaces:**
- Produces: `TranscriptionProvider::transcribe_step(&self, audio: Vec<f32>, source: DeviceType) -> Result<String, TranscriptionError>`, `TranscriptionProvider::reset_stream(&self, source: DeviceType) -> Result<(), TranscriptionError>`, `NemotronProvider::live_stream_count(&self) -> usize`. `TranscriptionEngine` forwards both with the same signatures.
- Consumes: `DeviceType` from `crate::audio::recording_state`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `src/audio/transcription/nemotron_provider.rs`:

```rust
    /// Two streams must not share a decoder. State is a running memory of one voice
    /// channel; feeding the room and the call into the same one corrupts both.
    #[tokio::test]
    async fn each_source_gets_its_own_decoder() {
        use crate::audio::recording_state::DeviceType;

        let provider = NemotronProvider::new(PathBuf::from("no-such-model"), None);

        // Reset is a no-op when nothing is running, and must stay per-source.
        provider.reset_stream(DeviceType::Microphone).await.expect("mic reset");
        provider.reset_stream(DeviceType::System).await.expect("system reset");

        assert_eq!(provider.live_stream_count().await, 0);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib --features cuda each_source_gets_its_own_decoder`
Expected: FAIL — `reset_stream` takes no argument, and `live_stream_count` does not exist.

- [ ] **Step 3: Make `DeviceType` usable as a map key**

In `src/audio/recording_state.rs`, extend the derive on `DeviceType`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceType {
    Microphone,
    System,
}
```

- [ ] **Step 4: Change the trait**

In `src/audio/transcription/provider.rs`, add the import:

```rust
use crate::audio::recording_state::DeviceType;
```

and replace the two streaming methods:

```rust
    /// Decode exactly one step of the stream carried by `source`.
    ///
    /// `audio` must be exactly [`STREAM_STEP_SAMPLES`] long. This is a hard
    /// requirement, not a hint: the underlying encoder advances its cursor by one
    /// step per call and buffers anything beyond that without ever catching up, so
    /// a longer buffer loses audio silently. Measured cost of getting this wrong:
    /// 62.6% word error rate against 5.9% when the size is right.
    ///
    /// Each `source` decodes independently, with its own state. The returned piece
    /// is **verbatim** - leading and trailing spaces are significant, because they
    /// are what marks the start of a word. Callers concatenate pieces with nothing
    /// between them and trim once at the end. An empty piece is normal and means
    /// the step decoded to nothing, which is what silence produces.
    async fn transcribe_step(
        &self,
        audio: Vec<f32>,
        source: DeviceType,
    ) -> std::result::Result<String, TranscriptionError> {
        let _ = (audio, source);
        Err(TranscriptionError::EngineFailed(format!(
            "{} does not decode streams a step at a time",
            self.provider_name()
        )))
    }

    /// Discard what the decoder for `source` remembers, so its next step starts fresh.
    ///
    /// State is deliberately kept *within* a recording - that continuity is what lets
    /// a word split across two steps come out whole. But it must not survive between
    /// recordings: without this the first words of a meeting are conditioned on the
    /// last sentence of the previous one, which is a different conversation.
    ///
    /// A provider that does not stream has no state to clear and does nothing.
    async fn reset_stream(
        &self,
        source: DeviceType,
    ) -> std::result::Result<(), TranscriptionError> {
        let _ = source;
        Ok(())
    }
```

- [ ] **Step 5: Forward the source through the engine**

In `src/audio/transcription/engine.rs`, replace the two forwarding methods:

```rust
    pub async fn transcribe_step(
        &self,
        audio: Vec<f32>,
        source: crate::audio::recording_state::DeviceType,
    ) -> std::result::Result<String, super::provider::TranscriptionError> {
        match self {
            Self::Provider(provider) => provider.transcribe_step(audio, source).await,
            _ => Err(super::provider::TranscriptionError::EngineFailed(format!(
                "{} does not decode streams a step at a time",
                self.provider_name()
            ))),
        }
    }

    /// Forget decoder state carried over from a previous recording.
    pub async fn reset_stream(
        &self,
        source: crate::audio::recording_state::DeviceType,
    ) -> std::result::Result<(), super::provider::TranscriptionError> {
        match self {
            Self::Provider(provider) => provider.reset_stream(source).await,
            _ => Ok(()),
        }
    }
```

- [ ] **Step 6: Hold one sidecar per source**

In `src/audio/transcription/nemotron_provider.rs`, add the imports:

```rust
use std::collections::HashMap;
use crate::audio::recording_state::DeviceType;
```

Replace the struct field `sidecar: Mutex<Option<Sidecar>>` with:

```rust
    /// One sidecar per audio source. Two streams cannot share one: the encoder cache
    /// and decoder state are a running memory of a single voice channel, so
    /// interleaving the room and the call would leave each conditioned on the other.
    sidecars: Mutex<HashMap<DeviceType, Sidecar>>,
```

In `new`, replace `sidecar: Mutex::new(None)` with `sidecars: Mutex::new(HashMap::new())`.

Add the accessor the test needs, in the inherent `impl NemotronProvider` block:

```rust
    /// How many decoders are currently running. Used by tests to prove sources stay
    /// separate; also useful in logs when diagnosing memory use.
    pub async fn live_stream_count(&self) -> usize {
        self.sidecars.lock().await.len()
    }
```

Every existing `self.sidecar.lock().await` becomes `self.sidecars.lock().await` keyed by
`DeviceType::Microphone` — that is the entry the non-streaming `transcribe`,
`ensure_started`, `is_model_loaded` and `get_current_model` use. For example
`ensure_started` becomes:

```rust
    pub async fn ensure_started(&self) -> std::result::Result<(), String> {
        let mut guard = self.sidecars.lock().await;
        if !guard.contains_key(&DeviceType::Microphone) {
            let started = self.spawn()?;
            guard.insert(DeviceType::Microphone, started);
        }
        Ok(())
    }
```

Rewrite the two streaming methods to key on `source`:

```rust
    async fn transcribe_step(
        &self,
        audio: Vec<f32>,
        source: DeviceType,
    ) -> std::result::Result<String, TranscriptionError> {
        if audio.len() != super::provider::STREAM_STEP_SAMPLES {
            // Refused rather than truncated or padded. The encoder advances one step
            // per call whatever it is handed, so a wrong size does not fail loudly -
            // it quietly leaves audio behind and every later step inherits the drift.
            return Err(TranscriptionError::EngineFailed(format!(
                "a streaming step must be exactly {} samples, got {}",
                super::provider::STREAM_STEP_SAMPLES,
                audio.len()
            )));
        }

        let mut bytes = Vec::with_capacity(audio.len() * 4);
        for sample in &audio {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        let mut guard = self.sidecars.lock().await;
        if !guard.contains_key(&source) {
            let started = self.spawn().map_err(TranscriptionError::EngineFailed)?;
            guard.insert(source.clone(), started);
        }

        let request = Request::TranscribeStream { audio_b64 };
        let result = {
            let sidecar = guard.get_mut(&source).expect("sidecar was just created");
            Self::exchange(sidecar, &request)
        };

        match result {
            // Returned untouched: trimming here would erase the leading space that
            // separates this piece's first word from the previous piece's last.
            Ok(Response::Piece { text }) => Ok(text),
            Ok(Response::Error { message }) => Err(TranscriptionError::EngineFailed(message)),
            Ok(_) => Err(TranscriptionError::EngineFailed(
                "unexpected reply to a streaming step".to_string(),
            )),
            Err(e) => {
                // The encoder cache dies with the process, so this stream restarts from
                // scratch rather than resuming mid-sentence. The other source is
                // untouched, which is the point of keeping them apart.
                warn!("nemotron-helper stream {:?} failed, restarting it next time: {}", source, e);
                guard.remove(&source);
                Err(TranscriptionError::EngineFailed(e))
            }
        }
    }

    async fn reset_stream(
        &self,
        source: DeviceType,
    ) -> std::result::Result<(), TranscriptionError> {
        let mut guard = self.sidecars.lock().await;
        // Nothing running means nothing to forget: the next start is already fresh.
        let sidecar = match guard.get_mut(&source) {
            Some(sidecar) => sidecar,
            None => return Ok(()),
        };

        match Self::exchange(sidecar, &Request::Reset) {
            Ok(Response::Error { message }) => Err(TranscriptionError::EngineFailed(message)),
            Ok(_) => Ok(()),
            Err(e) => {
                // Dropping the process is a heavier reset that reaches the same state.
                warn!("nemotron-helper reset failed, restarting it instead: {}", e);
                guard.remove(&source);
                Ok(())
            }
        }
    }
```

- [ ] **Step 7: Update the worker's call sites**

In `src/audio/transcription/worker.rs`, add `use crate::audio::recording_state::DeviceType;`
to the imports. The existing decode call becomes
`engine_clone.transcribe_step(step, chunk.device_type.clone()).await`, and the single
reset becomes one per source:

```rust
                    for source in [DeviceType::Microphone, DeviceType::System] {
                        if let Err(e) = engine_clone.reset_stream(source).await {
                            warn!("Worker {}: could not clear decoder state: {}", worker_id, e);
                        }
                    }
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `cargo test --lib --features cuda each_source_gets_its_own_decoder`
Expected: PASS

- [ ] **Step 9: Run the whole suite**

Run: `cargo test --lib --features cuda`
Expected: `test result: ok. 246 passed; 0 failed`

- [ ] **Step 10: Commit**

```bash
git add frontend/src-tauri/src/audio
git commit -m "refactor(asr): give each audio source its own decoder"
```

---

### Task 2: Resample and accumulate each source on its own lane

**Files:**
- Modify: `src/audio/pipeline.rs`
- Modify: `src/audio/recording_state.rs`

**Interfaces:**
- Consumes: `take_whole_steps(&mut Vec<f32>) -> Vec<Vec<f32>>` and `STREAM_STEP_SAMPLES` (both already in `pipeline.rs`).
- Produces: `AudioChunk::stream_step(data, sample_rate, timestamp, chunk_id, device_type)`; `struct StreamLane` with `new(DeviceType, u32) -> Result<Self>`, `push(&[f32]) -> Vec<(f64, Vec<f32>)>`, `flush() -> Option<(f64, Vec<f32>)>`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `src/audio/pipeline.rs`:

```rust
    /// Each source keeps its own place on the timeline. Sharing a counter would put
    /// one stream's text at the other stream's timestamps.
    #[test]
    fn lanes_advance_independently() {
        let mut mic = StreamLane::new(DeviceType::Microphone, 16_000).expect("lane");
        let mut system = StreamLane::new(DeviceType::System, 16_000).expect("lane");

        let one_step = vec![0.0f32; STREAM_STEP_SAMPLES];
        let mic_steps = mic.push(&one_step);
        assert_eq!(mic_steps.len(), 1);
        assert_eq!(mic_steps[0].0, 0.0, "first mic step starts at zero");

        let two_steps = [one_step.clone(), one_step.clone()].concat();
        let system_steps = system.push(&two_steps);
        assert_eq!(system_steps.len(), 2);
        assert_eq!(system_steps[1].0, 0.56, "second system step starts at 560 ms");

        // The microphone lane is untouched by the system lane's two steps.
        let next_mic = mic.push(&one_step);
        assert_eq!(next_mic[0].0, 0.56, "mic resumed from its own position");
    }

    /// A partial step waits rather than being sent short, on every lane.
    #[test]
    fn a_lane_holds_a_partial_step() {
        let mut lane = StreamLane::new(DeviceType::System, 16_000).expect("lane");
        assert!(lane.push(&vec![0.0f32; STREAM_STEP_SAMPLES - 1]).is_empty());

        let (starts_at, tail) = lane.flush().expect("the tail must not be dropped");
        assert_eq!(starts_at, 0.0);
        assert_eq!(tail.len(), STREAM_STEP_SAMPLES, "the tail is padded, not sent short");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib --features cuda lanes_advance_independently`
Expected: FAIL — `StreamLane` does not exist.

- [ ] **Step 3: Add `StreamLane`**

Insert into `src/audio/pipeline.rs` immediately after `take_whole_steps`:

```rust
/// One audio source on its way to the transcriber.
///
/// Holds everything that must not be shared between sources: a resampler with its own
/// filter state, the audio still waiting to complete a step, and the position on the
/// timeline. Two lanes running side by side is what stops the microphone's room noise
/// from ever reaching the words spoken by people dialling in.
struct StreamLane {
    source: DeviceType,
    resampler: Option<SincFixedIn<f32>>,
    resampler_chunk: usize,
    pending_input: Vec<f32>,
    /// 16 kHz audio waiting to complete the next step. Never longer than one step.
    accumulator: Vec<f32>,
    /// Absolute 16 kHz sample index of `accumulator[0]`.
    position: usize,
}

impl StreamLane {
    fn new(source: DeviceType, input_sample_rate: u32) -> Result<Self> {
        // Rebuilding a resampler per call loses its filter state at every boundary,
        // which the capture path already learned the hard way.
        let (resampler, resampler_chunk) = if input_sample_rate == 16_000 {
            (None, 0)
        } else {
            let chunk = 1024;
            let parameters = SincInterpolationParameters {
                sinc_len: 256,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 256,
                window: WindowFunction::BlackmanHarris2,
            };
            let built = SincFixedIn::<f32>::new(
                16_000.0 / input_sample_rate as f64,
                2.0,
                parameters,
                chunk,
                1,
            )?;
            (Some(built), chunk)
        };

        Ok(Self {
            source,
            resampler,
            resampler_chunk,
            pending_input: Vec::new(),
            accumulator: Vec::with_capacity(STREAM_STEP_SAMPLES),
            position: 0,
        })
    }

    /// Feed input-rate samples, get back every step they completed.
    ///
    /// Each returned step is paired with the second it starts at, so committed text
    /// lands on the recording's timeline without consulting a voice detector.
    fn push(&mut self, samples: &[f32]) -> Vec<(f64, Vec<f32>)> {
        let resampled = self.to_16k(samples);
        self.accumulator.extend_from_slice(&resampled);

        let mut steps = Vec::new();
        for step in take_whole_steps(&mut self.accumulator) {
            let starts_at = self.position as f64 / 16_000.0;
            self.position += step.len();
            steps.push((starts_at, step));
        }
        steps
    }

    /// Pad the leftover into one final step so the end of a recording is not lost.
    ///
    /// A step must be exactly one step long, so without this the last few hundred
    /// milliseconds would never be decoded - and that is where Stop lands, which is
    /// exactly when the speaker was mid-word.
    fn flush(&mut self) -> Option<(f64, Vec<f32>)> {
        if self.accumulator.is_empty() {
            return None;
        }
        let mut tail = std::mem::take(&mut self.accumulator);
        let starts_at = self.position as f64 / 16_000.0;
        self.position += tail.len();
        tail.resize(STREAM_STEP_SAMPLES, 0.0);
        Some((starts_at, tail))
    }

    fn to_16k(&mut self, samples: &[f32]) -> Vec<f32> {
        let chunk = self.resampler_chunk;
        let source = self.source.clone();
        let resampler = match self.resampler.as_mut() {
            Some(resampler) => resampler,
            None => return samples.to_vec(),
        };

        self.pending_input.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.pending_input.len() >= chunk {
            let input: Vec<f32> = self.pending_input.drain(..chunk).collect();
            match resampler.process(&[input], None) {
                Ok(mut waves) if !waves.is_empty() => out.append(&mut waves[0]),
                Ok(_) => {}
                Err(e) => {
                    warn!("resampling {:?} failed: {}", source, e);
                    break;
                }
            }
        }
        out
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib --features cuda lanes_advance_independently a_lane_holds_a_partial_step`
Expected: PASS

- [ ] **Step 5: Record the real source on the chunk**

In `src/audio/recording_state.rs`, replace `stream_step`:

```rust
    /// One 560 ms step of a single source's stream.
    ///
    /// `timestamp` is where the step *starts*, in seconds from the beginning of the
    /// recording, so committed text can be placed on the timeline without VAD.
    pub fn stream_step(
        data: Vec<f32>,
        sample_rate: u32,
        timestamp: f64,
        chunk_id: u64,
        device_type: DeviceType,
    ) -> Self {
        Self {
            data,
            sample_rate,
            timestamp,
            chunk_id,
            device_type,
            is_partial: false,
            utterance_id: None,
            is_stream_step: true,
        }
    }
```

- [ ] **Step 6: Drive both lanes from the pipeline**

In `struct AudioPipeline`, replace the `stream_accumulator` and `stream_position` fields:

```rust
    /// The two transcription lanes. The mixer still feeds the recorder, but the
    /// transcriber never sees the blend: summing the microphone into the system audio
    /// adds a delayed room copy of the same words, which measured 62.1% word error rate
    /// against 5.9% for the same content unmixed.
    mic_lane: StreamLane,
    system_lane: StreamLane,
```

In `AudioPipeline::new`, replace the two initialisers with:

```rust
            mic_lane: StreamLane::new(DeviceType::Microphone, sample_rate)
                .expect("microphone lane"),
            system_lane: StreamLane::new(DeviceType::System, sample_rate)
                .expect("system lane"),
```

Replace `dispatch_stream_steps` with:

```rust
    /// Hand each source's completed steps to the transcriber, separately.
    ///
    /// This runs alongside VAD rather than after it, because the pipeline starts before
    /// the engine has finished loading and cannot know whether a streaming engine is on
    /// the other end. A worker driving Whisper discards these; a worker driving Nemotron
    /// discards the VAD segments instead.
    fn dispatch_stream_steps(&mut self, mic_window: &[f32], sys_window: &[f32]) {
        let batches = [
            (DeviceType::Microphone, self.mic_lane.push(mic_window)),
            (DeviceType::System, self.system_lane.push(sys_window)),
        ];

        for (source, steps) in batches {
            for (starts_at, step) in steps {
                let chunk = AudioChunk::stream_step(
                    step,
                    16_000,
                    starts_at,
                    self.chunk_id_counter,
                    source.clone(),
                );
                self.chunk_id_counter += 1;
                if let Err(e) = self.transcription_sender.send(chunk) {
                    warn!("Failed to send streaming step: {}", e);
                    return;
                }
            }
        }
    }
```

In `run`, replace the existing `self.dispatch_stream_steps();` call with:

```rust
                            // STEP 3b: hand each source to the streaming lane, whole,
                            // unfiltered, and crucially unmixed.
                            self.dispatch_stream_steps(&mic_window, &sys_window);
```

`mix_window` already borrows both windows, so they are still available at this point.

In `flush_remaining_audio`, replace the tail block that used `stream_accumulator`:

```rust
        // The streaming lanes first, padding each tail rather than dropping it.
        let tails = [
            (DeviceType::Microphone, self.mic_lane.flush()),
            (DeviceType::System, self.system_lane.flush()),
        ];
        for (source, tail) in tails {
            if let Some((starts_at, samples)) = tail {
                let chunk = AudioChunk::stream_step(
                    samples,
                    16_000,
                    starts_at,
                    self.chunk_id_counter,
                    source,
                );
                self.chunk_id_counter += 1;
                if let Err(e) = self.transcription_sender.send(chunk) {
                    warn!("Failed to send the final streaming step: {}", e);
                }
            }
        }
```

Leave `ContinuousVadProcessor::drain_resampled_16k` in place but stop calling it — the VAD
path still resamples for its own use and the method is harmless.

- [ ] **Step 7: Run the whole suite**

Run: `cargo test --lib --features cuda`
Expected: `test result: ok. 248 passed; 0 failed`

- [ ] **Step 8: Commit**

```bash
git add frontend/src-tauri/src/audio
git commit -m "feat(asr): resample and accumulate each source on its own lane"
```

---

### Task 3: Keep one in-progress segment per source in the worker

**Files:**
- Modify: `src/audio/transcription/worker.rs`

**Interfaces:**
- Consumes: `AudioChunk.device_type`, `TranscriptionEngine::transcribe_step(audio, source)`.
- Produces: `TranscriptUpdate.source` carrying `"Microphone"` or `"System Audio"`; private `StreamingSegments::get(&DeviceType) -> &mut StreamingSegment` and `source_label(&DeviceType) -> &'static str`.

- [ ] **Step 1: Write the failing test**

Append a test module to `src/audio/transcription/worker.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A pause on one source must not close the other source's paragraph. Without
    /// separate state, one person stopping to breathe would cut the other off
    /// mid-sentence.
    #[test]
    fn a_pause_on_one_source_leaves_the_other_open() {
        let mut segments = StreamingSegments::default();

        segments.get(&DeviceType::System).text.push_str(" remote talking");
        segments.get(&DeviceType::Microphone).text.push_str(" local talking");

        for _ in 0..StreamingSegment::SILENT_STEPS_TO_CLOSE {
            segments.get(&DeviceType::Microphone).silent_steps += 1;
        }

        let system = segments.get(&DeviceType::System);
        assert!(system.has_text());
        assert_eq!(system.silent_steps, 0, "the system lane never went quiet");
    }

    /// The label is what makes the transcript answer "who", so it must not be blank.
    #[test]
    fn every_source_has_a_label() {
        assert_eq!(source_label(&DeviceType::Microphone), "Microphone");
        assert_eq!(source_label(&DeviceType::System), "System Audio");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib --features cuda a_pause_on_one_source`
Expected: FAIL — `StreamingSegments` and `source_label` do not exist.

- [ ] **Step 3: Add the per-source holder and the label**

In `src/audio/transcription/worker.rs`, after the `impl StreamingSegment` block:

```rust
/// One in-progress segment per audio source.
///
/// The room and the call pause at different moments, so they cannot share a silence
/// counter: one person stopping to breathe would otherwise cut the other off mid-word.
#[derive(Default)]
struct StreamingSegments {
    microphone: StreamingSegment,
    system: StreamingSegment,
}

impl StreamingSegments {
    fn get(&mut self, source: &DeviceType) -> &mut StreamingSegment {
        match source {
            DeviceType::Microphone => &mut self.microphone,
            DeviceType::System => &mut self.system,
        }
    }
}

/// What the transcript calls each source.
///
/// This is the first honest answer to "who said what": everything arriving on the
/// system audio came from someone dialling in, everything on the microphone came from
/// the room. It is coarse - it does not separate two people in the same room - but
/// unlike a diarisation model it cannot be wrong, because the two are never mixed.
fn source_label(source: &DeviceType) -> &'static str {
    match source {
        DeviceType::Microphone => "Microphone",
        DeviceType::System => "System Audio",
    }
}
```

- [ ] **Step 4: Stamp the source on every committed segment**

Replace `commit_streaming_segment`:

```rust
fn commit_streaming_segment<R: Runtime>(
    app: &AppHandle<R>,
    segment: &mut StreamingSegment,
    source: &DeviceType,
) {
    if !segment.has_text() {
        *segment = StreamingSegment::default();
        return;
    }

    let started_at = segment.started_at.unwrap_or(0.0);
    let update = TranscriptUpdate {
        text: segment.text.trim().to_string(),
        timestamp: format_current_timestamp(),
        source: source_label(source).to_string(),
        sequence_id: SEQUENCE_COUNTER.fetch_add(1, Ordering::SeqCst),
        chunk_start_time: started_at,
        is_partial: false,
        // The sidecar does not surface token probabilities, so there is no confidence
        // to report. This matches what the utterance path already sends for Nemotron.
        confidence: 0.85,
        audio_start_time: started_at,
        audio_end_time: segment.ends_at,
        duration: (segment.ends_at - started_at).max(0.0),
    };

    info!(
        "✅ Committing {} segment [{:.1}s-{:.1}s]: {}",
        update.source, update.audio_start_time, update.audio_end_time, update.text
    );

    if let Err(e) = app.emit("transcript-update", &update) {
        error!("Failed to emit streamed transcript segment: {}", e);
    }

    *segment = StreamingSegment::default();
}
```

- [ ] **Step 5: Use the per-source segments in the worker loop**

Replace `let mut segment = StreamingSegment::default();` with
`let mut segments = StreamingSegments::default();`.

Rewrite the body of the `chunk.is_stream_step` branch:

```rust
                                let source = chunk.device_type.clone();
                                let step = std::mem::take(&mut chunk.data);
                                match engine_clone.transcribe_step(step, source.clone()).await {
                                    Ok(piece) => {
                                        let segment = segments.get(&source);
                                        if piece.trim().is_empty() {
                                            // Silence, or a step the encoder had nothing
                                            // to say about. Either way it is how a pause
                                            // is found without a separate detector.
                                            if segment.has_text() {
                                                segment.silent_steps += 1;
                                                if segment.silent_steps
                                                    >= StreamingSegment::SILENT_STEPS_TO_CLOSE
                                                {
                                                    commit_streaming_segment(
                                                        &app_clone, segment, &source,
                                                    );
                                                }
                                            }
                                        } else {
                                            if segment.started_at.is_none() {
                                                segment.started_at = Some(chunk_timestamp);
                                            }
                                            segment.silent_steps = 0;
                                            segment.text.push_str(&piece);
                                            segment.ends_at = chunk_timestamp + chunk_duration;
                                            let preview = segment.text.trim().to_string();
                                            let overdue = segment.is_overdue();

                                            // Straight to screen, every 560 ms, without
                                            // waiting for the speaker to stop.
                                            let _ = app_clone.emit(
                                                "transcript-partial",
                                                serde_json::json!({
                                                    "text": preview,
                                                    "source": source_label(&source),
                                                    "utterance_id": chunk_utterance_id,
                                                }),
                                            );

                                            if !SPEECH_DETECTED_EMITTED
                                                .swap(true, Ordering::SeqCst)
                                            {
                                                let _ = app_clone.emit(
                                                    "speech-detected",
                                                    serde_json::json!({
                                                        "message": "Speech activity detected"
                                                    }),
                                                );
                                            }

                                            if overdue {
                                                commit_streaming_segment(
                                                    &app_clone,
                                                    segments.get(&source),
                                                    &source,
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        // One lost step is ~560 ms of audio. Say so rather
                                        // than letting the transcript quietly shorten.
                                        warn!(
                                            "Worker {}: {:?} step at {:.1}s failed: {}",
                                            worker_id, source, chunk_timestamp, e
                                        );
                                        let _ = app_clone
                                            .emit("transcription-warning", e.to_string());
                                    }
                                }
```

Replace the streaming branch that handled VAD segments:

```rust
                            if streaming {
                                // A VAD segment on a streaming engine is not audio to
                                // decode - the lanes already carried it - and it cannot
                                // say *which* source paused, because it ran on the mix.
                                // Each lane closes itself on its own silence.
                                chunks_completed_clone.fetch_add(1, Ordering::SeqCst);
                                continue;
                            }
```

And where the worker finishes, commit both lanes:

```rust
                                    // Whatever was still accumulating when the recording
                                    // stopped, on each lane.
                                    for source in [DeviceType::Microphone, DeviceType::System] {
                                        commit_streaming_segment(
                                            &app_clone,
                                            segments.get(&source),
                                            &source,
                                        );
                                    }
```

- [ ] **Step 6: Run the whole suite**

Run: `cargo test --lib --features cuda`
Expected: `test result: ok. 250 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add frontend/src-tauri/src/audio
git commit -m "feat(asr): label each transcript segment with the source it came from"
```

---

### Task 4: Prove a noisy microphone cannot reach the system stream

**Files:**
- Modify: `src/audio/pipeline.rs` (test module)

**Interfaces:**
- Consumes: `StreamLane`, `word_error_rate` (already in the test module), `NemotronProvider::transcribe_step`.

This is the task that shows the change was worth making. It feeds the clean source into
the system lane and the *degraded device capture* into the microphone lane at the same
time, then checks the system transcript is still as good as it was alone. Mixed, this
exact pairing scored 62.1%.

- [ ] **Step 1: Write the test**

Add to `mod tests` in `src/audio/pipeline.rs`:

```rust
    /// The whole point of the change, stated as a number.
    ///
    ///   $env:MEETILY_STREAM_CASE="tests/fixtures/watchshop_60s_16k.f32"
    ///   $env:MEETILY_STREAM_NOISY="tests/fixtures/device_60s_16k.f32"
    ///   $env:MEETILY_STREAM_REFERENCE="tests/fixtures/watchshop_60s_reference.txt"
    ///   cargo test --features cuda --lib separates -- --ignored --nocapture
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "needs MEETILY_STREAM_CASE, MEETILY_STREAM_NOISY and the Nemotron model"]
    async fn separates_a_clean_source_from_a_noisy_one() {
        use crate::audio::transcription::nemotron_provider::{
            NemotronProvider, DEFAULT_NEMOTRON_MODEL,
        };
        use crate::audio::transcription::provider::TranscriptionProvider;
        use std::path::PathBuf;

        fn load(key: &str) -> Vec<f32> {
            let path = std::env::var(key).unwrap_or_else(|_| panic!("set {key}"));
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        }

        let clean = load("MEETILY_STREAM_CASE");
        let noisy = load("MEETILY_STREAM_NOISY");

        std::env::set_var(
            "MEETILY_NEMOTRON_HELPER",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("binaries")
                .join("nemotron-helper-x86_64-pc-windows-msvc.exe"),
        );
        let model_dir = PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("com.meetily.ai")
            .join("models")
            .join("nemotron")
            .join(DEFAULT_NEMOTRON_MODEL);
        let provider = NemotronProvider::new(model_dir, Some("en-US".to_string()));
        provider.ensure_started().await.expect("sidecar should start");

        // Both fixtures are already 16 kHz, so the lanes pass audio straight through.
        let mut system_lane = StreamLane::new(DeviceType::System, 16_000).expect("lane");
        let mut mic_lane = StreamLane::new(DeviceType::Microphone, 16_000).expect("lane");
        let mut system_text = String::new();

        // Interleaved in 100 ms windows, the way the mixer hands them over live.
        let window = 1_600;
        let shared = clean.len().min(noisy.len());
        for start in (0..shared).step_by(window) {
            let end = (start + window).min(shared);

            for (_, step) in mic_lane.push(&noisy[start..end]) {
                provider
                    .transcribe_step(step, DeviceType::Microphone)
                    .await
                    .expect("mic step");
            }
            for (_, step) in system_lane.push(&clean[start..end]) {
                let piece = provider
                    .transcribe_step(step, DeviceType::System)
                    .await
                    .expect("system step");
                system_text.push_str(&piece);
            }
        }

        println!("system stream: {}", system_text.trim());

        let reference = std::fs::read_to_string(
            std::env::var("MEETILY_STREAM_REFERENCE").expect("set MEETILY_STREAM_REFERENCE"),
        )
        .expect("reference");
        let wer = word_error_rate(&reference, &system_text);
        println!("system stream wer {:.1}%", wer * 100.0);

        // Mixed, this pairing scored 62.1%. Unmixed, the clean lane must land near its
        // own ceiling of 5.9%; 10% leaves room for variation without tolerating the
        // contamination this change removes.
        assert!(
            wer <= 0.10,
            "the noisy microphone contaminated the system stream: {:.1}%",
            wer * 100.0
        );
    }
```

- [ ] **Step 2: Run it**

```bash
cd frontend/src-tauri
$env:MEETILY_STREAM_CASE="tests/fixtures/watchshop_60s_16k.f32"
$env:MEETILY_STREAM_NOISY="tests/fixtures/device_60s_16k.f32"
$env:MEETILY_STREAM_REFERENCE="tests/fixtures/watchshop_60s_reference.txt"
cargo test --features cuda --lib separates -- --ignored --nocapture
```

Expected: PASS, printing a word error rate near 5.9%.

- [ ] **Step 3: Re-run the earlier end-to-end test to confirm no regression**

```bash
$env:MEETILY_STREAM_CASE="tests/fixtures/watchshop_60s_48k.f32"
$env:MEETILY_STREAM_REFERENCE="tests/fixtures/watchshop_60s_reference.txt"
cargo test --features cuda --lib streams_a_real_recording -- --ignored --nocapture
```

Expected: PASS. This test feeds one source, so it now exercises a single lane; the
numbers should stay near WER 6.4%, first word 1.1 s, p95 gap 1.12 s.

- [ ] **Step 4: Commit**

```bash
git add frontend/src-tauri/src/audio/pipeline.rs
git commit -m "test(asr): prove a noisy microphone cannot reach the system stream"
```

---

### Task 5: Write down what changed and what is still open

**Files:**
- Create: `docs/superpowers/specs/2026-07-27-separate-streams-results.md`
- Modify: `docs/superpowers/specs/2026-07-26-continuous-commit-design.md`

- [ ] **Step 1: Record the measured before and after**

Write the results document containing: the 62.1%-versus-5.9% evidence; the spectral
measurements (+5.6 dB at 80-300 Hz, -4.1 dB at 300-1000 Hz, a second arrival at
~16.7 ms at 73% relative strength); the coverage figure (224.6 s of text from a 294.6 s
recording); the number Task 4 produced; and the two things this does **not** fix — the
microphone lane still hears the room, and acoustic echo cancellation is still absent.
State plainly that a user wearing headphones has no acoustic path and gets both lanes
clean today.

- [ ] **Step 2: Note the superseded diagram**

Add a line near the top of `2026-07-26-continuous-commit-design.md` pointing at the new
document, because its "mixed 100 ms windows → StreamingTranscriber" diagram no longer
describes the transcription path.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers
git commit -m "docs: record the separate-stream results and what remains open"
```

---

## Still open after this plan

- **Acoustic echo cancellation.** The microphone lane still contains whatever the
  speakers are playing. This is the difference between "much better" and "correct" for a
  user without headphones, and it is the natural next piece of work.
- **Diarisation within a lane.** Two people in the same room are still one lane.
  `parakeet-rs` ships `sortformer.rs` (4 speakers, streaming, 80 ms frames) and
  `multitalker.rs`; comparing them is the next investigation.
- **The mixer's limiter.** `sum / sum_abs` in `mix_window` is documented as soft scaling
  but is `sign(sum)` — a per-sample hard clipper. It affected 3 samples in the recording
  measured here, so it is latent rather than urgent, but the comment describes the
  opposite of what the code does.
- **Vocabulary.** `transcription_vocabulary.json` already exists and already holds a
  term, but nothing carries it to the Nemotron path.
- **Frontend display.** `TranscriptUpdate.source` now varies, but the UI does not yet
  show it. The data is persisted either way.
