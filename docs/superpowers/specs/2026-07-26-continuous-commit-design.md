# Design: continuous transcript commit, and the model to commit with

**Date:** 2026-07-26
**Branch:** `feat/gpu-whisper-live-transcription`
**Supersedes the open questions in:** `2026-07-26-streaming-asr-handoff.md`

---

## 1. What the measurements say

A new offline harness, `frontend/src-tauri/tests/streaming_bench.py`, drives the
`nemotron-helper` sidecar over its stdio protocol. It needs no microphone, no
Tauri build and no GPU rebuild, and a full sweep finishes in about a minute.

Test case: the first 60 s of the user's reference video, at
`frontend/src-tauri/tests/fixtures/watchshop_60s_16k.f32` (f32le mono 16 kHz).
Content is a fast, barely-pausing presenter in a watch boutique — the exact
speech shape that breaks pause-driven segmentation.

| strategy | feed size | WER | RTF | first word | worst gap |
|---|---|---|---|---|---|
| oneshot (accuracy ceiling, not live) | whole clip | **5.9%** | 0.21 | 60.0 s | 60.0 s |
| stream | 160 ms | **5.9%** | 0.19 | **1.1 s** | 5.6 s |
| stream | 320 ms | **5.9%** | 0.18 | 1.3 s | 5.8 s |
| stream | 560 ms | **5.9%** | 0.18 | 1.1 s | 5.6 s |
| stream | 1120 ms | 62.6% | 0.09 | 2.2 s | 11.2 s |
| chopped 4000 ms (one `transcribe` per span, as the provider does today) | — | 8.9% | 0.11 | 4.0 s | 4.0 s |

Today's behaviour, for comparison, is recorded in the handoff: at 00:56 into a
recording there was **one** committed segment, stamped 00:05. Worst gap ≈ 51 s,
and the rest arrived only after Stop.

Three conclusions, each load-bearing:

**Streaming costs nothing in accuracy.** 5.9% streaming versus 5.9% one-shot, and
the twelve errors are the *same* twelve (`Wash purchase`, `Baldus are`,
`Call and Han`, `season collectors`). This is the number that retires the
trade-off recorded in `vad.rs`. Cutting mid-utterance destroyed accuracy because
it cut the **audio** into independent pieces. A cache-aware streaming encoder
carries its left-context cache across step boundaries, so nothing is cut — only
the *reporting* is incremental. The reverted idea and this one are not the same
idea.

**1120 ms is not "the same but chunkier" — it is broken.**
`Nemotron::transcribe_chunk` (`parakeet-rs-0.3.6/src/nemotron.rs:615`) advances
`audio_processed` by exactly one 56-frame step per call and returns only that
step's tokens. Hand it two steps' worth and one step is buffered, then never
caught up. **Never feed more than 560 ms per call.** This is the single easiest
way to reintroduce silent audio loss.

**Segment misalignment is real but minor; VAD is the main thief.** Chopping the
clip into 4 s spans and calling `transcribe` per span — what the provider does
with VAD segments today — costs only 8.9% against the 5.9% ceiling, and the
damage is visible as shaved word openings (`spli t`, `assi sting`,
`esta blished`). The live app is far worse than 8.9%, so the rest of the loss is
VAD, from two directions:

- VAD forwards only ~67% of the audio (measured in the prior session), so a third
  of the words never reach the model at all. Words vanish from the middle of an
  otherwise correctly ordered transcript, which is exactly what the live output
  shows.
- Short spans return *nothing*. `nemotron_provider.rs:360` already records this:
  the encoder needs a whole 560 ms step plus look-ahead before it emits, so a
  ~1.5 s span comes back empty. Every short reply — "Yeah.", "Whoa.", "This is
  it." — is lost, and those are precisely the ones missing from the live screen.

All three failures share one fix: take VAD out of the transcription path. The
model already handles silence by emitting nothing, so VAD's filtering job is
redundant, and its gaps are pure loss.

**The sidecar had to stop trimming.** Nemotron emits SentencePiece text where a
leading space marks the start of a word. `Response::Transcript` trims, which is
right for a whole utterance and fatal per step: `"speed"` + `" masters"` became
`"speed"` + `"masters"` with no way to tell whether to join. Measured cost of the
old framing at 560 ms steps: **85.2% WER**. A new `transcribe_stream` request
returns the piece verbatim; `transcribe` is unchanged, so
`nemotron_provider.rs` is unaffected.

### Reproducing

The audio is not in the repository — it is copyrighted, and two commands rebuild
it. `yt-dlp` comes from `uv tool install yt-dlp`; `ffmpeg` is already vendored.

