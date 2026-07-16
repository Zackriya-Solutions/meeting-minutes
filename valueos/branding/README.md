# ValueOS Agent — Branding

This folder rebrands the app from **Meetily** to **ValueOS Agent** (name + icon)
**without editing any upstream file**. Everything lives here in a parallel folder and is
applied only at **build time**, so merges from upstream (Meetily) never conflict.

## What's here

| File | Purpose |
|------|---------|
| `source/valueos-agent-logo.svg` | The original VA logo (source of truth for the icon). |
| `icons/` | Generated app icons (`icon.png` 1024² RGBA, `app_icon.icns`, `app_icon.ico`, `icon.icns`, `icon.ico`, + PNG sizes). |
| `tauri.valueos.json` | Config **overlay** merged onto `tauri.conf.json` at build time — sets `productName`, `mainBinaryName`, the window title (*ValueOS Agent*), the bundle **`identifier` → `com.valueos.io`** (which moves app data to `~/Library/Application Support/com.valueos.io/`), and the **`style-src` CSP exemption** (see below). |
| `apply-branding.sh` | Stages `icons/` into `frontend/src-tauri/icons/` at build time (working tree only). |
| `make-ci-config.js` | Emits the combined CI overlay (branding + `createUpdaterArtifacts:false`). |

## How it's applied (nothing upstream is edited in git)

- **Name** → `tauri build --config valueos/branding/tauri.valueos.json`. Tauri deep-merges
  this over `tauri.conf.json` in memory; the file on disk is untouched.
- **Icon** → `apply-branding.sh` copies our icons over the filenames `tauri.conf.json`
  already references (`icon.png`, `app_icon.icns`, `app_icon.ico`). In CI this happens on a
  throwaway checkout; nothing is committed. (Locally it dirties `frontend/src-tauri/icons/`
  — restore with `git checkout -- frontend/src-tauri/icons` when done.)

The CI workflow [`valueos-build.yml`](../../.github/workflows/valueos-build.yml) does both
automatically on every build.

## Why the overlay also sets a `style-src` CSP exemption

In a **packaged (production) build only**, Tauri injects a nonce/hash into the `style-src`
CSP directive. Per the CSP spec, a nonce **nullifies `'unsafe-inline'`** — which silently
strips every React inline `style=` attribute, so our ValueOS screens (which style with
inline `style={}` objects) render **completely unstyled** (plain text on white). It never
shows in `tauri dev` or a browser, because neither injects that nonce — so it looks like a
"stale build" until you check.

The overlay adds `app.security.dangerousDisableAssetCspModification: ["style-src"]`, telling
Tauri to leave `style-src` exactly as authored (`'self' 'unsafe-inline'`). `script-src` is
**not** listed, so script nonce-hardening is fully preserved (styles can't exfiltrate data,
so exempting them is the standard, safe remedy). Regression-guarded by
[`valueos/shell-tests/branding-config.test.ts`](../shell-tests/branding-config.test.ts).

## Build it yourself (locally, on a Mac)

```bash
bash valueos/branding/apply-branding.sh          # stage the VA icons
cd frontend
pnpm install
pnpm exec tauri build --config ../valueos/branding/tauri.valueos.json
# add --config ...createUpdaterArtifacts:false or use make-ci-config.js if you want to
# skip updater signing (see the workflow)
git -C .. checkout -- frontend/src-tauri/icons   # optional: restore upstream icons
```

## Regenerating the icons (if the logo changes)

Icons were produced from `source/valueos-agent-logo.svg`:
1. Rasterize to 1024² PNG: `qlmanage -t -s 1024 -o <tmp> source/valueos-agent-logo.svg`
2. Resize to the PNG set with Pillow (keep **RGBA** — Tauri rejects a non-RGBA `icon.png`).
3. `.icns` via an `.iconset` + `iconutil -c icns`; `.ico` via Pillow multi-size.

## Not covered here (deliberately)

This changes the **native** identity only — the macOS/Windows app name, the dock/menu-bar
name, the installer/DMG name, the window title, and the app icon. It does **not** change
**in-app UI text or logos** inside the Next.js frontend (e.g. any "Meetily" wordmark on
screens). Those live in upstream source files, so changing them can't be done without
editing upstream and risking merge conflicts. If we want that too, options are: (a) small
`// VALUEOS:`-marked edits to the specific components, or (b) a runtime string/asset
override layer. Raise it separately — it's a conscious trade-off against merge-cleanliness.
