# Speaker Diarization Design

Date: 2026-06-26  
Status: design draft for review  
Scope: macOS Apple Silicon first, local-first speaker diarization for recorded calls

## Summary

Add speaker diarization to meeting transcripts. The feature separates "who spoke when" without identifying people by real identity or storing voiceprints. Live diarization gives provisional speaker labels during the call. After the meeting, an offline diarization pass reviews the full audio and replaces provisional labels with final labels when quality is acceptable.

The intended user experience is practical for dense work calls: main speakers and longer turns should be stable, interruptions should be marked honestly, and the user should be able to merge or split speakers quickly when clustering makes a mistake.

## Goals

- Add local speaker diarization for meeting transcripts.
- Support automatic speaker count with no hard participant cap.
- Treat live labels as provisional and post-call labels as authoritative.
- Preserve the current transcription and recording behavior while adding diarization metadata.
- Handle interruptions by marking overlap as "multiple speakers" instead of pretending to know a single speaker.
- Keep the first implementation scoped to macOS Apple Silicon.
- Expose feature controls through existing settings patterns and reusable design-system settings primitives.

## Non-goals

- Speaker identification by real name from voice.
- Voiceprint enrollment or biometric user profiles.
- Perfect attribution of every short backchannel, interruption, or "угу/да" in dense meetings.
- Cloud diarization as the default path.
- A hard maximum number of participants.

## Product requirements

### Speaker behavior

- Local microphone audio is assigned to a stable local speaker label, displayed as "You" or the localized equivalent.
- Remote/system audio is diarized separately from the local microphone when separate source audio is available.
- Speaker labels use neutral names by default: `Speaker 1`, `Speaker 2`, etc.
- Users can rename speakers after a meeting.
- Users can merge speakers when one real person is split into multiple clusters.
- Users can split or correct speaker assignments for obvious mistakes.
- Short, low-confidence clusters should not immediately appear as stable speakers. The UI can hold them as "Unknown speaker" or "Short speaker" until they accumulate enough speech. This is not a speaker-count cap.

### Participant count

- The diarization system must not impose a user-visible maximum such as 2, 4, or 20 speakers.
- Resource safeguards may exist for memory, runtime, and pathological audio, but they must not be described or implemented as a participant limit.
- The system should prefer quality flags over silent truncation when a recording is too noisy or too large to diarize well.

### Overlap and interruptions

- Overlapping speech is represented as `multiple speakers` / `overlap`.
- The first version should not try to assign the same transcript text to multiple speakers.
- Overlap markings should be visible in the transcript and available to summary generation.

### Live vs post-call

- Live labels are provisional.
- Post-call offline diarization is the primary source of truth.
- If the post-call pass succeeds, it atomically replaces provisional labels.
- If the post-call pass fails, times out, or produces poor coverage, the app keeps provisional labels and marks the meeting as `fallback_to_live`.
- Speaker colors and numbers should remain stable across the live-to-final transition by mapping final clusters to provisional clusters through temporal overlap.

## Current project context

The current app already has:

- A Tauri/Rust audio pipeline.
- Local transcription providers such as Parakeet and Whisper.
- VAD-based transcription segmentation.
- Meeting transcript storage with timing fields.
- Settings screens for General, Recordings, Transcription, Summary, and Beta.
- UI primitives such as `Switch`, `Select`, `Input`, `Tabs`, and form row components.

Current gaps:

- Transcript records do not consistently expose speaker metadata to the frontend.
- Recording currently centers on mixed audio. Diarization needs access to separate microphone and system audio where possible.
- There is no diarization coordinator, speaker segment store, status model, or speaker review UI.
- There is no settings block for enabling, configuring, or explaining speaker diarization.

## Recommended approach

Use a hybrid diarization pipeline:

1. Live pass: lightweight online diarization on system audio for provisional labels.
2. Post-call pass: offline diarization over the full recording for final labels.
3. Fallback: preserve provisional labels if the offline pass is unavailable or poor.
4. Optional later enhancement: heavier second-pass diarization for important meetings when default quality flags are poor.

This gives useful live feedback without making live clustering responsible for final transcript quality.

## Architecture

### Audio capture and storage

Keep the existing mixed audio path for recording and transcription. Add source-aware audio handling for diarization:

- microphone source: saved or buffered separately;
- system audio source: saved or buffered separately;
- mixed audio: continues to serve current playback/transcription flows.

The diarization pipeline should prefer separate stems:

