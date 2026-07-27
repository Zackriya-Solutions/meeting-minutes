# Speaker Identification Rework — Design

**Date:** 2026-07-28
**Status:** Approved, ready for implementation planning
**Trigger:** The Speaker Identification settings pane showed "may clash with" warnings on
7 of 16 saved voices. Investigation found the warnings are largely *correct*, and that
recognition quality had collapsed.

---

## 1. Summary

Meetily's saved voice profiles cannot distinguish speakers. Measured on the live profile
store (16 profiles, 52 exemplars, 512-dim CAM++):

| Metric | Measured | Healthy |
|---|---|---|
| same-speaker cosine | 0.8628 | — |
| different-speaker cosine | 0.8325 | — |
| **margin** | **0.0303** | > 0.3 |
| cosine to global mean | 0.9154 | < 0.5 |
| equal-error rate | **~34%** | < 5% |
| false-reject at production threshold 0.60 | **97%** | < 10% |

The embedding model is not at fault. The fault is a four-stage pipeline defect that
merges several distinct speakers into one saved profile. The clash detector is
correctly reporting the resulting collisions.

---

## 2. Evidence

All numbers below were measured against the live database and the 26 on-disk recordings,
using the real `wespeaker_en_voxceleb_CAM++.onnx` model and a faithful port of
`fbank.rs`. Read-only; no user data was modified.

### 2.1 The embedding model is healthy

| test | result | verdict |
|---|---|---|
| same utterance split in half (guaranteed same speaker) | **0.853** mean, 0.877 median | healthy |
| determinism (same input twice) | 1.000000 | correct |
| 10 ms time shift | 0.995 | correct |

`0.853` is the calibration anchor for everything else: it is what "same speaker" looks
like when the label is guaranteed correct.

### 2.2 Three plausible causes were falsified

Recorded so they are not re-investigated:

- **Input scaling.** The model's own metadata declares `normalize_samples = 0`
  (int16-range input), while Meetily feeds `f32` in `[-1,1]`. Measured impact: **none** —
  identical embeddings to four decimal places. Per-utterance CMN in `fbank.rs:136-153`
  subtracts a constant `2·log(scale)` offset identically across all bins and frames, so
  the scale cancels exactly. Only 0.02% of cells reach the `f32::EPSILON` log floor.
- **Mixer clipping.** 0.00% of samples clipped across all 26 recordings. Decoded peaks
  above 1.0 are AAC codec overshoot, not clipping.
- **Centroid averaging destroying the margin.** Averaging *within a pure cluster* actually
  *improves* the margin (0.06 at N=1 to 0.19 at N=8) by denoising. Averaging is therefore
  not the defect — averaging over a *contaminated* cluster is (see §3, Stage 3). Phase 3
  moves to multiple exemplars for a different reason: robustness to a single bad segment,
  not to avoid averaging as such.

### 2.3 The real defect is label misassignment, not voice drift

Same-label similarity, by time gap between the two utterances:

| gap | mean cosine |
|---|---|
| < 30 s | 0.4891 |
| 30 s – 2 m | 0.5002 |
| 2 – 5 m | 0.4947 |
| 5 – 15 m | 0.4872 |
| > 15 m | 0.4979 |

Decay across the whole meeting: **−0.0088**. Perfectly flat. Genuine voice or channel
drift would decay with time; this does not. The gap between 0.853 (true same speaker)
and ~0.49 (same *label*) is label noise.

Confirmed independently: complete-linkage clusters with internal coherence **0.927** —
tighter than the 0.853 same-speaker anchor, so almost certainly a single person — carry
**five different Meetily names**.

### 2.4 Confirmed data corruption

Two pairs of profiles hold byte-identical embedding vectors (same SHA-256):

- `Alice` @ 2026-07-21T03:25:09 ≡ `Camilia` @ 03:25:18
- `Nick` @ 2026-07-23T23:56:08 ≡ `Ralf` @ 23:57:04

Meeting `2026-07-21_11-47-20`'s `speakers.json` contains `Camilia` but **no `Alice`** —
a voice was labelled "Alice", corrected to "Camilia", and Alice silently kept the vector.

### 2.5 Better scoring cannot repair this

Held-out evaluation, split **by meeting** to avoid leakage:

| scoring | margin | EER |
|---|---|---|
| raw cosine | 0.0355 | 47.5% |
| cohort centering (current behaviour) | 0.0764 | 45.3% |
| centering + WCCN | 0.0630 | 39.0% |
| centering + LDA (dim 4) | 0.3057 | 37.6% |
| LDA(8) + WCCN | 0.2732 | 35.2% |

