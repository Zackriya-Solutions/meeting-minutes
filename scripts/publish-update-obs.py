#!/usr/bin/env python3
"""Publish Memento (Tauri) release artifacts to SberCloud OBS for the auto-updater.

Generates the Tauri `latest.json` manifest from locally built, signed updater
artifacts and uploads everything to the OBS prefix the app polls
(`plugins.updater.endpoints` in frontend/src-tauri/tauri.conf.json).

Object layout:
    {prefix}/latest.json                          <- manifest, no-cache
    {prefix}/v{version}/{platform}/{artifact}     <- payload + .sig, immutable

The version/platform directory is required because Tauri names the macOS
updater payload `Memento.app.tar.gz` with no version or arch in it — a flat
layout would collide between arm64 and x86_64 and between releases (and an
immutable Cache-Control on a reused key would serve a stale binary forever).

Because one manifest holds every platform, publishing a second platform for the
same version MERGES into the manifest already on OBS instead of replacing it.
That lets arm64, x86_64 and Windows be built on different machines at different
times. A different version starts a fresh manifest.

OBS quirks this script papers over (same three as GigaTool's scripts/publish.py):
1. Virtual-hosted addressing is mandatory for PUT/POST. Path-style reads work
   fine (the updater fetches the manifest over a path-style URL), but uploading
   with addressing_style=path returns NoSuchBucket.
2. boto3 1.36+ enables streaming SHA256 chunked transfer by default; OBS doesn't
   parse it and returns XAmzContentSHA256Mismatch. We force
   request_checksum_calculation=when_required to opt out.
3. Default object ACL is private. The updater reads anonymously, so objects go
   up with public-read.

Credentials and settings are read from the environment, or from .env /
frontend/.env.signing in the repo (both gitignored). Required:
  S3_ACCESS_KEY_ID, S3_SECRET_ACCESS_KEY
Optional overrides (defaults shown):
  OBS_ENDPOINT  https://obs.ru-moscow-1.hc.sbercloud.ru
  OBS_REGION    ru-moscow-1
  OBS_BUCKET    d-ssdev-crowd
  OBS_PREFIX    function_descriptions/memento

Usage:
  scripts/publish-update-obs.py                        # publish target/release/bundle
  scripts/publish-update-obs.py --dry-run              # print the plan, upload nothing
  scripts/publish-update-obs.py --notes "Fixes X"      # release notes shown in-app
  scripts/publish-update-obs.py --from target/x86_64-apple-darwin/release/bundle
  scripts/publish-update-obs.py --target darwin-x86_64 # force the platform key
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone

try:
    import boto3
    from botocore.config import Config
    from botocore.exceptions import ClientError
except ImportError:
    sys.exit("error: boto3 not installed. Run: python3 -m pip install --user boto3")

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
TAURI_CONF = REPO_ROOT / "frontend" / "src-tauri" / "tauri.conf.json"
DEFAULT_BUNDLE = REPO_ROOT / "target" / "release" / "bundle"

DEFAULT_ENDPOINT = "https://obs.ru-moscow-1.hc.sbercloud.ru"
DEFAULT_REGION = "ru-moscow-1"
DEFAULT_BUCKET = "d-ssdev-crowd"
DEFAULT_PREFIX = "function_descriptions/memento"

MANIFEST_NAME = "latest.json"
IMMUTABLE = "public, max-age=31536000, immutable"
NO_CACHE = "no-cache, no-store, must-revalidate"

# Updater payloads Tauri can install, by bundle subdirectory and suffix. Only
# files with a sibling .sig are eligible — Tauri refuses unsigned updates.
PAYLOAD_SUFFIXES = (".app.tar.gz", "-setup.exe", ".msi", ".AppImage")

# A Windows build emits both an NSIS -setup.exe and an .msi, and both map to
# windows-x86_64. Rank them so one wins instead of tripping the ambiguity check;
# NSIS is Tauri's recommended updater target (it supports passive installs).
PAYLOAD_PREFERENCE = ("-setup.exe", ".app.tar.gz", ".AppImage", ".msi")


def payload_rank(path: pathlib.Path) -> int:
    for rank, suffix in enumerate(PAYLOAD_PREFERENCE):
        if path.name.endswith(suffix):
            return rank
    return len(PAYLOAD_PREFERENCE)
# Shipped to users as a direct download; not referenced by the manifest.
INSTALLER_SUFFIXES = (".dmg", ".deb", ".rpm")

VALID_PLATFORMS = (
    "darwin-aarch64",
    "darwin-x86_64",
    "windows-x86_64",
    "windows-aarch64",
    "linux-x86_64",
    "linux-aarch64",
)

CONTENT_TYPES = {
    ".json": "application/json",
    ".sig": "text/plain",
    ".gz": "application/gzip",
    ".dmg": "application/x-apple-diskimage",
    ".exe": "application/vnd.microsoft.portable-executable",
    ".msi": "application/x-msi",
    ".appimage": "application/octet-stream",
    ".deb": "application/vnd.debian.binary-package",
    ".rpm": "application/x-rpm",
}


# ---------------------------------------------------------------- environment


def load_env_file(path: pathlib.Path) -> None:
    """Minimal .env loader — sets keys not already present in the environment."""
    if not path.is_file():
        return
    for raw in path.read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        val = val.strip()
        if len(val) >= 2 and val[0] == val[-1] and val[0] in ("'", '"'):
            val = val[1:-1]
        os.environ.setdefault(key, val)


def require(name: str) -> str:
    val = os.environ.get(name)
    if not val:
        sys.exit(f"error: {name} not set (check the environment, .env or frontend/.env.signing)")
    return val


def conf_version() -> str:
    conf = json.loads(TAURI_CONF.read_text())
    return conf["version"]


def is_semver(version: str) -> bool:
    """Tauri parses manifest versions as semver, so a four-component version is unusable.

    Guards against .github/workflows/release.yml, which appends a fourth component
    (0.4.0.1) when a tag already exists: clients silently see no update.
    """
    return bool(re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", version))


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def start_manifest(
    version: str,
    notes: str | None,
    pub_date: str,
    remote: dict | None,
    publishing: set[str],
) -> tuple[dict, str | None]:
    """Fresh manifest, or the remote one folded in when it covers the same version.

    Returns (manifest, status_line). Platform entries for the versions being published
    are left for the caller to fill in; entries for *other* platforms are carried over
    so a per-platform publish doesn't drop the rest of the release.
    """
    manifest = {
        "version": version,
        "notes": notes or f"Release {version}",
        "pub_date": pub_date,
        "platforms": {},
    }
    if not remote:
        return manifest, None

    if remote.get("version") != version:
        return manifest, f"replacing remote manifest (was v{remote.get('version')}, now v{version})"

    manifest["platforms"] = dict(remote.get("platforms") or {})
    if not notes and remote.get("notes"):
        manifest["notes"] = remote["notes"]
    if remote.get("pub_date"):
        manifest["pub_date"] = remote["pub_date"]
    kept = [p for p in manifest["platforms"] if p not in publishing]
    return manifest, f"merging into existing v{version} manifest (keeping: {', '.join(kept) or 'nothing'})"


def configured_endpoint() -> str | None:
    """The manifest URL the shipped app actually polls, for a sanity check."""
    conf = json.loads(TAURI_CONF.read_text())
    endpoints = conf.get("plugins", {}).get("updater", {}).get("endpoints") or []
    return endpoints[0] if endpoints else None


# ------------------------------------------------------------------ artifacts


def arch_from_name(name: str) -> str | None:
    """Read the arch out of a bundle filename, e.g. Memento_0.4.0_x64-setup.exe.

    aarch64 is checked first: "x86_64" contains "x86", and an arm64 name must not
    fall through to it.
    """
    lowered = name.lower()
    if any(token in lowered for token in ("aarch64", "arm64")):
        return "aarch64"
    if any(token in lowered for token in ("x86_64", "x64", "amd64", "intel")):
        return "x86_64"
    return None


def arch_from_path(path: pathlib.Path) -> str | None:
    """Read the Rust target triple out of e.g. target/x86_64-apple-darwin/release/..."""
    for part in path.parts:
        if part.startswith("x86_64-"):
            return "x86_64"
        if part.startswith("aarch64-") or part.startswith("arm64-"):
            return "aarch64"
    return None


def arch_from_sibling_app(payload: pathlib.Path) -> str | None:
    """Inspect the .app that produced a macOS .app.tar.gz, e.g. Memento.app.tar.gz."""
    app = payload.parent / payload.name[: -len(".tar.gz")]
    macos_dir = app / "Contents" / "MacOS"
    if not macos_dir.is_dir():
        return None
    for binary in sorted(macos_dir.iterdir()):
        if not binary.is_file():
            continue
        try:
            out = subprocess.run(
                ["lipo", "-archs", str(binary)],
                capture_output=True,
                text=True,
                timeout=15,
                check=False,
            ).stdout
        except (OSError, subprocess.SubprocessError):
            return None
        archs = out.split()
        if "arm64" in archs and "x86_64" not in archs:
            return "aarch64"
        if "x86_64" in archs and "arm64" not in archs:
            return "x86_64"
        return None  # universal or unknown — can't disambiguate
    return None


def platform_key_for(payload: pathlib.Path, override: str | None) -> tuple[str | None, str]:
    """Return (platform_key, how_it_was_determined), or (None, why_it_is_ambiguous).

    Never guesses from the host: the publishing machine's architecture says nothing
    about a cross-compiled or copied-in artifact, and a mislabelled entry serves an
    incompatible binary while leaving the correct entry stale in the manifest.
    Ambiguity is the caller's cue to demand --target.
    """
    if override:
        return override, "--target"

    name = payload.name.lower()
    if name.endswith(".app.tar.gz"):
        arch = arch_from_path(payload)
        if arch:
            return f"darwin-{arch}", "target triple in path"
        arch = arch_from_sibling_app(payload)
        if arch:
            return f"darwin-{arch}", "arch of the sibling .app binary"
        return None, "no target triple in the path, and the sibling .app is missing or universal"

    if name.endswith(".dmg"):
        arch = arch_from_name(name) or arch_from_path(payload)
        return (f"darwin-{arch}", "filename") if arch else (None, "no arch in the filename or path")

    if name.endswith("-setup.exe") or name.endswith(".msi"):
        arch = arch_from_name(name) or arch_from_path(payload)
        return (f"windows-{arch}", "filename") if arch else (None, "no arch in the filename or path")

    if name.endswith(".appimage") or name.endswith(".deb") or name.endswith(".rpm"):
        arch = arch_from_name(name) or arch_from_path(payload)
        return (f"linux-{arch}", "filename") if arch else (None, "no arch in the filename or path")

    return None, "unrecognized artifact type"


def find_files(roots: list[pathlib.Path], suffixes: tuple[str, ...]) -> list[pathlib.Path]:
    out: list[pathlib.Path] = []
    seen: set[pathlib.Path] = set()
    for root in roots:
        if not root.exists():
            continue
        if root.is_file():
            candidates = [root]
        else:
            candidates = [p for p in sorted(root.rglob("*")) if p.is_file()]
        for p in candidates:
            if p in seen or not any(p.name.endswith(s) for s in suffixes):
                continue
            # Skip the .app's own contents (an .app is a directory tree).
            if any(part.endswith(".app") for part in p.parts):
                continue
            # hdiutil leaves its intermediate read/write image behind as
            # rw.<pid>.<name>.dmg when a DMG build is interrupted.
            if p.name.startswith("rw."):
                continue
            seen.add(p)
            out.append(p)
    return out


# ----------------------------------------------------------------------- OBS


def make_client(endpoint: str, region: str):
    base_kwargs = {
        "signature_version": "s3v4",
        "s3": {"addressing_style": "virtual"},
    }
    try:
        config = Config(
            **base_kwargs,
            request_checksum_calculation="when_required",
            response_checksum_validation="when_required",
        )
    except TypeError:
        # boto3 < 1.36 doesn't accept these and doesn't enable the behavior.
        config = Config(**base_kwargs)
    return boto3.client(
        "s3",
        endpoint_url=endpoint,
        region_name=region,
        aws_access_key_id=require("S3_ACCESS_KEY_ID"),
        aws_secret_access_key=require("S3_SECRET_ACCESS_KEY"),
        config=config,
    )


def content_type_for(name: str) -> str:
    lowered = name.lower()
    if lowered.endswith(".tar.gz"):
        return "application/gzip"
    ext = pathlib.Path(lowered).suffix
    return CONTENT_TYPES.get(ext, "application/octet-stream")


def public_url(endpoint: str, bucket: str, key: str) -> str:
    return f"{endpoint.rstrip('/')}/{bucket}/{key}"


def fetch_remote_manifest(s3, bucket: str, key: str) -> dict | None:
    try:
        body = s3.get_object(Bucket=bucket, Key=key)["Body"].read()
    except ClientError as exc:
        code = exc.response.get("Error", {}).get("Code", "")
        if code in ("NoSuchKey", "404", "NotFound"):
            return None
        print(f"  warning: could not read existing {key}: {code or exc}", file=sys.stderr)
        return None
    try:
        return json.loads(body)
    except json.JSONDecodeError as exc:
        print(f"  warning: existing {key} is not valid JSON ({exc}) — starting fresh", file=sys.stderr)
        return None


def head_public(url: str) -> tuple[bool, str]:
    request = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(request, timeout=20) as response:  # noqa: S310
            return response.status == 200, str(response.status)
    except urllib.error.HTTPError as exc:
        return False, f"HTTP {exc.code}"
    except (urllib.error.URLError, OSError) as exc:
        return False, str(exc)


# ----------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__.split("\n\n")[0],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument(
        "--from",
        dest="sources",
        action="append",
        metavar="PATH",
        help=f"bundle dir or artifact to publish; repeatable (default: {DEFAULT_BUNDLE})",
    )
    ap.add_argument(
        "--version",
        help="release version (default: version in frontend/src-tauri/tauri.conf.json)",
    )
    ap.add_argument("--notes", help="release notes shown in the in-app update dialog")
    ap.add_argument("--notes-file", help="read release notes from a file")
    ap.add_argument(
        "--target",
        choices=VALID_PLATFORMS,
        help="force the platform key instead of detecting it from the artifacts",
    )
    ap.add_argument(
        "--no-merge",
        action="store_true",
        help="overwrite the remote manifest instead of merging other platforms into it",
    )
    ap.add_argument(
        "--no-installers",
        action="store_true",
        help="upload only updater payloads, skip the .dmg/.deb direct downloads",
    )
    ap.add_argument(
        "--no-verify",
        action="store_true",
        help="skip the anonymous HTTP readback of the uploaded objects",
    )
    ap.add_argument("--no-public", action="store_true", help="upload private instead of public-read")
    ap.add_argument("--dry-run", action="store_true", help="print the plan, upload nothing")
    args = ap.parse_args()

    load_env_file(REPO_ROOT / ".env")
    load_env_file(REPO_ROOT / "frontend" / ".env.signing")

    endpoint = os.environ.get("OBS_ENDPOINT", DEFAULT_ENDPOINT)
    region = os.environ.get("OBS_REGION", DEFAULT_REGION)
    bucket = os.environ.get("OBS_BUCKET", DEFAULT_BUCKET)
    prefix = os.environ.get("OBS_PREFIX", DEFAULT_PREFIX).strip("/")

    version = (args.version or conf_version()).lstrip("v")
    if not is_semver(version):
        print(f"error: '{version}' is not a semver version, so Tauri clients cannot parse it", file=sys.stderr)
        print("       and would silently see no update. Use MAJOR.MINOR.PATCH.", file=sys.stderr)
        return 1
    manifest_key = f"{prefix}/{MANIFEST_NAME}"
    manifest_url = public_url(endpoint, bucket, manifest_key)

    configured = configured_endpoint()
    if configured and configured != manifest_url:
        print("warning: the manifest URL does not match plugins.updater.endpoints[0]", file=sys.stderr)
        print(f"         publishing to: {manifest_url}", file=sys.stderr)
        print(f"         app polls:     {configured}", file=sys.stderr)

    roots = [pathlib.Path(s).expanduser().resolve() for s in (args.sources or [str(DEFAULT_BUNDLE)])]
    missing = [r for r in roots if not r.exists()]
    if missing:
        for r in missing:
            print(f"error: {r} not found", file=sys.stderr)
        return 1

    payloads = find_files(roots, PAYLOAD_SUFFIXES)
    signed = [p for p in payloads if (p.parent / f"{p.name}.sig").is_file()]
    unsigned = [p for p in payloads if p not in signed]
    for p in unsigned:
        print(f"warning: skipping unsigned payload (no {p.name}.sig): {p}", file=sys.stderr)
    if not signed:
        print(f"error: no signed updater payloads found under {', '.join(str(r) for r in roots)}", file=sys.stderr)
        print("       Build with an updater signing key set — see docs/AUTOUPDATE.md.", file=sys.stderr)
        return 1

    # platform key -> (payload path, signature path)
    entries: dict[str, tuple[pathlib.Path, pathlib.Path]] = {}
    for payload in signed:
        key, how = platform_key_for(payload, args.target)
        if key is None:
            print(f"error: cannot tell which platform {payload} is for — {how}.", file=sys.stderr)
            print("       Re-run with --target (e.g. --target darwin-aarch64). Guessing from the", file=sys.stderr)
            print("       publishing machine would risk serving an incompatible binary.", file=sys.stderr)
            return 1
        if key not in VALID_PLATFORMS:
            print(f"error: detected unsupported platform key '{key}' for {payload}", file=sys.stderr)
            return 1
        if key in entries:
            existing = entries[key][0]
            if payload_rank(payload) == payload_rank(existing):
                print(f"error: two equivalent payloads map to {key}:", file=sys.stderr)
                print(f"       {existing}", file=sys.stderr)
                print(f"       {payload}", file=sys.stderr)
                print("       Publish them separately with --from, or pass --target.", file=sys.stderr)
                return 1
            winner, loser = sorted((payload, existing), key=payload_rank)
            entries[key] = (winner, winner.parent / f"{winner.name}.sig")
            print(f"==> {key}: {winner.name}  (preferred over {loser.name})")
            continue
        entries[key] = (payload, payload.parent / f"{payload.name}.sig")
        print(f"==> {key}: {payload.name}  ({how})")

    installers: list[tuple[str, pathlib.Path]] = []
    if not args.no_installers:
        for installer in find_files(roots, INSTALLER_SUFFIXES):
            key, how = platform_key_for(installer, args.target)
            # Installers are direct downloads, not manifest entries, so an unplaceable
            # one is skipped rather than fatal. A single-platform publish is unambiguous.
            if key is None or key not in entries:
                if len(entries) == 1:
                    key = next(iter(entries))
                elif key is None:
                    print(f"warning: skipping {installer.name} — {how}", file=sys.stderr)
                    continue
            installers.append((key, installer))

    notes: str | None = args.notes
    if args.notes_file:
        notes = pathlib.Path(args.notes_file).read_text().strip()

    s3 = None if args.dry_run else make_client(endpoint, region)

    # Build the manifest, merging the remote one when it is the same version.
    remote = None if (args.no_merge or not s3) else fetch_remote_manifest(s3, bucket, manifest_key)
    manifest, status = start_manifest(version, notes, now_iso(), remote, set(entries))
    if status:
        print(f"==> {status}")

    uploads: list[tuple[str, pathlib.Path, str]] = []  # key, file, cache-control
    for plat, (payload, sig) in entries.items():
        payload_key = f"{prefix}/v{version}/{plat}/{payload.name}"
        uploads.append((payload_key, payload, IMMUTABLE))
        uploads.append((f"{payload_key}.sig", sig, IMMUTABLE))
        manifest["platforms"][plat] = {
            "signature": sig.read_text().strip(),
            "url": public_url(endpoint, bucket, payload_key),
        }
    for plat, installer in installers:
        uploads.append((f"{prefix}/v{version}/{plat}/{installer.name}", installer, IMMUTABLE))

    print()
    print(f"==> version:  {version}")
    print(f"==> target:   {endpoint}/{bucket}/{prefix}/")
    print(f"==> manifest: {manifest_url}")
    print(f"==> ACL:      {'private' if args.no_public else 'public-read'}")
    print(f"==> objects:  {len(uploads)} + manifest")
    print()
    print(json.dumps(manifest, indent=2, ensure_ascii=False))
    print()

    if args.dry_run:
        for key, path, _ in uploads:
            size_mb = path.stat().st_size / (1024 * 1024)
            print(f"  [dry-run] {key}  ({size_mb:.1f} MB)")
        print(f"  [dry-run] {manifest_key}")
        return 0

    extra = {} if args.no_public else {"ACL": "public-read"}

    # Payloads first, manifest last: the updater must never see a manifest that
    # points at an object which is not there yet.
    print("==> uploading artifacts")
    for key, path, cache in uploads:
        size_mb = path.stat().st_size / (1024 * 1024)
        print(f"  -> {key}  ({size_mb:.1f} MB)")
        s3.upload_file(
            str(path),
            bucket,
            key,
            ExtraArgs={**extra, "ContentType": content_type_for(path.name), "CacheControl": cache},
        )

    if not args.no_verify and not args.no_public:
        print("==> verifying anonymous access to the new payloads")
        broken = []
        for plat, entry in manifest["platforms"].items():
            ok, status = head_public(entry["url"])
            print(f"  {'ok  ' if ok else 'FAIL'} {plat}: {status}")
            if not ok:
                broken.append(plat)
        if broken:
            print(
                f"error: {', '.join(broken)} not publicly readable — refusing to publish the manifest.",
                file=sys.stderr,
            )
            print("       Fix the object/bucket ACL and re-run.", file=sys.stderr)
            return 1

    print("==> uploading manifest")
    s3.put_object(
        Bucket=bucket,
        Key=manifest_key,
        Body=json.dumps(manifest, indent=2, ensure_ascii=False).encode("utf-8"),
        ContentType="application/json",
        CacheControl=NO_CACHE,
        **extra,
    )
    print(f"  -> {manifest_key}")

    if not args.no_verify and not args.no_public:
        try:
            with urllib.request.urlopen(manifest_url, timeout=20) as response:  # noqa: S310
                served = json.loads(response.read())
            if served.get("version") != version:
                print(
                    f"warning: {MANIFEST_NAME} serves v{served.get('version')}, expected v{version} "
                    "(stale cache?)",
                    file=sys.stderr,
                )
            else:
                print(f"==> verified: {manifest_url} serves v{version} "
                      f"({', '.join(sorted(served.get('platforms', {})))})")
        except (urllib.error.URLError, OSError, json.JSONDecodeError) as exc:
            print(f"warning: could not read back {manifest_url}: {exc}", file=sys.stderr)

    print()
    print(f"Done. Clients below v{version} will offer the update on their next check.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
