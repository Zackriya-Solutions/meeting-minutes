#!/usr/bin/env python3
"""Pre-build installer branding gate for PulseTalq (Deep Focus look).

Fails with a non-zero exit code when any installer-facing asset is missing,
mis-sized, still purple/violet (the old Meetily mark), or byte-identical to
a known legacy Meetily asset.

Usage:
    python scripts/verify-installer-assets.py [--repo PATH] [--json]

Requires Python 3.9+ and Pillow.
"""

from __future__ import annotations

import argparse
import colorsys
import hashlib
import json
import math
import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:  # pragma: no cover
    print("FAIL: Pillow is required (pip install pillow)", file=sys.stderr)
    sys.exit(2)

# ---------------------------------------------------------------------------
# Known legacy Meetily asset hashes.
# Recorded 2026-09-02 from the working tree immediately before the Deep Focus
# installer asset regeneration (scripts/brand/generate-installer-assets.py).
# Any file that still matches one of these hashes has NOT been rebranded.
# Paths are relative to the repository root.
# ---------------------------------------------------------------------------
LEGACY_MEETILY_HASHES: dict[str, str] = {
    "frontend/public/icon_128x128.png": "684d0ca63a015985be9452efbc621d369d0929c30a6b9300cf2c40355d8f73f6",
    "frontend/public/icon_32x32@2x.png": "90318785bbe5dd76f1d346b795c9f40f686e6e87ed6fa2f32c542ccad5a7c549",
    "frontend/public/logo-collapsed.png": "36ed5c2ae09675f3ad20951ecc66b3bdd3e312df8866c85677f754b6135d970a",
    "frontend/public/logo.png": "8d03d7a8b14d8592609ccea338b4c9281d99729c03e582f9b3d5a26787430dac",
    "frontend/src-tauri/icons/128x128.png": "56617dfc0cd0558792d6debba9e816423b8cda2b5cf5005d544d6436cb11c928",
    "frontend/src-tauri/icons/128x128@2x.png": "a94dc84f16026dde302c481822f8aeca8331be2f50cec544371842b7db30a32d",
    "frontend/src-tauri/icons/32x32.png": "1b15acaa6847bcb1a19383dee3470763c3abb8076cdb7a2efaa63985fd6f4964",
    "frontend/src-tauri/icons/Square107x107Logo.png": "33446a858d982a529554e562114673f8b15620588137cf0bf6c56a7441c78cd7",
    "frontend/src-tauri/icons/Square142x142Logo.png": "633d6dba504faa844b309e01272386e4523cddd4a3639c77c0eb5d2ce3eac752",
    "frontend/src-tauri/icons/Square150x150Logo.png": "d90061ad71e6f5882b759d80cf58cc16aa5021fa29ba4b2614adea982a4d7ee5",
    "frontend/src-tauri/icons/Square284x284Logo.png": "5feaa4f18f404843551756dd2791d7654ef471b6b007767ae40f60c7aebcad5b",
    "frontend/src-tauri/icons/Square30x30Logo.png": "68bc1b585f5685e0922f14b1ca7eb2d618213c684c89e759c6a5eb8578c3fe8d",
    "frontend/src-tauri/icons/Square310x310Logo.png": "5df8d779ab57c63ee06ed237b94c3c0954711e88117dccfa72b2aec5d83b0d80",
    "frontend/src-tauri/icons/Square44x44Logo.png": "c46208286b1ea1b4841b714972f3cb0a8abc6d3bfa71a29238f7b4d62f511f89",
    "frontend/src-tauri/icons/Square71x71Logo.png": "cb0ea93cbd13509d32c6bc06da71687e73d673d166fde91b23061771abe16b86",
    "frontend/src-tauri/icons/Square89x89Logo.png": "2208da1d741a20e902a59d7c39a5e58af74ec823db13113bf8ac6f555fa25bca",
    "frontend/src-tauri/icons/StoreLogo.png": "9f84d0ce09df51bac0a4855042b3e49b190cc2b9761cd5eab61ebec5cd343820",
    "frontend/src-tauri/icons/app_icon.icns": "5372e4e38dc25ced12200ac60b0df4b1e2248f95ab624be154bee90c33b819d1",
    "frontend/src-tauri/icons/app_icon.ico": "a8430cd75190f672c070b275715331b1e17bbd2f97649abca5e613f2ee49a5da",
    "frontend/src-tauri/icons/icon.icns": "f63a50179cdc88d06518dd737edca4f8359adece8ac8b6555593f1e17b25a6c0",
    "frontend/src-tauri/icons/icon.ico": "fd2d8f0ddd505ec8e7b64bbfde63898e09d96b1dbb246e741ee3f941aa8a2621",
    "frontend/src-tauri/icons/icon.png": "25f2f4b8a7b49521b3aa8d61c65c81459a2f6d9be3f6ced25f3dcd33ad5fa49a",
    "frontend/src-tauri/icons/icon_128x128.png": "684d0ca63a015985be9452efbc621d369d0929c30a6b9300cf2c40355d8f73f6",
    "frontend/src-tauri/icons/icon_128x128@2x.png": "79c07b8e5188dcbacdebd6a58e2e5421e6eee8561b6bfa99fd03eb313c9efb4c",
    "frontend/src-tauri/icons/icon_16x16.png": "77ad730ddd3adbb9f37b6b3754b6b80948a5d7c978aabfcf18291fdffdce84ed",
    "frontend/src-tauri/icons/icon_16x16@2x.png": "99210afaeed03cd611b8eaf92bccd088040d40b729d37297f63a9024fb61db39",
    "frontend/src-tauri/icons/icon_256x256.png": "79c07b8e5188dcbacdebd6a58e2e5421e6eee8561b6bfa99fd03eb313c9efb4c",
    "frontend/src-tauri/icons/icon_256x256@2x.png": "77e236ae7a4bce96895519bb3a1445615394e8c66fbe051d57824996f68e909b",
    "frontend/src-tauri/icons/icon_32x32.png": "99210afaeed03cd611b8eaf92bccd088040d40b729d37297f63a9024fb61db39",
    "frontend/src-tauri/icons/icon_32x32@2x.png": "90318785bbe5dd76f1d346b795c9f40f686e6e87ed6fa2f32c542ccad5a7c549",
    "frontend/src-tauri/icons/icon_512x512.png": "77e236ae7a4bce96895519bb3a1445615394e8c66fbe051d57824996f68e909b",
    "frontend/src-tauri/icons/icon_512x512@2x.png": "c1bd094c06270dc00ca1cdad0e23b527edbde612fdf48ec5390db3b4d9d750ee",
}