LDA lifts the margin nearly 9× yet barely moves EER, because the evaluation targets
themselves are wrong. This is why the fix must be applied to labelling, not scoring.

---

## 3. Root cause: four-stage amplification

**Stage 1 — several distinct speakers are assigned the same name.**
`batch.rs::map_local_speakers_to_profiles` loops over local clusters independently with
no exclusivity constraint. Nothing prevents six local clusters all matching "Ralf".

**Stage 2 — the collisions are merged into one entry.**
`commands.rs::relabel_and_merge_centroids` merges same-named entries into one
segment-weighted mean. The `2026-07-17` recording has a single `Ralf` entry holding
**304 segments — 53% of the meeting**.

**Stage 3 — the blended centroid becomes the profile.**
`load_centroid_from_folder` reads that merged entry and stores it as an exemplar. The
profile is now an average of several people, sitting at 0.9154 cosine to the global mean.

**Stage 4 — blended profiles legitimately collide.**
Two profiles each containing a blend of the same room *are* similar. The clash detector
reports this accurately.

A separate, independent defect compounds it: `diarization_rename_speaker`
(`commands.rs:233-262`) writes an exemplar for the new name on every rename but never
removes the one it wrote under the previous name, so **correcting a mislabel permanently
contaminates the profile you corrected away from**.

### 3.1 Secondary finding: the flagging statistic is biased

`flagging.rs:70-77` scores a profile pair by the **maximum over all exemplar pairs** — a
single-linkage extreme-value statistic that grows with sample count:

| exemplar comparisons | mean max-score |
|---|---|
| 1 | 0.042 |
| 6 | 0.195 |
| 36 | 0.410 |

Clash warnings therefore increase mechanically as more meetings are recorded. Mean-linkage
flags 1 pair where max-linkage flags 4.

---

## 4. Design

Five phases, sequenced so each is provable before the next depends on it.

### Phase 0 — `speaker-eval` dev tool

A Rust binary (`src/bin/speaker_eval.rs`) that calls the real
`diarization::{fbank, embedding, normalize, clustering, batch}` modules rather than
reimplementing them, so it measures the shipping path.

Reports: within-utterance truth benchmark, same/different margins, EER, threshold sweep,
per-cluster coherence and purity, and the clash matrix.

Dev-only; not wired into the shipped app. This lands first because it is the missing
feedback loop — its absence is how `0.60` and `0.55` became uncalibrated.

**Done when:** it reproduces the §2 numbers from the live database.

### Phase 1 — Integrity fixes + provenance

Un-enrolling requires knowing where an exemplar came from, which is not currently
recorded. Migration adds `source_meeting_id` and `source_label` to
`speaker_profile_embeddings` (nullable, so existing rows remain valid).

- **Un-enroll on rename** — renaming away from a name deletes the exemplar that rename
  contributed from that meeting.
- **Near-duplicate rejection** — refuse an exemplar > 0.99 cosine to one owned by a
  different name; surface it as a conflict rather than storing silently.
- **Review-gated cleanup** — a command listing cross-profile duplicate exemplars for
  confirmation before removal. Never deletes without confirmation.
- **Flagging statistic** — split one overloaded signal into two, each using the statistic
  that suits it.

  An earlier draft of this spec proposed simply replacing max-linkage with mean-linkage.
  That is wrong on its own: averaging dilutes a single contaminated exemplar, so plain
  mean-linkage misses *both* real corruptions (Alice/Camilia, Nick/Ralf) and catches only
  the genuinely-similar Ciaran/Felix pair. It would have hidden the actual problem.

  Instead:
  - **Confusability** (`flag_confusable_profiles`) uses **mean-linkage**, so the warning
    means the same thing regardless of how many meetings a profile has accumulated.
  - **Corruption** (`SpeakerProfilesRepository::duplicate_exemplars`) compares individual
    exemplars against a near-1.0 bar, naming the offending row so it can be removed.

  Verified on the live store: the settings pane drops from 7 flagged profiles to 2
  (Felix/Ciaran North, genuinely similar), while Alice/Camilia and Nick/Ralf move to a
  precise duplicates list for review.

**Done when:** the two known corrupt exemplars are detected and listed; a rename that
reassigns a speaker leaves no residue; unit tests cover all four behaviours.

### Phase 2 — Exclusive assignment

