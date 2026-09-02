#!/usr/bin/env python3
"""Generate the PulseTalq "Deep Focus" icon family and installer imagery.

Idempotent: every output is rendered deterministically from the palette and
geometry below, so re-running produces byte-identical files unless the inputs
change. Run from anywhere; paths are resolved relative to this file.

Outputs (see scripts/brand/README.md for the full list):
  scripts/brand/out/            source icons (rounded + square)
  frontend/src-tauri/icons/     full icon family via the Tauri icon CLI
  frontend/src-tauri/installer/ NSIS, WiX and DMG imagery
  frontend/public/, frontend/src/app/favicon.ico  web-facing brand assets
"""
from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# --------------------------------------------------------------------------- #
# Paths
# --------------------------------------------------------------------------- #
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FRONTEND = ROOT / "frontend"
OUT = HERE / "out"
FONT_DIR = HERE / "fonts"
ICONS = FRONTEND / "src-tauri" / "icons"
INSTALLER = FRONTEND / "src-tauri" / "installer"
PUBLIC = FRONTEND / "public"
FAVICON = FRONTEND / "src" / "app" / "favicon.ico"

# --------------------------------------------------------------------------- #
# Brand palette (DESIGN.md section 2). Flat fills only: no gradients, no violet.
# --------------------------------------------------------------------------- #
BLACKOUT = (0x0B, 0x0B, 0x0C)
READOUT = (0xF7, 0xF6, 0xF2)
HOT_SIGNAL = (0xFF, 0x3B, 0x1F)
AFTERGLOW = (0xFF, 0xB3, 0x9F)
COAL = (0x18, 0x19, 0x1B)
MACHINE_FOG = (0x9D, 0xA5, 0xA6)
ACCENT_WASH = (0xFF, 0xF0, 0xEC)
INVERSE_MUTED = (0xB9, 0xB8, 0xB4)

# --------------------------------------------------------------------------- #
# Mark and wordmark geometry
# --------------------------------------------------------------------------- #
ICON_SIZE = 1024
CORNER_RATIO = 0.22          # rounded-square corner radius as a share of side
MARK_INK_RATIO = 0.62        # height of the "p" ink box as a share of side
OPTICAL_LIFT = 0.015         # nudge the p up: the descender reads lighter than the bowl
WORDMARK_TRACKING = -0.06    # em, matches docs/pulsetalq-identity.html
ARCHIVO_WEIGHT = 500         # DESIGN.md: headings and marks use weight 500
SUPERSAMPLE = 4              # render at Nx then downsample for clean edges
TAGLINE = "Private at full speed."

# --------------------------------------------------------------------------- #
# Font loading: Archivo (OFL) variable font, weight axis pinned to 500.
# Falls back to a geometric "p" if the font is missing.
# --------------------------------------------------------------------------- #
FONT_CANDIDATES = [
    FONT_DIR / "Archivo[wdth,wght].ttf",
    FONT_DIR / "Archivo-Medium.ttf",
]
FONT_PATH: Path | None = next((p for p in FONT_CANDIDATES if p.exists()), None)
FONT_MODE = "archivo" if FONT_PATH else "geometric"


def font(size: int) -> ImageFont.FreeTypeFont:
    if FONT_PATH is None:
        raise RuntimeError("Archivo font missing; text rendering unavailable")
    f = ImageFont.truetype(str(FONT_PATH), size)
    try:
        axes = f.get_variation_axes()
    except OSError:
        return f  # static font, nothing to set
    values = []
    for axis in axes:
        name = axis["name"]
        name = name.decode() if isinstance(name, bytes) else name
        values.append(ARCHIVO_WEIGHT if name.lower() == "weight" else axis["default"])
    f.set_variation_by_axes(values)
    return f


# --------------------------------------------------------------------------- #
# Drawing primitives
# --------------------------------------------------------------------------- #
def rgba(rgb) -> tuple:
    return rgb + (255,)