- local mic stem -> fixed local speaker;
- system stem -> diarized remote speakers.

If separate stems are unavailable, diarization can run on mixed audio with a visible quality warning.

### Diarization engine

For macOS Apple Silicon first:

- Use a local Swift sidecar for FluidAudio integration.
- Live mode uses online diarization/clustering for provisional speaker intervals.
- Post-call mode uses offline diarization, preferably full-recording VBx-style clustering.
- The Rust app owns orchestration, persistence, status events, and transcript alignment.

The sidecar interface should exchange timestamped intervals rather than transcript text:

```json
{
  "meeting_id": "string",
  "source": "system",
  "status": "provisional",
  "segments": [
    {
      "start_ms": 12500,
      "end_ms": 18100,
      "speaker_id": "remote-1",
      "confidence": 0.82,
      "is_overlap": false
    }
  ]
}
```

### Transcript alignment

A Rust diarization coordinator aligns diarization intervals to transcript chunks using existing `audio_start_time`, `audio_end_time`, and duration fields.

Rules:

- Assign the speaker with the largest overlap with the transcript chunk.
- If overlap speech is detected for the chunk, mark `is_overlap = true` and display `Multiple speakers`.
- If no interval matches confidently, keep `speaker_id = null` and display `Unknown speaker`.
- Preserve transcript text and timing; diarization updates metadata only.

### Persistence model

Add transcript-level fields:

- `speaker_id`
- `speaker_label`
- `speaker_color`
- `is_overlap`
- `diarization_status`: `none | provisional | final | fallback_to_live | failed | needs_review`
- `diarization_method`: e.g. `fluid_audio_online`, `fluid_audio_offline`, `manual`
- `diarization_confidence`

Add a meeting-level diarization status:

- `enabled`
- `live_enabled`
- `post_call_enabled`
- `current_status`
- `quality_flags`
- `processed_at`

Consider a separate `speaker_segments` table for raw diarization intervals. Transcript rows should store the resolved speaker assignment for fast rendering, while raw intervals remain available for re-alignment.

### Frontend transcript UI

The transcript UI should show:

- speaker label;
- speaker color;
- provisional/final state where relevant;
- overlap badge for interruptions;
- unknown/low-confidence label when needed.

During a live meeting:

- display provisional labels with a subtle `Provisional` badge;
- avoid implying final accuracy;
- do not require the user to fix speakers during the call.

After post-call processing:

- replace provisional labels with final labels;
- show `Final labels applied` when successful;
- show `Fallback to live labels` or `Needs review` when quality is poor.

### Speaker review UI

Post-call review should support:

- rename speaker;
- merge two or more speakers;
- split selected transcript segments into a new speaker;
- mark selected segment as multiple speakers;
- clear speaker assignment.

This is required for dense work calls. Automatic diarization will sometimes split one person across clusters or merge similar voices.

## Settings and design-system controls

### Placement

Add the primary controls to `Settings -> Transcription`, because diarization changes transcript output. Show source-audio quality requirements inline or link to `Settings -> Recordings`.

If the feature is initially beta-gated, expose a short beta toggle in `Settings -> Beta`, but keep detailed controls in `Transcription`.

### User-facing settings

Required settings:

- `Enable speaker diarization`
  - main switch;
  - default off unless the feature is explicitly beta-enabled.
- `Mode`
  - `Live + post-call refinement` as the recommended default;
  - `Post-call only`;
  - `Off`.
- `Show provisional speaker labels during call`
  - enabled only when live mode is active.
- `Post-call refinement`
  - `Run automatically after meeting`;
  - default on.
- `Overlap handling`
  - first version fixed to `Mark as multiple speakers`;
  - do not expose multiple advanced options yet.
- `Speaker review`
  - enable merge/split/rename review tools after processing.
- `Separate audio sources`
  - status row or warning showing whether microphone and system audio can be saved separately.

Advanced or later settings:

- optional heavier second-pass enhancement for important meetings;
- model download/status details;
- manual re-run diarization;
- export speaker segments.

### Design-system primitives

Add or standardize these settings primitives:

- `SettingsSection`
  - card wrapper with title, description, optional badge, and children.
- `SettingsRow`
  - label, description, right-aligned control, disabled state, disabled reason.
- `SettingsStatusBadge`
  - statuses: `Beta`, `Local`, `Provisional`, `Final`, `Fallback`, `Needs review`, `Unavailable`.
