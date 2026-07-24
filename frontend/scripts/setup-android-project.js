#!/usr/bin/env node
/**
 * Post-`tauri android init` setup for the Meetily Android project.
 *
 * Applies the Android-specific pieces that Tauri's generated project does not
 * include (see docs/ANDROID.md):
 *   1. AndroidManifest.xml — microphone/notification/foreground-service permissions
 *                            and the <service> declaration for RecordingService
 *   2. MainActivity.kt     — runtime permission request (RECORD_AUDIO, POST_NOTIFICATIONS)
 *   3. RecordingService.kt — mic foreground service (persistent notification) so
 *                            recording survives the app being backgrounded; started
 *                            /stopped from Rust via JNI (see src/android_jni.rs)
 *   4. jniLibs             — libonnxruntime.so from the official onnxruntime-android
 *                            AAR (required by Silero VAD; `ort` dlopens it at runtime)
 *
 * Safe to re-run (idempotent). Requires: node >= 18 (fetch), a JDK on PATH
 * (`jar` is used to extract the AAR — already required for Android builds).
 *
 * Usage: node scripts/setup-android-project.js   (from the frontend/ directory)
 */

const fs = require('fs');
const path = require('path');
const os = require('os');
const { execFileSync } = require('child_process');

// Must match the ONNXRUNTIME_VERSION expected by the `ort-sys` crate
const ONNXRUNTIME_VERSION = '1.22.0';
const ONNXRUNTIME_AAR_URL =
  `https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/${ONNXRUNTIME_VERSION}/onnxruntime-android-${ONNXRUNTIME_VERSION}.aar`;

const ANDROID_ABIS = ['arm64-v8a'];

const PERMISSIONS = [
  'android.permission.RECORD_AUDIO',
  'android.permission.MODIFY_AUDIO_SETTINGS',
  'android.permission.POST_NOTIFICATIONS',
  'android.permission.WAKE_LOCK',
  'android.permission.FOREGROUND_SERVICE',
  'android.permission.FOREGROUND_SERVICE_MICROPHONE',
];

const genAndroid = path.join(__dirname, '..', 'src-tauri', 'gen', 'android');
const appMain = path.join(genAndroid, 'app', 'src', 'main');

function fail(msg) {
  console.error(`❌ ${msg}`);
  process.exit(1);
}

function patchManifest() {
  const manifestPath = path.join(appMain, 'AndroidManifest.xml');
  if (!fs.existsSync(manifestPath)) {
    fail(`AndroidManifest.xml not found at ${manifestPath} — run \`pnpm tauri android init\` first.`);
  }
  let manifest = fs.readFileSync(manifestPath, 'utf8');
  const missing = PERMISSIONS.filter((p) => !manifest.includes(`"${p}"`));
  if (missing.length === 0) {
    console.log('✅ AndroidManifest.xml already has all permissions');
    return;
  }
  const block = missing
    .map((p) => `    <uses-permission android:name="${p}" />`)
    .join('\n');
  if (!/<application\b/.test(manifest)) {
    fail('Could not find <application> element in AndroidManifest.xml');
  }
  manifest = manifest.replace(/(\n\s*<application\b)/, `\n${block}\n$1`);
  fs.writeFileSync(manifestPath, manifest);
  console.log(`✅ AndroidManifest.xml: added ${missing.length} permission(s)`);
}

// Locate MainActivity.kt (its directory + package depend on the app identifier)
// and return { path, dir, packageName } so other patches can co-locate files.
function findMainActivity() {
  const javaRoot = path.join(appMain, 'java');
  let mainActivityPath = null;
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(p);
      else if (entry.name === 'MainActivity.kt') mainActivityPath = p;
    }
  };
  walk(javaRoot);
  if (!mainActivityPath) fail(`MainActivity.kt not found under ${javaRoot}`);
  const src = fs.readFileSync(mainActivityPath, 'utf8');
  const pkg = src.match(/^package (.+)$/m);
  if (!pkg) fail('Could not determine package of MainActivity.kt');
  return {
    path: mainActivityPath,
    dir: path.dirname(mainActivityPath),
    packageName: pkg[1].trim(),
  };
}

function patchMainActivity() {
  const { path: mainActivityPath } = findMainActivity();
  const current = fs.readFileSync(mainActivityPath, 'utf8');
  if (current.includes('requestPermissions')) {
    console.log('✅ MainActivity.kt already requests runtime permissions');
    return;
  }
  const packageLine = current.match(/^package .+$/m);
  if (!packageLine) fail('Could not determine package of MainActivity.kt');

  // TauriActivity is generated into the same package, so no import is needed.
  // Plain Activity APIs (checkSelfPermission/requestPermissions) are available
  // from API 23; Tauri's Android minSdk is 24.
  const contents = `${packageLine[0]}

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)

    val wanted = mutableListOf(Manifest.permission.RECORD_AUDIO)
    if (Build.VERSION.SDK_INT >= 33) {
      wanted.add(Manifest.permission.POST_NOTIFICATIONS)
    }
    val missing = wanted.filter {
      checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED
    }
    if (missing.isNotEmpty()) {
      requestPermissions(missing.toTypedArray(), 1000)
    }
  }
}
`;
  fs.writeFileSync(mainActivityPath, contents);
  console.log(`✅ MainActivity.kt: added runtime permission request (${mainActivityPath})`);
}

