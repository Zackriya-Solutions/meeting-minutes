# Plan: Recording Keyboard Shortcuts

**Branch**: enhance/recording-keyboard-shortcuts
**Requirements**: docs/superpowers/runs/20260804-1341-main/requirements.md
**Routing**: writing-plans (score 18)

---

## Task T1 — Add tauri-plugin-global-shortcut to Cargo.toml and package.json

**File(s)**: `frontend/src-tauri/Cargo.toml`, `frontend/package.json`
**Test-first**: yes — failing test: `cargo check` fails with `error[E0433]: failed to resolve: use of undeclared crate or module tauri_plugin_global_shortcut`; after: `cargo check` exits 0

Add the Rust crate and the JS npm package for the global-shortcut Tauri plugin (Tauri 2.x version, `tauri-plugin-global-shortcut = "2"`).

---

## Task T2 — Add global-shortcut permission to tauri.conf.json

**File(s)**: `frontend/src-tauri/tauri.conf.json`
**Test-first**: yes — failing test: `cargo check` with the plugin registered but no permission emits a Tauri build warning/error about missing capability; after the permission is added, `cargo check` exits 0 and the capability JSON is valid

Add `"global-shortcut:default"` to the `permissions` array under the `"main"` capability in `tauri.conf.json`.

---

## Task T3 — Add macOS Accessibility entitlement to entitlements.plist

**File(s)**: `frontend/src-tauri/entitlements.plist`
**Test-first**: yes — failing test: `pnpm exec tsc --noEmit` passes (no TS changes yet) but the entitlements file lacks the Accessibility key; after: the plist contains `com.apple.security.temporary-exception.mach-lookup.global-name` or the simpler sandboxed accessibility key

Add `com.apple.security.automation.apple-events` (needed by macOS sandboxed apps requesting Accessibility in Hardened Runtime).

---

## Task T4 — Create Rust shortcuts module

**File(s)**: `frontend/src-tauri/src/shortcuts.rs` (new), `frontend/src-tauri/src/lib.rs`
**Test-first**: yes — failing test: `cargo test shortcuts::` emits `error[E0433]: failed to resolve: use of undeclared module shortcuts`; after: module compiles, unit test for shortcut string normalization passes

Create `src/shortcuts.rs` with:
- `DEFAULT_SHORTCUT: &str = "Control+F8"`
- `SHORTCUT_STORE_KEY: &str = "recording_shortcut"`
- `fn normalize_shortcut(s: &str) -> String` — trims, capitalizes modifiers
- `async fn load_shortcut<R: Runtime>(app: &AppHandle<R>) -> String` — reads from plugin-store, falls back to DEFAULT_SHORTCUT
- `async fn save_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String>`
- `async fn register_shortcut<R: Runtime>(app: &AppHandle<R>, shortcut: &str) -> Result<(), String>` — uses `tauri_plugin_global_shortcut::GlobalShortcutExt`, calls `toggle_recording_handler` on trigger
- `async fn unregister_all<R: Runtime>(app: &AppHandle<R>)`
- `pub async fn init<R: Runtime>(app: &AppHandle<R>)` — loads shortcut from store and registers it
- Tauri commands:
  - `get_recording_shortcut(app) -> Result<String, String>`
  - `set_recording_shortcut(app, shortcut: String) -> Result<(), String>` — unregisters old, validates, saves, registers new
  - `check_shortcut_permission() -> Result<bool, String>` — on macOS checks AXIsProcessTrusted; on other platforms always returns true

Declare `pub mod shortcuts;` in `lib.rs`, register the plugin `tauri_plugin_global_shortcut::init()`, call `shortcuts::init` in `.setup()`, and add all shortcut commands to `invoke_handler`.

Unit test: `normalize_shortcut("ctrl+f8") == "Control+F8"` (or stays unchanged if already normalized).

---

## Task T5 — Write JS test for shortcut settings validation logic

**File(s)**: `frontend/tests/lib/shortcut-settings.test.ts` (new)
**Test-first**: yes — failing test: `bun test tests/lib/shortcut-settings.test.ts` exits non-zero because the module `@/lib/shortcutUtils` does not exist yet

Create `frontend/src/lib/shortcutUtils.ts` with:
- `validateShortcut(s: string): boolean` — requires at least one modifier key and one non-modifier key
- `formatShortcut(s: string): string` — canonical display format

Create `frontend/tests/lib/shortcut-settings.test.ts` with tests for `validateShortcut` and `formatShortcut`.

---

## Task T6 — Create ShortcutSettings React component

**File(s)**: `frontend/src/components/ShortcutSettings.tsx` (new)
**Test-first**: no — UI component, no logic unit tests; verified by type check

Create a React component that:
1. Loads the current shortcut via `invoke('get_recording_shortcut')` on mount.
2. Checks permission status via `invoke('check_shortcut_permission')`.
3. On macOS (detected via `@tauri-apps/plugin-os`), shows a yellow banner if permission is not granted with a "Open Accessibility Settings" button that opens `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility` via shell or Tauri opener.
4. Shows the current shortcut in a styled chip/badge.
5. Has a "Change Shortcut" button that enters a capture mode — listens to `keydown` events to record the new combination, shows it live, and has Confirm/Cancel.
6. Has a "Reset to Default" button.
7. On save, calls `invoke('set_recording_shortcut', { shortcut })`.
8. Shows success/error toasts (using `sonner`).

---

## Task T7 — Integrate ShortcutSettings into Settings page

**File(s)**: `frontend/src/app/settings/page.tsx`
**Test-first**: yes — failing test: `pnpm exec tsc --noEmit` fails because the import `ShortcutSettings` is not found; after: type check passes

Import `ShortcutSettings` and add a new tab `{ value: 'shortcuts', label: 'Shortcuts', icon: Keyboard }` to the `TABS` array. Add `<TabsContent value="shortcuts"><ShortcutSettings /></TabsContent>` to the page.

---

## Summary of Files Changed

| File | Change |
|---|---|
| `frontend/src-tauri/Cargo.toml` | +1 crate dependency |
| `frontend/package.json` | +1 npm package |
| `frontend/src-tauri/tauri.conf.json` | +1 permission string |
| `frontend/src-tauri/entitlements.plist` | +1 entitlement key |
| `frontend/src-tauri/src/shortcuts.rs` | new module ~150 lines |
| `frontend/src-tauri/src/lib.rs` | 3 small additions |
| `frontend/src/lib/shortcutUtils.ts` | new utility ~40 lines |
| `frontend/tests/lib/shortcut-settings.test.ts` | new test file |
| `frontend/src/components/ShortcutSettings.tsx` | new component ~180 lines |
| `frontend/src/app/settings/page.tsx` | add tab + import |
