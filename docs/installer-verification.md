# Installer verification protocol (Deep Focus look)

Status: active. Last edited 2026-09-02. Owner: process effectiveness (release gates).

This document defines how we prove that the PulseTalq installers (Windows NSIS, Windows MSI, macOS DMG, Linux deb and AppImage) carry the Deep Focus brand end to end, and how they behave on machines that still have the legacy Meetily build installed.

Related files:

- `DESIGN.md` sections 1 and 2: the visual theme and the palette the installer art must follow (Blackout `#0b0b0c`, Readout `#f7f6f2`, Hot Signal `#ff3b1f`). Purple or violet anywhere is a defect; it is the legacy Meetily mark.
- `docs/branding-migration-register.md`: Cluster C (visual assets) and Cluster D (release and distribution identifiers) carry the per-file status.
- `docs/installer-ux.md`: the intended installer look and copy (owned by the installer UX task).
- `scripts/brand/generate-installer-assets.py`: the single generator for icons, NSIS/WiX bitmaps and the DMG background. Never hand-edit a generated asset; change the generator and rerun it.
- `scripts/verify-installer-assets.py`: pre-build gate (Python 3 + Pillow).
- `scripts/verify-installer-branding.ps1`: post-build gate for Windows artifacts and installed state.

## 1. Definition of done

The installer new-look migration is done when all of the following hold on a clean build from the release branch:

1. `python scripts/verify-installer-assets.py` exits 0 (warnings about unused Next.js scaffold SVGs are acceptable until those files are deleted).
2. `pwsh scripts/verify-installer-branding.ps1` exits 0 against the NSIS `.exe` and the `.msi` produced by `pnpm run tauri:build` on Windows.
3. After installing the NSIS build on a Windows test machine, `pwsh scripts/verify-installer-branding.ps1 -Path <setup.exe> -CheckInstalled` exits 0.
4. A human has looked at the screenshots in `docs/installer-preview*` (or at the live installer) and confirmed: no purple, the wordmark reads "pulsetalq" with "talq" in Hot Signal, the header and sidebar bitmaps are not stretched, and no "Meetily" or "Zackriya" text appears in any dialog, license page, shortcut, tray tooltip, About dialog, or uninstall entry.
5. macOS DMG and Linux packages have been through the manual checks in sections 4 and 5, with results recorded in the release PR.
6. The behaviour for machines with an existing Meetily install (section 6) is documented in the release notes, and nothing in the installer uninstalls or deletes Meetily.

If any of these fail, the release is not brand-complete. Do not paper over a failing gate by editing the expected values in the scripts.

## 2. Pre-build gate (all platforms)

Run from the repository root before any `tauri build`:

```bash
python scripts/verify-installer-assets.py
```

What it checks and why:

| Check | Failure means |
|---|---|
| Every path in `bundle.icon` exists | Tauri would fail late in the build, or fall back to a default icon |
| `icons/icon.png` is square and at least 512 px | Small or non-square sources produce blurry or cropped icons at every size |
| `icons/icon.ico` and `icons/icon.icns` exist and are non-trivial | NSIS installer/uninstaller icons and the macOS bundle icon reference these directly |
| NSIS header 150x57, sidebar 164x314; WiX banner 493x58, dialog 493x312; all BMP RGB | NSIS and WiX stretch or reject other sizes; alpha channels render black |
| `bundle.macOS.dmg.background` exists | Missing background silently reverts to a plain DMG window |
| Centre 60 percent of `icon.png` is not purple (hue 240 to 300 deg, saturation above 0.3) | The old Meetily mark is violet; the Deep Focus mark is near-red (measured mean hue 7.5 deg) |
| No file under `frontend/src-tauri/icons` or `frontend/public` matches a recorded legacy SHA-256 | A Meetily asset survived the regeneration |

