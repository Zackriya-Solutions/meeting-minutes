# PulseTalk Windows dictation acceptance

Updated: 2026-09-01

## Verified

- The production frontend builds, including `/dictation-overlay` and `/dictation-history`.
- The unsigned local NSIS build completes successfully through `pnpm tauri:build:windows-local`.
- The packaged release opens as PulseTalk and retains the existing Meetily database and downloaded model profile.
- The production home uses the selected Hub 1 voice-workspace layout with real shortcut status, dictation history, daily word totals, and Meetily meetings. The PulseTalk speech-pulse mark now appears in the title bar, sidebar, and bundle icons.
- Settings reports the shortcut actually acquired at runtime. On the acceptance machine it is `Ctrl+Shift+Space`.
- General keeps the compact system-wide shortcut status, while the Dictation settings tab groups activation, transcription, delivery safety, and recovery-history controls.
- Dictation history loads on entry, polls every five seconds only while visible, refreshes when the window becomes visible again, and prevents overlapping requests. No manual refresh is required.
- A live hold/release capture reached the configured local Parakeet model, saved history before delivery, and pasted a transcript successfully.
- Four successful sessions at 17:42–17:43 are stored with target `pid:47640`; the current Windows process table identifies that PID as T3 Code. This verifies repeated system-wide paste into an Electron editor.
- A deliberately short capture is classified as `audio_capture_failed` and remains visible in history.
- The core dictation suite passes 20 tests; two interactive Windows tests are ignored by default because they temporarily take foreground focus or own the clipboard.
- The native Windows delivery test passes when run explicitly. It uses a real top-level window and child edit control to prove both caret insertion and selected-text replacement through the production delivery path.
- The native delivery and real-clipboard tests prove an application-specific non-text clipboard format survives staging and restoration.
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
| T3 Code editor | Pass | — | — | Pass | Verified from four completed sessions and captured target PID |

The automated native Windows edit-control check separately passes caret insertion, selection replacement, and rich clipboard preservation. Overlay behavior is not part of that fixture.

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

## Current local package

- Installer: `target/release/bundle/nsis/PulseTalk_0.4.0_x64-setup.exe`
- SHA-256: `530438AA838CBF3F0A11145CEC3A140DE86C99B64D9107C3C7FBC8EACF3735A7`
