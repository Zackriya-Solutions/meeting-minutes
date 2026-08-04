# Requirements: Recording Keyboard Shortcuts

## Goal

Add global keyboard shortcuts for toggling recording start/stop in Meetily. The default shortcut is `Control+F8`. Users must be able to configure this shortcut through the app's Settings UI. Shortcuts must be stored persistently. Platform-specific permissions for macOS are required (Accessibility); macOS is the primary target.

## Functional Requirements

### F1 — Global Shortcut Toggle
- Pressing `Control+F8` (default) globally toggles recording: if stopped, starts recording; if recording, stops it.
- The shortcut must work even when the app window is not focused (global shortcut).
- The shortcut must be registered at app startup using the stored user preference.

### F2 — Start/Stop as Separate Shortcuts (optional, single toggle is MVP)
- The primary design is a single toggle shortcut. A separate start-only and stop-only shortcut is out of scope for this feature.

### F3 — Shortcut Configuration UI
- A new "Shortcuts" section must appear in the Settings page under the "General" tab (or a new "Shortcuts" tab).
- The UI shows the current shortcut binding for "Toggle Recording".
- Users can click "Change" / a keybinding input to record a new shortcut.
- The new shortcut is saved on confirmation and re-registered immediately.
- The user can reset to the default (`Control+F8`).

### F4 — Persistence
- The configured shortcut is saved using `tauri-plugin-store` (existing pattern in the app).
- It is loaded and registered at app startup (in `lib.rs` or a new `shortcuts` module).

### F5 — Platform Permissions
- **macOS**: Global shortcuts via Tauri's `tauri-plugin-global-shortcut` require Accessibility permission. The app must request this permission (show a system dialog or deep-link to System Preferences). The entitlements file must include `com.apple.security.automation.apple-events` if needed. The app should detect whether the permission is granted and show a status indicator in the UI.
- **Windows**: No special system permission needed; `tauri-plugin-global-shortcut` works via WinAPI.
- **Linux**: Uses X11/Wayland hooks; no special permission flow required.

### F6 — Visual Feedback
- The Settings UI shows whether the global shortcut is currently active (permission granted).
- On macOS, if Accessibility is not granted, a banner/notice explains that the user needs to enable it in System Preferences.

## Non-Functional Requirements

- The shortcut registration/de-registration must not block the main thread.
- Invalid or conflicting shortcuts are caught gracefully (fallback to default or show error).
- The implementation uses `tauri-plugin-global-shortcut` (Tauri 2.x).

## Constraints

- This feature touches: `frontend/src-tauri/Cargo.toml`, `frontend/src-tauri/src/lib.rs`, `frontend/src-tauri/tauri.conf.json`, `frontend/src-tauri/entitlements.plist`, `frontend/src/app/settings/page.tsx`, `frontend/src/components/PreferenceSettings.tsx` (or new component), and `frontend/src/contexts/ConfigContext.tsx`.
- Uses existing `tauri-plugin-store` for persistence.
- Must not break existing tray recording toggle behavior.

## Out of Scope

- Separate start and stop shortcuts (single toggle only).
- Pause/Resume shortcuts.
- Custom shortcuts for other app functions.

## Resolved Questions

_None — requirements are complete._
