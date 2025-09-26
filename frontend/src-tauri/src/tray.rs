use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};

pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app, false)?; // Default to not recording

    TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("Meetily")
        .icon(app.default_window_icon().unwrap().clone())
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .build(app)?;
    Ok(())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, item_id: &str) {
    match item_id {
        "toggle_recording" => toggle_recording_handler(app),
        "open_window" => focus_main_window(app),
        "settings" => {
            focus_main_window(app);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.eval("(window.openSettings && window.openSettings())");
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}
fn toggle_recording_handler<R: Runtime>(app: &AppHandle<R>) {
    focus_main_window(app);
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        if crate::is_recording() {
            log::info!("Emitting stop recording event from tray");

            let _ = app_clone
                .get_webview_window("main")
                .unwrap()
                .eval("window.handleRecordingStop()");
            if let Err(e) = app_clone.emit("tray:stop-recording", ()) {
                log::error!("Failed to emit stop recording event: {}", e);
            }
        } else {
            log::info!("Emitting start recording event from tray");
            if let Some(window) = app_clone.get_webview_window("main") {
                let _ = window.eval("sessionStorage.setItem('autoStartRecording', 'true')"); // Set the flag to start recording automatically
                let _ = window.eval("window.location.assign('/')");
            }
        }
    });
}

pub fn update_tray_menu<R: Runtime>(app: &AppHandle<R>) {
    // For now, use a default state since we can't await in sync function
    if let Ok(menu) = build_menu(app, false) {
        if let Some(tray) = app.tray_by_id("main-tray") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    is_recording: bool,
) -> tauri::Result<tauri::menu::Menu<R>> {
    let label = if is_recording {
        "Stop Recording"
    } else {
        "Start Recording"
    };
    MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("toggle_recording", label).build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("open_window", "Open Main Window").build(app)?)
        .item(&MenuItemBuilder::with_id("settings", "Settings").build(app)?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&MenuItemBuilder::with_id("quit", "Quit").build(app)?)
        .build()
}

fn focus_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.eval("window.focus()");
    } else {
        log::warn!("Could not find main window");
    }
}