```bash
cd frontend/src-tauri/tests/fixtures
yt-dlp -f bestaudio -o watchshop.webm "https://www.youtube.com/watch?v=ev0VPI7Zgh8"
../../binaries/ffmpeg-x86_64-pc-windows-msvc.exe -y -i watchshop.webm \
  -t 60 -ac 1 -ar 16000 -f f32le watchshop_60s_16k.f32
```

Then score any strategy:

```bash
cd frontend/src-tauri/tests
python streaming_bench.py --audio fixtures/watchshop_60s_16k.f32 \
  --reference fixtures/watchshop_60s_reference.txt \
  --strategy stream --chunk-ms 160 --chunk-ms 560
```

### How the reference transcript was built

The video has no author-written captions, so the reference is an adjudication of
two independent systems: YouTube's auto-captions and a Nemotron one-shot decode.
Where they agree, the text stands. Where they disagree, domain knowledge decides
and the decision is recorded here: `Speedmasters` (Omega), `Teddy Baldassarre`,
`John Callahan`, `seasoned collectors`. Two spots are genuinely unresolved —
`Watch purchase` (both systems hear "Wash") and `back at/in the boutique`. They
are left at the more plausible reading; because every strategy is scored against
the same reference, they cannot bias a comparison. WER normalisation folds case,
punctuation, `>>` speaker markers, digits-versus-words (`15`/`fifteen`) and
casual spellings (`gonna`/`going to`), so nobody is charged for formatting.

---

## 2. Recommendation: keep Nemotron 3.5 ASR streaming 0.6B

Ranked on the handoff's criteria — streaming first, then accuracy, then speed,
then VRAM — the model already on this machine wins, and the reason it looked bad
was integration, not the model.

**Streaming: native.** Cache-aware FastConformer with caches on every encoder
self-attention and convolution layer. Configurable right context from 0 to 13
frames of 80 ms. Published FLEURS English WER: 9.43% at an 80 ms chunk, 7.99% at
560 ms, 7.91% at 1120 ms — accuracy barely moves as latency drops, which is the
whole point of the architecture.

**Accuracy: measured 5.9% on the user's own hard test case**, which beats the
published FLEURS figure and beats what YouTube's captions manage on the same
audio (they miss `Speedmasters` and `seasoned`). `large-v3` needs LocalAgreement-
style tricks to stream at all, because it is a 30-second-window offline model;
that machinery is pure downside next to a model that streams natively.

**Speed: RTF 0.18–0.21**, i.e. ~5× real time with one stream — on **DirectML**,
not even CUDA. There is no performance problem to solve, so the CUDA/TensorRT
work the handoff contemplated can wait until something actually needs it.

**VRAM:** 0.6B parameters, comfortable in 16 GB alongside the summary models.

Candidates considered and rejected: `parakeet-tdt-0.6b-v3` (already installed,
but TDT is an offline decoder — no cache-aware streaming path in `parakeet-rs`);
Canary / Canary-Qwen (attention-encoder-decoder, offline-shaped, same problem as
Whisper); `whisper-large-v3-turbo` and distil-whisper (faster, still 30-second
windows, still need LocalAgreement).

**Nothing to download.** `nemotron-3.5-asr-streaming-0.6b` is already at
`%APPDATA%\com.meetily.ai\models\nemotron\`. The work is wiring, not acquisition.

---

## 3. Design: stream instead of segment

### Today

`AudioPipeline` (`audio/pipeline.rs`) mixes to 100 ms windows, hands them to
`ContinuousVadProcessor`, and emits an `AudioChunk` only when VAD closes an
utterance — which needs `PIPELINE_VAD_REDEMPTION_MS = 800` of silence. A
presenter who does not pause produces one unbounded segment, so nothing is
committed. `emit_partial_if_due` re-decodes the whole in-progress utterance every
2 s for a preview, and gives up entirely past 15 s (`MAX_PREVIEWED_SAMPLES`) —
which is why the screen goes completely still on long stretches.

### Proposed

Add a streaming path that commits on the model's own cadence, and let VAD stop
being the thing that decides when text is allowed to appear.

```
mixed 100 ms windows
        │
        ├──> RecordingSaver                       (unchanged)
        │
        ├──> ContinuousVadProcessor               (kept, demoted)
        │      └─ segment boundaries become *punctuation and speaker-turn hints*,
        │         and still drive the saved transcript's timestamps
        │
        └──> StreamingTranscriber                 (new)
               accumulates to exactly 560 ms, then one transcribe_stream call
               per step; every non-empty piece is committed immediately
