//! The macOS application menu bar.
//!
//! Without this the app ships Tauri's stock menu, which has no "Check for Updates" — so the
//! only ways to trigger a check were the tray and Settings → About, and neither is where a
//! Mac user looks first. Menu item ids are deliberately the same strings the tray uses
//! (`check_updates`, `settings`), so both menus dispatch through
//! [`crate::tray::handle_menu_event`] and can't drift apart.
//!
//! macOS only: on Windows and Linux a `Menu` set on the app becomes a menu bar *inside* the
//! window, which is not this app's design.

use tauri::{
    menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Runtime,
};

/// Build and install the menu bar. Call once, from `setup`.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let about = PredefinedMenuItem::about(
        app,
        Some("About Memento"),
        Some(AboutMetadata {
            name: Some("Memento".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            ..Default::default()
        }),
    )?;
    let check_updates =
        MenuItemBuilder::with_id("check_updates", "Check for Updates…").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "Memento")
        .item(&about)
        .separator()
        .item(&check_updates)
        .separator()
        .item(&settings)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    // Replacing the default menu also removes the standard editing and window commands
    // along with their shortcuts (⌘C, ⌘V, ⌘M …), so they have to be re-declared here.
    // Text fields in the app would otherwise lose copy and paste entirely.
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .fullscreen()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &edit_menu, &window_menu])
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}