def new_canvas(w: int, h: int, bg, scale: int) -> Image.Image:
    return Image.new("RGBA", (w * scale, h * scale), rgba(bg) if bg else (0, 0, 0, 0))


def finish(img: Image.Image, w: int, h: int) -> Image.Image:
    if img.size == (w, h):
        return img
    return img.resize((w, h), Image.LANCZOS)


def draw_text_tracked(draw: ImageDraw.ImageDraw, xy, text: str, f: ImageFont.FreeTypeFont,
                      colors, tracking_em: float) -> None:
    """Draw text glyph by glyph with letter spacing while keeping font kerning.

    Pillow without libraqm has no tracking option, so each glyph is placed at the
    kerned advance of its prefix plus the accumulated tracking.
    """
    x, y = xy
    tracking = tracking_em * f.size
    for i, ch in enumerate(text):
        draw.text((x + f.getlength(text[:i]) + tracking * i, y), ch, font=f, fill=rgba(colors[i]))


def text_bbox(f: ImageFont.FreeTypeFont, text: str, tracking_em: float = 0.0):
    """Ink bbox (l, t, r, b) relative to the drawing origin for tracked text."""
    l, t, r, b = f.getbbox(text)
    r += tracking_em * f.size * (len(text) - 1)
    return l, t, r, b


def draw_wordmark(draw: ImageDraw.ImageDraw, xy, size: int, pulse_color, talq_color) -> None:
    """Lowercase wordmark: "pulse" in pulse_color, "talq" in talq_color."""
    colors = [pulse_color] * 5 + [talq_color] * 4
    draw_text_tracked(draw, xy, "pulsetalq", font(size), colors, WORDMARK_TRACKING)


def place_wordmark_centred_y(draw, x, centre_y, size, pulse_color, talq_color) -> None:
    f = font(size)
    l, t, r, b = text_bbox(f, "pulsetalq", WORDMARK_TRACKING)
    draw_wordmark(draw, (x, centre_y - (t + b) / 2), size, pulse_color, talq_color)


