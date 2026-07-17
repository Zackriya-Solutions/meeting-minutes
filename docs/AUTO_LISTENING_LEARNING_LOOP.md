# Memento auto-listening and long-term learning loop

## Outcome

Memento can start a normal local recording when a supported native meeting
client begins using the microphone, save the result into a system Inbox, and
offer reviewable meeting boundaries, type/series placement, transcript terms,
and speaker identities. Confirmed evidence improves later meetings. Historical
changes are proposed through reconciliation and are never applied silently.

The supported implementation is entirely inside the Tauri desktop app. The
archived FastAPI backend is not involved.

## End-to-end flow

1. The OS detector observes a normalized client kind and microphone ownership.
   Process names, window titles, URLs, and frame-level activity are not stored.
2. A supported native microphone signal starts the existing recording pipeline
   on its first poll. Browser calls, Telegram, and process-only evidence remain
   confirmation prompts. This avoids keeping an ambient pre-call audio buffer.
3. A capture session records privacy-safe lifecycle transitions. After 45
   seconds without the call signal, only the auto-started recording is stopped;
   manual recordings are never auto-stopped.
4. The saved meeting is linked to the capture, gets time bounds and optional
   split suggestions for transcript gaps of five minutes or more, and enters
   `Memento Inbox`.
5. The meeting page presents one review surface for boundaries, meeting type,
   voice candidates, terminology, and historical reconciliation.
6. Accepted meeting types and collection suggestions remove the item from the
   Inbox. A reviewed standup uses the existing Standup V2 workflow.
7. Transcript corrections preserve raw ASR text and create scoped terminology
   candidates. Confirmed canonical terms are supplied to later Whisper runs as
   a bounded vocabulary prompt and to summarization as context explicitly
   marked “not evidence”.
8. Local diarization stores immutable cluster observations. A voice match is a
   prediction, not a label. Only explicit user confirmation can create a
   trusted voice sample. Eligible samples build versioned multi-centroid
   profiles.
9. A changed profile or confirmed term scans older observations and creates
   versioned reconciliation proposals. Applying a proposal is an explicit user
   action and can be rolled back.

## P0–P5 coverage matrix

| Phase | Requirement | Implementation/evidence |
| --- | --- | --- |
| P0 | Threat model and local consent boundary | Local-only capture/learning tables; no raw client identity; outbound summary policy remains centralized; voice learning and advanced profiles are opt-in. |
| P0 | Immutable evidence and open-set evaluation | `learning_events`, `identity_assertions`, raw transcript preservation, `quality_observations`; `quality-gate.mjs` includes known/unknown identity precision, false-accept, and profile-contamination metrics. |
| P0 | Retention and crash recovery | `capture_retention_policy`, startup recovery for interrupted captures, expiry of unpromoted observations, optional safe deletion of saved auto-captures. |
| P1 | Trusted confirmed voice | Quality/duration/overlap eligibility, user confirmation UI, Unknown fallback, audit event. |
| P2 | Multi-centroid profiles and negatives | Up to five centroids, dispersion penalty, channel hints, trusted negative assertions, voice floor, top-two margin, versions and rollback. |
| P3 | Series context with bounded influence | Reviewed series-attendance edges require repeated support; context boost is capped and cannot override the voice floor. |
| P3 | Terminology correction loop | Versioned raw-preserving corrections, scoped aliases/evidence, confirmation UI, Whisper vocabulary hints, summary context, stale artifact tracking. |
| P4 | Cross-meeting reconciliation | Identity and terminology backfill proposals preserve previous values, require review, and support rollback. |
| P4 | Deletion/unlearning | Speaker purge removes samples, centroids, profile versions, context/language/dynamics derivatives and detaches transcript identity. |
| P5 | Language/dynamics and drift | Explicitly enabled shadow profiles require at least three reviewed meetings and are not used for identity decisions; inference score/margin/Unknown and profile dispersion are recorded for drift analysis. |

## Non-negotiable safeguards

- A model prediction is never a positive training label.
- Automatic known-speaker assignment is off by default. Even when enabled it
  requires the voice floor, confidence threshold, and top-two margin.
- Context can generate/rank candidates but cannot rescue a weak voice match.
- Short, low-quality, or overlapping samples are retained as excluded evidence,
  not training data.
- A transcript correction never overwrites `raw_transcript`.
- Glossary entries reach ASR/summarization only after confirmation.
- Historical changes are proposals; acceptance is auditable and reversible.
- Deleting speaker learning data removes biometric-like derivatives rather than
  merely hiding a display name.
