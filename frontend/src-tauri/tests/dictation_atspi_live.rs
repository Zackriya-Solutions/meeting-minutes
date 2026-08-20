// Live end-to-end test for the dictation AT-SPI injector (issue #719).
//
// This is a genuine, non-mocked test: it opens a real `gnome-text-editor`
// window on whatever desktop session is available, waits for AT-SPI to
// report its text buffer as focused, injects a marker string through the
// exact `atspi_injector::inject_segment` code path used in production, and
// then reads the text back from the *live* application over a fresh AT-SPI
// D-Bus round trip to confirm it actually landed.
//
// It requires a running AT-SPI accessibility bus and a launchable GUI editor
// (i.e. an actual desktop session, not a bare headless CI container). Both
// are auto-detected and the test is skipped (not failed) when unavailable,
// so this file is safe to leave in the normal `cargo test` run on machines
// without a desktop session, while still doing real work wherever one exists.

#![cfg(target_os = "linux")]

use app_lib::dictation::atspi_injector::{connect, inject_segment, spawn_focus_tracker, FocusCache, InjectError};
use atspi::proxy::accessible::ObjectRefExt;
use atspi::proxy::proxy_ext::ProxyExt;
use std::process::{Child, Command};
use std::time::Duration;

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Both tests in this file spawn a real GUI window and rely on it becoming
/// the AT-SPI-focused object. Rust's test harness runs `#[tokio::test]`
/// functions within one binary concurrently by default, which would let the
/// two spawned windows race for OS focus on the same desktop session; this
/// lock serializes them regardless of the harness's thread-pool settings.
static GUI_TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tokio::test]
async fn live_injects_into_a_real_gnome_text_editor_window() {
    let _serialize = GUI_TEST_LOCK.lock().await;

    let Ok(connection) = connect().await else {
        eprintln!("SKIP live_injects_into_a_real_gnome_text_editor_window: no AT-SPI accessibility bus available in this environment");
        return;
    };

    let cache = FocusCache::new();
    let _focus_task = spawn_focus_tracker(connection.clone(), cache.clone());
    // Let the registry event subscription land before anything could focus.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let Ok(child) = Command::new("gnome-text-editor")
        .arg("--standalone")
        .spawn()
    else {
        eprintln!(
            "SKIP live_injects_into_a_real_gnome_text_editor_window: gnome-text-editor is not launchable in this environment"
        );
        return;
    };
    let _child_guard = KillOnDrop(child);

    let marker = format!("dictation-live-test-{}", std::process::id());

    // Poll: wait for the editor's text field to become focused AND for a
    // real injection attempt to succeed. This exercises the identical retry
    // pattern the production injector task uses when it drains the queue.
    let mut last_err = None;
    let mut injected = false;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        match inject_segment(&connection, &cache, &marker).await {
            Ok(()) => {
                injected = true;
                break;
            }
            Err(e) => last_err = Some(e.to_string()),
        }
    }

    assert!(
        injected,
        "gnome-text-editor's text field never became AT-SPI-focused and injectable within 30s \
         (last error: {:?}); this indicates either no accessibility bridge is active in this \
         session, or the AT-SPI focus-tracking / injection code path is broken",
        last_err
    );

    // Read the text back from the live application via a completely fresh
    // AT-SPI query (not reusing any injector-side state) to prove the text
    // really landed in the other process's real widget.
    let item = cache
        .get()
        .await
        .expect("focus cache should still hold the just-injected object");
    let accessible = item
        .as_accessible_proxy(connection.connection())
        .await
        .expect("should resolve AccessibleProxy for the focused object");
    let proxies = accessible
        .proxies()
        .await
        .expect("should resolve interface proxies");
    let text_proxy = proxies.text().await.expect("object should implement Text");
    let full_text = text_proxy
        .get_text(0, -1)
        .await
        .expect("should be able to read back the buffer contents");

    assert!(
        full_text.contains(&marker),
        "injected marker text {:?} was not found in the live GNOME Text Editor buffer \
         (actual contents: {:?})",
        marker,
        full_text
    );

    eprintln!(
        "PASS: live AT-SPI injection into gnome-text-editor verified end-to-end (buffer now contains {:?})",
        full_text
    );
}

#[tokio::test]
async fn live_skips_a_real_password_field_and_never_types_into_it() {
    let _serialize = GUI_TEST_LOCK.lock().await;

    let Ok(connection) = connect().await else {
        eprintln!("SKIP live_skips_a_real_password_field_and_never_types_into_it: no AT-SPI accessibility bus available");
        return;
    };

    let cache = FocusCache::new();
    let _focus_task = spawn_focus_tracker(connection.clone(), cache.clone());
    tokio::time::sleep(Duration::from_millis(500)).await;

    // `zenity --password` opens a real GTK password entry dialog (AT-SPI
    // role `PasswordText`), auto-focused on open.
    let Ok(child) = Command::new("zenity")
        .args(["--password", "--title=dictation-live-password-test"])
        .spawn()
    else {
        eprintln!(
            "SKIP live_skips_a_real_password_field_and_never_types_into_it: zenity is not launchable in this environment"
        );
        return;
    };
    let _child_guard = KillOnDrop(child);

    // Poll until the password field is focused AND we get back the specific
    // `PasswordField` rejection -- not just "no focused field yet".
    let mut saw_password_rejection = false;
    let mut last_err = None;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        match inject_segment(&connection, &cache, "should-never-appear").await {
            Ok(()) => panic!(
                "inject_segment must never succeed against a real password field -- Fix 2 (the \
                 password-field skip) did not fire"
            ),
            Err(InjectError::PasswordField) => {
                saw_password_rejection = true;
                break;
            }
            Err(e) => last_err = Some(e.to_string()),
        }
    }

    assert!(
        saw_password_rejection,
        "never observed the zenity password entry being AT-SPI-focused and rejected as a \
         password field within 30s (last error: {:?})",
        last_err
    );

    // Confirm the field is genuinely empty -- the skip must be an actual
    // observed no-op, not merely an error being ignored downstream.
    let item = cache
        .get()
        .await
        .expect("focus cache should still hold the password field");
    let accessible = item
        .as_accessible_proxy(connection.connection())
        .await
        .expect("should resolve AccessibleProxy for the password field");
    let proxies = accessible
        .proxies()
        .await
        .expect("should resolve interface proxies");
    let text_proxy = proxies.text().await.expect("password entry should implement Text");
    let full_text = text_proxy
        .get_text(0, -1)
        .await
        .expect("should be able to read back the (empty) password buffer");

    assert!(
        full_text.is_empty(),
        "password field should remain untouched, but contains {:?}",
        full_text
    );

    eprintln!("PASS: live AT-SPI password-field skip verified end-to-end (field left empty: {:?})", full_text);
}
