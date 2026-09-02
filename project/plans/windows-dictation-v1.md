# Windows dictation v1 implementation plan

`plan_id: windows-dictation-v1`

## Goal DoD

- G1: Hold and release the configured shortcut to insert locally transcribed text at the original caret.
- G2: Replace selected text and restore the prior clipboard intact.
- G3: Run transcription and cleanup offline, with raw text returned when cleanup fails or times out.
- G4: Preserve failed delivery in history for copy, repaste, and retry.
- G5: Keep the floating overlay synchronized without stealing focus.
- G6: Refuse unsafe injection into secure or elevated targets.
- G7: Cover the lifecycle, fallback, history, failure classification, and delivery selection with automated tests.
- G8: Pass manual acceptance in Chrome, Word or Outlook, VS Code, and Windows Terminal.
- G9: Keep Meetily upstream updates mergeable through new modules and small integration points.

## Task DAG

### T1 - Dictation lifecycle and persistence

`dag_level: 0`
`blocked_by: []`
`files_touched: frontend/src-tauri/src/dictation/**, frontend/src-tauri/migrations/**`

Acceptance: lifecycle tests cover success, cancellation, failure, and cleanup fallback; SQLite can store recoverable sessions.

### T2 - Windows activation and target capture

`dag_level: 1`
`blocked_by: [T1]`
`files_touched: frontend/src-tauri/src/dictation/windows/**, frontend/src-tauri/src/lib.rs`

Acceptance: press and release events start and stop one session; the adapter records target identity without logging content.

### T3 - Short-session audio and local transcription

`dag_level: 1`
`blocked_by: [T1]`
`files_touched: frontend/src-tauri/src/dictation/audio.rs, frontend/src-tauri/src/dictation/transcription.rs`

Acceptance: microphone samples reach the selected loaded Meetily provider and return text without meeting-pipeline dependencies.

### T4 - Local cleanup and raw fallback

`dag_level: 2`
`blocked_by: [T3]`
`files_touched: frontend/src-tauri/src/dictation/cleanup.rs`

Acceptance: cleanup has a deadline and returns raw text on timeout or model error.

### T5 - Safe Windows delivery

`dag_level: 2`
`blocked_by: [T2, T4]`
`files_touched: frontend/src-tauri/src/dictation/windows/delivery.rs`

Acceptance: app-aware delivery replaces selection, preserves the clipboard, detects secure/elevated targets, and retains failed text.

### T6 - Overlay, history, settings, and diagnostics UI

`dag_level: 3`
`blocked_by: [T5]`
`files_touched: frontend/src/components/Dictation/**, frontend/src/app/**, frontend/src-tauri/tauri.conf.json`

Acceptance: overlay reflects lifecycle without focus theft; history exposes copy, repaste, retry, edit, and delete; errors give a next action.

### T7 - Acceptance, packaging, and upstream merge check

`dag_level: 4`
`blocked_by: [T6]`
`files_touched: frontend/tests/**, docs/**`

Acceptance: automated suites pass, manual app matrix is recorded, Windows installer builds, and a dry-run upstream merge reports conflicts explicitly.

## Dispatch plan

Opening frontier: T1. After T1, T2 and T3 may proceed independently. T4 follows T3; T5 joins T2 and T4; T6 and T7 are sequential integration gates.

**Created:** 2026-09-01 . **Last opened:** 2026-09-01 . **Last edited:** 2026-09-01 . **Status:** stable . **Owner:** Q. Blaauw