- `ExpandableAdvancedSettings`
  - collapsed by default; used for model/provider diagnostics and heavier second-pass options.
- `ModelAvailabilityRow`
  - model installed/downloading/unavailable states.

These should be generic components, not diarization-only components.

### Copy rules

- Use "speaker diarization" in technical settings descriptions only when useful.
- Prefer "speaker labels" in user-facing text.
- Always label live output as provisional.
- Do not promise exact attribution in dense overlapping speech.

## Fallback and failure handling

### Offline pass succeeds

- Save final diarization intervals.
- Map final speakers onto live speaker labels by temporal overlap.
- Update transcript speaker fields atomically.
- Mark meeting diarization status as `final`.

### Offline pass fails

- Keep live labels.
- Mark meeting diarization status as `fallback_to_live`.
- Show a non-blocking warning in meeting details.
- Allow manual speaker edits.

### Offline pass quality is poor

Quality can be considered poor when:

- too much speech is unknown;
- too much speech is overlap;
- speaker clusters are heavily fragmented;
- very short clusters dominate the result;
- source audio was mixed-only or missing.

In this case:

- keep final labels if they are better than live labels;
- otherwise keep live labels;
- mark the meeting as `needs_review`;
- offer manual review and, later, optional enhancement.

### Models unavailable

- Keep diarization off.
- Show model availability in settings.
- Do not block transcription or recording.

## Dense work call expectations

This feature is sufficient for a useful beta when:

- main speakers and longer turns are labeled reliably;
- short backchannels may be unknown or overlap;
- interruptions are marked instead of over-attributed;
- manual merge/split fixes are fast;
- summaries can reference speakers after final processing.

It is not sufficient to claim exact speaker recognition for every utterance in crowded meetings.

## Summary integration

Summary generation should prefer final diarization.

Rules:

- If final diarization is available, include speaker labels in the transcript context passed to summaries.
- If only provisional labels are available, either wait for post-call processing or mark speaker labels as provisional in the summary input.
- If diarization failed, generate summaries without speaker attribution rather than using misleading labels.

## Privacy

- Diarization runs locally by default.
- Do not store biometric voiceprints.
- Do not infer real names from voices.
- Speaker renames are user-authored meeting metadata.
- Any optional future cloud or heavier external processing must require explicit user action.

## Testing plan

### Unit tests

- interval-to-transcript alignment;
- overlap detection and `Multiple speakers` assignment;
- live-to-final speaker mapping by temporal overlap;
- fallback status transitions;
- settings serialization defaults.

### Integration tests

- diarization disabled does not alter current recording/transcription flows;
- live provisional events render and persist;
- offline pass replaces provisional labels atomically;
- offline failure preserves live labels and sets fallback status;
- mixed-only audio produces quality warning.

### UI tests

- settings rows render correct enabled/disabled states;
- live transcript shows provisional badges;
- final transcript hides provisional badges;
- overlap segments show `Multiple speakers`;
- speaker review supports rename and merge flows.

## Rollout plan

1. Add settings/data model behind a beta flag.
2. Add source-aware audio capture and persistence without changing existing mixed recording behavior.
3. Add diarization sidecar integration for macOS Apple Silicon.
4. Add live provisional labels.
5. Add post-call offline refinement and fallback.
6. Add transcript UI and post-call speaker review.
7. Add summary integration using final speaker labels.
8. Add quality telemetry that remains local unless analytics consent explicitly allows aggregated event tracking.

## Follow-up implementation plans

- FluidAudio macOS sidecar and model packaging.
- Source-aware microphone/system audio stem persistence.
- Live provisional diarization event stream.
- Offline post-call refinement orchestration.
- Speaker review editor actions backed by database updates.

## Open implementation decisions

- Exact minimum accumulated speech duration before surfacing a new stable speaker.
- Whether raw diarization intervals belong in SQLite only or also in a sidecar JSON artifact per recording.
- Exact UI location for speaker review: inline transcript toolbar vs separate review panel.
- Whether optional pyannote-based enhancement is shipped in v1 or delayed until after FluidAudio beta validation.

## Success criteria

- No visible hard participant cap.
- Provisional labels appear during live calls when enabled.
- Final labels replace provisional labels after post-call processing.
- Overlaps are visible as `Multiple speakers`.
- Manual merge/split/rename can correct common dense-call diarization mistakes.
- Disabling diarization leaves existing recording and transcription behavior unchanged.
