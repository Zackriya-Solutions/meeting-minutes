use tauri::{AppHandle, Manager};

#[cfg(not(target_os = "macos"))]
use tauri::LogicalSize;

const MAIN_WINDOW_LABEL: &str = "main";

/// Animates the main window to a centered content size.
///
/// AppKit owns the animation on macOS, so the title bar, shadow and webview resize as one
/// native surface. Other desktop platforms keep a safe immediate resize fallback.
#[tauri::command]
pub async fn animate_main_window(
    app: AppHandle,
    width: f64,
    height: f64,
    duration_ms: u64,
) -> Result<u64, String> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("Window dimensions must be finite positive numbers".to_string());
    }

    let window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "Main window is not available".to_string())?;

    #[cfg(target_os = "macos")]
    {
        use core_graphics::geometry::{CGPoint, CGRect, CGSize};
        use objc::runtime::{Object, YES};
        use objc::{class, msg_send, sel, sel_impl};
        use tokio::sync::oneshot;

        let native_window = window
            .ns_window()
            .map_err(|error| format!("Could not access NSWindow: {error}"))?
            as usize;
        let duration_ms = duration_ms.clamp(1, 1_000);
        let (sender, receiver) = oneshot::channel();

        window
            .run_on_main_thread(move || {
                let result = unsafe {
                    let native_window = native_window as *mut Object;
                    if native_window.is_null() {
                        Err("NSWindow pointer is null".to_string())
                    } else {
                        let current_frame: CGRect = msg_send![native_window, frame];
                        let content_rect =
                            CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(width, height));
                        let mut target_frame: CGRect =
                            msg_send![native_window, frameRectForContentRect: content_rect];

                        // Preserve the current centre so all four edges move symmetrically.
                        // The onboarding window is centred when it opens, but this also avoids
                        // teleporting it if the user dragged it before continuing.
                        target_frame.origin.x = current_frame.origin.x
                            + (current_frame.size.width - target_frame.size.width) / 2.0;
                        target_frame.origin.y = current_frame.origin.y
                            + (current_frame.size.height - target_frame.size.height) / 2.0;

                        let animation_context = class!(NSAnimationContext);
                        let _: () = msg_send![animation_context, beginGrouping];
                        let current_context: *mut Object =
                            msg_send![animation_context, currentContext];
                        let duration_seconds = duration_ms as f64 / 1_000.0;
                        let _: () = msg_send![current_context, setDuration: duration_seconds];
                        let animator: *mut Object = msg_send![native_window, animator];
                        let _: () = msg_send![animator, setFrame: target_frame display: YES];
                        let _: () = msg_send![animation_context, endGrouping];

                        Ok(duration_ms)
                    }
                };

                let _ = sender.send(result);
            })
            .map_err(|error| format!("Could not schedule NSWindow animation: {error}"))?;

        return receiver
            .await
            .map_err(|_| "NSWindow animation task was cancelled".to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        window
            .set_size(LogicalSize::new(width, height))
            .map_err(|error| format!("Could not resize main window: {error}"))?;
        window
            .center()
            .map_err(|error| format!("Could not center main window: {error}"))?;
        Ok(0)
    }
}