def glyph_ink_bbox(f: ImageFont.FreeTypeFont, ch: str):
    """Exact ink bbox of a glyph by rasterising it once, plus the origin used."""
    origin = (f.size // 2, f.size // 4)
    probe = Image.new("L", (f.size * 2, f.size * 2), 0)
    ImageDraw.Draw(probe).text(origin, ch, font=f, fill=255)
    return probe.getbbox(), origin


def draw_p_mark(draw: ImageDraw.ImageDraw, box, color) -> None:
    """Render the lowercase "p" optically centred in `box` (x0, y0, x1, y1).

    Optical centring: the ink bbox of the glyph (stem top to descender bottom,
    stem left to bowl right) is centred in the box, then lifted by OPTICAL_LIFT
    because the descender reads lighter than the bowl.
    """
    x0, y0, x1, y1 = box
    side = min(x1 - x0, y1 - y0)
    target_h = side * MARK_INK_RATIO
    cx = (x0 + x1) / 2
    cy = (y0 + y1) / 2 - side * OPTICAL_LIFT

    if FONT_MODE == "archivo":
        probe = font(1000)
        bbox, _ = glyph_ink_bbox(probe, "p")
        size = max(8, int(round(1000 * target_h / (bbox[3] - bbox[1]))))
        f = font(size)
        bbox, origin = glyph_ink_bbox(f, "p")
        ink_w, ink_h = bbox[2] - bbox[0], bbox[3] - bbox[1]
        draw_x = cx - ink_w / 2 - (bbox[0] - origin[0])
        draw_y = cy - ink_h / 2 - (bbox[1] - origin[1])
        draw.text((draw_x, draw_y), "p", font=f, fill=rgba(color))
        return

    # Geometric fallback: a stem plus a bowl, proportions borrowed from Archivo.
    h = target_h
    stroke = h * 0.19
    bowl_h = h * 0.64
    bowl_w = h * 0.66
    left = cx - bowl_w / 2
    top = cy - h / 2
    draw.ellipse([left, top, left + bowl_w, top + bowl_h], fill=rgba(color))
    draw.ellipse([left + stroke, top + stroke, left + bowl_w - stroke, top + bowl_h - stroke],
                 fill=rgba(BLACKOUT))
    draw.rectangle([left, top, left + stroke, top + h], fill=rgba(color))


def render_mark(size: int, rounded: bool, bg=BLACKOUT) -> Image.Image:
    """The compact app mark: Blackout square (rounded or not) with a Hot Signal p."""
    s = SUPERSAMPLE
    img = Image.new("RGBA", (size * s, size * s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if rounded:
        d.rounded_rectangle([0, 0, size * s - 1, size * s - 1],
                            radius=int(size * s * CORNER_RATIO), fill=rgba(bg))
    else:
        d.rectangle([0, 0, size * s - 1, size * s - 1], fill=rgba(bg))
    draw_p_mark(d, (0, 0, size * s, size * s), HOT_SIGNAL)
    return finish(img, size, size)


def paste_mark(canvas: Image.Image, xy, size: int, scale: int) -> None:
    """Composite the rounded mark onto a supersampled canvas (xy and size in 1x units)."""
    mark = render_mark(size * scale, rounded=True)
    canvas.alpha_composite(mark, (int(round(xy[0] * scale)), int(round(xy[1] * scale))))


def save_bmp(img: Image.Image, w: int, h: int, name: str) -> Path:
    """NSIS and WiX need 24-bit uncompressed BMP: flatten to RGB before saving."""
    out = INSTALLER / name
    finish(img, w, h).convert("RGB").save(out, format="BMP")
    return out


# --------------------------------------------------------------------------- #
# Asset builders
# --------------------------------------------------------------------------- #
def build_source_icons() -> list[Path]:
    OUT.mkdir(parents=True, exist_ok=True)
    p1 = OUT / "icon-source-1024.png"
    p2 = OUT / "icon-source-1024-square.png"
    render_mark(ICON_SIZE, rounded=True).save(p1, optimize=True)
    render_mark(ICON_SIZE, rounded=False).save(p2, optimize=True)
    return [p1, p2]


def run_tauri_icon(source: Path) -> tuple[list[Path], str]:
    """Regenerate frontend/src-tauri/icons via the Tauri CLI."""
    before = {p.name for p in ICONS.iterdir()} if ICONS.exists() else set()
    rel = os.path.relpath(source, FRONTEND)
    cmd = ["pnpm", "tauri", "icon", rel, "--output", "src-tauri/icons"]
    proc = subprocess.run(cmd, cwd=FRONTEND, capture_output=True, text=True,
                          shell=(os.name == "nt"))
    log = (proc.stdout + proc.stderr).strip()
    if proc.returncode != 0:
        sys.stderr.write(log + "\n")
        raise SystemExit("tauri icon failed")
    # tauri.conf.json references app_icon.* alongside icon.*; keep them in sync.
    shutil.copyfile(ICONS / "icon.ico", ICONS / "app_icon.ico")
    shutil.copyfile(ICONS / "icon.icns", ICONS / "app_icon.icns")
    # The CLI also emits android/ and ios/ trees. This project has no mobile
    # targets, so drop them unless they were already checked in.
    for mobile in ("android", "ios"):
        if mobile not in before and (ICONS / mobile).is_dir():
            shutil.rmtree(ICONS / mobile)
    # Legacy icon_<n>x<n>[@2x].png files predate the CLI naming and are not
    # regenerated by it. Re-derive them from the source so no old art survives.
    src = Image.open(source)
    for base in (16, 32, 128, 256, 512):
        for suffix, factor in (("", 1), ("@2x", 2)):
            px = base * factor
            src.resize((px, px), Image.LANCZOS).save(
                ICONS / f"icon_{base}x{base}{suffix}.png", optimize=True)
    written = sorted(p for p in ICONS.iterdir() if p.is_file())
    return written, log


def build_nsis_header() -> Path:
    W, H, s = 150, 57, SUPERSAMPLE
    img = new_canvas(W, H, BLACKOUT, s)
    d = ImageDraw.Draw(img)
    mark = 28
    paste_mark(img, (12, (H - mark) / 2), mark, s)
    place_wordmark_centred_y(d, (12 + mark + 9) * s, H * s / 2, 18 * s, READOUT, HOT_SIGNAL)
    return save_bmp(img, W, H, "nsis-header.bmp")


def draw_dark_column(img: Image.Image, d: ImageDraw.ImageDraw, col_w: int, H: int, s: int,
                     mark: int, mark_top: int) -> None:
    """Shared layout for the NSIS sidebar and the WiX dialog's left column."""
    paste_mark(img, ((col_w - mark) / 2, mark_top), mark, s)
    fw = font(22 * s)
    l, t, r, b = text_bbox(fw, "pulsetalq", WORDMARK_TRACKING)
    draw_wordmark(d, (16 * s, (H - 62) * s - t), 22 * s, READOUT, HOT_SIGNAL)
    d.text((16 * s, (H - 34) * s), TAGLINE, font=font(11 * s), fill=rgba(INVERSE_MUTED))


def build_nsis_sidebar() -> Path:
    W, H, s = 164, 314, SUPERSAMPLE
    img = new_canvas(W, H, BLACKOUT, s)
    d = ImageDraw.Draw(img)
    draw_dark_column(img, d, W, H, s, mark=88, mark_top=40)
    return save_bmp(img, W, H, "nsis-sidebar.bmp")


def build_wix_banner() -> Path:
    W, H, s = 493, 58, SUPERSAMPLE
    img = new_canvas(W, H, READOUT, s)
    d = ImageDraw.Draw(img)
    mark = 30
    rule = 2
    paste_mark(img, (14, (H - rule - mark) / 2), mark, s)
    place_wordmark_centred_y(d, (14 + mark + 10) * s, (H - rule) * s / 2, 19 * s, BLACKOUT, HOT_SIGNAL)
    d.rectangle([0, (H - rule) * s, W * s, H * s], fill=rgba(HOT_SIGNAL))
    return save_bmp(img, W, H, "wix-banner.bmp")


def build_wix_dialog() -> Path:
    W, H, s = 493, 312, SUPERSAMPLE
    COL = 164
    img = new_canvas(W, H, READOUT, s)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, COL * s - 1, H * s], fill=rgba(BLACKOUT))
    draw_dark_column(img, d, COL, H, s, mark=72, mark_top=44)
    return save_bmp(img, W, H, "wix-dialog.bmp")


def build_dmg_background(scale: int) -> Path:
    """660x400 DMG background at 1x or 2x. Layout coordinates are in 1x points."""
    W, H = 660, 400
    s = SUPERSAMPLE * scale
    img = new_canvas(W, H, READOUT, s)
    d = ImageDraw.Draw(img)
    draw_wordmark(d, (32 * s, 28 * s), 26 * s, BLACKOUT, HOT_SIGNAL)
    # Hot Signal arrow between the app icon (centre x=180) and the Applications
    # folder (centre x=480), both at y=170 to match tauri.conf.json dmg positions. Ends clear 128px icons on each side.
    y = 170 * s
    x_start, x_end = 262 * s, 398 * s
    d.line([(x_start, y), (x_end, y)], fill=rgba(HOT_SIGNAL), width=2 * s)
    head = 9 * s
    d.polygon([(x_end, y), (x_end - head, y - head * 0.6), (x_end - head, y + head * 0.6)],
              fill=rgba(HOT_SIGNAL))
    ft = font(14 * s)
    label = "Drag PulseTalq to Applications"
    l, t, r, b = ft.getbbox(label)
    d.text(((W * s - (r - l)) / 2 - l, 322 * s), label, font=ft, fill=rgba(BLACKOUT))
    out = INSTALLER / ("dmg-background.png" if scale == 1 else "dmg-background@2x.png")
    finish(img, W * scale, H * scale).convert("RGB").save(out, optimize=True)
    return out


def build_public_assets() -> list[Path]:
    written = []
    # logo.png: full lowercase wordmark on transparent, 512 wide, light-ground colours.
    s = SUPERSAMPLE
    size = 120 * s
    f = font(size)
    l, t, r, b = text_bbox(f, "pulsetalq", WORDMARK_TRACKING)
    ink_w, ink_h = r - l, b - t
    pad = int(0.06 * ink_w)
    img = Image.new("RGBA", (int(ink_w + 2 * pad), int(ink_h + 2 * pad)), (0, 0, 0, 0))
    draw_wordmark(ImageDraw.Draw(img), (pad - l, pad - t), size, BLACKOUT, HOT_SIGNAL)
    target_w = 512
    target_h = int(round(img.height * target_w / img.width))
    p = PUBLIC / "logo.png"
    img.resize((target_w, target_h), Image.LANCZOS).save(p, optimize=True)
    written.append(p)

    p = PUBLIC / "logo-collapsed.png"
    render_mark(128, rounded=True).save(p, optimize=True)
    written.append(p)

    src = Image.open(OUT / "icon-source-1024.png")
    for name, px in (("icon_128x128.png", 128), ("icon_32x32@2x.png", 64)):
        p = PUBLIC / name
        src.resize((px, px), Image.LANCZOS).save(p, optimize=True)
        written.append(p)

    FAVICON.parent.mkdir(parents=True, exist_ok=True)
    src.resize((48, 48), Image.LANCZOS).save(
        FAVICON, format="ICO", sizes=[(16, 16), (32, 32), (48, 48)])
    written.append(FAVICON)
    return written


# --------------------------------------------------------------------------- #
# Manifest
# --------------------------------------------------------------------------- #
def describe(path: Path) -> str:
    try:
        with Image.open(path) as im:
            if path.suffix.lower() == ".ico":
                dims = ",".join(f"{w}x{h}" for w, h in sorted(im.ico.sizes()))
            else:
                dims = f"{im.size[0]}x{im.size[1]}"
            return f"{dims} {im.mode}"
    except Exception:
        return "-"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def print_manifest(files: list[Path]) -> None:
    rows = [(str(p.relative_to(ROOT)).replace("\\", "/"), describe(p), p.stat().st_size, sha256(p))
            for p in files]
    w0 = max(len(r[0]) for r in rows)
    w1 = max(len(r[1]) for r in rows)
    print(f"\n{'file'.ljust(w0)}  {'dimensions/mode'.ljust(w1)}  {'bytes':>8}  sha256")
    print("-" * (w0 + w1 + 8 + 6 + 64))
    for r in rows:
        print(f"{r[0].ljust(w0)}  {r[1].ljust(w1)}  {r[2]:>8}  {r[3]}")


def main() -> int:
    detail = f" ({FONT_PATH.name}, wght {ARCHIVO_WEIGHT})" if FONT_PATH else ""
    print(f"font mode: {FONT_MODE}{detail}")
    if FONT_MODE != "archivo":
        print("warning: no Archivo TTF in scripts/brand/fonts; drawing a geometric p and "
              "skipping every text-bearing asset", file=sys.stderr)
    INSTALLER.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    written += build_source_icons()
    icons, log = run_tauri_icon(OUT / "icon-source-1024.png")
    if log:
        print("tauri icon output:\n" + log)
    written += icons
    if FONT_MODE == "archivo":
        written += [build_nsis_header(), build_nsis_sidebar(), build_wix_banner(),
                    build_wix_dialog(), build_dmg_background(1), build_dmg_background(2)]
        written += build_public_assets()
    print_manifest(written)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