```

Concretely:

1. **`StreamingTranscriber`**, a small unit owning a 560 ms accumulator and the
   sidecar handle. Its whole contract: `push(&[f32]) -> Vec<String>`, returning
   the pieces that completed. It never buffers more than one step and never
   sends more than one step per call — the 1120 ms failure above is the reason
   that rule is in the type rather than in a comment.
2. **Commit each piece as final.** Pieces concatenate verbatim; a segment is
   closed for persistence when VAD says the utterance ended, or after a bounded
   number of steps if it does not. Closing a segment is now a *display and
   storage* decision, not a decoding one, so a bad cut costs punctuation rather
   than words.
3. **Retire the partial re-decode.** `emit_partial_if_due` exists to paint text
   before a pause; the streaming path does that natively, at 1.1 s instead of
   2 s, without re-running the utterance and without the quadratic cost or the
   15 s cliff. `transcript-partial` stays as the event for the not-yet-closed
   tail of the current segment.
4. **Keep Whisper selectable.** The engine already dispatches through
   `TranscriptionEngine`; Whisper keeps the VAD-segmented path unchanged, since
   it genuinely cannot stream. Streaming is the Nemotron path's behaviour.

### Implementation, file by file

Not yet applied. Nothing below has been written; the harness, the sidecar's
`transcribe_stream` and this document are all that landed.

1. `audio/transcription/provider.rs:50` — add two methods to
   `TranscriptionProvider`, both defaulted so Whisper and Parakeet need no edit:
   `fn supports_streaming(&self) -> bool { false }` and
   `async fn transcribe_step(&self, audio: Vec<f32>) -> Result<String, TranscriptionError>`
   returning `EngineFailed` by default. A step is exactly 560 ms; the contract
   says so, because handing it more silently loses audio.
2. `audio/transcription/nemotron_provider.rs` — add `Request::TranscribeStream`
   and `Response::Piece` to the local mirrors of the protocol (lines 26-42),
   implement `transcribe_step` on top of `exchange`, and return `true` from
   `supports_streaming`. Do **not** trim the piece.
3. `audio/recording_state.rs:19` — add `pub is_stream_step: bool` to
   `AudioChunk`, defaulted `false` in `final_chunk` (line 36). Three literal
   constructions in `pipeline.rs` need the field.
4. `audio/pipeline.rs` — in `run`, after mixing each window, also push the mixed
   audio into a 560 ms accumulator and emit one `is_stream_step` chunk per
   completed step, **outside** the VAD branch. This is the change that stops the
   33% loss: the transcriber now sees every sample. VAD keeps running and its
   segment ends become segment-close markers rather than the only source of text.
5. `audio/transcription/worker.rs:181` — before the `chunk_is_partial` branch,
   handle `is_stream_step`: call `transcribe_step`, append the piece verbatim to a
   per-recording accumulator, and emit `transcript-partial` with the accumulated
   text so the screen moves every 560 ms. On a segment-close marker, emit the
   accumulated text as a `transcript-update` and clear. This keeps one clean
   bubble per utterance instead of 92 tiny ones per minute, while the visible
   text advances continuously.
6. Delete `emit_partial_if_due` and `MAX_PREVIEWED_SAMPLES`
   (`pipeline.rs:794-843`) once step 5 works — the streaming path replaces it and
   the 15 s cliff goes with it.
7. Reset `SEQUENCE_COUNTER` (`worker.rs:16`) per recording, the cosmetic bug the
   handoff noted, since this change is already in that function.

Then re-run the harness and check the four numbers against the targets below.
The `stream` strategy in the harness already exercises exactly the model call
sequence steps 1-4 produce, so a gap between the harness and the app means the
app is doing something extra — most likely still filtering through VAD.

### What this does not do

It does not touch mixing, recording, or the VAD improvements from `1a34886`.
It does not add LocalAgreement — that exists to make an offline model stream, and
is unnecessary once the model streams.

### Risks

- **Sidecar throughput at 560 ms cadence.** At RTF 0.18 there is ~5× headroom,
  but the call is synchronous under a mutex shared with the utterance path. If
  both run, they serialise. Mitigation: the streaming path owns the sidecar while
  recording; the utterance path is not used concurrently.
- **Decoder state and long sessions.** State is never reset during a recording,
  which is correct for continuity, but `accumulated_tokens` grows. Bounded by
  resetting on genuine long silences, where a reset costs nothing.
- **Punctuation at step boundaries.** Nemotron punctuates natively, but a segment
  closed by a step boundary rather than a pause may end mid-clause. This is
  cosmetic and is the deliberate trade being made.

### How each step gets verified

Every change is scored with the harness before and after, on the four metrics.
Acceptance targets, from the numbers above:

| metric | must be |
|---|---|
| WER on the 60 s case | ≤ 6.5% (ceiling is 5.9%) |
| first committed word | ≤ 2.0 s |
| worst gap between commits | ≤ 6.0 s |
| RTF | ≤ 0.5 |