Replace the independent per-cluster loop with optimal (Hungarian) assignment over a
`local clusters × profiles` cost matrix, gated by threshold. A saved profile claims **at
most one** local cluster per meeting. Unmatched clusters become `Speaker N` rather than
silently inheriting another person's name.

`relabel_and_merge_centroids` keeps its merge behaviour: merging is legitimate when the
*user* renames two clusters to one name. It only became harmful because auto-assignment
manufactured the duplicates, which this phase eliminates.

Thresholds are then recalibrated against measured distributions using Phase 0, replacing
the current guessed constants (`CENTERED_HIGH_MATCH_THRESHOLD = 0.60`,
`CLUSTER_SIMILARITY_THRESHOLD = 0.55`, `CONFUSABLE_THRESHOLD = 0.55`).

**Done when:** no meeting assigns one profile to two clusters; Phase 0 shows the
same/different margin improving on held-out meetings.

### Phase 3 — Multi-exemplar enrollment + coherence gate

Store K diverse per-utterance embeddings per speaker, selected for coverage rather than
proximity, and score by top-k.

Note this is *not* a reversal of §2.2: averaging a pure cluster is beneficial. The
motivation here is robustness — with one stored vector, a single contaminated segment
poisons the entire profile and cannot be identified or removed afterwards. With K
exemplars carrying provenance (Phase 1), a bad one is both visible and individually
removable.

Add a **coherence gate**: a cluster whose internal coherence falls below a
harness-tuned bar (~0.75, against the 0.853 anchor) is not eligible for enrollment,
because it visibly contains more than one person.

`speakers.json` gains `coherence` and a sampled `exemplars` array. Both additive —
existing files keep working, and `centroid` remains for backward compatibility.

**Done when:** enrollment refuses a demonstrably blended cluster; profile margin measured
by Phase 0 improves over the Phase 2 baseline.

### Phase 4 — Rebuild and seed wizard

A one-time maintenance flow that recovers the value in ~361 minutes of manually labelled
audio across 26 recordings.

The manual labels cannot be trusted per-segment (§2.3), and naive purification fails —
selecting each name's dominant sub-cluster picks a shared acoustic mode common to every
name (Adrian and Dean cores scored 0.958). So the wizard treats the **names** as the
trustworthy asset and rebuilds the **vectors** from audio:

1. Re-run the fixed pipeline over all recordings.
2. Recover coherence-gated clusters and group them across meetings.
3. For each recovered group, play ~6 s and ask for confirmation, pre-filled with the
   existing 15 names as suggestions.
4. Write new profiles from confirmed groups. **Archive old profiles; do not delete.**

Resumable, and writes nothing until the user confirms.

**Done when:** the user can rebuild the profile store end-to-end, and Phase 0 shows EER
on held-out meetings substantially below the 34% baseline.

### Phase 5 (contingent, not currently in scope)

Segmentation and clustering come from the external `speakrs` v0.5.0 crate, outside this
codebase. If Phase 2 measurement shows clustering is still the bottleneck, options are
tuning the sidecar, per-channel diarization (mic vs system audio are currently summed
before diarization in `pipeline.rs:826-835`), or overlap rejection. Deliberately
unscoped until measured.

---

## 5. Error handling

Every stage degrades to current behaviour rather than failing:

- missing provenance → skip un-enroll, leave the exemplar
- assignment infeasible → all clusters become `Speaker N`
- coherence unavailable → fall back to the existing centroid path
- wizard interrupted → resumable, nothing written before confirmation

No change deletes user data without explicit confirmation.

---

## 6. Testing

Unit tests:

- Hungarian assignment, including the six-clusters-one-name case that produced `Ralf`
- un-enroll on rename leaves no residue
- duplicate exemplar rejection at the 0.99 boundary
- coherence gate accepts a clean cluster and rejects a blended one
- count-robustness of the replacement flagging statistic

Integration: Phase 0 supplies before/after numbers on held-out meetings. No phase is
considered done on the strength of code review alone — each must move a measured number.

---

## 7. Open questions

1. **Final achievable EER is unknown.** Clean clusters demonstrably exist in the audio
   (coherence 0.927 groups), so the ceiling should be well below 34%, but the `speakrs`
   clustering quality is outside this codebase and sets the limit.
2. **Coherence bar (~0.75) is provisional** and must be tuned against the harness rather
   than assumed.
3. **K for multi-exemplar storage** is currently 6 (`DEFAULT_MAX_EXEMPLARS`); whether
   that is right for per-utterance rather than per-meeting exemplars is a Phase 3
   measurement.
