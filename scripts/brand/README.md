# Brand asset generator

`generate-installer-assets.py` renders every shipped PulseTalq brand bitmap from
a single definition of the Deep Focus mark: a lowercase Archivo "p" in Hot Signal
on a Blackout rounded square. It regenerates the Tauri icon family, the NSIS,
WiX and DMG installer imagery, and the web-facing logos in one idempotent run.

## Run it

From `frontend/`:

```bash
pnpm brand:assets
```

or from anywhere:

```bash
python scripts/brand/generate-installer-assets.py
```

Requirements: Python 3.10+, Pillow 10+, Node with `pnpm`, and `@tauri-apps/cli`
(already a dev dependency; the script calls `pnpm tauri icon`). No ImageMagick.

The run is deterministic for everything the script renders itself. Running it
twice without changing inputs produces byte-identical PNG, BMP and ICO files,
so the manifest hashes printed at the end double as a regression check. The one
exception is `icon.icns` (and its `app_icon.icns` copy): the Tauri CLI's ICNS
encoder emits a different byte stream on every run even though the pixel
content is the same, so expect those two hashes to move.

## Inputs

| Input | Purpose |
|---|---|
| `fonts/Archivo[wdth,wght].ttf` | Archivo variable font from google/fonts (SIL OFL 1.1, see `fonts/OFL.txt`). The weight axis is pinned to 500 at load time. |
| Constants at the top of the script | Palette, corner radius, mark size, tracking, tagline. |

If the font is missing the script draws a geometric "p" (stem plus bowl) for the
icon family and skips every text-bearing asset, printing a warning. Restore the
font with:

```bash
curl -L -o "scripts/brand/fonts/Archivo[wdth,wght].ttf" \
  "https://raw.githubusercontent.com/google/fonts/main/ofl/archivo/Archivo%5Bwdth%2Cwght%5D.ttf"
```

## Outputs

| Path | Size | Notes |
|---|---|---|
| `scripts/brand/out/icon-source-1024.png` | 1024x1024 RGBA | Rounded source fed to `tauri icon`. |
| `scripts/brand/out/icon-source-1024-square.png` | 1024x1024 RGBA | Square variant, kept for reference (see trade-off below). |
| `frontend/src-tauri/icons/*` | Tauri family | `icon.png`, `icon.ico`, `icon.icns`, `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`, `Square*Logo.png`, `StoreLogo.png`, plus `app_icon.ico` and `app_icon.icns` copies that `tauri.conf.json` references, plus the legacy `icon_<n>x<n>[@2x].png` sizes re-derived from the source. Android and iOS trees emitted by the CLI are removed because this project has no mobile targets. |
| `frontend/src-tauri/installer/nsis-header.bmp` | 150x57 RGB | Blackout, small mark left, wordmark. |
| `frontend/src-tauri/installer/nsis-sidebar.bmp` | 164x314 RGB | Blackout, large mark upper third, wordmark and tagline at the bottom. |
| `frontend/src-tauri/installer/wix-banner.bmp` | 493x58 RGB | Readout, mark and wordmark left, 2px Hot Signal rule at the bottom. |
| `frontend/src-tauri/installer/wix-dialog.bmp` | 493x312 RGB | Readout with a 164px Blackout left column; right side left empty for WiX text. |
| `frontend/src-tauri/installer/dmg-background.png` | 660x400 RGB | Readout, wordmark top-left, Hot Signal arrow between the app (x=180, y=220) and Applications (x=480, y=220) drop targets, instruction text. |
| `frontend/src-tauri/installer/dmg-background@2x.png` | 1320x800 RGB | Same layout at 2x. |
| `frontend/public/logo.png` | 512 wide RGBA | Full lowercase wordmark on transparent, light-ground colours. |
| `frontend/public/logo-collapsed.png` | 128x128 RGBA | The mark alone. |
| `frontend/public/icon_128x128.png`, `icon_32x32@2x.png` | 128, 64 RGBA | Web copies of the mark. |
| `frontend/src/app/favicon.ico` | 16, 32, 48 | Multi-size ICO. |

All BMPs are 24-bit uncompressed (Pillow `RGB` mode), which is what NSIS and
WiX require.

### Rounded versus square source

Tauri accepts one source image for the whole family. macOS applies its own
squircle mask to Dock icons and expects the artwork to already carry the
rounded shape with transparent corners, while Windows and Linux display the PNG
as supplied. The rounded source is used because it looks correct everywhere;
the only cost is that Windows tiles show slightly rounded corners, which
matches how most modern Windows apps present. The square variant is written to
`out/` in case a platform needs it later.

## Brand rules the script enforces

- Palette only: Blackout `#0b0b0c`, Readout `#f7f6f2`, Hot Signal `#ff3b1f`,
  Afterglow `#ffb39f`, Coal `#18191b`, Machine Fog `#9da5a6`, Accent Wash
  `#fff0ec`, and the inverse muted text tone `#b9b8b4`. No violet, no legacy
  Meetily purple.
- Flat surfaces, no gradients, no shadows, no outlines.
- Hot Signal appears only on the active element: the "p" mark, the "talq" half
  of the wordmark, the DMG arrow and the WiX banner rule. Everything else is
  Blackout or Readout.
- Archivo weight 500 for the mark and wordmark, with the identity page's
  -0.06em tracking. The wordmark is lowercase, "pulse" in the ground's ink
  colour and "talq" in Hot Signal.
- The mark is optically centred: the glyph ink box is centred, then lifted
  1.5 percent of the tile because the descender reads lighter than the bowl.

## Changing the mark

- Corner radius: `CORNER_RATIO` (share of the tile side, default 0.22).
- Glyph size: `MARK_INK_RATIO` (ink height as a share of the side, default 0.62).
  Check the 16px rendering after changing it.
- Optical nudge: `OPTICAL_LIFT`.
- Weight: `ARCHIVO_WEIGHT`, applied on the variable font's weight axis.
- Different glyph or font: edit `draw_p_mark` and `FONT_CANDIDATES`. Any font
  Pillow can open works; if it is static the axis step is skipped.
- Tagline and tracking: `TAGLINE`, `WORDMARK_TRACKING`.

After any change, run the script, inspect `scripts/brand/out/icon-source-1024.png`
at 16px and 32px, and review the four BMPs (open them in an image viewer; the
NSIS sidebar shows at 1:1 in the installer).

## What the script does not touch

`frontend/src-tauri/tauri.conf.json` already points at the installer paths
above; the script never edits it. It also does not commit.