# Next.js scaffold assets that were present in frontend/public at the same
# time. They are not Meetily branding, so an unchanged hash is a warning
# (remove or replace them), not a failure.
LEGACY_SCAFFOLD_HASHES: dict[str, str] = {
    "frontend/public/file.svg": "2b67812c325c199a02536cdbeea0c593a72f707d323b72ee3e08dbab06753bd4",
    "frontend/public/globe.svg": "b614b9bf183925957661ac851498fe1d8029fd43a62fbfed86f9e2624a57e7cf",
    "frontend/public/next.svg": "55995dfad6ecb4945a1e856ddca03c5e16aa5bf13fd21b4df6a74ae79357bcfc",
    "frontend/public/vercel.svg": "f081337b2fee635b455b63275406a3e7f39d6a014e25ad90dab5a67e62a12ac4",
    "frontend/public/window.svg": "644768c4aaeb4767bce293344eeb0c125fb804a94d801440424072202d85e3a1",
}

# Exact sizes that NSIS and WiX require. Wrong sizes are silently stretched
# or rejected by the installer toolchain.
INSTALLER_IMAGE_SPECS = {
    ("windows", "nsis", "headerImage"): {"size": (150, 57), "bmp_rgb": True},
    ("windows", "nsis", "sidebarImage"): {"size": (164, 314), "bmp_rgb": True},
    ("windows", "wix", "bannerPath"): {"size": (493, 58), "bmp_rgb": True},
    ("windows", "wix", "dialogImagePath"): {"size": (493, 312), "bmp_rgb": True},
    ("macOS", "dmg", "background"): {"size": None, "bmp_rgb": False},
}

MIN_ICON_PNG_SIZE = 512
MIN_ICO_BYTES = 4_096
MIN_ICNS_BYTES = 16_384
PURPLE_HUE_RANGE = (240.0, 300.0)
PURPLE_SAT_THRESHOLD = 0.3
CENTRE_FRACTION = 0.6

# ---------------------------------------------------------------------------


