# Wave 16: Real-time Processing Progress UI (PR-48)

> **Parent:** PR-46 speaker label renaming UI
> **Target branch:** devtest

## Background

After a recording stops, meetily waits for transcription completion, flushes buffered segments, and saves the meeting. The previous overlay showed only a generic spinner, so users could not tell which stage was active or whether the app was still making progress.

## Goals

- Show a live three-stage progress indicator for stopping, transcription finalization, and saving.
- Surface the current status message, including the remaining chunk count when available.
- Keep recording controls disabled throughout the stop and save lifecycle.
- Localize all new lifecycle messages for en-US, en-GB, zh-CN, zh-TW, ja-JP, and ko-KR.
- Reuse the existing `RecordingStateContext` and polling flow without database or Rust changes.

## Non-Goals

- Adding a false percentage when the backend does not expose total work.
- Changing transcription polling, timeout, or save behavior.
- Adding progress UI to model downloads, audio imports, or manual retranscription.
- Adding database queries or persistence.

## Scope (PR-48, 1 commit)

| File | Change |
|---|---|
| `frontend/src/app/_components/StatusOverlays.tsx` | render the live stage indicator and current message |
| `frontend/src/app/page.tsx` | pass global lifecycle status and lock controls during stopping |
| `frontend/src/hooks/useRecordingStop.ts` | emit localized lifecycle messages |
| `frontend/locales/*/summary.json` | add stopping, waiting, chunk-processing, and flush messages |
| `docs/superpowers/specs/2026-07-15-processing-progress.md` | this spec |

## Design

`RecordingStateContext.status` selects the active stage. `statusMessage` supplies the current detail text for transcription and saving; the overlay falls back to the localized stage label when no detail is available. The stage indicator is determinate by lifecycle stage, not by a fabricated percentage.

When the status is `STOPPING`, the page hides recording controls and prevents a second recording from starting while the existing stop sequence completes.

## Acceptance

- [x] The overlay appears during stopping, transcription finalization, and saving.
- [x] The active stage and completed stages are visually distinguishable.
- [x] Remaining chunk messages update without exposing raw English in supported locales.
- [x] Recording controls remain hidden during the complete stop lifecycle.
- [x] All six locales contain the new lifecycle messages.
- [x] `pnpm check:i18n` passes.
- [x] `pnpm test:i18n` passes (19/19).
- [x] `pnpm build` succeeds (11 routes).

## Risks

| Risk | Mitigation |
|---|---|
| Backend does not expose total work | Show honest lifecycle stages instead of an inaccurate percentage |
| Polling returns no queue count | Keep the localized stage message as the fallback |
| User attempts a second recording during stop | Hide controls and pass the processing guard to `RecordingControls` |

## Follow-up

A future backend progress event can add a determinate percentage and richer per-stage details without changing the overlay contract.
