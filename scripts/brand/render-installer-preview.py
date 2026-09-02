#!/usr/bin/env python3
"""Render the PulseTalq installer asset review sheet.

Produces:
  docs/installer-preview.png   single contact sheet (Deep Focus styling)
  docs/installer-preview.html  static HTML twin that references the real files

Usage:
  python scripts/brand/render-installer-preview.py

Requires Python 3.9+ and Pillow 10+ (tested with Pillow 12). No ImageMagick.
Missing assets are rendered as placeholders labelled "missing"; the script
never fails because an asset is absent.
"""

from __future__ import annotations

import colorsys
import datetime as dt
import hashlib
import html
import json
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"
OUT_PNG = DOCS / "installer-preview.png"
OUT_HTML = DOCS / "installer-preview.html"

# ---------------------------------------------------------------- brand tokens
BLACKOUT = "#0b0b0c"
READOUT = "#f7f6f2"
HOT = "#ff3b1f"
AFTERGLOW = "#ffb39f"
BORDER = "#d2cfca"
BORDER_STRONG = "#b7b3ad"
TEXT2 = "#414042"
TEXT3 = "#6f6d6d"
SURFACE = "#ffffff"
SURFACE_ALT = "#efede8"
SURFACE_DARK = "#18191b"
SUCCESS = "#237a57"
ERROR = "#b42318"
WARNING = "#9a5b00"
INFO = "#365f91"

# The token set an asset is allowed to be built from (DESIGN.md section 2).
BRAND_TOKENS = {
    "blackout": BLACKOUT,
    "readout": READOUT,
    "hot-signal": HOT,
    "afterglow": AFTERGLOW,
    "border": BORDER,
    "border-strong": BORDER_STRONG,
    "border-hover": "#ff8a73",
    "text-secondary": TEXT2,
    "text-tertiary": TEXT3,
    "text-inverse-muted": "#b9b8b4",
    "surface": SURFACE,
    "surface-alt": SURFACE_ALT,
    "surface-dark": SURFACE_DARK,
    "accent-hover": "#e92f16",
    "accent-active": "#c92510",
    "accent-wash": "#fff0ec",
}
BRAND_DISTANCE = 24  # RGB euclidean tolerance

# Extra swatches for the icon ladder
WIN_TASKBAR = "#202020"
MAC_DOCK = "#e8e8e8"

# ---------------------------------------------------------------- asset list
ICONS = ROOT / "frontend" / "src-tauri" / "icons"
INSTALLER = ROOT / "frontend" / "src-tauri" / "installer"
PUBLIC = ROOT / "frontend" / "public"
FAVICON = ROOT / "frontend" / "src" / "app" / "favicon.ico"


@dataclass
class Asset:
    key: str
    path: Path
    expected: Optional[tuple[int, int]] = None
    image: Optional[Image.Image] = None
    sha: str = ""
    size: Optional[tuple[int, int]] = None
    colours: list = field(default_factory=list)  # (hex, pct, purple, offbrand)
    purple: bool = False
    offbrand: bool = False
    state: str = "missing"  # missing | old | new

    @property
    def exists(self) -> bool:
        return self.image is not None

    @property
    def rel(self) -> str:
        return self.path.relative_to(ROOT).as_posix()


ASSETS: list[Asset] = [
    Asset("icon.png", ICONS / "icon.png"),
    Asset("icon.ico", ICONS / "icon.ico"),
    Asset("icon.icns", ICONS / "icon.icns"),
    Asset("32x32.png", ICONS / "32x32.png", (32, 32)),
    Asset("128x128.png", ICONS / "128x128.png", (128, 128)),
    Asset("128x128@2x.png", ICONS / "128x128@2x.png", (256, 256)),
    Asset("Square30x30Logo.png", ICONS / "Square30x30Logo.png", (30, 30)),
    Asset("Square44x44Logo.png", ICONS / "Square44x44Logo.png", (44, 44)),
    Asset("Square71x71Logo.png", ICONS / "Square71x71Logo.png", (71, 71)),
    Asset("Square89x89Logo.png", ICONS / "Square89x89Logo.png", (89, 89)),
    Asset("Square107x107Logo.png", ICONS / "Square107x107Logo.png", (107, 107)),
    Asset("Square142x142Logo.png", ICONS / "Square142x142Logo.png", (142, 142)),
    Asset("Square150x150Logo.png", ICONS / "Square150x150Logo.png", (150, 150)),
    Asset("Square284x284Logo.png", ICONS / "Square284x284Logo.png", (284, 284)),
    Asset("Square310x310Logo.png", ICONS / "Square310x310Logo.png", (310, 310)),
    Asset("StoreLogo.png", ICONS / "StoreLogo.png", (50, 50)),
    Asset("nsis-header.bmp", INSTALLER / "nsis-header.bmp", (150, 57)),
    Asset("nsis-sidebar.bmp", INSTALLER / "nsis-sidebar.bmp", (164, 314)),
    Asset("wix-banner.bmp", INSTALLER / "wix-banner.bmp", (493, 58)),
    Asset("wix-dialog.bmp", INSTALLER / "wix-dialog.bmp", (493, 312)),
    Asset("dmg-background.png", INSTALLER / "dmg-background.png", (660, 400)),
    Asset("dmg-background@2x.png", INSTALLER / "dmg-background@2x.png", (1320, 800)),
    Asset("logo.png", PUBLIC / "logo.png"),
    Asset("logo-collapsed.png", PUBLIC / "logo-collapsed.png"),
    Asset("icon_128x128.png", PUBLIC / "icon_128x128.png", (128, 128)),
    Asset("icon_32x32@2x.png", PUBLIC / "icon_32x32@2x.png", (64, 64)),
    Asset("favicon.ico", FAVICON),
]
BY_KEY = {a.key: a for a in ASSETS}


# ---------------------------------------------------------------- helpers
def hex_to_rgb(h: str) -> tuple[int, int, int]:
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))  # type: ignore


def rgb_to_hex(rgb) -> str:
    return "#%02x%02x%02x" % tuple(rgb[:3])