async function installOnnxRuntime() {
  const jniLibs = path.join(appMain, 'jniLibs');
  const allPresent = ANDROID_ABIS.every((abi) =>
    fs.existsSync(path.join(jniLibs, abi, 'libonnxruntime.so'))
  );
  if (allPresent) {
    console.log('✅ libonnxruntime.so already present in jniLibs');
    return;
  }

  console.log(`⬇️  Downloading onnxruntime-android ${ONNXRUNTIME_VERSION} AAR...`);
  const res = await fetch(ONNXRUNTIME_AAR_URL);
  if (!res.ok) fail(`Failed to download ${ONNXRUNTIME_AAR_URL}: HTTP ${res.status}`);
  const aar = Buffer.from(await res.arrayBuffer());

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'ort-aar-'));
  const aarPath = path.join(tmp, 'onnxruntime-android.aar');
  fs.writeFileSync(aarPath, aar);

  // AARs are zip files; `jar` ships with the JDK required for Android builds
  execFileSync('jar', ['xf', aarPath], { cwd: tmp });

  for (const abi of ANDROID_ABIS) {
    const src = path.join(tmp, 'jni', abi, 'libonnxruntime.so');
    if (!fs.existsSync(src)) fail(`AAR does not contain jni/${abi}/libonnxruntime.so`);
    const destDir = path.join(jniLibs, abi);
    fs.mkdirSync(destDir, { recursive: true });
    fs.copyFileSync(src, path.join(destDir, 'libonnxruntime.so'));
    console.log(`✅ Installed jniLibs/${abi}/libonnxruntime.so`);
  }
  fs.rmSync(tmp, { recursive: true, force: true });
}

// Register the RecordingService inside <application>. foregroundServiceType
// "microphone" is required (API 29+) to run a mic-capturing FGS; the matching
// FOREGROUND_SERVICE_MICROPHONE permission is added by patchManifest().
function patchServiceManifest() {
  const manifestPath = path.join(appMain, 'AndroidManifest.xml');
  let manifest = fs.readFileSync(manifestPath, 'utf8');
  if (manifest.includes('.RecordingService')) {
    console.log('✅ AndroidManifest.xml already declares RecordingService');
    return;
  }
  const service =
    '        <service\n' +
    '            android:name=".RecordingService"\n' +
    '            android:exported="false"\n' +
    '            android:foregroundServiceType="microphone" />\n';
  if (!/<\/application>/.test(manifest)) {
    fail('Could not find </application> in AndroidManifest.xml');
  }
  manifest = manifest.replace(/(\s*)<\/application>/, `\n${service}$1</application>`);
  fs.writeFileSync(manifestPath, manifest);
  console.log('✅ AndroidManifest.xml: declared RecordingService');
}

// Write RecordingService.kt next to MainActivity (same package). This is a
// "hollow" foreground service: the mic is captured by Rust/cpal — the service
// exists only to keep the process alive and satisfy Android's requirement that
// background microphone use run under a visible foreground-service notification.
function writeRecordingService(mainActivity) {
  const servicePath = path.join(mainActivity.dir, 'RecordingService.kt');
  const contents = `package ${mainActivity.packageName}

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

/**
 * Microphone foreground service. Started/stopped from Rust over JNI when
 * recording begins/ends (see src/android_jni.rs). Shows a persistent, low-
 * importance notification tapping through to the app.
 */
class RecordingService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val nm = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val channel = NotificationChannel(
        CHANNEL_ID,
        "Recording",
        NotificationManager.IMPORTANCE_LOW
      )
      channel.setShowBadge(false)
      nm?.createNotificationChannel(channel)
    }

    val launch = packageManager.getLaunchIntentForPackage(packageName)
    val pi = launch?.let {
      var piFlags = PendingIntent.FLAG_UPDATE_CURRENT
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
        piFlags = piFlags or PendingIntent.FLAG_IMMUTABLE
      }
      PendingIntent.getActivity(this, 0, it, piFlags)
    }

    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      Notification.Builder(this, CHANNEL_ID)
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(this)
    }
    val notification = builder
      .setContentTitle("VoiceMe is recording")
      .setContentText("Transcribing in the background")
      .setSmallIcon(applicationInfo.icon)
      .setOngoing(true)
      .apply { if (pi != null) setContentIntent(pi) }
      .build()

    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      startForeground(NOTIF_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
    } else {
      startForeground(NOTIF_ID, notification)
    }
    return START_STICKY
  }

  companion object {
    private const val CHANNEL_ID = "meetily_recording"
    private const val NOTIF_ID = 4242
  }
}
`;
  fs.writeFileSync(servicePath, contents);
  console.log(`✅ RecordingService.kt written (${servicePath})`);
}

(async () => {
  if (!fs.existsSync(genAndroid)) {
    fail(`Android project not found at ${genAndroid} — run \`pnpm tauri android init\` first.`);
  }
  patchManifest();
  patchMainActivity();
  patchServiceManifest();
  writeRecordingService(findMainActivity());
  await installOnnxRuntime();
  console.log('🎉 Android project setup complete');
})();
