# Separate transcription streams: what the change did

**Date:** 2026-07-27
**Branch:** `feat/gpu-whisper-live-transcription`
**Plan:** `docs/superpowers/plans/2026-07-27-separate-transcription-streams.md`

---

## The problem, as measured

A real 5-minute recording made with the app exposed the one link neither harness had
reached: the capture chain itself. It played the reference video through the speakers,
so the same content existed as both a clean fixture and a device capture, which made the
comparison exact.

| audio | WER | first word |
|---|---|---|
| clean source | **5.9%** | 1.1 s |
| the same content captured and mixed | **62.1%** | 6.2 s |

The app's own saved transcript agreed: `"We have Dellehand, our teaching director"`
where the offline run on clean audio gave `"We have John Call and Han, our boutique
director"`. Its 21 segments carried text for **224.6 s of a 294.6 s recording** — 24% of
the audio decoded to nothing at all, in a video with continuous speech.

Levels were not the problem, which is what ruled out the obvious explanations:

| | RMS | peak | clipped | near-silent 10 ms frames |
|---|---|---|---|---|
| clean source | −22.7 dBFS | 1.118 | 0.001% | **6%** |
| device capture | −23.1 dBFS | 1.000 | 0.000% | **1%** |

Same loudness, no clipping. The last column is the tell: every gap in the captured
version is filled with something.

## The cause

`mix_window` in `pipeline.rs` summed the two sources outright — `sum = mic + sys_scaled`
with `sys_scaled = sys * 1.0` — with no echo cancellation, and with comments describing
ducking that the code no longer did. When the speakers play, the microphone hears them,
so that sum contained the same signal twice.

Cross-correlating the capture against the clean source found a second arrival about
**16.7 ms** later at 73% relative strength — roughly 5.7 m of extra path, consistent with
speaker to room to microphone. Summing a delayed copy of a signal is comb filtering, and
the band energies show exactly its shape:

| band | device relative to clean |
|---|---|
| 80–300 Hz | **+5.6 dB** |
| 300–1000 Hz | **−4.1 dB** |
| 1000–3000 Hz | +0.1 dB |
| 3000–6000 Hz | −1.8 dB |

Room rumble amplified — the microphone's −23 LUFS loudness normalisation raises the noise
floor to broadcast level whenever the room is quiet, which is also why near-silent frames
dropped from 6% to 1% — and the first-formant band suppressed. That band is what
distinguishes one vowel from another.

## The change

The mixer still produces the blended track for the saved recording. The transcription
path stopped consuming it. Each source now has a `StreamLane` holding its own resampler,
its own 560 ms accumulator and its own position on the timeline, and each has its own
decoder in the provider, because a streaming decoder's state is a running memory of one
voice channel.

## The result

`separates_a_clean_source_from_a_noisy_one` runs both lanes at once — the clean source on
the system lane, the device capture on the microphone lane — and scores the system lane:

| | WER |
|---|---|
| the two summed, as before | 62.1% |
| the two as separate lanes, system lane | **6.4%** |

Roughly a tenfold improvement, on the same audio, with the noisy lane running alongside.
The single-lane end-to-end test is unchanged at WER 6.4%, first word 1.1 s, median gap
0.56 s, p95 1.12 s, RTF 0.30.

251 Rust tests pass, 19 frontend.

## What this does not fix

**The microphone lane still hears the room.** It is no longer dragging the system lane
down with it, but on its own it still contains reverberation and whatever the speakers
are playing. Acoustic echo cancellation — using the system audio as a reference and
subtracting it from the microphone — is the piece that would fix it, and is not done.

**A user wearing headphones has no acoustic path at all** and gets both lanes clean
today. That is worth saying in the product, not just here.

**Two people in the same room are still one lane.** The source label answers "the room"
versus "the call", not "who". `parakeet-rs` ships `sortformer.rs` (streaming, 4 speakers,
80 ms frames) and `multitalker.rs`; comparing them is the next investigation.

**`mix_window`'s limiter is mislabelled.** `sum / sum_abs` is documented as soft scaling
but evaluates to `sign(sum)` — a per-sample hard clipper. It affected 3 samples in the
recording measured here, so it is latent rather than urgent, but the comment says the
opposite of what the code does.

**The UI does not show the source yet.** `TranscriptUpdate.source` now carries
`"Microphone"` or `"System Audio"` instead of a constant `"Audio"`, and
`transcript-partial` carries the same, but nothing renders it. The data is persisted
either way.