The legacy hashes were captured on 2026-09-02 from the working tree immediately before regeneration and live in the script as `LEGACY_MEETILY_HASHES`. Validation of the colour heuristic: the committed pre-migration `icon.png` measures hue 266.3 deg, saturation 0.63 (flagged); the Deep Focus icon measures hue 7.5 deg, saturation 0.88 (passes).

## 3. Windows NSIS and MSI

### 3.1 Build and inspect artifacts (no install required)

```powershell
# From a "x64 Native Tools Command Prompt for VS 2022" shell, or after
# calling vcvars64.bat, so that cl.exe and the MSVC INCLUDE paths are set.
$env:WHISPER_DONT_GENERATE_BINDINGS = "1"   # use whisper-rs-sys bundled bindings
pnpm run tauri:build          # from frontend/
pwsh scripts/verify-installer-branding.ps1
```

Build prerequisite note (2026-09-02): the repo build scripts assume a full LLVM
install at `C:\Program Files\LLVM\bin`, which is absent on the current build
machine. Root cause of the `whisper-rs` failure (dozens of `no field ... on type
whisper_full_params` errors): `whisper-rs-sys` 0.11 pins bindgen 0.69, which
silently produces opaque structs against libclang 22. bindgen 0.72 handles
libclang 22 correctly, so this is a version mismatch. Use libclang 18 or 19 for
the release build (for example `LLVM-18.1.8-win64.exe` unpacked with 7-Zip into
a user folder; only `bin\libclang.dll` and `lib\clang\18\include` are needed)
and point `LIBCLANG_PATH` at its `bin`. `WHISPER_DONT_GENERATE_BINDINGS=1` does
not work on Windows: the bundled bindings are macOS layouts and fail their
layout tests.
Artifacts land in `target/release/bundle/{nsis,msi}` at the repo root because
the Cargo workspace root is the repo root.

The script searches `target/**/bundle/{nsis,msi}` at the repo root (the Cargo workspace root) or takes explicit `-Path` values. It:

- Reads PE version resources (`ProductName`, `CompanyName`, `FileDescription`, `LegalCopyright`, `OriginalFilename`, `InternalName`) from the NSIS `.exe`, fails on any Meetily or Zackriya string, and requires `ProductName` to equal `PulseTalq`.
- Opens the `.msi` read-only through `WindowsInstaller.Installer` and reads the Property table: `ProductName`, `Manufacturer`, `ProductCode`, `UpgradeCode`, `ProductVersion`, plus the ARP link properties. `Manufacturer` must be `PolyphronAI` (this comes from `bundle.publisher` in `tauri.conf.json`; without it Tauri derives the lowercase `pulsetalq` from the identifier, which is what the 2026-09-01 build shipped).
- Records the `UpgradeCode`. Tauri derives it from the bundle identifier, so `com.pulsetalq.app` must yield a GUID different from the Meetily `com.meetily.ai` one. Once an old Meetily MSI is available, paste its UpgradeCode into `$legacyUpgradeCodes` in the script so a regression fails loudly.

Manual checks (record a screenshot for each in the release PR):

1. Explorer > file Properties > Details on the `.exe` and `.msi`: product name, company, copyright.
2. Run the NSIS installer up to but not including the Install step: welcome page header image, sidebar image, license page text, install-directory default (`%LOCALAPPDATA%\PulseTalq` because `installMode` is `currentUser`), installer window title and taskbar icon.
3. Run `msiexec /i PulseTalq_<version>_x64_en-US.msi` up to but not including the Install step: banner and dialog bitmaps, dialog title.
4. Cancel both installers.

### 3.2 After a real install

Install the NSIS build, then:

```powershell
pwsh scripts/verify-installer-branding.ps1 -Path <setup.exe> -CheckInstalled
```

It verifies the Uninstall entry (HKCU for NSIS `currentUser`, HKLM for MSI) has `DisplayName` `PulseTalq`, that `Publisher`, `DisplayIcon`, `InstallLocation` and `UninstallString` carry no legacy string, that the `DisplayIcon` path exists, and that a Start menu shortcut exists at `%APPDATA%\Microsoft\Windows\Start Menu\Programs\PulseTalq.lnk` (or the ProgramData equivalent) and points at an existing executable.

