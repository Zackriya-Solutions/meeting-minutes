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

The application identifier remains `com.meetily.ai` deliberately. That preserves existing Meetily application data, downloaded transcription models, and database migrations while PulseTalk changes remain isolated on the feature branch for future upstream merges.
