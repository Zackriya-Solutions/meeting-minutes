# PulseTalk Windows dictation acceptance

Updated: 2026-09-01

## Verified

- The production frontend builds, including `/dictation-overlay` and `/dictation-history`.
- The unsigned local NSIS build completes successfully through `pnpm tauri:build:windows-local`.
- The packaged release opens as PulseTalk and retains the existing Meetily database and downloaded model profile.
- Settings reports the shortcut actually acquired at runtime. On the acceptance machine it is `Ctrl+Shift+Space`.
- General keeps the compact system-wide shortcut status, while the Dictation settings tab groups activation, transcription, delivery safety, and recovery-history controls.
- Dictation history loads on entry, polls every five seconds only while visible, refreshes when the window becomes visible again, and prevents overlapping requests. No manual refresh is required.
- A live hold/release capture reached the configured local Parakeet model, saved history before delivery, and pasted a transcript successfully.
- A deliberately short capture is classified as `audio_capture_failed` and remains visible in history.
- The core dictation suite passes 20 tests; one real-clipboard test is ignored by default because it temporarily owns the Windows clipboard.
- The real-clipboard test passes when run explicitly and proves an application-specific non-text clipboard format survives staging and restoration.
- Delivery is bound to the foreground window captured at key-down. Tests cover target closure, focus changes, and higher-integrity targets.
- History stores only the target process ID (`pid:<number>`), not window titles or document content.
- A dry-run merge against the current `origin/main` reports no conflict. PulseTalk changes remain isolated in focused modules and commits.

## Manual application matrix

These checks require a person to speak while holding the shortcut. They must be completed against the final packaged release before Windows dictation v1 is considered fully accepted.

| Target | Insert at caret | Replace selection | Clipboard preserved | Overlay keeps focus | Status |
| --- | --- | --- | --- | --- | --- |
| Chrome text field | — | — | — | — | Pending |
| Microsoft Word or Outlook | — | — | — | — | Pending |
| Visual Studio Code | — | — | — | — | Pending |
| Windows Terminal | — | — | — | — | Pending |

For each target:

1. Copy rich content or an image so the clipboard has more than plain text.
2. Place the caret in an editable field and hold the shortcut while speaking for at least one second.
3. Release and confirm the transcript appears at the caret.
4. Select existing text, dictate again, and confirm the selection is replaced.
5. Paste the clipboard somewhere suitable and confirm its original rich content remains intact.
6. Move focus to another window during transcription and confirm PulseTalk saves the transcript to history instead of pasting into the new target.

## Known release limitations

- The local installer is unsigned, so Windows can show a SmartScreen warning. Official distribution still requires signing credentials.
- The current package is CPU-only. The build detects the NVIDIA GPU, but CUDA packaging has not been enabled or accepted yet.
- PulseTalk tries three built-in shortcut choices when another application owns the preferred chord. Settings shows the winner, but custom shortcut editing is not implemented yet.
- Dictation currently returns the raw local transcript. A dedicated local cleanup pass with timeout and raw fallback is still pending.