Manual checks: Settings > Apps > Installed apps shows one `PulseTalq` entry with the Deep Focus icon; the Start menu tile, taskbar icon, window title bar, tray tooltip and About dialog all read PulseTalq; pinning to taskbar keeps the new icon after sign-out and sign-in (icon cache).

## 4. macOS DMG

No automated gate exists yet (the build runs on macOS only). Manual protocol:

1. `pnpm run tauri:build` on macOS, then open the `.dmg`.
2. The DMG window shows `installer/dmg-background.png` (660x400) with the app at (180,170) and the Applications alias at (480,170), matching `bundle.macOS.dmg` in `tauri.conf.json`.
3. `codesign -dv --verbose=2 PulseTalq.app` reports `Identifier=com.pulsetalq.app`.
4. `plutil -p PulseTalq.app/Contents/Info.plist | grep -E 'CFBundleName|CFBundleDisplayName|CFBundleIdentifier|NSHumanReadableCopyright'` shows PulseTalq values and no Meetily or Zackriya text.
5. Finder icon, Dock icon and the About window show the Deep Focus mark. Check `PulseTalq.app/Contents/Resources/*.icns` against `icons/app_icon.icns` with `shasum -a 256`.
6. Existing Meetily.app in /Applications is untouched (different bundle identifier, different app name).

## 5. Linux deb and AppImage

Manual protocol on a Debian-family VM:

1. `dpkg-deb -I PulseTalq_<version>_amd64.deb` shows `Package: pulse-talq`, `Maintainer` and `Description` without Meetily or Zackriya, `Section: utils`.
2. `dpkg-deb -c ... | grep -E 'icons|applications'` lists `usr/share/applications/PulseTalq.desktop` and the hicolor icon set; `sha256sum` the largest icon against `icons/icon.png` after extraction.
3. `.desktop` file: `Name=PulseTalq`, `Icon=pulse-talq`, `Categories` includes Office or Utility, no Meetily strings.
4. AppImage: `./PulseTalq_<version>_amd64.AppImage --appimage-extract` then inspect `.DirIcon`, `*.desktop`, and `usr/share/icons`.
5. Old `meetily` deb (if installed) is a separate package and remains installed; `apt list --installed | grep -i -E 'meetily|pulse'` shows both.

## 6. How existing Meetily installs behave

Verified values (2026-09-02):

| Surface | Legacy Meetily | PulseTalq now | Source |
|---|---|---|---|
| Bundle identifier | `com.meetily.ai` | `com.pulsetalq.app` | `frontend/src-tauri/tauri.conf.json` (`git show 8aab2aa^` for the old value) |
| Product name | `meetily` | `PulseTalq` | same |
| Crate / executable | `meetily` | `pulse-talq` (`pulse-talq.exe`) | `frontend/src-tauri/Cargo.toml` |
| Tauri app data dir (SQLite `meeting_minutes.sqlite`, `models/`) | `%APPDATA%\com.meetily.ai` | `%APPDATA%\com.pulsetalq.app` | `database/manager.rs`, `parakeet_engine/commands.rs`, `summary_engine/commands.rs` use `app.path().app_data_dir()` |
| Models fallback when no dir is passed | `%APPDATA%\Meetily\models` | `%APPDATA%\PulseTalq\models` | `whisper_engine.rs:122`, `parakeet_engine.rs:142`, `summary_engine/model_manager.rs:148` |
| Custom summary templates | `%APPDATA%\Meetily\templates` | `%APPDATA%\PulseTalq\templates` | `summary/templates/loader.rs:27` |
| Notification settings | `%APPDATA%\meetily\notifications.json` | `%APPDATA%\pulse-talq\notifications.json` | `notifications/settings.rs:118` |
| Default recordings folder | `~\Music\meetily-recordings` | `~\Music\pulse-talq-recordings` | `audio/recording_preferences.rs` |
| Windows uninstall entry | HKCU `...\Uninstall\meetily` | HKCU `...\Uninstall\PulseTalq` (NSIS) and HKLM `{ProductCode}` (MSI) | observed on the dev machine |