def rel_lum(rgb) -> float:
    def ch(c):
        c = c / 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (ch(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(fg: str, bg: str) -> float:
    l1, l2 = rel_lum(hex_to_rgb(fg)), rel_lum(hex_to_rgb(bg))
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def rgb_dist(a, b) -> float:
    return sum((x - y) ** 2 for x, y in zip(a, b)) ** 0.5


def dist_to_segment(p, a, b) -> float:
    """Distance from colour p to the straight RGB blend between a and b."""
    ab = [y - x for x, y in zip(a, b)]
    ap = [y - x for x, y in zip(a, p)]
    denom = sum(v * v for v in ab)
    if denom == 0:
        return rgb_dist(p, a)
    t = max(0.0, min(1.0, sum(x * y for x, y in zip(ap, ab)) / denom))
    q = [x + t * v for x, v in zip(a, ab)]
    return rgb_dist(p, q)


_BRAND_RGB = [hex_to_rgb(v) for v in BRAND_TOKENS.values()]


def is_on_brand(rgb) -> bool:
    for t in _BRAND_RGB:
        if rgb_dist(rgb, t) < BRAND_DISTANCE:
            return True
    for i, a in enumerate(_BRAND_RGB):
        for b in _BRAND_RGB[i + 1 :]:
            if dist_to_segment(rgb, a, b) < BRAND_DISTANCE:
                return True
    return False


def is_purple(rgb) -> bool:
    r, g, b = (c / 255 for c in rgb)
    h, s, v = colorsys.rgb_to_hsv(r, g, b)
    # Value guard: near-black anti-aliasing bins (for example #04040c) have a
    # violet hue by accident and are not the legacy purple.
    return 240 <= h * 360 <= 300 and s > 0.3 and v > 0.2


def sha256_prefix(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()[:8]


def git_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, text=True
        ).strip()
    except Exception:  # noqa: BLE001
        return "unknown"


def load_asset(a: Asset) -> None:
    if not a.path.exists():
        a.state = "missing"
        return
    a.sha = sha256_prefix(a.path)
    try:
        im = Image.open(a.path)
        if a.path.suffix.lower() == ".ico":
            sizes = sorted(im.ico.sizes())  # type: ignore[attr-defined]
            im.size = sizes[-1]
        im.load()
        a.image = im.convert("RGBA")
        a.size = a.image.size
    except Exception as exc:  # noqa: BLE001
        print(f"warn: cannot decode {a.rel}: {exc}", file=sys.stderr)
        a.state = "missing"
        return
    audit_colours(a)
    a.state = "old" if a.purple else "new"


def audit_colours(a: Asset) -> None:
    im = a.image
    if im.width * im.height > 512 * 512:
        im = im.copy()
        im.thumbnail((512, 512), Image.Resampling.BOX)
    counter: Counter = Counter()
    px = list(zip(*(ch.tobytes() for ch in im.split())))
    for r, g, b, al in px:
        if al < 128:
            continue
        counter[(r >> 3, g >> 3, b >> 3)] += 1
    total = sum(counter.values()) or 1
    rows = []
    for (r, g, b), n in counter.most_common(5):
        rgb = (min(255, r * 8 + 4), min(255, g * 8 + 4), min(255, b * 8 + 4))
        purple = is_purple(rgb)
        off = not is_on_brand(rgb)
        rows.append((rgb_to_hex(rgb), 100 * n / total, purple, off))
        a.purple |= purple
        a.offbrand |= off
    a.colours = rows


def ico_frame(a: Asset, size: int) -> Optional[Image.Image]:
    if not a.exists:
        return None
    try:
        im = Image.open(a.path)
        sizes = im.ico.sizes()  # type: ignore[attr-defined]
        if (size, size) not in sizes:
            return None
        im.size = (size, size)
        im.load()
        return im.convert("RGBA")
    except Exception:  # noqa: BLE001
        return None


def icon_at(size: int) -> tuple[Optional[Image.Image], str]:
    """Return the icon rendered at size, plus a note on which source was used."""
    if size <= 48:
        fr = ico_frame(BY_KEY["icon.ico"], size)
        if fr is not None:
            return fr, f"icon.ico {size}px frame"
    # Prefer an exact-size PNG, then the smallest larger one.
    candidates = [
        BY_KEY["32x32.png"],
        BY_KEY["128x128.png"],
        BY_KEY["128x128@2x.png"],
        BY_KEY["icon.png"],
    ]
    exact = [c for c in candidates if c.exists and c.size == (size, size)]
    if exact:
        return exact[0].image, exact[0].key
    bigger = sorted(
        (c for c in candidates if c.exists and c.size and c.size[0] >= size),
        key=lambda c: c.size[0],
    )
    if bigger:
        src = bigger[0]
        return (
            src.image.resize((size, size), Image.Resampling.LANCZOS),
            f"{src.key} scaled",
        )
    fr = ico_frame(BY_KEY["icon.ico"], 256)
    if fr is not None:
        return fr.resize((size, size), Image.Resampling.LANCZOS), "icon.ico 256 scaled"
    return None, "missing"


# ---------------------------------------------------------------- fonts
def find_font() -> tuple[Optional[Path], bool]:
    """Return (path, is_variable) for Archivo, else a fallback."""
    for base in (ROOT / "scripts" / "brand" / "fonts", ROOT / "frontend" / "node_modules"):
        if base.exists():
            hits = sorted(base.rglob("Archivo*.ttf"))
            if hits:
                p = hits[0]
                return p, "[" in p.name
    for cand in (
        Path("C:/Windows/Fonts/DejaVuSans.ttf"),
        Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        Path("C:/Windows/Fonts/arial.ttf"),
    ):
        if cand.exists():
            return cand, False
    return None, False


FONT_PATH, FONT_VARIABLE = find_font()
_font_cache: dict = {}


def font(size: int, weight: int = 400) -> ImageFont.FreeTypeFont:
    key = (size, weight)
    if key in _font_cache:
        return _font_cache[key]
    if FONT_PATH is None:
        f = ImageFont.load_default(size)
    else:
        f = ImageFont.truetype(str(FONT_PATH), size)
        if FONT_VARIABLE:
            try:
                f.set_variation_by_axes([weight, 100])
            except Exception:  # noqa: BLE001
                pass
    _font_cache[key] = f
    return f


# ---------------------------------------------------------------- canvas
W = 1600
M = 40  # outer margin
GAP = 24


class Sheet:
    def __init__(self, height: int):
        self.im = Image.new("RGBA", (W, height), READOUT)
        self.d = ImageDraw.Draw(self.im)

    # text -------------------------------------------------------------
    def label(self, x, y, text, colour=TEXT2, tracking=1.4, size=10):
        """10px 600 uppercase tracked label."""
        f = font(size, 600)
        text = text.upper()
        cx = x
        for ch in text:
            self.d.text((cx, y), ch, font=f, fill=colour)
            cx += f.getlength(ch) + tracking
        return cx - x

    def label_width(self, text, tracking=1.4, size=10):
        f = font(size, 600)
        return sum(f.getlength(c) + tracking for c in text.upper()) - tracking

    def text(self, x, y, s, size=12, weight=400, colour=BLACKOUT, anchor="la"):
        self.d.text((x, y), s, font=font(size, weight), fill=colour, anchor=anchor)

    def text_w(self, s, size=12, weight=400):
        return font(size, weight).getlength(s)

    def h2(self, x, y, s):
        self.d.text((x, y), s, font=font(21, 500), fill=BLACKOUT)

    def rule(self, x, y, w, colour=BORDER):
        self.d.line([(x, y), (x + w, y)], fill=colour, width=1)

    def section(self, y, num, title, note=""):
        self.d.rectangle([M, y, M + 24, y + 1], fill=HOT)
        self.label(M, y + 8, f"{num:02d}")
        self.h2(M + 36, y + 2, title)
        if note:
            self.text(M + 36, y + 32, note, 12, 400, TEXT2)
        return y + 60

    # boxes -----------------------------------------------------------
    def box(self, x, y, w, h, fill=None, outline=BORDER):
        self.d.rectangle([x, y, x + w - 1, y + h - 1], fill=fill, outline=outline)

    def paste(self, im: Image.Image, x, y):
        self.im.alpha_composite(im, (int(x), int(y)))

    def missing(self, x, y, w, h, name=""):
        self.box(x, y, w, h, fill=SURFACE_ALT, outline=BORDER_STRONG)
        self.d.line([(x, y), (x + w - 1, y + h - 1)], fill=BORDER_STRONG)
        self.d.line([(x + w - 1, y), (x, y + h - 1)], fill=BORDER_STRONG)
        lab = "missing"
        lw = self.label_width(lab)
        if w >= lw + 8 and h >= 16:
            self.d.rectangle(
                [x + w / 2 - lw / 2 - 4, y + h / 2 - 8, x + w / 2 + lw / 2 + 4, y + h / 2 + 8],
                fill=SURFACE_ALT,
            )
            self.label(x + w / 2 - lw / 2, y + h / 2 - 6, lab, ERROR)

    def surface(self, a: Asset, x, y, caption):
        """Draw an asset at 1:1 with a labelled outline."""
        w, h = a.expected or (a.size or (120, 80))
        if a.exists:
            self.paste(a.image, x, y)
            self.box(x - 1, y - 1, a.image.width + 2, a.image.height + 2)
            dims = f"{a.image.width}x{a.image.height}"
            if a.expected and a.size != a.expected:
                dims += f"  expected {w}x{h}"
                col = ERROR
            else:
                col = TEXT2
        else:
            self.missing(x, y, w, h, a.key)
            dims = f"expected {w}x{h}"
            col = ERROR
        self.label(x, y + h + 8, caption)
        self.text(x, y + h + 22, f"{a.key}  {dims}", 12, 500, col)
        return h + 40


# ---------------------------------------------------------------- sections
LADDER = [16, 20, 24, 32, 48, 64, 128, 256]


def draw_icon_ladder(s: Sheet, y: int) -> int:
    y = s.section(
        y,
        1,
        "Icon ladder",
        "icon.ico frames for 16 to 48 px, PNGs for 64 px and up. Judge silhouette and glyph legibility at 16 and 20.",
    )
    swatches = [
        ("Readout  #f7f6f2", READOUT, BLACKOUT),
        ("Blackout  #0b0b0c", BLACKOUT, READOUT),
        ("Windows taskbar dark  #202020", WIN_TASKBAR, READOUT),
        ("macOS dock light  #e8e8e8", MAC_DOCK, BLACKOUT),
    ]
    pw = (W - 2 * M - GAP) // 2
    ph = 316
    inner = 20
    sources = []
    for i, (name, bg, fg) in enumerate(swatches):
        px = M + (i % 2) * (pw + GAP)
        py = y + (i // 2) * (ph + GAP + 20)
        s.box(px, py, pw, ph, fill=bg)
        s.label(px + inner, py + inner - 4, name, fg if bg in (BLACKOUT, WIN_TASKBAR) else TEXT2)
        x = px + inner
        base = py + ph - inner - 14
        for size in LADDER:
            im, src = icon_at(size)
            if i == 0:
                sources.append((size, src))
            if im is not None:
                s.paste(im, x, base - size)
            else:
                s.missing(x, base - size, size, size)
            s.label(x, base + 6, str(size), fg, tracking=1.0)
            x += size + 16
    y += 2 * (ph + GAP + 20)
    # source notes
    s.label(M, y, "source per size")
    x = M
    for size, src in sources:
        col = ERROR if src == "missing" else TEXT2
        s.text(x, y + 14, f"{size}: {src}", 12, 500, col)
        x += s.text_w(f"{size}: {src}", 12, 500) + 24
    return y + 44


def draw_surfaces(s: Sheet, y: int) -> int:
    y = s.section(y, 2, "Installer surfaces at 1:1", "Pixel-exact. The outline is the expected bitmap bound, not part of the asset.")
    x = M
    h1 = s.surface(BY_KEY["nsis-header.bmp"], x, y, "NSIS header")
    x2 = x
    s.surface(BY_KEY["nsis-sidebar.bmp"], x2, y + h1 + 16, "NSIS sidebar")
    x = M + 240
    hb = s.surface(BY_KEY["wix-banner.bmp"], x, y, "WiX banner")
    s.surface(BY_KEY["wix-dialog.bmp"], x, y + hb + 16, "WiX dialog")
    x = M + 240 + 493 + GAP + 20
    s.surface(BY_KEY["dmg-background.png"], x, y, "DMG background (1x)")
    return y + 400 + 44 + 12


def win11_frame(s: Sheet, x, y, cw, ch, title):
    """Windows 11 style frame: 32px title bar, client area cw x ch."""
    th = 32
    s.box(x, y, cw + 2, ch + th + 2, fill=SURFACE, outline=BORDER_STRONG)
    s.text(x + 12, y + 10, title, 12, 400, BLACKOUT)
    # caption buttons
    bx = x + cw - 46 * 3 + 2
    for i, glyph in enumerate(("min", "max", "close")):
        cx = bx + i * 46 + 23
        cy = y + th // 2
        if glyph == "min":
            s.d.line([(cx - 5, cy), (cx + 5, cy)], fill=BLACKOUT)
        elif glyph == "max":
            s.d.rectangle([cx - 5, cy - 5, cx + 5, cy + 5], outline=BLACKOUT)
        else:
            s.d.line([(cx - 5, cy - 5), (cx + 5, cy + 5)], fill=BLACKOUT)
            s.d.line([(cx - 5, cy + 5), (cx + 5, cy - 5)], fill=BLACKOUT)
    return x + 1, y + th + 1


def nsis_footer(s: Sheet, cx, cy, cw, ch, buttons):
    fy = cy + ch - 46
    s.rule(cx, fy, cw, BORDER)
    bx = cx + cw - 12
    for lab in reversed(buttons):
        bw = 76
        bx -= bw
        s.box(bx, fy + 11, bw, 24, fill=SURFACE_ALT, outline=BORDER_STRONG)
        s.text(bx + bw / 2, fy + 23, lab, 12, 400, BLACKOUT, anchor="mm")
        bx -= 8


def draw_mockups(s: Sheet, y: int) -> int:
    y = s.section(y, 3, "In-context mockups", "Simplified frames. NSIS Modern UI 2 places the sidebar on the welcome page and the header bitmap top-right on inner pages.")
    cw, ch = 503, 392
    # Welcome page with sidebar
    cx, cy = win11_frame(s, M, y, cw, ch, "PulseTalq Setup")
    sb = BY_KEY["nsis-sidebar.bmp"]
    if sb.exists:
        s.paste(sb.image, cx, cy)
    else:
        s.missing(cx, cy, 164, 314)
    s.text(cx + 164 + 20, cy + 20, "Welcome to PulseTalq Setup", 16, 500, BLACKOUT)
    for i, line in enumerate(
        (
            "Setup will guide you through the installation",
            "of PulseTalq.",
            "",
            "Audio stays on this device.",
        )
    ):
        s.text(cx + 164 + 20, cy + 56 + i * 18, line, 12, 400, TEXT2)
    nsis_footer(s, cx, cy, cw, ch, ["Next >", "Cancel"])
    s.label(M, y + ch + 34 + 10, "nsis welcome page  sidebar 164x314 at 0,0")

    # Inner page with header
    x2 = M + cw + 2 + GAP
    cx, cy = win11_frame(s, x2, y, cw, ch, "PulseTalq Setup")
    s.box(cx, cy, cw, 57, fill=SURFACE, outline=None)
    hd = BY_KEY["nsis-header.bmp"]
    if hd.exists:
        s.paste(hd.image, cx + cw - 150, cy)
    else:
        s.missing(cx + cw - 150, cy, 150, 57)
    s.text(cx + 16, cy + 12, "Choose Install Location", 12, 500, BLACKOUT)
    s.text(cx + 16, cy + 30, "Choose the folder in which to install PulseTalq.", 11, 400, TEXT2)
    s.rule(cx, cy + 57, cw, BORDER)
    s.text(cx + 20, cy + 80, "Destination Folder", 12, 400, TEXT2)
    s.box(cx + 20, cy + 100, cw - 40 - 90, 26, fill=SURFACE, outline=BORDER_STRONG)
    s.text(cx + 28, cy + 113, "C:\\Program Files\\PulseTalq", 12, 400, BLACKOUT, anchor="lm")
    s.box(cx + cw - 20 - 84, cy + 100, 84, 26, fill=SURFACE_ALT, outline=BORDER_STRONG)
    s.text(cx + cw - 20 - 42, cy + 113, "Browse...", 12, 400, BLACKOUT, anchor="mm")
    nsis_footer(s, cx, cy, cw, ch, ["< Back", "Install", "Cancel"])
    s.label(x2, y + ch + 34 + 10, "nsis inner page  header 150x57 top right")

    y += ch + 34 + 40

    # DMG Finder window
    fw, fh = 660, 400
    tb = 40
    fx = M
    s.box(fx, y, fw + 2, fh + tb + 2, fill=SURFACE, outline=BORDER_STRONG)
    s.box(fx + 1, y + 1, fw, tb, fill=SURFACE_ALT, outline=None)
    for i, c in enumerate((BORDER_STRONG, BORDER_STRONG, BORDER_STRONG)):
        s.d.ellipse([fx + 14 + i * 20, y + 14, fx + 26 + i * 20, y + 26], fill=c)
    s.text(fx + 1 + fw / 2, y + 1 + tb / 2, "PulseTalq", 12, 500, BLACKOUT, anchor="mm")
    bgx, bgy = fx + 1, y + tb + 1
    bg = BY_KEY["dmg-background.png"]
    if bg.exists:
        im = bg.image
        if im.size != (fw, fh):
            im = im.resize((fw, fh), Image.Resampling.LANCZOS)
        s.paste(im, bgx, bgy)
    else:
        s.missing(bgx, bgy, fw, fh)
    # app icon at 180,170 centre; Applications at 480,170 centre (tauri defaults)
    app_c = (180, 170)
    apps_c = (480, 170)
    ic, _ = icon_at(128)
    if ic is not None:
        s.paste(ic, bgx + app_c[0] - 64, bgy + app_c[1] - 64)
    else:
        s.missing(bgx + app_c[0] - 64, bgy + app_c[1] - 64, 128, 128)
    # generic folder glyph
    gx, gy = bgx + apps_c[0] - 56, bgy + apps_c[1] - 44
    s.d.rounded_rectangle([gx, gy + 10, gx + 112, gy + 88], radius=6, fill="#9fb3c8", outline="#7f93a8")
    s.d.rounded_rectangle([gx, gy, gx + 46, gy + 20], radius=4, fill="#9fb3c8", outline="#7f93a8")
    s.d.rounded_rectangle([gx, gy + 22, gx + 112, gy + 88], radius=6, fill="#b7c8da", outline="#7f93a8")
    # arrow between, only when the background (which owns the arrow) is missing
    if not bg.exists:
      s.d.line([(bgx + app_c[0] + 80, bgy + app_c[1]), (bgx + apps_c[0] - 80, bgy + apps_c[1])], fill=TEXT2, width=2)
      s.d.polygon(
        [
            (bgx + apps_c[0] - 80, bgy + apps_c[1]),
            (bgx + apps_c[0] - 92, bgy + apps_c[1] - 6),
            (bgx + apps_c[0] - 92, bgy + apps_c[1] + 6),
        ],
        fill=TEXT2,
      )
    for cxx, lab, fg in ((app_c[0], "PulseTalq", BLACKOUT), (apps_c[0], "Applications", BLACKOUT)):
        f = font(12, 500)
        tw = f.getlength(lab)
        s.d.rounded_rectangle(
            [bgx + cxx - tw / 2 - 6, bgy + 170 + 74, bgx + cxx + tw / 2 + 6, bgy + 170 + 92],
            radius=3,
            fill=(255, 255, 255, 200),
        )
        s.text(bgx + cxx, bgy + 170 + 83, lab, 12, 500, fg, anchor="mm")
    s.label(fx, y + fh + tb + 12, "dmg finder window 660x400  app icon at 180,170  applications at 480,170")

    # Note column to the right of DMG
    nx = fx + fw + 2 + GAP + 16
    s.label(nx, y, "what to check")
    checks = [
        "Sidebar bitmap fills 164x314 with no seam against the white page.",
        "Header bitmap reads at 150x57 with the page title to its left.",
        "DMG background stays legible under a 128 px icon and a folder.",
        "Icon silhouette remains distinct against the folder glyph.",
        "No purple or violet survives anywhere on this sheet.",
        "Bitmaps are 24 bit BMP without alpha (NSIS and WiX reject alpha).",
    ]
    for i, c in enumerate(checks):
        s.d.rectangle([nx, y + 20 + i * 22 + 5, nx + 5, y + 20 + i * 22 + 10], fill=TEXT2)
        s.text(nx + 14, y + 20 + i * 22, c, 12, 400, TEXT2)
    return y + fh + tb + 40


def draw_colour_audit(s: Sheet, y: int, x: int, width: int) -> int:
    y0 = y
    s.label(x, y, "colour audit  top 5 by pixel count, quantised to 32 levels")
    y += 18
    s.text(x, y, "Flags: violet hue 240 to 300 with saturation above 0.3 (legacy purple), and any colour farther than 24 RGB from every brand token or two-token blend.", 11, 400, TEXT3)
    y += 22
    row_h = 40
    name_w = 200
    sw = 60
    for a in ASSETS:
        s.rule(x, y, width, BORDER)
        s.text(x, y + 9, a.key, 12, 500, BLACKOUT)
        state_col = {"missing": ERROR, "old": ERROR, "new": SUCCESS}[a.state]
        s.label(x, y + 25, a.state, state_col, tracking=1.0)
        cx = x + name_w
        if not a.exists:
            s.text(cx, y + 13, "no file on disk", 12, 400, TEXT3)
        for hx, pct, purple, off in a.colours:
            s.box(cx, y + 6, sw, 16, fill=hx, outline=BORDER)
            flag_col = ERROR if (purple or off) else TEXT2
            s.text(cx, y + 26, f"{hx} {pct:.0f}%", 10, 500, flag_col)
            if purple:
                s.d.rectangle([cx + sw - 8, y + 6, cx + sw - 1, y + 13], fill=ERROR)
            elif off:
                s.d.rectangle([cx + sw - 8, y + 6, cx + sw - 1, y + 13], fill=WARNING)
            cx += sw + 40
        # asset level flags
        fx = x + name_w + 5 * (sw + 40) + 8
        flags = []
        if a.purple:
            flags.append(("purple", ERROR))
        if a.offbrand:
            flags.append(("off-brand", ERROR if a.purple else WARNING))
        if a.exists and a.expected and a.size != a.expected:
            flags.append(("size", ERROR))
        if a.exists and not flags:
            flags.append(("ok", SUCCESS))
        for lab, col in flags:
            lw = s.label_width(lab, 1.0)
            s.box(fx, y + 8, lw + 12, 18, fill=None, outline=col)
            s.label(fx + 6, y + 12, lab, col, tracking=1.0)
            fx += lw + 20
        y += row_h
    s.rule(x, y, width, BORDER)
    y += 12
    s.text(x, y, "Corner mark on a swatch: error = violet, warning = outside the brand token set.", 11, 400, TEXT3)
    return y + 20


CONTRAST_PAIRS = [
    ("Hot Signal on Blackout", HOT, BLACKOUT),
    ("Readout on Blackout", READOUT, BLACKOUT),
    ("Blackout on Readout", BLACKOUT, READOUT),
    ("Blackout on Hot Signal", BLACKOUT, HOT),
    ("Text secondary on Readout", TEXT2, READOUT),
]


def draw_contrast(s: Sheet, y: int, x: int, width: int) -> int:
    s.label(x, y, "wcag contrast  single series, 4.5 reference")
    y += 18
    s.text(x, y, "Bars use a neutral mark; the value label carries the number.", 11, 400, TEXT3)
    s.text(x, y + 14, "Any pair under 4.5 must pair colour with shape or text (never rely on red alone).", 11, 400, TEXT3)
    y += 40
    label_w = 190
    plot_x = x + label_w
    plot_w = width - label_w - 60
    maxv = 21.0
    row = 34
    bar_h = 18
    # gridlines and axis ticks
    for v in (0, 5, 10, 15, 20):
        gx = plot_x + plot_w * v / maxv
        s.d.line([(gx, y), (gx, y + row * len(CONTRAST_PAIRS))], fill=BORDER, width=1)
        s.text(gx, y + row * len(CONTRAST_PAIRS) + 6, f"{v}", 10, 500, TEXT3, anchor="ma")
    # reference line 4.5
    rx = plot_x + plot_w * 4.5 / maxv
    s.d.line([(rx, y - 6), (rx, y + row * len(CONTRAST_PAIRS))], fill=BLACKOUT, width=1)
    s.label(rx + 6, y - 14, "4.5 aa", BLACKOUT, tracking=1.0)
    for i, (name, fg, bg) in enumerate(CONTRAST_PAIRS):
        ratio = contrast(fg, bg)
        by = y + i * row + (row - bar_h) // 2
        s.text(x, by + bar_h / 2, name, 12, 400, BLACKOUT, anchor="lm")
        bw = max(4, int(plot_w * ratio / maxv))
        # neutral mark, rounded data end, square at baseline
        s.d.rounded_rectangle([plot_x, by, plot_x + bw, by + bar_h], radius=4, fill=TEXT2)
        s.d.rectangle([plot_x, by, plot_x + 4, by + bar_h], fill=TEXT2)
        s.text(plot_x + bw + 8, by + bar_h / 2, f"{ratio:.2f}:1", 12, 500, BLACKOUT, anchor="lm")
        # pair swatch showing the actual fg/bg
        s.box(x + label_w - 32, by, 24, bar_h, fill=bg, outline=BORDER)
        s.d.rectangle([x + label_w - 26, by + 5, x + label_w - 14, by + bar_h - 5], fill=fg)
        if ratio < 4.5:
            s.text(plot_x + bw + 8 + s.text_w(f"{ratio:.2f}:1", 12, 500) + 8, by + bar_h / 2, "below AA, shape required", 10, 500, ERROR, anchor="lm")
    return y + row * len(CONTRAST_PAIRS) + 30


def draw_footer(s: Sheet, y: int, sha: str, stamp: str) -> int:
    s.rule(M, y, W - 2 * M, BORDER_STRONG)
    y += 14
    s.label(M, y, "generated")
    s.text(M, y + 14, f"{stamp}   git {sha}   font {FONT_PATH.name if FONT_PATH else 'default'}", 12, 500, BLACKOUT)
    y += 44
    cols = 3
    per = (len(ASSETS) + cols - 1) // cols
    col_w = (W - 2 * M - GAP * (cols - 1)) // cols
    for c in range(cols):
        cx = M + c * (col_w + GAP)
        s.label(cx, y, "file")
        s.label(cx + 250, y, "sha-256")
        s.label(cx + 340, y, "size")
        s.rule(cx, y + 14, col_w, BORDER)
        for i, a in enumerate(ASSETS[c * per : (c + 1) * per]):
            ry = y + 20 + i * 18
            s.text(cx, ry, a.key, 11, 400, BLACKOUT)
            s.text(cx + 250, ry, a.sha or "missing", 11, 500, TEXT2 if a.sha else ERROR)
            s.text(cx + 340, ry, f"{a.size[0]}x{a.size[1]}" if a.size else "", 11, 500, TEXT2)
    return y + 20 + per * 18 + M


# ---------------------------------------------------------------- HTML twin
def build_html(sha: str, stamp: str) -> str:
    def rel(a: Asset) -> str:
        return "../" + a.rel

    def esc(x) -> str:
        return html.escape(str(x))

    ladder = ""
    for bg_name, bg_class in (("Readout", "readout"), ("Blackout", "blackout"), ("Windows taskbar dark", "taskbar"), ("macOS dock light", "dock")):
        cells = ""
        for size in LADDER:
            a = BY_KEY["icon.png"]
            if a.exists:
                cells += f'<figure><img src="{rel(a)}" width="{size}" height="{size}" alt="icon {size}px"><figcaption>{size}</figcaption></figure>'
            else:
                cells += f'<figure><span class="missing" style="width:{size}px;height:{size}px"></span><figcaption>{size}</figcaption></figure>'
        ladder += f'<div class="swatch {bg_class}"><span class="label">{bg_name}</span><div class="row">{cells}</div></div>'

    def surface(key, caption):
        a = BY_KEY[key]
        w, h = a.expected or (a.size or (120, 80))
        if a.exists:
            img = f'<img src="{rel(a)}" width="{a.size[0]}" height="{a.size[1]}" alt="{esc(caption)}">'
            dims = f"{a.size[0]}x{a.size[1]}"
            bad = a.expected and a.size != a.expected
        else:
            img = f'<span class="missing" style="width:{w}px;height:{h}px">missing</span>'
            dims = f"expected {w}x{h}"
            bad = True
        return f'<figure class="surface">{img}<figcaption><span class="label">{esc(caption)}</span><span class="data {"error" if bad else ""}">{esc(key)} {dims}</span></figcaption></figure>'

    surfaces = "".join(
        [
            surface("nsis-header.bmp", "NSIS header"),
            surface("nsis-sidebar.bmp", "NSIS sidebar"),
            surface("wix-banner.bmp", "WiX banner"),
            surface("wix-dialog.bmp", "WiX dialog"),
            surface("dmg-background.png", "DMG background (1x)"),
        ]
    )

    def img_or_missing(key, w, h, cls=""):
        a = BY_KEY[key]
        if a.exists:
            return f'<img class="{cls}" src="{rel(a)}" width="{w}" height="{h}" alt="{esc(key)}">'
        return f'<span class="missing {cls}" style="width:{w}px;height:{h}px">missing</span>'

    audit_rows = ""
    for a in ASSETS:
        sw = ""
        for hx, pct, purple, off in a.colours:
            flag = "purple" if purple else ("off" if off else "")
            sw += f'<span class="chip {flag}" title="{hx} {pct:.1f}%"><i style="background:{hx}"></i><span class="data{" error" if (purple or off) else ""}">{hx} {pct:.0f}%</span></span>'
        if not a.exists:
            sw = '<span class="data muted">no file on disk</span>'
        flags = []
        if a.purple:
            flags.append('<span class="flag error">purple</span>')
        if a.offbrand:
            flags.append(f'<span class="flag {"error" if a.purple else "warning"}">off-brand</span>')
        if a.exists and a.expected and a.size != a.expected:
            flags.append('<span class="flag error">size</span>')
        if a.exists and not flags:
            flags.append('<span class="flag success">ok</span>')
        audit_rows += f'<tr><th scope="row"><span>{esc(a.key)}</span><span class="label {"success" if a.state == "new" else "error"}">{a.state}</span></th><td>{sw}</td><td>{"".join(flags)}</td></tr>'

    # contrast SVG
    label_w, plot_w, row, bar_h, maxv = 200, 420, 34, 18, 21.0
    svg_h = row * len(CONTRAST_PAIRS) + 40
    parts = [f'<svg class="contrast" viewBox="0 0 {label_w + plot_w + 200} {svg_h}" width="{label_w + plot_w + 200}" height="{svg_h}" role="img" aria-labelledby="contrast-title">']
    parts.append('<title id="contrast-title">WCAG contrast ratio per brand pair, 4.5 reference</title>')
    for v in (0, 5, 10, 15, 20):
        gx = label_w + plot_w * v / maxv
        parts.append(f'<line class="grid" x1="{gx:.1f}" y1="14" x2="{gx:.1f}" y2="{14 + row * len(CONTRAST_PAIRS)}"/>')
        parts.append(f'<text class="tick" x="{gx:.1f}" y="{14 + row * len(CONTRAST_PAIRS) + 16}" text-anchor="middle">{v}</text>')
    rx = label_w + plot_w * 4.5 / maxv
    parts.append(f'<line class="ref" x1="{rx:.1f}" y1="6" x2="{rx:.1f}" y2="{14 + row * len(CONTRAST_PAIRS)}"/>')
    parts.append(f'<text class="label-svg" x="{rx + 6:.1f}" y="10">4.5 AA</text>')
    rows_table = ""
    for i, (name, fg, bg) in enumerate(CONTRAST_PAIRS):
        ratio = contrast(fg, bg)
        by = 14 + i * row + (row - bar_h) / 2
        bw = max(4, plot_w * ratio / maxv)
        parts.append(f'<g class="bar-row" tabindex="0"><title>{esc(name)}: {ratio:.2f}:1</title>')
        parts.append(f'<rect x="{label_w - 40}" y="{by}" width="24" height="{bar_h}" fill="{bg}" class="pair"/><rect x="{label_w - 34}" y="{by + 5}" width="12" height="{bar_h - 10}" fill="{fg}"/>')
        parts.append(f'<text class="name" x="0" y="{by + bar_h / 2 + 4}">{esc(name)}</text>')
        parts.append(f'<path class="bar" d="M{label_w},{by} h{bw - 4:.1f} a4,4 0 0 1 4,4 v{bar_h - 8} a4,4 0 0 1 -4,4 h-{bw - 4:.1f} z"/>')
        parts.append(f'<text class="value" x="{label_w + bw + 8:.1f}" y="{by + bar_h / 2 + 4}">{ratio:.2f}:1</text>')
        if ratio < 4.5:
            parts.append(f'<text class="value error" x="{label_w + bw + 70:.1f}" y="{by + bar_h / 2 + 4}">below AA, shape required</text>')
        parts.append("</g>")
        rows_table += f'<tr><td>{esc(name)}</td><td>{fg}</td><td>{bg}</td><td>{ratio:.2f}:1</td><td>{"pass" if ratio >= 4.5 else "fail, shape required"}</td></tr>'
    parts.append("</svg>")
    contrast_svg = "".join(parts)

    footer_rows = "".join(
        f'<tr><td>{esc(a.rel)}</td><td class="data">{a.sha or "missing"}</td><td class="data">{f"{a.size[0]}x{a.size[1]}" if a.size else ""}</td><td><span class="label {"success" if a.state == "new" else "error"}">{a.state}</span></td></tr>'
        for a in ASSETS
    )

    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>PulseTalq installer preview</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Archivo:wght@400;500;600&display=swap" rel="stylesheet">
<style>
:root {{
  color-scheme: light;
  --pt-bg: #f7f6f2; --pt-surface: #ffffff; --pt-surface-alt: #efede8; --pt-surface-hover: #fff0ec;
  --pt-surface-dark: #18191b; --pt-sidebar: #0b0b0c;
  --pt-border: #d2cfca; --pt-border-strong: #b7b3ad; --pt-border-hover: #ff8a73;
  --pt-text: #0b0b0c; --pt-text-secondary: #414042; --pt-text-tertiary: #6f6d6d;
  --pt-text-inverse: #f7f6f2; --pt-text-inverse-muted: #b9b8b4;
  --pt-accent: #ff3b1f; --pt-accent-hover: #e92f16; --pt-accent-active: #c92510;
  --pt-accent-soft: #ffb39f; --pt-accent-wash: #fff0ec;
  --pt-success: #237a57; --pt-error: #b42318; --pt-warning: #9a5b00; --pt-info: #365f91;
  --pt-font-ui: "Archivo", "Segoe UI", sans-serif;
  --mark: #414042;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    color-scheme: dark;
    --pt-bg: #0b0b0c; --pt-surface: #18191b; --pt-surface-alt: #222326;
    --pt-border: #2f3033; --pt-border-strong: #45464a;
    --pt-text: #f7f6f2; --pt-text-secondary: #b9b8b4; --pt-text-tertiary: #8b8a86;
    --pt-success: #4fae86; --pt-error: #f0705f; --pt-warning: #d99a3b; --pt-info: #7fa4d0;
    --mark: #b9b8b4;
  }}
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--pt-bg); color: var(--pt-text); font: 400 14px/1.5 var(--pt-font-ui); }}
.wrap {{ max-width: 1560px; margin: 0 auto; padding: 32px 40px 64px; }}
h1 {{ font-size: 30px; font-weight: 500; letter-spacing: -0.04em; line-height: 1.1; margin: 0 0 8px; }}
h1 i {{ font-style: normal; color: var(--pt-accent); }}
h2 {{ font-size: 21px; font-weight: 500; letter-spacing: -0.025em; margin: 0; }}
.meta {{ color: var(--pt-text-secondary); font-size: 12px; letter-spacing: 0.015em; }}
section {{ margin-top: 48px; }}
.section-head {{ display: flex; align-items: baseline; gap: 12px; padding-top: 8px; border-top: 2px solid var(--pt-accent); border-image: linear-gradient(to right, var(--pt-accent) 24px, transparent 24px) 1; margin-bottom: 16px; }}
.section-head p {{ margin: 0; color: var(--pt-text-secondary); font-size: 12px; }}
.label {{ font-size: 10px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--pt-text-secondary); line-height: 1.3; }}
.data {{ font-size: 12px; font-weight: 500; letter-spacing: 0.015em; color: var(--pt-text-secondary); }}
.error {{ color: var(--pt-error); }} .success {{ color: var(--pt-success); }} .warning {{ color: var(--pt-warning); }} .muted {{ color: var(--pt-text-tertiary); }}
.ladder {{ display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }}
.swatch {{ border: 1px solid var(--pt-border); padding: 16px 20px 12px; border-radius: 3px; }}
.swatch.readout {{ background: #f7f6f2; }} .swatch.readout .label {{ color: #414042; }}
.swatch.blackout {{ background: #0b0b0c; }} .swatch.blackout .label, .swatch.blackout figcaption {{ color: #f7f6f2; }}
.swatch.taskbar {{ background: #202020; }} .swatch.taskbar .label, .swatch.taskbar figcaption {{ color: #f7f6f2; }}
.swatch.dock {{ background: #e8e8e8; }} .swatch.dock .label, .swatch.dock figcaption {{ color: #0b0b0c; }}
.row {{ display: flex; align-items: flex-end; gap: 22px; margin-top: 12px; overflow-x: auto; }}
.row figure {{ margin: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 6px; }}
.row figcaption {{ font-size: 10px; font-weight: 600; letter-spacing: 0.1em; color: #414042; }}
.pixelated img {{ image-rendering: pixelated; }}
.toggle {{ display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--pt-text-secondary); margin-left: auto; }}
.toggle input {{ accent-color: var(--pt-accent); }}
.missing {{ display: inline-flex; align-items: center; justify-content: center; background: var(--pt-surface-alt); border: 1px solid var(--pt-border-strong); color: var(--pt-error); font-size: 10px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; background-image: linear-gradient(to top right, transparent calc(50% - 0.5px), var(--pt-border-strong) 50%, transparent calc(50% + 0.5px)); }}
.surfaces {{ display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }}
.surface {{ margin: 0; }}
.surface img, .surface .missing {{ display: block; outline: 1px solid var(--pt-border); outline-offset: 1px; }}
.surface figcaption {{ display: flex; flex-direction: column; gap: 2px; margin-top: 10px; }}
.mockups {{ display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }}
.win {{ width: 505px; background: #ffffff; color: #0b0b0c; border: 1px solid #b7b3ad; border-radius: 6px; overflow: hidden; }}
.win .title {{ height: 32px; display: flex; align-items: center; padding: 0 12px; font-size: 12px; gap: 8px; }}
.win .title span:last-child {{ margin-left: auto; letter-spacing: 12px; color: #0b0b0c; font-size: 12px; }}
.win .client {{ position: relative; width: 503px; height: 392px; }}
.win .client .sidebar {{ position: absolute; left: 0; top: 0; }}
.win .client .header {{ position: absolute; right: 0; top: 0; }}
.win .client .copy {{ position: absolute; left: 184px; top: 20px; right: 20px; color: #414042; font-size: 12px; }}
.win .client .copy b {{ display: block; color: #0b0b0c; font-weight: 500; font-size: 16px; margin-bottom: 14px; }}
.win .client .inner-title {{ position: absolute; left: 16px; top: 12px; font-size: 12px; color: #0b0b0c; }}
.win .client .inner-title b {{ font-weight: 500; display: block; }} .win .client .inner-title span {{ font-size: 11px; color: #414042; }}
.win .client .hr {{ position: absolute; left: 0; right: 0; top: 57px; border-top: 1px solid #d2cfca; }}
.win .client .field {{ position: absolute; left: 20px; top: 80px; right: 20px; font-size: 12px; color: #414042; }}
.win .client .field div {{ display: flex; gap: 8px; margin-top: 8px; }}
.win .client .field div span {{ flex: 1; border: 1px solid #b7b3ad; padding: 4px 8px; color: #0b0b0c; background: #fff; }}
.win .client .field div span:last-child {{ flex: 0 0 84px; text-align: center; background: #efede8; }}
.win .client .footer {{ position: absolute; left: 0; right: 0; bottom: 0; height: 46px; border-top: 1px solid #d2cfca; display: flex; justify-content: flex-end; align-items: center; gap: 8px; padding: 0 12px; }}
.win .client .footer span {{ width: 76px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: 1px solid #b7b3ad; background: #efede8; font-size: 12px; }}
.finder {{ width: 662px; background: #ffffff; border: 1px solid #b7b3ad; border-radius: 8px; overflow: hidden; }}
.finder .bar {{ height: 40px; background: #efede8; display: flex; align-items: center; justify-content: center; position: relative; font-size: 12px; font-weight: 500; color: #0b0b0c; }}
.finder .bar i {{ position: absolute; left: 14px; top: 14px; width: 12px; height: 12px; border-radius: 50%; background: #b7b3ad; box-shadow: 20px 0 0 #b7b3ad, 40px 0 0 #b7b3ad; }}
.finder .stage {{ position: relative; width: 660px; height: 400px; background: var(--pt-surface); }}
.finder .stage > img.bg, .finder .stage > .missing.bg {{ position: absolute; left: 0; top: 0; width: 660px; height: 400px; }}
.finder .app {{ position: absolute; left: 116px; top: 106px; width: 128px; height: 128px; }}
.finder .folder {{ position: absolute; left: 424px; top: 126px; width: 112px; height: 88px; }}
.finder .folder i {{ position: absolute; display: block; background: #b7c8da; border: 1px solid #7f93a8; border-radius: 6px; }}
.finder .folder i:nth-child(1) {{ left: 0; top: 0; width: 46px; height: 20px; border-radius: 4px; background: #9fb3c8; }}
.finder .folder i:nth-child(2) {{ left: 0; top: 10px; width: 112px; height: 78px; background: #9fb3c8; }}
.finder .folder i:nth-child(3) {{ left: 0; top: 22px; width: 112px; height: 66px; }}
.finder .cap {{ position: absolute; top: 244px; transform: translateX(-50%); background: rgba(255,255,255,.8); color: #0b0b0c; font-size: 12px; font-weight: 500; padding: 1px 6px; border-radius: 3px; }}
.finder .arrow {{ position: absolute; left: 260px; top: 169px; width: 140px; height: 2px; background: #414042; }}
.finder .arrow::after {{ content: ""; position: absolute; right: -2px; top: -5px; border: 6px solid transparent; border-left: 12px solid #414042; }}
.checks {{ list-style: none; padding: 0; margin: 0; max-width: 460px; }}
.checks li {{ position: relative; padding-left: 16px; font-size: 12px; color: var(--pt-text-secondary); margin-bottom: 8px; }}
.checks li::before {{ content: ""; position: absolute; left: 0; top: 7px; width: 5px; height: 5px; background: var(--pt-text-secondary); }}
table {{ border-collapse: collapse; width: 100%; font-size: 12px; }}
th, td {{ text-align: left; padding: 8px 8px 8px 0; border-top: 1px solid var(--pt-border); vertical-align: middle; }}
thead th {{ border-top: 0; }}
th[scope=row] {{ font-weight: 500; white-space: nowrap; }} th[scope=row] span {{ display: block; }}
.chip {{ display: inline-flex; flex-direction: column; gap: 4px; margin-right: 24px; }}
.chip i {{ display: block; width: 60px; height: 16px; border: 1px solid var(--pt-border); position: relative; }}
.chip.purple i::after, .chip.off i::after {{ content: ""; position: absolute; right: 0; top: 0; width: 8px; height: 8px; background: var(--pt-warning); }}
.chip.purple i::after {{ background: var(--pt-error); }}
.chip .data {{ font-size: 10px; }}
.flag {{ display: inline-block; border: 1px solid currentColor; padding: 2px 6px; margin-right: 8px; font-size: 10px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; }}
.audit-grid {{ display: grid; grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr); gap: 40px; align-items: start; }}
svg.contrast {{ max-width: 100%; height: auto; font-family: var(--pt-font-ui); }}
svg.contrast .grid {{ stroke: var(--pt-border); stroke-width: 1; }}
svg.contrast .ref {{ stroke: var(--pt-text); stroke-width: 1; }}
svg.contrast .bar {{ fill: var(--mark); }}
svg.contrast .bar-row:hover .bar, svg.contrast .bar-row:focus .bar {{ fill: var(--pt-text); }}
svg.contrast .bar-row:focus {{ outline: none; }}
svg.contrast .name {{ font-size: 12px; fill: var(--pt-text); }}
svg.contrast .value {{ font-size: 12px; font-weight: 500; fill: var(--pt-text); }}
svg.contrast .value.error {{ fill: var(--pt-error); font-size: 10px; font-weight: 600; }}
svg.contrast .tick {{ font-size: 10px; font-weight: 500; fill: var(--pt-text-tertiary); }}
svg.contrast .label-svg {{ font-size: 10px; font-weight: 600; letter-spacing: 0.14em; fill: var(--pt-text); }}
svg.contrast .pair {{ stroke: var(--pt-border); }}
details summary {{ cursor: pointer; font-size: 12px; color: var(--pt-text-secondary); margin-top: 12px; }}
footer {{ margin-top: 48px; border-top: 1px solid var(--pt-border-strong); padding-top: 16px; }}
@media (max-width: 1100px) {{ .ladder, .audit-grid {{ grid-template-columns: 1fr; }} }}
</style>
</head>
<body>
<div class="wrap">
<header>
  <span class="label">Deep Focus review sheet</span>
  <h1>pulse <i>talq</i> installer preview</h1>
  <p class="meta">Generated {esc(stamp)} at git {esc(sha)}. Images are the live files under frontend/, loaded by relative path. Regenerate with <code>python scripts/brand/render-installer-preview.py</code>.</p>
</header>

<section id="ladder">
  <div class="section-head"><span class="label">01</span><h2>Icon ladder</h2><p>icon.png scaled by the browser to 16 to 256 px on four grounds.</p>
    <label class="toggle"><input type="checkbox" id="pix" onchange="document.getElementById('ladder').classList.toggle('pixelated', this.checked)"> image-rendering: pixelated</label></div>
  <div class="ladder">{ladder}</div>
</section>

<section>
  <div class="section-head"><span class="label">02</span><h2>Installer surfaces at 1:1</h2><p>The outline is the expected bitmap bound, not part of the asset.</p></div>
  <div class="surfaces">{surfaces}</div>
</section>

<section>
  <div class="section-head"><span class="label">03</span><h2>In-context mockups</h2><p>Simplified Windows 11 and Finder frames. Placement follows NSIS Modern UI 2 and the Tauri DMG defaults.</p></div>
  <div class="mockups">
    <div class="win"><div class="title">PulseTalq Setup<span>&#8211; &#9633; &#10005;</span></div><div class="client">
      {img_or_missing("nsis-sidebar.bmp", 164, 314, "sidebar")}
      <div class="copy"><b>Welcome to PulseTalq Setup</b>Setup will guide you through the installation of PulseTalq.<br><br>Audio stays on this device.</div>
      <div class="footer"><span>Next &gt;</span><span>Cancel</span></div>
    </div></div>
    <div class="win"><div class="title">PulseTalq Setup<span>&#8211; &#9633; &#10005;</span></div><div class="client">
      {img_or_missing("nsis-header.bmp", 150, 57, "header")}
      <div class="inner-title"><b>Choose Install Location</b><span>Choose the folder in which to install PulseTalq.</span></div>
      <div class="hr"></div>
      <div class="field">Destination Folder<div><span>C:\\Program Files\\PulseTalq</span><span>Browse...</span></div></div>
      <div class="footer"><span>&lt; Back</span><span>Install</span><span>Cancel</span></div>
    </div></div>
    <div class="finder"><div class="bar"><i></i>PulseTalq</div><div class="stage">
      {img_or_missing("dmg-background.png", 660, 400, "bg")}
      {img_or_missing("icon.png", 128, 128, "app")}
      <div class="folder"><i></i><i></i><i></i></div>
      {'<div class="arrow"></div>' if not BY_KEY["dmg-background.png"].exists else ''}
      <span class="cap" style="left:180px">PulseTalq</span><span class="cap" style="left:480px">Applications</span>
    </div></div>
    <div><span class="label">What to check</span>
      <ul class="checks">
        <li>Sidebar bitmap fills 164x314 with no seam against the white page.</li>
        <li>Header bitmap reads at 150x57 with the page title to its left.</li>
        <li>DMG background stays legible under a 128 px icon and a folder.</li>
        <li>Icon silhouette remains distinct against the folder glyph.</li>
        <li>No purple or violet survives anywhere on this sheet.</li>
        <li>Bitmaps are 24 bit BMP without alpha (NSIS and WiX reject alpha).</li>
      </ul></div>
  </div>
</section>

<section>
  <div class="section-head"><span class="label">04</span><h2>Colour and contrast</h2><p>Colour audit is computed at generation time; the contrast bars are a single neutral series with a 4.5 reference.</p></div>
  <div class="audit-grid">
    <div>
      <span class="label">Colour audit, top 5 by pixel count, quantised to 32 levels</span>
      <p class="meta">Corner mark on a swatch: error = violet hue 240 to 300 at saturation above 0.3, warning = farther than 24 RGB from every brand token or two-token blend.</p>
      <table><thead><tr><th class="label">Asset</th><th class="label">Top colours</th><th class="label">Flags</th></tr></thead><tbody>{audit_rows}</tbody></table>
    </div>
    <div>
      <span class="label">WCAG contrast, 4.5 reference</span>
      <p class="meta">Any pair under 4.5 must pair colour with shape or text, per the rule "never rely on red alone".</p>
      {contrast_svg}
      <details><summary>Table view</summary>
      <table><thead><tr><th>Pair</th><th>Foreground</th><th>Background</th><th>Ratio</th><th>AA 4.5</th></tr></thead><tbody>{rows_table}</tbody></table></details>
    </div>
  </div>
</section>

<footer>
  <span class="label">Generated</span>
  <p class="data">{esc(stamp)} &nbsp; git {esc(sha)}</p>
  <table><thead><tr><th class="label">File</th><th class="label">SHA-256</th><th class="label">Size</th><th class="label">State</th></tr></thead><tbody>{footer_rows}</tbody></table>
</footer>
</div>
</body>
</html>
"""


# ---------------------------------------------------------------- main
def main() -> int:
    for a in ASSETS:
        load_asset(a)

    sha = git_sha()
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

    # Two-pass: measure with a tall canvas, then crop.
    s = Sheet(7000)
    y = M
    s.label(M, y, "deep focus review sheet")
    s.d.text((M, y + 16), "pulse ", font=font(40, 500), fill=BLACKOUT)
    s.d.text((M + font(40, 500).getlength("pulse "), y + 16), "talq", font=font(40, 500), fill=HOT)
    s.text(M + font(40, 500).getlength("pulse talq") + 16, y + 40, "installer preview", 21, 500, TEXT2)
    s.text(W - M, y + 16, f"{stamp}   git {sha}", 12, 500, TEXT2, anchor="ra")
    y += 84

    y = draw_icon_ladder(s, y)
    y += GAP
    y = draw_surfaces(s, y)
    y += GAP
    y = draw_mockups(s, y)
    y += GAP
    y = s.section(y, 4, "Colour and contrast", "Automated checks for the legacy purple and for brand token adherence, plus WCAG ratios for the core pairs.")
    audit_w = 900
    ya = draw_colour_audit(s, y, M, audit_w)
    yc = draw_contrast(s, y, M + audit_w + 60, W - 2 * M - audit_w - 60)
    y = max(ya, yc) + GAP
    y = draw_footer(s, y, sha, stamp)

    out = s.im.crop((0, 0, W, y)).convert("RGB")
    DOCS.mkdir(exist_ok=True)
    out.save(OUT_PNG, optimize=True)
    OUT_HTML.write_text(build_html(sha, stamp), encoding="utf-8")

    summary = {
        "png": str(OUT_PNG.relative_to(ROOT)),
        "html": str(OUT_HTML.relative_to(ROOT)),
        "size": out.size,
        "font": str(FONT_PATH) if FONT_PATH else None,
        "assets": {a.key: {"state": a.state, "purple": a.purple, "offbrand": a.offbrand, "size": a.size, "sha": a.sha} for a in ASSETS},
    }
    print(json.dumps(summary, indent=1, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())
