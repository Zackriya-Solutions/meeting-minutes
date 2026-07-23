//! JNI bridge for Android platform features Tauri doesn't expose:
//!   * a **microphone foreground service** so recording keeps running (and the
//!     process keeps a high scheduling priority) after the app is backgrounded,
//!     with the persistent notification Android requires for mic capture, and
//!   * the **"ignore battery optimizations"** system prompt, so long recordings
//!     aren't killed by Doze / App Standby.
//!
//! `ndk_context` hands us the `JavaVM` and Application `Context` that Tauri
//! already initialized on startup. We construct framework `Intent`s and call
//! `Context` methods directly; the only app class we reference (the service) is
//! addressed by string name via `Intent.setClassName`, so we never have to load
//! an app class through JNI's bootstrap classloader (which fails off the JVM's
//! own threads).
#![cfg(target_os = "android")]

use anyhow::{anyhow, Result};
use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

/// Attach the current thread to the JVM and run `f` with a live `JNIEnv` and the
/// Android Application `Context`. The attach guard is held for the closure's
/// duration and detaches on drop.
fn with_context<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&mut JNIEnv, &JObject) -> Result<T>,
{
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }?;
    let mut guard = vm.attach_current_thread()?;
    let env: &mut JNIEnv = &mut guard;
    // Safety: ndk_context guarantees a valid, non-null Application Context for
    // the lifetime of the process.
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    f(env, &context)
}

/// `context.getPackageName()` → owned Rust String.
fn package_name(env: &mut JNIEnv, context: &JObject) -> Result<String> {
    let name = env
        .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])?
        .l()?;
    let jstr = JString::from(name);
    let s: String = env.get_string(&jstr)?.into();
    Ok(s)
}

/// Build an `Intent` addressed at our own `RecordingService` by class name.
fn service_intent<'a>(env: &mut JNIEnv<'a>, context: &JObject) -> Result<JObject<'a>> {
    let pkg = package_name(env, context)?;
    let class_name = format!("{}.RecordingService", pkg);
    let intent = env.new_object("android/content/Intent", "()V", &[])?;
    let jclass = env.new_string(class_name)?;
    env.call_method(
        &intent,
        "setClassName",
        "(Landroid/content/Context;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(context), JValue::Object(&jclass)],
    )?;
    Ok(intent)
}

/// Start the microphone foreground service (persistent notification). Idempotent
/// on the Android side — a second start just refreshes the notification.
pub fn start_recording_service() -> Result<()> {
    let res = with_context(|env, context| {
        let intent = service_intent(env, context)?;
        // startForegroundService requires API 26; Tauri's minSdk is 24 but every
        // device we ship to is far newer. The service must call startForeground
        // within ~5s (it does, in onStartCommand).
        env.call_method(
            context,
            "startForegroundService",
            "(Landroid/content/Intent;)Landroid/content/ComponentName;",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    });
    if let Err(ref e) = res {
        log::warn!("Failed to start recording foreground service: {e}");
    }
    res
}

/// Stop the microphone foreground service (dismisses the notification).
pub fn stop_recording_service() -> Result<()> {
    let res = with_context(|env, context| {
        let intent = service_intent(env, context)?;
        env.call_method(
            context,
            "stopService",
            "(Landroid/content/Intent;)Z",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    });
    if let Err(ref e) = res {
        log::warn!("Failed to stop recording foreground service: {e}");
    }
    res
}

/// True if the OS is already exempting us from battery optimizations, so the UI
/// can avoid nagging. Returns Ok(true) on any lookup failure (don't nag on doubt).
pub fn is_ignoring_battery_optimizations() -> Result<bool> {
    with_context(|env, context| {
        let pkg = env
            .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])?
            .l()?;
        let svc = env.new_string("power")?;
        let pm = env
            .call_method(
                context,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&svc)],
            )?
            .l()?;
        if pm.is_null() {
            return Ok(true);
        }
        let ignoring = env
            .call_method(
                &pm,
                "isIgnoringBatteryOptimizations",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&pkg)],
            )?
            .z()?;
        Ok(ignoring)
    })
}

/// Launch the system "ignore battery optimizations" request dialog for our app.
pub fn request_ignore_battery_optimizations() -> Result<()> {
    with_context(|env, context| {
        let pkg = package_name(env, context)?;
        let action = env.new_string("android.settings.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS")?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )?;
        let uri_str = env.new_string(format!("package:{}", pkg))?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&uri_str)],
            )?
            .l()?;
        env.call_method(
            &intent,
            "setData",
            "(Landroid/net/Uri;)Landroid/content/Intent;",
            &[JValue::Object(&uri)],
        )?;
        // FLAG_ACTIVITY_NEW_TASK (0x10000000) — required to start an Activity
        // from a non-Activity Context.
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(0x1000_0000)],
        )?;
        env.call_method(
            context,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
    .map_err(|e| anyhow!("battery-optimization prompt failed: {e}"))
}