class Report:
    def __init__(self) -> None:
        self.rows: list[tuple[str, str, str]] = []  # (status, check, detail)

    def ok(self, check: str, detail: str = "") -> None:
        self.rows.append(("PASS", check, detail))

    def warn(self, check: str, detail: str = "") -> None:
        self.rows.append(("WARN", check, detail))

    def fail(self, check: str, detail: str = "") -> None:
        self.rows.append(("FAIL", check, detail))

    @property
    def failures(self) -> int:
        return sum(1 for s, _, _ in self.rows if s == "FAIL")

    @property
    def warnings(self) -> int:
        return sum(1 for s, _, _ in self.rows if s == "WARN")

    def print_table(self) -> None:
        width_check = max((len(c) for _, c, _ in self.rows), default=10)
        print(f"{'STATUS':<6} | {'CHECK':<{width_check}} | DETAIL")
        print("-" * 6 + "-+-" + "-" * width_check + "-+-" + "-" * 40)
        for status, check, detail in self.rows:
            print(f"{status:<6} | {check:<{width_check}} | {detail}")
        print()
        print(f"{len(self.rows)} checks, {self.failures} failed, {self.warnings} warnings")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def load_conf(conf_path: Path) -> dict:
    with conf_path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def check_bundle_icons(report: Report, src_tauri: Path, bundle: dict) -> None:
    icons = bundle.get("icon") or []
    if not icons:
        report.fail("bundle.icon", "no icons listed in tauri.conf.json bundle.icon")
        return
    for rel in icons:
        p = src_tauri / rel
        if p.is_file():
            report.ok(f"bundle.icon {rel}", f"exists ({p.stat().st_size} bytes)")
        else:
            report.fail(f"bundle.icon {rel}", "file missing")


def check_primary_icon(report: Report, icons_dir: Path) -> Path | None:
    icon_png = icons_dir / "icon.png"
    if not icon_png.is_file():
        report.fail("icons/icon.png", "missing")
        return None
    try:
        with Image.open(icon_png) as im:
            w, h = im.size
    except Exception as exc:  # noqa: BLE001
        report.fail("icons/icon.png", f"unreadable: {exc}")
        return None
    if w != h:
        report.fail("icons/icon.png square", f"{w}x{h} is not square")
    elif w < MIN_ICON_PNG_SIZE:
        report.fail("icons/icon.png size", f"{w}x{h} is below {MIN_ICON_PNG_SIZE}px")
    else:
        report.ok("icons/icon.png square >=512", f"{w}x{h}")

    for name, min_bytes in (("icon.ico", MIN_ICO_BYTES), ("icon.icns", MIN_ICNS_BYTES)):
        p = icons_dir / name
        if not p.is_file():
            report.fail(f"icons/{name}", "missing")
            continue
        size = p.stat().st_size
        if size < min_bytes:
            report.fail(f"icons/{name} size", f"{size} bytes is below {min_bytes} (trivial/placeholder)")
        else:
            report.ok(f"icons/{name} non-trivial", f"{size} bytes")
    return icon_png


def dominant_hue(icon_png: Path) -> tuple[float, float, int]:
    """Return (circular mean hue in degrees, mean saturation, sampled pixel count).

    Samples the centre 60 percent of the image, ignoring transparent and
    near-black pixels so the brand mark's colour dominates, not the field.
    """
    with Image.open(icon_png) as im:
        im = im.convert("RGBA")
        w, h = im.size
        margin_w = int(w * (1 - CENTRE_FRACTION) / 2)
        margin_h = int(h * (1 - CENTRE_FRACTION) / 2)
        crop = im.crop((margin_w, margin_h, w - margin_w, h - margin_h))
        # Downsample for speed; hue statistics are robust to this.
        if crop.width > 256:
            crop = crop.resize((256, max(1, int(256 * crop.height / crop.width))))
        getter = getattr(crop, "get_flattened_data", None) or crop.getdata
        pixels = list(getter())

    sin_sum = cos_sum = sat_sum = 0.0
    count = 0
    for r, g, b, a in pixels:
        if a < 128:
            continue
        hue, sat, val = colorsys.rgb_to_hsv(r / 255, g / 255, b / 255)
        if val < 0.15:
            continue  # near-black, no meaningful hue
        weight = sat  # saturated pixels carry the hue signal
        sin_sum += math.sin(math.radians(hue * 360)) * weight
        cos_sum += math.cos(math.radians(hue * 360)) * weight
        sat_sum += sat
        count += 1
    if count == 0:
        return float("nan"), 0.0, 0
    mean_hue = math.degrees(math.atan2(sin_sum, cos_sum)) % 360
    return mean_hue, sat_sum / count, count


def check_not_purple(report: Report, icon_png: Path) -> None:
    hue, sat, n = dominant_hue(icon_png)
    if n == 0:
        report.fail("icons/icon.png colour", "no opaque, non-black pixels in centre region")
        return
    is_purple = PURPLE_HUE_RANGE[0] <= hue <= PURPLE_HUE_RANGE[1] and sat > PURPLE_SAT_THRESHOLD
    detail = f"mean hue {hue:.1f} deg, mean saturation {sat:.2f}, {n} px sampled"
    if is_purple:
        report.fail("icons/icon.png not purple", detail + " (matches legacy Meetily violet)")
    else:
        report.ok("icons/icon.png not purple", detail)


