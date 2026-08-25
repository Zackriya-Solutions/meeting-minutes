# Wave 16: Speaker Label Renaming UI (PR-46)

> **Base branch:** feature/stability-wave8

## Background

Speaker-aware transcript payloads need a readable label in meeting details. Raw labels such as `Speaker 0` are hard to follow in long Chinese meetings, so users need a quick way to replace them with names such as `陈总` or `产品经理`.

This PR adds the frontend rename path. It does not invent speaker assignments: when a transcript segment has no `speaker` value, the existing transcript layout remains unchanged.

## Goals

- Carry an optional `speaker` value from `Transcript` into `TranscriptSegmentData`.
- Render the speaker label in both simple and virtualized transcript lists.
- Support inline rename, save, cancel, Enter, and Escape interactions.
- Persist overrides per meeting in localStorage.
- Provide matching UI text for all six supported locales.

## Non-Goals

- Speaker detection, embedding clustering, or diarization model changes.
- Database schema or transcript-row updates.
- Changing transcript text or exported transcript content.
- Synchronizing custom names between devices.

## Scope (PR-46, 1 commit)

| File | Change |
|---|---|
| `frontend/src/types/index.ts` | add optional speaker fields |
| `frontend/src/hooks/useSpeakerNames.ts` | load and save per-meeting name overrides |
| `frontend/src/hooks/usePaginatedTranscripts.ts` | retain optional speaker data in the paginated path |
| `frontend/src/components/MeetingDetails/TranscriptPanel.tsx` | forward speaker data and rename state |
| `frontend/src/components/VirtualizedTranscriptView.tsx` | render and edit speaker labels in both list modes |
| `frontend/locales/*/settings.json` | add rename placeholder, save, and cancel text |
| `docs/superpowers/specs/2026-07-14-diarization-rename.md` | this spec |

## Design

Overrides use the localStorage key `meetily:speakerNames:<meetingId>` and the value shape `Record<speakerId, friendlyName>`. An empty submitted name removes the override and restores the raw speaker label.

The rename control only renders when a segment supplies `speaker`. This keeps current meetings backward compatible and prevents the UI from presenting guessed speaker identities.

## Acceptance

- [x] Speaker labels render in simple and virtualized transcript lists when supplied.
- [x] Enter saves, Escape cancels, and an empty name clears the override.
- [x] Overrides are isolated by meeting ID and survive page reloads.
- [x] Segments without speaker data keep the existing layout.
- [x] All six locales include the three rename strings.
- [x] `pnpm check:i18n` passes.
- [x] `pnpm test:i18n` passes (19/19).
- [x] `pnpm build` succeeds (11 routes).

## Risks

| Risk | Mitigation |
|---|---|
| Existing meetings have no speaker field | Control stays hidden; no guessed labels are shown |
| localStorage is unavailable | Rename remains usable for the current render; storage errors are non-fatal |
| Long names crowd transcript text | Input and label use compact transcript-row styling |
| Virtualized and simple lists diverge | Both paths render the same `TranscriptSegment` component |

## Follow-up

A separate diarization PR must assign and persist stable speaker IDs before this control becomes available for every recorded meeting.
