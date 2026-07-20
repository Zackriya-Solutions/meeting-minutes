#!/usr/bin/env python3
"""Standalone SaluteSpeech speaker-diarization client (via the speech.giga.chat gateway).

Uploads a meeting recording to SaluteSpeech's async recognition with speaker
separation and prints/saves the detected speaker turns (with recognized text when
the service returns it). Mirrors the protocol verified live in the app's Rust
client (frontend/src-tauri/src/salutespeech/): OAuth Basic token -> data:upload ->
speech:async_recognize -> task:get poll -> data:download. The async REST base is
derived from the recognize URL by dropping its final `/speech:recognize` segment.

Usage:
    python3 scripts/salutespeech_diarize.py audio.mp4
    python3 scripts/salutespeech_diarize.py audio.mp4 --speakers 7 --model transcribation_hq

Credentials (.env in the current directory, next to this script, or --env PATH). The
SBER_SALUTE_* names are primary; the older SALUTESPEECH_* names still work as fallbacks:
    SBER_SALUTE_AUTH_KEY=...             # required: the base64 "Authorization Key"
                                         # (Basic auth, base64(login:password))
    SBER_SALUTE_OAUTH_URL=...            # optional; default https://speech.giga.chat/v1/token
    SBER_SALUTE_RECOGNIZE_URL=...        # optional; default
                                         # https://speech.giga.chat/rest/v1/speech:recognize
    SBER_SALUTE_RECOGNITION_MODEL=...    # optional; default universal_turbo. The async
                                         # (diarization) endpoint accepts only transcribation_hq
                                         # or universal_turbo; voice_messaging is the sync-recognize
                                         # model and is auto-swapped to universal_turbo here.
    SBER_SALUTE_SCOPE=...                # optional; the gateway ignores it (only the raw
                                         # ngw.devices.sberbank.ru endpoint needs a scope)
    SBER_SALUTE_CA_BUNDLE=...            # optional; path to a custom CA PEM

TLS note: speech.giga.chat verifies with standard system trust roots, so no custom CA
is normally needed. If you point this at the raw Sber endpoints (signed by the Russian
Trusted Root CA, absent from standard trust stores), download the bundle
(https://gu-st.ru/content/Other/doc/russiantrustedca.pem) and set SBER_SALUTE_CA_BUNDLE,
or pass --insecure to skip verification.

Requires ffmpeg on PATH (or FFMPEG env var) to convert the input to 16 kHz mono PCM.
Standard library only — no pip installs.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import ssl
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

DEFAULT_OAUTH_URL = "https://speech.giga.chat/v1/token"
DEFAULT_RECOGNIZE_URL = "https://speech.giga.chat/rest/v1/speech:recognize"
# The async recognition endpoint used for speaker diarization accepts only these models.
# `voice_messaging` is the *sync* speech:recognize model and is rejected here (HTTP 400),
# so it is swapped for the async default below.
ASYNC_MODELS = ("transcribation_hq", "universal_turbo")
DEFAULT_MODEL = "universal_turbo"
# The giga.chat WAF requires a "GigaChat-*" User-Agent (the Rust client sends "GigaChat-Meetily").
USER_AGENT = "GigaChat-diarize-script"


def env_any(*keys: str, default: str | None = None) -> str | None:
    """First non-empty value among the given env vars (SBER_SALUTE_* before SALUTESPEECH_*)."""
    for key in keys:
        value = os.environ.get(key)
        if value and value.strip():
            return value.strip()
    return default


def rest_base(recognize_url: str) -> str:
    """Gateway REST base (e.g. https://speech.giga.chat/rest/v1), derived from the recognize
    URL by dropping its final `/speech:recognize` segment. Mirrors salutespeech/diarize.rs."""
    trimmed = recognize_url.rstrip("/")
    base, sep, _ = trimmed.rpartition("/")
    return base if sep else trimmed


def load_env(explicit: str | None) -> None:
    """Minimal .env loader: KEY=VALUE lines into os.environ (existing env wins)."""
    candidates = (
        [Path(explicit)]
        if explicit
        else [Path.cwd() / ".env", Path(__file__).resolve().parent / ".env"]
    )
    for path in candidates:
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, _, value = line.partition("=")
            key, value = key.strip(), value.strip().strip("'\"")
            if key and key not in os.environ:
                os.environ[key] = value
        print(f"loaded env from {path}", file=sys.stderr)
        return
    if explicit:
        sys.exit(f"error: env file not found: {explicit}")


def build_opener(insecure: bool) -> urllib.request.OpenerDirector:
    ca_bundle = env_any("SBER_SALUTE_CA_BUNDLE", "SALUTESPEECH_CA_BUNDLE")
    if insecure:
        ctx = ssl._create_unverified_context()  # noqa: SLF001 — explicit user opt-in
    elif ca_bundle:
        ctx = ssl.create_default_context(cafile=ca_bundle)
    else:
        ctx = ssl.create_default_context()
    # ProxyHandler() with no args honors http_proxy/https_proxy from the environment.
    return urllib.request.build_opener(
        urllib.request.ProxyHandler(), urllib.request.HTTPSHandler(context=ctx)
    )


# Retry on transient failures — the gateway intermittently drops connections mid-request
# (broken pipe / connection reset), which would otherwise abort an already-running task.
RETRYABLE_HTTP = frozenset({429, 500, 502, 503, 504})
MAX_ATTEMPTS = 5


def request_json(
    opener: urllib.request.OpenerDirector,
    method: str,
    url: str,
    headers: dict[str, str],
    body: bytes | None = None,
    ctx_label: str = "",
) -> dict | list:
    label = ctx_label or url
    last_err = ""
    for attempt in range(1, MAX_ATTEMPTS + 1):
        req = urllib.request.Request(url, data=body, method=method)
        req.add_header("User-Agent", USER_AGENT)
        for k, v in headers.items():
            req.add_header(k, v)
        try:
            with opener.open(req, timeout=300) as resp:
                return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:300]
            if e.code not in RETRYABLE_HTTP or attempt == MAX_ATTEMPTS:
                # A definite client/server verdict (e.g. 400 invalid model) — fail fast.
                sys.exit(f"error: {label}: HTTP {e.code}: {detail}")
            last_err = f"HTTP {e.code}: {detail}"
        except ssl.SSLCertVerificationError as e:
            sys.exit(
                f"error: TLS verification failed for {url}: {e}\n"
                "speech.giga.chat normally verifies with system roots. If you point this at the raw\n"
                "Sber endpoints (Russian Trusted Root CA), set SBER_SALUTE_CA_BUNDLE to the PEM from\n"
                "https://gu-st.ru/content/Other/doc/russiantrustedca.pem, or pass --insecure."
            )
        except (urllib.error.URLError, ConnectionError, TimeoutError, OSError) as e:
            reason = getattr(e, "reason", e)
            if attempt == MAX_ATTEMPTS:
                sys.exit(f"error: {label}: {reason}")
            last_err = str(reason)
        wait = 2 ** (attempt - 1)
        print(f"  {label}: transient error ({last_err}); "
              f"retry {attempt}/{MAX_ATTEMPTS - 1} in {wait}s…", file=sys.stderr)
        time.sleep(wait)
    sys.exit(f"error: {label}: giving up after {MAX_ATTEMPTS} attempts: {last_err}")


def to_pcm16(input_path: Path) -> bytes:
    ffmpeg = os.environ.get("FFMPEG") or shutil.which("ffmpeg")
    if not ffmpeg:
        sys.exit("error: ffmpeg not found on PATH (set FFMPEG=/path/to/ffmpeg)")
    proc = subprocess.run(
        [ffmpeg, "-v", "error", "-i", str(input_path),
         "-ac", "1", "-ar", "16000", "-f", "s16le", "-"],
        capture_output=True,
    )
    if proc.returncode != 0:
        sys.exit(f"error: ffmpeg failed: {proc.stderr.decode('utf-8', 'replace')[:400]}")
    if not proc.stdout:
        sys.exit("error: ffmpeg produced no audio")
    return proc.stdout


def mint_token(opener: urllib.request.OpenerDirector, oauth_url: str,
               auth_key: str, scope: str | None) -> str:
    headers = {
        "Authorization": f"Basic {auth_key}",
        "RqUID": str(uuid.uuid4()),
        "Accept": "application/json",
    }
    # The gateway (speech.giga.chat) ignores scope; only the raw ngw endpoint needs it.
    body = None
    if scope:
        headers["Content-Type"] = "application/x-www-form-urlencoded"
        body = urllib.parse.urlencode({"scope": scope}).encode("ascii")
    v = request_json(opener, "POST", oauth_url, headers=headers, body=body,
                     ctx_label="oauth token")
    token = v.get("tok") or v.get("access_token") or v.get("token")
    if not token:
        sys.exit(f"error: token response has no token field: {json.dumps(v)[:200]}")
    return token


def parse_go_duration(s: str | None) -> float | None:
    """'2.280s' / '2s' / '0.040s' -> seconds."""
    if not isinstance(s, str):
        return None
    try:
        return float(s.strip().removesuffix("s"))
    except ValueError:
        return None


def parse_turns(payload: list) -> list[dict]:
    """Per-speaker partial entries (speaker_id >= 0) carry the turn boundaries; the
    eou=true aggregate has speaker_id = -1 and is skipped. Recognized text rides along
    when present."""
    turns = []
    if not isinstance(payload, list):
        return turns
    for entry in payload:
        sid = (entry.get("speaker_info") or {}).get("speaker_id")
        if not isinstance(sid, int) or sid < 0:
            continue
        results = entry.get("results") or [{}]
        first = results[0] if isinstance(results[0], dict) else {}
        start = parse_go_duration(first.get("start"))
        end = parse_go_duration(first.get("end"))
        if start is None or end is None or end <= start:
            continue
        turns.append({
            "speaker": sid,
            "start_s": round(start, 3),
            "end_s": round(end, 3),
            "text": (first.get("normalized_text") or first.get("text") or "").strip(),
        })
    turns.sort(key=lambda t: (t["start_s"], t["end_s"]))
    return turns


def fmt_ts(seconds: float) -> str:
    s = int(seconds)
    return f"{s // 3600:02d}:{(s % 3600) // 60:02d}:{s % 60:02d}"


def main() -> None:
    ap = argparse.ArgumentParser(
        description="SaluteSpeech speaker diarization (via the speech.giga.chat gateway)")
    ap.add_argument("audio", help="input audio/video file (anything ffmpeg reads, e.g. audio.mp4)")
    ap.add_argument("--speakers", type=int, default=None,
                    help="expected speaker count hint (speaker_separation_options.count)")
    ap.add_argument("--model", default=None,
                    help=f"async recognition model (default {DEFAULT_MODEL}; only "
                         f"{' / '.join(ASYNC_MODELS)} are valid for speaker diarization)")
    ap.add_argument("--env", default=None, help="path to .env with credentials")
    ap.add_argument("--out", default=None,
                    help="output prefix (default: alongside the input file)")
    ap.add_argument("--timeout", type=int, default=600, help="poll ceiling, seconds (default 600)")
    ap.add_argument("--insecure", action="store_true", help="skip TLS verification")
    args = ap.parse_args()

    load_env(args.env)
    auth_key = env_any("SBER_SALUTE_AUTH_KEY", "SALUTESPEECH_AUTH_KEY")
    if not auth_key:
        sys.exit("error: SBER_SALUTE_AUTH_KEY is not set (put it in .env)")
    scope = env_any("SBER_SALUTE_SCOPE", "SALUTESPEECH_SCOPE")
    oauth_url = env_any("SBER_SALUTE_OAUTH_URL", "SALUTESPEECH_OAUTH_URL", default=DEFAULT_OAUTH_URL)
    recognize_url = env_any(
        "SBER_SALUTE_RECOGNIZE_URL", "SALUTESPEECH_RECOGNIZE_URL", default=DEFAULT_RECOGNIZE_URL)
    base = rest_base(recognize_url)
    model = args.model or env_any(
        "SBER_SALUTE_RECOGNITION_MODEL", "SALUTESPEECH_MODEL", default=DEFAULT_MODEL)
    if model == "voice_messaging":
        # Sync-recognize model; the async diarization endpoint rejects it (HTTP 400).
        print(f"warning: model 'voice_messaging' is sync-only; using '{DEFAULT_MODEL}' for "
              "async speaker diarization (valid: " + " / ".join(ASYNC_MODELS) + ")",
              file=sys.stderr)
        model = DEFAULT_MODEL

    input_path = Path(args.audio)
    if not input_path.is_file():
        sys.exit(f"error: no such file: {input_path}")

    print(f"converting {input_path.name} to 16 kHz mono PCM…", file=sys.stderr)
    pcm16 = to_pcm16(input_path)
    print(f"  {len(pcm16) / 1e6:.1f} MB ({len(pcm16) / 32_000:.0f}s of audio)", file=sys.stderr)

    opener = build_opener(args.insecure)
    print("minting access token…", file=sys.stderr)
    token = mint_token(opener, oauth_url, auth_key, scope)
    bearer = {"Authorization": f"Bearer {token}"}

    print("uploading…", file=sys.stderr)
    upload = request_json(
        opener, "POST", f"{base}/data:upload",
        headers={**bearer, "Content-Type": "audio/x-pcm;bit=16;rate=16000"},
        body=pcm16, ctx_label="data:upload",
    )
    request_file_id = (upload.get("result") or {}).get("request_file_id")
    if not request_file_id:
        sys.exit(f"error: upload response has no request_file_id: {json.dumps(upload)[:200]}")

    separation: dict = {"enable": True}
    if args.speakers and args.speakers >= 1:
        separation["count"] = args.speakers
    recognize_body = json.dumps({
        "options": {
            "model": model,
            "audio_encoding": "PCM_S16LE",
            "sample_rate": 16000,
            "language": "ru-RU",
            "speaker_separation_options": separation,
        },
        "request_file_id": request_file_id,
    }).encode("utf-8")

    print(f"starting async recognition (model={model}, hint={args.speakers})…", file=sys.stderr)
    started = request_json(
        opener, "POST", f"{base}/speech:async_recognize",
        headers={**bearer, "Content-Type": "application/json"},
        body=recognize_body, ctx_label="speech:async_recognize",
    )
    task_id = (started.get("result") or {}).get("id")
    if not task_id:
        sys.exit(f"error: async_recognize response has no task id: {json.dumps(started)[:200]}")

    deadline = time.monotonic() + args.timeout
    response_file_id = None
    while time.monotonic() < deadline:
        task = request_json(
            opener, "GET",
            f"{base}/task:get?" + urllib.parse.urlencode({"id": task_id}),
            headers=bearer, ctx_label="task:get",
        )
        status = (task.get("result") or {}).get("status", "")
        if status == "DONE":
            response_file_id = (task.get("result") or {}).get("response_file_id")
            break
        if status in ("ERROR", "CANCELED"):
            sys.exit(f"error: recognition task failed: {json.dumps(task)[:400]}")
        print(f"  task {status or 'PENDING'}…", file=sys.stderr)
        time.sleep(2)
    if not response_file_id:
        sys.exit(f"error: task did not finish within {args.timeout}s")

    payload = request_json(
        opener, "GET",
        f"{base}/data:download?" + urllib.parse.urlencode(
            {"response_file_id": response_file_id}),
        headers=bearer, ctx_label="data:download",
    )
    turns = parse_turns(payload)
    speakers = sorted({t["speaker"] for t in turns})
    print(f"\n{len(turns)} turns, {len(speakers)} speakers detected\n", file=sys.stderr)

    for t in turns:
        text = f" {t['text']}" if t["text"] else ""
        print(f"{fmt_ts(t['start_s'])} Speaker {t['speaker']}:{text}"
              f"  [{t['start_s']:.1f}–{t['end_s']:.1f}s]")

    prefix = Path(args.out) if args.out else input_path.with_suffix("")
    json_path = Path(f"{prefix}.diarization.json")
    csv_path = Path(f"{prefix}.diarization.csv")
    json_path.write_text(
        json.dumps({"model": model, "speakers": speakers, "turns": turns},
                   ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    csv_path.write_text(
        "start_ms,end_ms,cluster_id\n"
        + "".join(f"{round(t['start_s'] * 1000)},{round(t['end_s'] * 1000)},{t['speaker']}\n"
                  for t in turns),
        encoding="utf-8",
    )
    print(f"\nwrote {json_path}\nwrote {csv_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
