# PulseTalk Windows dictation v1

## Goal

Add private, system-wide Windows dictation to the Meetily desktop app. Holding a configurable shortcut records microphone audio; releasing it transcribes and cleans the speech locally, restores the original target, and inserts text at the caret. A local history preserves every result and exposes recovery when insertion fails.

## Scope

Version one includes:

- Windows 11 hold-to-talk activation with cancel and paste-last shortcuts.
- A bottom-center floating status pill for idle, listening, processing, inserted, and failed states.
- Microphone-only short-session capture through Meetily's supported Rust/Tauri audio stack.
- Meetily Whisper and Parakeet providers behind one dictation transcription interface.
- Local text cleanup with a strict latency budget and raw-transcript fallback.
- Focus and selection capture, clipboard-safe paste, Unicode typing fallback, and per-app delivery rules.
- Local history with copy, repaste, retry, edit, delete, duration, timings, and failure reason.
- Structured local diagnostics keyed by a correlation ID. Logs redact transcript, clipboard, selected text, and document content.
- Temporary failed-audio retention for retry. The retention period remains a user setting and defaults to 24 hours.

Version one excludes meeting/calendar diary, project linking, voice commands, cloud transcription, mobile platforms, and exact copying of Wispr Flow branding or assets.

## Architecture

The dictation module is a separate bounded context inside the existing Tauri core. It reuses Meetily infrastructure through narrow adapters and does not route short dictations through the meeting recorder's mixing, diarization, or long-form VAD pipeline.

The external interface is one state machine:

```text
idle -> listening -> transcribing -> cleaning -> delivering -> completed
  |         |              |            |             |
  +---------+--------------+------------+-------------+-> failed or cancelled
```

The module owns session identity, state transitions, timing, failure classification, persistence, and recovery. Internal adapters own keyboard activation, microphone capture, transcription, cleanup, Windows target capture, delivery, history storage, overlay events, and diagnostics.

## Delivery rules

On activation, PulseTalk snapshots the foreground process, window, focused control, selection state, and keyboard modifiers. On completion it verifies that the target is still safe, restores focus when possible, and inserts at the captured caret. Selected text is replaced.

Delivery tries an app-aware method first, clipboard paste second, and Unicode typing last. Clipboard delivery snapshots all supported formats, writes the transcript without adding it to Windows cloud clipboard history where possible, waits for asynchronous consumers, and restores the snapshot. Elevated and secure targets fail closed and leave the transcript in history.

## Reliability and privacy

The hotkey hook has a watchdog and reinstalls after sleep, wake, session unlock, or silent hook loss. Audio capture begins before model work and retains the first samples until the engine is ready. The model remains warm when configured, but the microphone records only during an active session.

Every failure is assigned a stable class and a user action. Diagnostic exports contain environment, versions, timings, state transitions, and redacted errors. They exclude speech audio unless the user explicitly includes a failed recording.

## Definition of done

- Holding and releasing the configured shortcut inserts locally transcribed text at the original caret in supported non-elevated Windows applications.
- Selected text is replaced and the prior clipboard is restored intact.
- Transcription and cleanup work offline; cleanup timeout returns the raw transcript.
- Failed delivery is visible in history and can be copied or repasted without repeating transcription.
- The overlay always reflects the current state and never steals focus.
- Secure and elevated targets do not receive unsafe synthetic input.
- Automated tests cover state transitions, cleanup fallback, history, failure classification, and delivery selection.
- Manual acceptance covers Chrome, Word or Outlook, VS Code, and Windows Terminal.
- Meetily upstream updates remain mergeable because all PulseTalk behavior lives in new modules or small integration points.

**Created:** 2026-09-01 . **Last opened:** 2026-09-01 . **Last edited:** 2026-09-01 . **Status:** stable . **Owner:** Q. Blaauw
