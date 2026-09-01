# Decisions

- **Windows dictation first:** Ship system-wide Windows dictation before the meeting/calendar diary.
- **Local processing:** Use Meetily's local transcription models and a local cleanup model, with raw transcription as the timeout or failure fallback.
- **Focused insertion:** Insert at the captured caret, replace selected text, preserve clipboard contents, and retain recoverable history.
- **Failure visibility:** Store structured redacted diagnostics locally and show actionable failure reasons in history.
- **Upstream compatibility:** Keep Meetily `main` clean and implement PulseTalk on isolated feature branches.

**Created:** 2026-09-01 . **Last opened:** 2026-09-01 . **Last edited:** 2026-09-01 . **Status:** stable . **Owner:** Q. Blaauw
