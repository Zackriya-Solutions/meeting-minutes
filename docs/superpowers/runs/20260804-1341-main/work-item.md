# Work Item: Recording Keyboard Shortcuts Feature

**Status**: In Progress
**Branch**: enhance/recording-keyboard-shortcuts
**Run ID**: 20260804-1341-main

## Summary

Create a new feature to allow global keyboard shortcuts for start and stop recording. Default shortcut is `Control+F8`. Update the UI to allow configuring these shortcuts. Ensure proper platform-aligned permissions for macOS (primary), Windows, and Linux.

## Acceptance Criteria

- Global shortcut `Control+F8` triggers start/stop recording toggle
- UI settings page allows users to configure/rebind shortcuts
- macOS permissions properly handled (Accessibility API / permission prompt)
- Windows permissions handled (no special permission needed)
- Shortcuts persist across app restarts (stored in settings)
- Visual feedback in UI when shortcut is active/inactive (permission status shown)

## Linked Artifacts

- `docs/superpowers/runs/20260804-1341-main/requirements.md`

## History

| Timestamp | Event | Actor |
|---|---|---|
| 2026-08-04T13:41:00Z | work_item.created | sdlc-pipeline |
| 2026-08-04T13:43:00Z | work_item.linked_artifact | sdlc-pipeline — requirements.md written |
