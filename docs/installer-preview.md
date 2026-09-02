# Installer asset preview

`docs/installer-preview.png` is a single review contact sheet for the Deep Focus
installer and icon assets. `docs/installer-preview.html` is the same review as a
static page that loads the live files under `frontend/` by relative path, so it
always shows what is on disk right now. Both are generated; do not edit them by
hand.

## What the sheet is for

The sheet lets a reviewer sign off the rebranded installer assets in one place
without building an installer or opening each file:

1. Icon ladder: `icon.ico` frames at 16 to 48 px and the PNGs at 64 to 256 px on
   Readout, Blackout, Windows dark taskbar and macOS light dock grounds.
2. Installer surfaces at 1:1: NSIS header (150x57), NSIS sidebar (164x314), WiX
   banner (493x58), WiX dialog (493x312) and the 1x DMG background (660x400).
3. In-context mockups: a simplified Windows 11 NSIS window (welcome page with
   sidebar, inner page with header) and a simplified Finder DMG window with the
   128 px icon at the configured app position and a folder glyph at the
   Applications position.
4. Colour audit: the top 5 colours per asset, with flags for the legacy purple and
   for colours outside the brand token set.
5. Contrast: WCAG ratios for the five core brand pairs against a 4.5 reference.
6. Footer: timestamp, git short SHA, and per-file SHA-256 prefix plus dimensions.

Missing files render as a hatched placeholder labelled "missing", so the sheet
can be produced before the asset pipeline has finished.

## How to regenerate

```bash
python scripts/brand/render-installer-preview.py
```

Requirements: Python 3.9+ and Pillow (tested with 12.2). No ImageMagick. The
script uses `scripts/brand/fonts/Archivo[wdth,wght].ttf` when present, otherwise
DejaVu Sans or Arial. It prints a JSON summary (state, flags, size and hash per
asset) to stdout and writes both the PNG and the HTML.

Run it after any change to `frontend/src-tauri/icons/`,
`frontend/src-tauri/installer/`, `frontend/public/logo*.png`,
`frontend/public/icon_*.png` or `frontend/src/app/favicon.ico`, and commit the
regenerated outputs together with the assets.

## Reviewer checklist

Sign off only when every line holds:

- Every asset row in the footer table shows `NEW`. `OLD` means a legacy purple
  colour was found; `MISSING` means the file is absent.
- No `PURPLE`, `OFF-BRAND` or `SIZE` flag appears in the colour audit.
- The 16 and 20 px icons keep a readable "p" and a distinct dark tile on all four
  grounds. If the glyph collapses at 16 px the .ico needs a hinted small frame.
- The NSIS sidebar shows no seam against the white page, and the header sits
  top-right at 150x57 with the page title to its left.
- The DMG background reads under the 128 px icon and the folder glyph, and the
  arrow on the background points from the app position to Applications.
- NSIS and WiX bitmaps are 24 bit BMP without alpha (the audit reports their
  dimensions; check the format with the asset pipeline's own report).
- The contrast panel shows every pair at or above 4.5, or the pair is one where the
  design explicitly adds shape or text (rule: never rely on red alone).
- The HTML page shows the same content in light and dark schemes and the
  pixelated toggle does not change layout.

## Reading the colour and contrast panels

| Signal | Meaning | Action |
|---|---|---|
| Swatch with a red corner mark, `PURPLE` flag | A top-5 colour has hue 240 to 300, saturation above 0.3 and value above 0.2: the old Meetily purple survived | Regenerate the asset from the Deep Focus source |
| Swatch with a warning corner mark, `OFF-BRAND` flag | A top-5 colour is more than 24 RGB from every brand token and from every straight blend between two tokens | Check for stray colours or a wrong background; anti-aliased edges between two tokens are allowed and do not flag |
| `SIZE` flag | Decoded dimensions differ from what NSIS, WiX or the DMG bundler expects | Re-export at the exact size listed in red |
| `OK` flag | Top-5 colours are all brand tokens or blends, dimensions match | No action |
| Contrast bar at or right of the 4.5 line | Pair passes WCAG AA for normal text | Usable for text |
| Contrast bar left of the 4.5 line, note "below AA, shape required" | Pair fails AA text | Use only with an icon, shape change or label; never as the sole state signal |

Reference values: Blackout on Readout and Readout on Blackout 18.19:1, Blackout
on Hot Signal and Hot Signal on Blackout 5.52:1, text secondary on Readout 9.53:1.
The bars are one neutral series; the value label carries the number and the
small two-tone swatch beside each name shows the actual pair.
