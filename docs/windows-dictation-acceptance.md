# PulseTalk Windows dictation acceptance

Updated: 2026-09-01

## Verified

- The production frontend builds, including `/dictation-overlay` and `/dictation-history`.
- The unsigned local NSIS build completes successfully through `pnpm tauri:build:windows-local`.
- The packaged release opens as PulseTalk and retains the existing Meetily database and downloaded model profile.
- The production home uses the selected Hub 1 voice-workspace layout with real shortcut status, dictation history, daily word totals, and Meetily meetings. The PulseTalk speech-pulse mark now appears in the title bar, sidebar, and bundle icons.
- Settings reports the shortcut actually acquired at runtime. On the acceptance machine it is `Ctrl+Shift+Space`.
- General keeps the compact system-wide shortcut status, while the Dictation settings tab groups activation, transcription, delivery safety, and recovery-history controls.
- Dictation settings includes a persisted floating-overlay switch. When enabled, a compact microphone stays above other windows, expands on hover to show the acquired shortcut, and remains expanded through listening, transcription, cleanup, delivery, and the brief completion or failure result. Each activation first moves it to the monitor containing the focused text control; the mouse pointer is used only when Windows cannot expose a focused control. Negative desktop coordinates support displays placed left of or above the primary monitor.
- Dictation history loads on entry, polls every five seconds only while visible, refreshes when the window becomes visible again, and prevents overlapping requests. No manual refresh is required.
- The inherited meeting transcript listener retries three times, disposes listeners that finish setup after unmount, and logs `meeting_transcript_listener_setup_failed` with attempt and retry state. Exhausted retries use a non-blocking toast; they no longer open a modal alert or disable dictation.
- Dictation now enters the `cleaning` phase after local transcription. The offline cleanup removes English hesitation fillers and repairs whitespace and punctuation spacing within a 150 ms budget; explicit non-English selections retain their words.
- Cleanup timeout, failure, or empty output returns the exact raw transcript. Stable `dictation_cleanup_raw_fallback` logs record the session ID and reason without recording spoken text.
- A live hold/release capture reached the configured local Parakeet model, saved history before delivery, and pasted a transcript successfully.
- Four successful sessions at 17:42–17:43 are stored with target `pid:47640`; the current Windows process table identifies that PID as T3 Code. This verifies repeated system-wide paste into an Electron editor.
- A deliberately short capture is classified as `audio_capture_failed` and remains visible in history.
- The core dictation suite passes 31 tests; four interactive Windows tests are ignored by default because they temporarily take foreground focus or own the clipboard.
- The native Windows delivery test passes when run explicitly. It uses a real top-level window and child edit control to prove both caret insertion and selected-text replacement through the production delivery path.
- A separate two-field native Windows test passes explicitly and proves that moving focus to another edit control in the same top-level window refuses the paste and restores the rich clipboard payload.
- The native delivery and real-clipboard tests prove an application-specific non-text clipboard format survives staging and restoration.
- Two explicit Chrome runs used the production target, clipboard, and `SendInput` adapters. They inserted at the address-field caret and replaced its selected text; the captured target PID resolved to `chrome.exe`, and the application-specific clipboard format survived both runs.
- Delivery is bound to the foreground window and, when Windows exposes it, the exact focused child control captured at key-down. Moving to another field in the same window fails safely to history instead of pasting. Tests cover target closure, window and control focus changes, unreadable captured focus, and higher-integrity targets.
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

The Chrome delivery adapter is verified independently of speech capture. Chrome remains pending in the manual matrix until a person completes the same checks by speaking through the packaged app.

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

## Current local package

- Installer: `target/release/bundle/nsis/PulseTalk_0.4.0_x64-setup.exe`
- Size: `43,526,159 bytes`
- SHA-256: `9672FD910BE33476066119DCD1485485FDC6596A34A06C10BC49DAF85627F58F`
