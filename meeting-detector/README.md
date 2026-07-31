# Memento Detector

A tiny macOS **menu-bar auto-recorder** companion for the Memento/Meetily desktop
app. It lives in the tray, watches for a meeting, records your **microphone** while
the call is live, and — when the call ends — drops the recording into Memento's
recordings folder and registers it in the meeting list so you can transcribe it
later.

It is intentionally lean: no webview, no Whisper, no async runtime. The release
binary is ~5 MB (the bundle is larger only because it ships an ffmpeg sidecar).

> **The main app now does this too.** Settings → **Фоновая автозапись** (background
> auto-recording) runs the same detect → capture → register flow inside Memento
> (`frontend/src-tauri/src/background_capture/`), reusing the app's own audio
> streams, mixer, and database instead of a self-contained copy. Prefer that switch
> when Memento is open. This tray app remains the option that keeps recording when
> the main app is **closed** — that is the one thing the in-app version cannot do,
> since it only runs while Memento is running.

## How it works

1. **Detect** — every 2 s it asks CoreAudio which processes are actively using the
   microphone and keeps only recognized meeting clients (Zoom, Microsoft Teams,
   Telegram, Yandex Telemost, SberJazz, and browser calls). This is a self-contained
   copy of the main app's heuristic (`frontend/src-tauri/src/meeting_detection.rs`).
   Because it filters by the *meeting app's* identity, its own recording never trips
   the detector (no feedback loop). Detection is **macOS-only**.
2. **Record** — on detection it shows a "Recording started" notification and captures
   **both your microphone and system audio**, mixing them (like the main app) into a
   crash-safe growing `audio.wav`. System audio comes from a CoreAudio process tap
   (`system_audio.rs`, copied from the app); mic comes from cpal. Both are resampled to
   48 kHz and summed (`mixer.rs`). If system capture is unavailable (pre-14.4, permission
   denied, no output device) it degrades gracefully to mic-only.
3. **Stop** — when the meeting client stops using the mic for ~46 s, recording stops.
   Recordings shorter than 60 s are treated as false positives and discarded.
4. **Register** — the WAV is transcoded to `audio.mp4` (AAC-LC, 48 kHz mono, matching
   the main app), a meeting folder with `metadata.json` + empty `transcripts.json` is
   written, and a row is inserted into the app's SQLite DB. The meeting then appears in
   Memento with audio but no transcript — press **Enhance / Retranscribe** in the main
   app to transcribe the saved audio.

> **Mix quality.** Like the main app's shipping mixer, this is a plain `clamp(mic +
> system)` — no dynamic ducking. Levels are raw (no EBU normalization), so balance
> depends on your mic/output levels. Good enough for transcription; tune in `mixer.rs`
> if needed.

## Build & run

```bash
# One-off dev run (unbundled; notifications/mic prompts attributed to your terminal):
cargo run -p meeting-detector
RUST_LOG=debug cargo run -p meeting-detector   # verbose detection logs

# Build the signed .app bundle (recommended — proper mic permission + notifications):
./meeting-detector/build.sh
open meeting-detector/MementoDetector.app

# Validate mic + system capture without a meeting (records ~8s to a WAV you play back):
cargo run -p meeting-detector -- --selftest 8
```

The bundle embeds the ffmpeg sidecar from `frontend/src-tauri/binaries/`, so build the
main app at least once first (or point `MEMENTO_DETECTOR_FFMPEG` at an ffmpeg binary).

For distribution, re-sign `MementoDetector.app` with your Developer ID + hardened
runtime and `macos/entitlements.plist` (see the main app's `build-mac-signed.sh`).

## Permissions

- **Microphone** — required to record your side. Granted on first recording via
  System Settings → Privacy & Security → Microphone (`NSMicrophoneUsageDescription`).
- **Audio Capture** — required for system audio (the far end) via the CoreAudio tap on
  macOS 14.4+ (`NSAudioCaptureUsageDescription`). Prompted automatically on first
  recording; if denied, system audio is silent and you get mic-only.
- Detection itself only reads CoreAudio process state and needs neither.

## Menu

- **Status line** — Watching / Recording / Paused.
- **Pause auto-detection** — stop watching (and stop any active recording).
- **Start at login** — install/remove a per-user LaunchAgent (`com.meetily.detector`).
- **Open recordings folder** — reveal the recordings folder in Finder.
- **Quit** — finalizes an in-progress recording before exiting.

## Validating detection

Run with logs and join a real call:

```bash
RUST_LOG=debug cargo run -p meeting-detector
```

While you're in a meeting you should see lines like:

```
mic-active process: bundle='us.zoom.xos' name='zoom.us' -> Some(Zoom)
meeting started (Zoom) — ...
```

If your meeting app isn't recognized, the `bundle=`/`name=` it logs tells you what to
add to `classify_audio_process` in `src/detection.rs`.

## Layout

| File | Purpose |
|------|---------|
| `src/app.rs` | Tray, event loop, start/stop/finalize orchestration |
| `src/detection.rs` | Meeting detection (signals + state machine) |
| `src/recorder.rs` | cpal mic + CoreAudio system capture → mixed crash-safe WAV |
| `src/system_audio.rs` | CoreAudio process-tap system-audio capture (copied from app) |
| `src/mixer.rs` | Ring-buffer alignment, clamp-sum mixer, linear resampler |
| `src/register.rs` | Transcode to mp4, write metadata, insert DB row |
| `src/login_item.rs` | "Start at login" LaunchAgent |
| `src/paths.rs` | Resolve recordings folder / DB / ffmpeg (shared with main app) |
| `build.sh`, `macos/` | `.app` bundle assembly + Info.plist / entitlements |