Also derived from the identifier: the WebView2 profile and the log directory, at `%LOCALAPPDATA%\com.meetily.ai\{EBWebView,logs}` before and `%LOCALAPPDATA%\com.pulsetalq.app\...` now, and the NSIS `currentUser` install directory, `%LOCALAPPDATA%\meetily` before and `%LOCALAPPDATA%\PulseTalq` now (the bundled `templates\` folder lives there). Tauri store plugin state sits under `HKCU\Software\meetily` and `HKCU\Software\pulsetalq` respectively.

Observed on the development machine on 2026-09-02 with both products installed: `%APPDATA%\com.meetily.ai` holds `meeting_minutes.sqlite`, `models\`, `preferences.json`, `recording_preferences.json`, `onboarding-status.json` and `analytics.json`; `%APPDATA%\meetily\notifications.json` exists; `%APPDATA%\com.pulsetalq.app` holds a fresh copy of the same file set. `%APPDATA%\Meetily\models` (the code fallback path) does not exist because the app always passes `app_data_dir()` in production, so the fallback branch is effectively dead code. Windows folder names are case-insensitive, so `%APPDATA%\Meetily` and `%APPDATA%\meetily` are one folder.

On macOS and Linux the same folder names apply under `~/Library/Application Support/` and `~/.config/` or `~/.local/share/` respectively, and those file systems are case-sensitive, so `Meetily` and `meetily` are distinct folders there.

Consequences:

- Windows treats PulseTalq as a separate product. Installing it does not upgrade, replace, or remove Meetily. Both appear in Installed apps, both have Start menu shortcuts, and both can run at once.
- PulseTalq starts with an empty database. **No Rust code reads from any legacy Meetily location.** The grep on 2026-09-02 for `Meetily` and `meetily` in `frontend/src-tauri/src` returns only the Parakeet model download host (`meetily.towardsgeneralintelligence.com`) and the dead `lib_old_complex.rs`. There is no fallback read of `com.meetily.ai`, `Meetily\models`, `Meetily\templates`, `meetily\notifications.json`, or `meetily-recordings`. The list of legacy folders still read from is therefore empty; the list of legacy folders that become orphaned is the whole left-hand column above.
- Downloaded Whisper, Parakeet and summary models are not shared. A user who installs PulseTalq re-downloads models unless they copy `%APPDATA%\com.meetily.ai\models` to `%APPDATA%\com.pulsetalq.app\models` by hand.

Recommended handling (decision recorded in `project/decisions.md`):

1. Leave Meetily installed. The installer must not detect, uninstall, or modify the Meetily product, its registry keys, or its data. Users uninstall it themselves from Installed apps when they are ready.
2. Document the data location change in the release notes and in the in-app database import flow (`frontend/src/components/DatabaseImport/`), which already offers a manual import of a legacy database file.
3. Track the missing model and template migration as an open gap (`project/known-gaps.md`). If a migration is added later it must be an explicit copy with user consent, never a silent move or a shared folder.
4. The verification script emits a `WARN`, not a failure, when it sees a `meetily` uninstall entry next to the PulseTalq one; that is the expected coexistence state.

## 7. Evidence to attach to a release

- Output of `scripts/verify-installer-assets.py` (text table).
- Output of `scripts/verify-installer-branding.ps1` before install and with `-CheckInstalled` after install.
- Screenshots: NSIS welcome page, MSI first dialog, Windows Installed apps row, Start menu entry, macOS DMG window, Linux application menu entry.
- SHA-256 of `frontend/src-tauri/icons/icon.png` used for the build, so a later audit can tie artifacts to the generator run.
