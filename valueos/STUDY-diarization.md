# Study: real speaker diarization for ValueOS Agent

> **Status:** research + design study (no code yet). Written for a strong engineer who knows this
> codebase but not the diarization literature. Companion to [FEATURE-speakers.md](FEATURE-speakers.md)
> (the current best-effort Me/Other heuristic).
>
> **Sourcing caveat:** the external facts below were gathered by a fan-out web-research pass over
> primary sources (Hugging Face model cards, the sherpa-onnx / diart repos, arXiv, docs.rs). The
> automated *adversarial verification* stage was **rate-limited and did not run**, so treat cited
> facts as "from a primary source + cross-checked against domain knowledge," not "independently
> triple-verified." The two items I'd personally re-confirm before betting on them are flagged
> **⚠️verify** inline (the `sherpa-rs` maintenance status, and exact per-embedding-model licenses).

---

## 0. TL;DR / recommendation

- **Yes, this is very doable on your stack, and pyannote is relevant — but not as the Python
  library.** You already ship **ONNX Runtime** (`ort`) for Parakeet + Silero VAD. The whole
  diarization pipeline can run as **ONNX models on the runtime you already have**. Bundling
  Python/PyTorch/pyannote into a Tauri app would be the wrong move (huge, fragile).
- **Your architecture gives you a massive head start:** you capture **mic and system audio
  separately**. The mic stream is *definitionally* the local user ("Me") — it needs **no
  diarization**. Diarization only has to run on the **system stream** (the combined remote
  participants). So the target output is naturally **`Me`, `Other 1`, `Other 2`, …**.
- **Two viable implementations, both no-Python:**
  1. **`sherpa-onnx`** (k2-fsa, Apache-2.0) — a ready-made offline diarization pipeline
     (pyannote segmentation-3.0 ONNX + a speaker-embedding ONNX + clustering) driven from Rust.
     Fastest path to *accurate* labels; **offline/batch only** (labels at end-of-call); adds a
     C++ native dependency.
  2. **Roll it on your existing `ort`** — add ONE speaker-embedding ONNX model, extract an
     embedding per VAD segment, cluster in Rust (`kodama`). No new native dependency; gives you
     **live/streaming** labels; more code to own.
- **Recommended path:** **a phased hybrid.**
  - **Phase 1 (offline, high value, low risk):** at *End & upload*, run diarization over the
    recorded **system** track and relabel the transcript `Other 1..N`. Reuse `ort` + an embedding
    model + `kodama` clustering. This is the 80/20 — accurate multi-speaker labels with no
    real-time constraints.
  - **Phase 2 (online):** promote it to live labels during the call with incremental (online)
    clustering; keep the Phase-1 pass as an end-of-call "clean-up" re-diarization (online labels
    are provisional; the final pass fixes early mistakes).
- **Licensing verdict: shippable.** pyannote's `segmentation-3.0` is **MIT**, so you may
  **re-host the ONNX weights yourself** (exactly as you already host Parakeet/whisper models) and
  the Hugging Face "gated model" token requirement **does not apply to your end users**. sherpa-onnx
  is **Apache-2.0**. Embedding models are typically Apache-2.0 (**⚠️verify per model**).

---

## 1. What "diarization" means here (and what you already solved)

Speaker diarization answers **"who spoke when"** — it partitions audio into segments and assigns
each to an (initially anonymous) speaker label, *without* knowing the speakers in advance. It is
**not** speaker *identification* (matching to a known named person) and not *separation* (splitting
overlapping voices into clean tracks).

Your current `Me/Other` heuristic ([FEATURE-speakers.md](FEATURE-speakers.md)) is a 2-way
*source* attribution: raw mic energy vs raw system energy per utterance. It cannot tell **two
different remote people apart** — a 4-person Zoom call is all "Other". Real diarization is exactly
the missing piece: distinguishing the several voices *inside* the system stream.

**Key architectural insight — you only need to diarize ONE stream.** Because you capture mic and
system separately:

- **Mic stream → always "Me".** No diarization needed. (The local user is the only person on
  their own microphone.)
- **System stream → "Other 1..N".** This is where diarization runs. Cleaner than the usual
  problem, because the local user's voice is *not* mixed into the stream you're clustering (no
  need to first separate "me" from "them").

This halves the problem and removes the most error-prone part (self vs. others). It also means the
diarizer never has to reconcile with the mic — you fuse by construction: `Me` from the mic tag,
`Other k` from the system-stream diarizer.

---

## 2. Diarization 101 — the standard pipeline

The classic **clustering-based** pipeline is four stages
([AssemblyAI overview](https://www.assemblyai.com/blog/what-is-speaker-diarization-and-how-does-it-work)):

1. **Segmentation / VAD** — split audio into speech utterances (~0.5–10 s) and drop silence.
   *You already do this* with Silero VAD. Advanced segmenters (pyannote segmentation-3.0) also do
   **local** diarization on a sliding 10 s window and flag **overlapping speech**.
2. **Speaker embedding** — turn each segment into a fixed-length vector (typically **192–256-d**)
   that captures voice identity (x-vector / ResNet / ECAPA-TDNN / ERes2Net families). Same-speaker
   vectors are close (cosine), different speakers far apart.
3. **Clustering** — group the embeddings into K clusters = K speakers. K is usually **unknown**, so
   the clusterer must estimate it (via a distance threshold or eigen-gap heuristics). Common
   algorithms: **agglomerative hierarchical clustering (AHC)** and **spectral clustering**.
4. **Labelling / reconciliation** — map clusters back onto the timeline and stitch labels to the
   transcript segments.

Two regimes:

- **Offline / batch:** you have the *whole* recording, so clustering sees every embedding at once →
  best accuracy, and labels are globally consistent. This is what almost all "good" diarizers do.
- **Online / streaming:** labels must be emitted *during* the audio with no look-ahead, and once
  assigned **a label is permanent — it can't be revised**
  ([AssemblyAI streaming](https://www.assemblyai.com/blog/streaming-speaker-diarization)). Early in
  a call the clusterer has too little data, so it mislabels, and can't take it back. This is the
  central reason streaming diarization is materially harder and less accurate than offline.

**Accuracy** is measured as **DER (Diarization Error Rate)** = missed speech + false alarm +
speaker-confusion, as a fraction of speech time. Good offline systems land in the **single digits
to ~15% DER** depending on domain, number of speakers, and overlap. **Overlapping speech** (two
people at once) is the dominant error source; pyannote segmentation-3.0 handles up to **2
simultaneous speakers per frame** natively
([pyannote/segmentation-3.0](https://huggingface.co/pyannote/segmentation-3.0)).

**Unknown speaker count:** don't ask the user "how many people?". Use a clustering **distance
threshold** (merge clusters closer than τ; K falls out) rather than a fixed K. (sherpa-onnx lets
you pass *either* `num_clusters` *or* a `threshold` — you'll want the threshold path.)

---

## 3. pyannote.audio — the deep dive (and why "the models, not the library")

pyannote is the de-facto open-source reference. The current pipeline is **`speaker-diarization-3.1`**,
composed of **`pyannote/segmentation-3.0`** (segmentation + overlap) plus a **speaker-embedding
model** (WeSpeaker-derived).

Decisive facts:

- **License = MIT** on both the 3.1 pipeline and `segmentation-3.0`
  ([3.1 card](https://huggingface.co/pyannote/speaker-diarization-3.1),
  [seg-3.0 card](https://huggingface.co/pyannote/segmentation-3.0)). MIT permits commercial use
  **and redistribution of the weights**.
- **Gated on Hugging Face** — to *download from HF* you must accept conditions and use an access
  token, and the pipeline pulls several gated repos. This is a **distribution gate, not a license
  restriction**: because the weights are MIT, **you may re-host them yourself** (your own S3, next
  to the Parakeet/whisper models you already distribute) and your users never see a token. This is
  precisely what sherpa-onnx does.
- **3.1 is now pure PyTorch** — the official 3.1 pipeline *removed* its onnxruntime dependency;
  "both speaker segmentation and embedding now run in pure PyTorch"
  ([3.1 card](https://huggingface.co/pyannote/speaker-diarization-3.1)). ⇒ **The `pyannote`
  *library* is a PyTorch app.** Running it as-is means bundling Python + PyTorch. Don't.
- **…but the models export to ONNX, and prebuilt exports exist.** There is a public ONNX export of
  the segmentation model ([onnx-community/pyannote-segmentation-3.0](https://huggingface.co/onnx-community/pyannote-segmentation-3.0),
  MIT), and a full pure-ONNX reimplementation of the 3.1 *pipeline*
  ([pyannote-onnx-extended](https://github.com/samson6460/pyannote-onnx-extended), segmentation.onnx
  + embedding.onnx, no PyTorch). ⇒ **pyannote's brains are available as ONNX**, which is the only
  form that matters for you.

**Verdict:** *use pyannote's segmentation model as ONNX (self-hosted), not the pyannote Python
library.* Whether you get that ONNX via sherpa-onnx's prebuilt bundle or via `ort` directly is the
next decision.

---

## 4. The options, compared (Rust / on-device / no Python)

| Option | How it runs in Rust | Streaming? | Model size (approx) | DER / quality | License (code) | Integration effort |
|---|---|---|---|---|---|---|
| **A. sherpa-onnx** (k2-fsa) | C API → `sherpa-rs` crate (**⚠️verify** maintenance) or your own FFI; ships pyannote seg-3.0 ONNX + 3D-Speaker/WeSpeaker ONNX + clustering | **No — offline/batch only** | seg ~6 MB + embedding ~25–70 MB | High (full pyannote-grade pipeline + overlap) | **Apache-2.0** | **Low-med:** wire a native lib + feed a WAV; but adds a C++ build dep |
| **B. Roll on existing `ort`** | Reuse the `ort` you already have; load 1 embedding ONNX; embed per VAD segment; cluster with `kodama` | **Yes** (online clustering) *and* offline | + embedding model only (~25–70 MB); optional seg-3.0 ~6 MB | Good; you own overlap/boundary quality | **Apache-2.0 / MIT** (crates) | **Med-high:** you write embedding glue + clustering + online logic |
| **C. Bundle Python + PyTorch + pyannote** | Ship a Python runtime inside Tauri | Only via `diart` (also Python) | **+500 MB–1 GB+** runtime | Highest (reference) | pipeline MIT, but PyTorch/CUDA deps heavy | **High + fragile:** packaging, startup, size — against your ethos |
| **D. NeMo diarization** (NVIDIA; Sortformer/streaming) | Python/NeMo; not a Rust/ONNX drop-in today | Research streaming (Sortformer) | large | SOTA-ish, GPU-oriented | Apache-2.0 (code) | **High:** Python/NeMo runtime; export path immature |
| **E. WhisperX** | Python; wraps whisper + **pyannote** for diarization | No | pyannote + whisper | Good | mixed | **High:** Python; also you use Parakeet, not whisper |

**Read of the table:** C, D, E all drag in Python and don't fit a Rust/Tauri/`ort` app. The real
contest is **A vs B**:

- **A (sherpa-onnx)** = least ML code to write, most battle-tested pipeline, **but offline-only and
  a new C++ native dependency** (build + binary size + cross-platform packaging for macOS/Windows).
- **B (`ort` direct)** = **zero new native deps** (you already link ONNX Runtime), unlocks
  **streaming**, and reuses your existing model-download/`ort` session plumbing — at the cost of
  writing the embedding + clustering + online-assignment logic yourself.

Given (1) you already own `ort`, (2) the app's identity is *live* transcription, and (3) the mic=Me
split removes the hardest sub-problem, **B is the better long-term fit** — with **A as a legitimate
fast-prototype / fallback** if you want accurate offline labels in a weekend.

---

## 5. Streaming reality check

Almost every high-quality diarizer is **offline**. Options for "live":

- **`diart`** ([juanmc2005/diart](https://github.com/juanmc2005/diart)) is the reference online
  system: **incremental clustering + local diarization over a rolling buffer updated every ~500 ms**,
  with **tunable latency ~0.5–5 s** (latency ↔ accuracy trade), and it gets more accurate as the
  call proceeds. But it's **Python + PyTorch** (RxPY, Optuna) — a design blueprint for you, not a
  dependency. Notably, a DIART-style pipeline on pyannote segmentation+embedding has been measured
  at **~0.057 s/chunk on a server CPU** ([arXiv 2109.06483](https://arxiv.org/pdf/2109.06483)) —
  i.e. the *compute* is cheap enough for real time; the hard part is *label stability*, not speed.
- **Neural streaming** (Google **UIS-RNN** ~7.6% DER online
  [[Google]](https://research.google/blog/accurate-online-speaker-diarization-with-supervised-learning/);
  NVIDIA **Streaming Sortformer**, RTF 0.005–0.18, latency 0.32–10 s
  [[arXiv 2507.18446]](https://arxiv.org/html/2507.18446v1)) shows near-real-time neural diarization
  is feasible, but these are GPU/Python research systems without an ONNX/Rust story today.

**Pragmatic recommendation — the hybrid, which fits your app perfectly:**

- **During the call:** cheap **online clustering** — maintain a running set of speaker centroids;
  for each new *system* VAD segment, embed it and assign to the nearest centroid above a cosine
  threshold, else spawn a new speaker. Show provisional `Other 1..N` live. (You already have the
  faded/confirmed live model — provisional labels fit it.)
- **At End & upload:** run the **offline pass** over the full system track (global AHC re-cluster)
  and **relabel** the saved/uploaded transcript with the clean, globally-consistent labels. This
  repairs the inevitable early-call online mistakes before anything is persisted/uploaded.

This gives a live experience *and* a correct artifact, and it degrades gracefully: if you ship only
the offline pass first (Phase 1), you already deliver correct multi-speaker transcripts.

---

## 6. Clustering in Rust

You do **not** need to hand-roll this.

- **`kodama`** ([docs.rs/kodama](https://docs.rs/kodama)) — fast **pure-Rust agglomerative
  hierarchical clustering** (matches Müllner's `fastcluster`). API: `linkage(&mut condensed_dissim,
  n, Method::Average)` → a `Dendrogram`; cut it at threshold τ to get flat clusters. Feed it a
  condensed (upper-triangular) **cosine-distance** matrix of your segment embeddings. This is the
  right tool for the **offline** pass. **Average/Ward linkage + a tuned τ** is the standard recipe.
- **`linfa-hierarchical`** ([crates.io](https://crates.io/crates/linfa-hierarchical)) — AHC in the
  `linfa` ecosystem; an alternative if you already lean on `linfa`.
- **Online** clustering (Phase 2) is a small hand-rolled centroid tracker (cosine threshold,
  running mean per speaker) — trivial Rust, no crate needed.
- Spectral clustering (pyannote/most papers' default) is higher quality on hard cases but needs an
  eigensolver; **start with AHC (`kodama`)** — it's simpler and good enough for meeting audio with a
  handful of speakers.

---

## 7. Concrete integration into ValueOS Agent

Where it slots into the existing native pipeline (`frontend/src-tauri/src/audio/`):

```
capture (mic + system, SEPARATE)                     ← already have both streams
        └─ system stream ──────────────┐
ring_buffer → mix → Silero VAD ─────────┤             ← VAD segments already exist
        │                               │
        │  (per closed VAD segment,     ▼
        │   for SYSTEM-dominant ones)   [NEW] speaker-embedding ONNX  (via existing `ort`)
        │                               ▼
        │                               [NEW] cluster
        │                                 · live: online centroid assign  → "Other k" (provisional)
        │                                 · end : kodama AHC re-cluster    → "Other k" (final)
        ▼                               ▼
   transcription (Parakeet/ort) ──► segment gets  source = "Me" | "Other 1" | "Other 2" …
                                          │
                                          ▼
                              transcript-update event → reduceLive → chat bubbles
```

**What's reused (most of it):**

- The **system stream is already isolated** in the mixer ring buffer — you can tap the raw
  system-only audio for a VAD segment without new capture work.
- **VAD segmentation** (Silero) already produces the utterance boundaries to embed.
- **`ort`** sessions + your model-download/caching machinery (Parakeet models) → add one embedding
  model the same way.
- The **`source` field** on `TranscriptUpdate` already flows end-to-end to the UI and the enriched
  export. Today it's `"Me"`/`"Other"`; diarization just makes it `"Me"`/`"Other 1"`/`"Other 2"`.
  The frontend `speakerLabels.ts` / `toLiveLines` and the bubble renderer already key off `source`
  — extend the label map + assign a colour per speaker. Minimal UI change.

**New pieces:**

1. **Embedding model** (ONNX, downloaded like Parakeet): a 3D-Speaker **ERes2Net** or a WeSpeaker
   **ResNet/CAM++** speaker model (192–256-d output). ~25–70 MB. (**⚠️verify** the exact model's
   license before shipping; 3D-Speaker is Apache-2.0.)
2. **`valueos_diarize` module (Rust):**
   - `embed(segment_16k_mono) -> [f32; D]` via `ort` (mean/L2-normalize the frame embeddings).
   - **online:** `assign(embedding) -> speaker_id` (nearest centroid by cosine, threshold τ, else
     new; update running centroid).
   - **offline:** `recluster(all_embeddings) -> labels[]` via `kodama` AHC cut at τ, then map old
     provisional ids → final ids and emit a `transcript-relabel` event.
3. **Fusion rule:** mic-dominant segment → `Me` (skip diarization); system-dominant segment → run
   the diarizer → `Other k`. (Reuse the existing raw-mic-vs-system energy decision you already
   compute for `valueos_attribute`.)
4. **Overlap (later):** if two remote people talk at once matters, add **pyannote segmentation-3.0
   ONNX** as a pre-step on the system stream (it emits up to 2 overlapping speakers/frame); until
   then, VAD segments + embedding + clustering is the pragmatic v1.

**Effort estimate (rough):**

- **Phase 1 (offline, end-of-call):** ~2–4 focused days — add the embedding ONNX + `ort` inference,
  `kodama` clustering over the system track, relabel the transcript. Highest value/lowest risk.
- **Phase 2 (online/live):** ~+2–3 days — the centroid tracker, provisional labels in the live
  view, and the end-of-call re-cluster + relabel event.
- **Alternative Phase-0 spike:** wire **sherpa-onnx** (Option A) for an offline proof in ~1–2 days
  to validate embedding/clustering quality on your real call audio before committing to the `ort`
  build.

**Testability note (matches your constraints):** the clustering + online-assignment logic is pure
and unit-testable in Rust with synthetic embeddings (no audio, no `ort`); the ONNX inference is
CI/desktop-verified like the rest of the native audio code.

---

## 8. Model size / CPU / GPU

- **Footprint:** an embedding model (~25–70 MB) ± segmentation-3.0 (~6 MB) is in the same
  ballpark as models you already download — no meaningful bloat, and downloadable post-install like
  Parakeet.
- **CPU:** embedding is cheap (one forward pass per utterance, not per frame of the whole file);
  clustering of a meeting's worth of segments is milliseconds. The DIART CPU figure (~0.057 s/chunk)
  confirms real-time is compute-feasible.
- **GPU (macOS):** `ort` supports the **CoreML execution provider**
  ([ONNX Runtime CoreML docs](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html)),
  which can offload to the Apple Neural Engine / GPU. Given how light embedding is, **CPU is
  almost certainly fine**; treat CoreML as an optional later optimization, not a prerequisite.

---

## 9. Licensing verdict (explicit)

| Component | License | Shippable in a commercial closed-source desktop app? |
|---|---|---|
| pyannote `segmentation-3.0` (weights) | **MIT** | **Yes** — MIT permits redistribution; **self-host the ONNX weights** to bypass the HF token gate for users |
| pyannote `speaker-diarization-3.1` pipeline | MIT | Yes (but it's the PyTorch pipeline — you're not shipping it, only the models as ONNX) |
| ONNX exports (onnx-community, pyannote-onnx-extended) | MIT | Yes |
| sherpa-onnx (code) | **Apache-2.0** | **Yes** |
| `sherpa-rs` crate | MIT (**⚠️verify** it's still maintained / not superseded by upstream bindings) | Yes |
| 3D-Speaker / WeSpeaker embedding model (weights) | typically **Apache-2.0** (**⚠️verify the specific model**) | Yes if Apache/MIT — check the exact checkpoint |
| `kodama`, `linfa-hierarchical` | MIT/Apache-2.0 | Yes |

**Bottom line:** nothing here blocks shipping. The one operational must-do is **re-host the model
weights yourself** (you already do this for Parakeet/whisper) so end users never need a Hugging Face
token — the MIT/Apache licenses on the weights make that redistribution legal.

---

## 10. Recommendation & next step

1. **Adopt the hybrid, phased plan** (Phase 1 offline via `ort` + `kodama` on the system stream;
   Phase 2 online labels + end-of-call re-cluster).
2. **De-risk first with a ~1–2 day sherpa-onnx spike** on real recorded call audio to confirm the
   *quality* of pyannote-seg + embedding + AHC on your actual meetings, before investing in the
   hand-rolled `ort` version. If sherpa's offline quality is good, you know the model choice is
   right and Phase 1 is "reproduce it on `ort`."
3. **Keep `Me` free:** never diarize the mic stream — it's the local user by construction. This is
   your unfair advantage over generic single-stream diarizers.

---

## Sources

Primary/technical (gathered this pass; verification stage was rate-limited — see caveat at top):

- Diarization pipeline overview — https://www.assemblyai.com/blog/what-is-speaker-diarization-and-how-does-it-work
- Streaming diarization (no look-ahead, permanent labels) — https://www.assemblyai.com/blog/streaming-speaker-diarization
- pyannote speaker-diarization-3.1 (MIT; pure-PyTorch; gated) — https://huggingface.co/pyannote/speaker-diarization-3.1
- pyannote segmentation-3.0 (MIT; gated; powerset/overlap) — https://huggingface.co/pyannote/segmentation-3.0
- pyannote segmentation-3.0 ONNX export (MIT) — https://huggingface.co/onnx-community/pyannote-segmentation-3.0
- pyannote-onnx-extended (pure-ONNX 3.1 pipeline) — https://github.com/samson6460/pyannote-onnx-extended
- sherpa-onnx (Apache-2.0; on-device diarization via ONNX) — https://github.com/k2-fsa/sherpa-onnx
- sherpa-onnx speaker-diarization docs — https://k2-fsa.github.io/sherpa/onnx/speaker-diarization/index.html
- sherpa-rs (Rust bindings; **⚠️verify** maintenance) — https://github.com/thewh1teagle/sherpa-rs
- diart (online diarization design; Python/PyTorch) — https://github.com/juanmc2005/diart
- Online diarization latency on CPU — https://arxiv.org/pdf/2109.06483
- Google UIS-RNN (online, 7.6% DER) — https://research.google/blog/accurate-online-speaker-diarization-with-supervised-learning/
- NVIDIA Streaming Sortformer — https://arxiv.org/html/2507.18446v1
- On-device pyannote-3.1 acceleration study — https://arxiv.org/abs/2606.08505
- kodama (Rust AHC) — https://docs.rs/kodama
- linfa-hierarchical (Rust AHC) — https://crates.io/crates/linfa-hierarchical
- ONNX Runtime CoreML execution provider — https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html
