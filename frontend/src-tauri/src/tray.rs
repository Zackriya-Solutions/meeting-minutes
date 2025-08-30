use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};


pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app, crate::is_recording())?;

    TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Meetily")
        .icon(app.default_window_icon().unwrap().clone())
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                focus_main_window(&tray.app_handle());
            }
        })
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
            log::info!("Stopping recording from tray");
            let args = crate::RecordingArgs {
                save_path: String::new(),
            };

            if let Some(window) = app_clone.get_webview_window("main") {
                let _ = window.eval("(window.handleRecordingStop && window.handleRecordingStop(true))");
            }

            if let Err(e) = crate::stop_recording(app_clone.clone(),args).await {
                log::error!("Failed to stop recording: {}", e);
            }
        } else {
            log::info!("Starting recording from tray");
            if let Err(e) = crate::start_recording(app_clone.clone()).await {
                log::error!("Failed to start recording: {}", e);
            }
        }
    });
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, _is_recording: bool) -> tauri::Result<tauri::menu::Menu<R>> {
    MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("toggle_recording", "Toggle Recording").build(app)?)
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