- System Inbox membership is controlled by the classification workflow and the
  system collection cannot be renamed or deleted.

## P0 threat model and consent boundary

| Asset or failure mode | Boundary | Control |
| --- | --- | --- |
| Ambient speech before a call | Capture | No continuous audio pre-roll; native-client recording starts on the first strong microphone poll. |
| Sensitive client/window metadata | Detection | Raw process names, titles, URLs, and frame-level activity remain in memory; the database receives normalized client kinds and lifecycle transitions only. |
| A wrong voice prediction contaminates a person profile | Identity | Predictions are untrusted assertions. Only an explicit confirmation with the separate learning checkbox can create a sample, and duration/quality/overlap gates may still exclude it. |
| A familiar attendee makes a weak voice match look certain | Fusion | Context contributes at most 0.05 and never bypasses the voice floor or top-two margin. Unknown remains a first-class result. |
| A transcript correction rewrites history | Terminology | Raw ASR text is immutable; normalized versions, aliases, evidence, model versions, and reviewer actions are stored separately. |
| A new profile rewrites old meetings incorrectly | Reconciliation | Backfill creates proposals with previous/proposed values and evidence. Apply and rollback are explicit, logged operations. |
| Deletion leaves biometric-like derivatives | Unlearning | Purge removes samples, centroids, versions, context and shadow profiles, revokes consent, and detaches transcript identity. |
| Retained recordings outlive user expectations | Retention | Unpromoted metadata expires by default; saved-audio expiry is opt-in, bounded, and uses path-safe deletion. |

Voice learning consent is separate from accepting a displayed speaker name.
Advanced language/dynamics memory is a second explicit opt-in and remains
shadow-only. No local evidence is uploaded by this feature.

## What is collected from meeting clients — and what improves voices

Client detection data answers only *when a meeting probably starts and ends*
and which normalized client class supplied the signal. It does not improve
speaker recognition by itself. The voice loop uses different, content-derived
evidence:

1. local diarization creates a meeting-scoped cluster embedding and exact
   transcript-segment provenance;
2. the user confirms or rejects the proposed person, or keeps it Unknown;
3. only a separately approved, eligible speech sample enters the profile;
4. repeated confirmed samples form multiple versioned centroids rather than a
   single continuously averaged vector;
5. future clusters are compared to those centroids with open-set thresholds,
   negative evidence, and a capped reviewed-series prior;
6. improved profiles produce reviewable proposals for older meetings.

This distinction prevents harmless client telemetry from being mistaken for
training data and makes every actual source of voice improvement inspectable.

## Data model

Capture:

- `capture_sessions`, `capture_observations`, `capture_retention_policy`
- `meeting_windows`

Review/classification:

- `meeting_type_suggestions`, `collection_suggestions`, `learning_events`

People memory:

- `speaker_clusters`, `speaker_cluster_segments`
- `identity_inference_runs`, `identity_assertions`
- `voice_samples`, `voice_centroids`, `speaker_profile_versions`
- `context_edges`, `language_profiles`, `conversation_dynamics_profiles`

Terminology/reconciliation:

- `transcript_corrections`
- `terminology_terms`, `terminology_aliases`, `terminology_evidence`
- `reconciliation_runs`, `reconciliation_suggestions`
- `artifact_versions`, `quality_observations`

## Operational notes

- macOS supplies the current microphone-session observer. Other platforms keep
  the bounded process-launch confirmation path until equivalent native
  observers are implemented.
- The detector polls every two seconds; starting on the first strong native
  signal bounds missed audio without continuous ambient capture.
- Auto-stop uses a 45-second quiet grace period to survive reconnects and brief
  client transitions.
- Saved audio retention is unset by default. If a user configures it, deletion
  uses the same path-safety checks as Interview Memory retention.
- Language/dynamics profiles are shadow-only until the open-set corpus proves a
  safe incremental gain across channel and duration cohorts.

## Verification

Run from `frontend` unless otherwise stated:

```bash
pnpm run typecheck
pnpm run quality:smoke
```

Run from `frontend/src-tauri`:

```bash
cargo test --lib
cargo check --lib
```

The release quality gate additionally requires a real, consented identity corpus
with at least 15 known-speaker and 10 open-set examples. Synthetic smoke data
only validates metric wiring and invariants; it does not prove production
accuracy.
