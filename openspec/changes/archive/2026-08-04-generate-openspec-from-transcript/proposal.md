# Proposal: Generate OpenSpec artifacts from meeting transcript

## Why

Users already generate an AI Summary from a meeting transcript. Some meetings
describe a feature/change to build. Today the user must manually retype that
discussion into OpenSpec (proposal/spec/design/tasks) files. We want a
one-click "Generate OpenSpec" action next to the existing Summary action that
turns the transcript into real OpenSpec artifacts the user can download.

## What Changes

- Add a "Generar especificaciones" (Generate OpenSpec) button in the meeting
  details page, next to `SummaryGeneratorButtonGroup`.
- Detect whether Node.js is installed (OpenSpec CLI, https://github.com/Fission-AI/OpenSpec,
  is an npm package and requires Node). If missing, show a blocking, actionable
  message (same UX pattern as the existing "Ollama not installed" error) with
  install instructions/link. No auto-install of an embedded Node runtime.
- On click, run the real `openspec` CLI (via `npx openspec@latest` or a
  detected global install) from the Rust/Tauri backend, feeding it a working
  directory seeded with a prompt/context file derived from the transcript
  (and summary, if present), producing `openspec/changes/<slug>/{proposal.md,
  specs/**, design.md, tasks.md}`.
- Zip the generated `openspec/changes/<slug>/` folder and trigger a native
  "Save As" dialog so the user downloads it.
- Regeneration overwrites the previous result for that meeting (same
  overwrite semantics as Summary regeneration) — no versioning.

## Non-Goals

- No auto-installation of a bundled/portable Node.js.
- No in-app viewer/editor for the generated OpenSpec files (download only,
  for v1).
- No multi-meeting/batch generation.

## Impact

- Affected specs: new capability `openspec-generation` (frontend UI + Tauri
  backend command + external CLI invocation + packaging/zip + Node.js
  prerequisite detection).
- Affected code (expected, to be confirmed in design/tasks):
  - `frontend/src/components/MeetingDetails/` (new button group, mirroring
    `SummaryGeneratorButtonGroup.tsx`)
  - `frontend/src/app/meeting-details/page-content.tsx` (wiring)
  - `frontend/src-tauri/src/` (new `openspec/` module: commands.rs,
    service.rs — mirrors `summary/` module shape; Node/CLI detection mirrors
    existing "Ollama not installed" detection pattern in
    `frontend/src/lib/utils.ts` / model settings flow)
  - Need new save-dialog + zip capability — no existing download/export
    mechanism exists elsewhere in the app today (confirmed via exploration:
    no `tauri-apps/plugin-dialog`, no `writeFile`/`saveAs` usage found), so
    this introduces the first export path in the codebase.

## Decisions Locked (from product Q&A)

1. Button placement: next to the Summary button on meeting details.
2. Node.js prerequisite: detect + guide manual install (no embedded Node).
3. Delivery: zip download via native Save As dialog.
4. Regeneration: overwrite, no versioning.
