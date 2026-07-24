# Meetily for Android (Experimental)

Meetily's desktop app is built on Tauri 2.x, which also targets Android. The
repository is set up so the same Rust core (audio capture, Whisper
transcription, SQLite storage, summary orchestration) and the same Next.js UI
compile into an Android app.

**Status: experimental.** The Android target is a port of the desktop app, not
a separate codebase. Read [Known limitations](#known-limitations) before
building.

## What works on Android

| Feature | Status | Notes |
|---|---|---|
| Microphone recording | ✅ | Via cpal's AAudio/oboe backend. Meeting audio is picked up through the mic. |
| Whisper transcription | ✅ | whisper.cpp cross-compiled with the NDK, CPU (NEON). Use `tiny`/`base`/`small` models on phones. |
| Meetings, transcripts, notes storage | ✅ | Same SQLite database layer (sqlx). |
| Summaries via Ollama (LAN) / Claude / Groq / OpenRouter / custom OpenAI endpoint | ✅ | Network providers work as on desktop. |
| Silero VAD | ⚠️ | Requires bundling `libonnxruntime.so` (see below). |
| System audio capture | ❌ | Android does not allow capturing other apps' audio. Mic-only. |
| Built-in AI summaries (llama-helper sidecar) | ❌ | Android apps cannot exec sidecar binaries. Returns a clear error pointing users to Ollama/cloud providers. |
| Parakeet (ONNX) transcription | ⚠️ | Compiles with `ort` load-dynamic; untested. Whisper is the supported engine on Android. |
| Auto-updater / system tray / audio import & export via ffmpeg | ❌ | Desktop-only (gated out of the mobile build). |

## Prerequisites

1. **Rust** with the Android targets:
   ```bash
   rustup target add aarch64-linux-android  # arm64 devices (recommended)
   ```
2. **Android Studio** (or plain SDK command-line tools) with:
   - Android SDK Platform 34+
   - Android SDK Build-Tools
   - **NDK (Side by side)** r26+
   - Android SDK Platform-Tools (adb)
3. **JDK 17**
4. **pnpm** and the project's Node dependencies (`pnpm install` in `frontend/`)
5. Environment variables:
   ```bash
   export ANDROID_HOME="$HOME/Android/Sdk"          # macOS: ~/Library/Android/sdk
   export NDK_HOME="$ANDROID_HOME/ndk/<version>"
   export JAVA_HOME=/path/to/jdk-17
   ```

## First-time setup

```bash
cd frontend
pnpm install

# Generate the Android project (creates src-tauri/gen/android)
pnpm tauri android init
```

### Automated setup (recommended)

Steps 1–3 below are automated by a script — after `pnpm tauri android init`,
run:

```bash
node scripts/setup-android-project.js
```

It patches the manifest permissions, adds the runtime permission request to
`MainActivity.kt`, and installs `libonnxruntime.so` into jniLibs. It is
idempotent and safe to re-run. The sections below describe what it does, for
reference or manual setup.

### CI builds

The repository ships a GitHub Actions workflow,
[`.github/workflows/build-android.yml`](../.github/workflows/build-android.yml),
that runs the full pipeline (init → setup script → NDK build → sign → upload)
and publishes an installable arm64 APK as a build artifact. Without signing
secrets it signs with an ephemeral debug key; set `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD`, and `ANDROID_KEY_ALIAS` repository secrets for
release signing.

### 1. Add permissions to the generated AndroidManifest.xml

Edit `src-tauri/gen/android/app/src/main/AndroidManifest.xml` and add inside
`<manifest>` (Tauri already adds `INTERNET`):

```xml
<uses-permission android:name="android.permission.RECORD_AUDIO" />
<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
<uses-permission android:name="android.permission.WAKE_LOCK" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_MICROPHONE" />
```

### 2. Request the microphone permission at runtime

Android requires a runtime permission prompt for the microphone. The simplest
approach is to request it from the generated `MainActivity.kt`
(`src-tauri/gen/android/app/src/main/java/com/meetily/ai/MainActivity.kt`):

```kotlin
import android.Manifest
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    val perms = arrayOf(
      Manifest.permission.RECORD_AUDIO,
      Manifest.permission.POST_NOTIFICATIONS,
    )
    val missing = perms.filter {
      ContextCompat.checkSelfPermission(this, it) != PackageManager.PERMISSION_GRANTED
    }
    if (missing.isNotEmpty()) {
      ActivityCompat.requestPermissions(this, missing.toTypedArray(), 1)
    }
  }
}
```

### 3. Bundle ONNX Runtime (required for Silero VAD)

The Rust `ort` crate is built with `load-dynamic` on Android: it `dlopen`s
`libonnxruntime.so` at runtime instead of downloading prebuilt binaries at
compile time (none exist for Android).

1. Download the official [`onnxruntime-android`](https://mvnrepository.com/artifact/com.microsoft.onnxruntime/onnxruntime-android) AAR from Maven Central.
2. Unzip it (AARs are zip files) and copy `jni/arm64-v8a/libonnxruntime.so` to:
   ```
   src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a/libonnxruntime.so
   ```

If the library is missing, VAD initialization fails at runtime (recording still
works; every audio window is forwarded to Whisper without speech filtering).

## Development

> **Required:** export `ORT_SKIP_DOWNLOAD=1` when building for Android. The
> `ort-sys` crate has no prebuilt ONNX Runtime binaries for Android and would
> fail the build trying to download them; on Android the library is loaded at
> runtime from jniLibs instead (see above). The `pnpm tauri:android:*` scripts
> set this automatically.

```bash
cd frontend

# Run on a connected device or emulator with hot reload
pnpm tauri:android:dev
# equivalent to: ORT_SKIP_DOWNLOAD=1 pnpm tauri android dev

# Pick a specific device
ORT_SKIP_DOWNLOAD=1 pnpm tauri android dev --device
```

On Windows (PowerShell): `$env:ORT_SKIP_DOWNLOAD="1"` before running the
`pnpm tauri android ...` commands directly.

Logs go to logcat with the tag `meetily`:

```bash
adb logcat -s meetily RustStdoutStderr
```

## Release build

```bash
cd frontend
pnpm tauri:android:build
# equivalent to: ORT_SKIP_DOWNLOAD=1 pnpm tauri android build --apk --target aarch64
# Output: src-tauri/gen/android/app/build/outputs/apk/...
```

For a signed APK/AAB, configure signing in
`src-tauri/gen/android/app/build.gradle.kts` per the
[Tauri Android signing docs](https://tauri.app/distribute/sign/android/).

## How the port is wired

- `src-tauri/tauri.android.conf.json` — Android config overlay: removes
  desktop-only capabilities (updater, process, tray, menu) and the
  `llama-helper`/`ffmpeg` sidecar binaries.
- `src/lib.rs` — `#[cfg_attr(mobile, tauri::mobile_entry_point)]` entry point;
  updater/process plugins and hide-to-tray behavior are `#[cfg(desktop)]`;
  a no-op `tray` shim keeps call sites unchanged on mobile; `android_logger`
  routes Rust logs to logcat.
- `src/audio/devices/platform/android.rs` — mic-only device enumeration via
  cpal (AAudio/oboe). System-audio paths already bail on non-macOS/Windows.
- `Cargo.toml` — `[target.'cfg(target_os = "android")'.dependencies]`:
  CPU whisper-rs, vendored OpenSSL (for reqwest), `ort` with `load-dynamic`,
  `android_logger`.
- `src/summary/summary_engine/sidecar.rs` — returns a descriptive error on
  Android instead of spawning the llama-helper process.

## Known limitations

- **Mic-only capture.** Android forbids capturing other apps' audio output.
  For online meetings, use speakerphone so the mic hears both sides.
  (`AudioPlaybackCapture` exists on API 29+ but only for apps that opt in —
  most meeting apps do not.)
- **No built-in (offline) summaries yet.** The llama.cpp sidecar model does
  not translate to Android; an in-process `llama-cpp-2` integration is the
  planned replacement. Until then use Ollama on your LAN or a cloud provider.
- **No audio import/export.** These flows shell out to an ffmpeg sidecar.
- **Model sizes.** `medium`/`large` Whisper models are impractical on phones;
  the model manager works, but stick to `tiny`/`base`/`small` (quantized).
- **Background recording** keeps running while the app is foregrounded or the
  screen is off only if the OS doesn't kill the process; a proper
  `FOREGROUND_SERVICE_MICROPHONE` service is future work.
- **UI is the desktop layout.** It renders in the Android WebView but is not
  yet optimized for small screens.
