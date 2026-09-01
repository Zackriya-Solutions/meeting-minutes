# PulseTalk Windows local build

PulseTalk keeps Meetily's signed release configuration intact. Local Windows installers use a small overlay that disables only updater artifact signing, because Meetily's updater private key is intentionally unavailable to local developers.

From `frontend`:

```powershell
pnpm tauri:build:windows-local
```

The installer is written to:

```text
target\release\bundle\nsis\PulseTalk_0.4.0_x64-setup.exe
```

This local installer is unsigned and Windows may show a SmartScreen warning. Official distribution builds still use `pnpm tauri:build` with the configured code-signing and updater keys.

Release diagnostics are written to:

```text
%LOCALAPPDATA%\com.meetily.ai\logs\PulseTalk.log
```

The active log is capped at 1 MB and PulseTalk retains four rotated archives. Dictation failures use stable codes such as `audio_capture_failed`, `target_lost`, `delivery_failed`, and `persistence_failed` so a failed session can be matched to its history entry without logging spoken text.

The application identifier remains `com.meetily.ai` deliberately. That preserves existing Meetily application data, downloaded transcription models, and database migrations while PulseTalk changes remain isolated on the feature branch for future upstream merges.