def check_installer_images(report: Report, src_tauri: Path, bundle: dict) -> None:
    for (platform, tool, key), spec in INSTALLER_IMAGE_SPECS.items():
        rel = ((bundle.get(platform) or {}).get(tool) or {}).get(key)
        label = f"bundle.{platform}.{tool}.{key}"
        if not rel:
            report.warn(label, "not set in tauri.conf.json (installer will use toolchain default art)")
            continue
        p = src_tauri / rel
        if not p.is_file():
            report.fail(label, f"{rel} missing")
            continue
        try:
            with Image.open(p) as im:
                size = im.size
                mode = im.mode
                fmt = im.format
        except Exception as exc:  # noqa: BLE001
            report.fail(label, f"{rel} unreadable: {exc}")
            continue
        problems = []
        if spec["size"] and size != spec["size"]:
            problems.append(f"size {size[0]}x{size[1]}, required {spec['size'][0]}x{spec['size'][1]}")
        if spec["bmp_rgb"]:
            if fmt != "BMP":
                problems.append(f"format {fmt}, required BMP")
            if mode != "RGB":
                problems.append(f"mode {mode}, required RGB (24-bit, no alpha)")
        if problems:
            report.fail(label, f"{rel}: " + "; ".join(problems))
        else:
            report.ok(label, f"{rel} {size[0]}x{size[1]} {fmt} {mode}")


def check_legacy_hashes(report: Report, repo: Path) -> None:
    scan_roots = [repo / "frontend" / "src-tauri" / "icons", repo / "frontend" / "public"]
    legacy_by_hash = {h: p for p, h in LEGACY_MEETILY_HASHES.items()}
    scaffold_by_hash = {h: p for p, h in LEGACY_SCAFFOLD_HASHES.items()}
    scanned = 0
    for root in scan_roots:
        if not root.is_dir():
            report.fail(f"scan {root.relative_to(repo).as_posix()}", "directory missing")
            continue
        for p in sorted(root.rglob("*")):
            if not p.is_file():
                continue
            scanned += 1
            rel = p.relative_to(repo).as_posix()
            digest = sha256(p)
            if digest in legacy_by_hash:
                report.fail(f"legacy hash {rel}", f"byte-identical to old Meetily asset ({digest[:12]}...)")
            elif digest in scaffold_by_hash:
                report.warn(f"scaffold asset {rel}", "unchanged Next.js starter asset; remove or replace")
    report.ok("legacy hash scan", f"{scanned} files scanned under icons/ and public/")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1], help="repository root")
    parser.add_argument("--json", action="store_true", help="also print machine-readable JSON")
    args = parser.parse_args(argv)

    repo: Path = args.repo.resolve()
    src_tauri = repo / "frontend" / "src-tauri"
    icons_dir = src_tauri / "icons"
    conf_path = src_tauri / "tauri.conf.json"

    report = Report()
    print("PulseTalq installer asset gate")
    print(f"repo: {repo}")
    print(f"conf: {conf_path}")
    print()

    if not conf_path.is_file():
        report.fail("tauri.conf.json", "missing")
        report.print_table()
        return 1
    try:
        conf = load_conf(conf_path)
    except json.JSONDecodeError as exc:
        report.fail("tauri.conf.json", f"invalid JSON: {exc}")
        report.print_table()
        return 1

    product = conf.get("productName")
    identifier = conf.get("identifier")
    if product == "PulseTalq":
        report.ok("productName", product)
    else:
        report.fail("productName", f"{product!r}, expected 'PulseTalq'")
    if identifier and "meetily" not in identifier.lower():
        report.ok("identifier", identifier)
    else:
        report.fail("identifier", f"{identifier!r} still references Meetily")

    bundle = conf.get("bundle") or {}
    check_bundle_icons(report, src_tauri, bundle)
    icon_png = check_primary_icon(report, icons_dir)
    if icon_png is not None:
        check_not_purple(report, icon_png)
    check_installer_images(report, src_tauri, bundle)
    check_legacy_hashes(report, repo)

    report.print_table()
    if args.json:
        print(json.dumps([{"status": s, "check": c, "detail": d} for s, c, d in report.rows], indent=2))

    if report.failures:
        print()
        print("RESULT: FAIL")
        return 1
    print()
    print("RESULT: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
