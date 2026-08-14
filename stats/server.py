#!/usr/bin/env python3
"""Memento product statistics module for the Traction hub."""
from __future__ import annotations

import asyncio
import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter, defaultdict, deque
from contextlib import asynccontextmanager, suppress
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import FileResponse, JSONResponse

from posthog_sync import sync_once
from storage import (
    connect,
    delete_events_before,
    excluded_device_ids,
    exclusion_clause,
    insert_events,
)

HERE = Path(__file__).resolve().parent
PORT = int(os.environ.get("STATS_PORT", "9901"))
DB_PATH = Path(os.environ.get("STATS_DB", HERE / "data" / "events.db"))
INGEST_TOKEN = os.environ.get("STATS_INGEST_TOKEN", "")
ALLOWED_GATEWAY_IDENTITY_HOSTS = frozenset({
    "gw.multitool.works",
    "gw2.multitool.works",
})


def _gateway_urls(raw: str, path: str, env_name: str) -> tuple[str, ...]:
    """Return exact managed gateway URLs for `path`, or fail closed at startup.

    The env var supplies bases only; the path is appended by this code, so a
    configured value can never redirect a credential to another endpoint.
    """
    result = []
    for value in raw.split(","):
        value = value.strip().rstrip("/")
        if not value:
            continue
        parsed = urllib.parse.urlsplit(value)
        try:
            port = parsed.port
        except ValueError as error:
            raise RuntimeError(f"untrusted {env_name} entry: {value!r}") from error
        if (
            parsed.scheme != "https"
            or parsed.hostname not in ALLOWED_GATEWAY_IDENTITY_HOSTS
            or port not in (None, 443)
            or parsed.username is not None
            or parsed.password is not None
            or parsed.path not in ("", "/")
            or parsed.query
            or parsed.fragment
        ):
            raise RuntimeError(f"untrusted {env_name} entry: {value!r}")
        result.append(f"https://{parsed.hostname}{path}")
    return tuple(dict.fromkeys(result))


def _gateway_identity_urls(raw: str) -> tuple[str, ...]:
    """Return exact managed `/me` URLs or fail closed at process startup."""
    return _gateway_urls(raw, "/me", "STATS_GATEWAY_IDENTITY_URLS")


def _gateway_product_stats_urls(raw: str) -> tuple[str, ...]:
    """Return exact managed `/product-stats/memento` URLs, or fail closed."""
    return _gateway_urls(
        raw, "/product-stats/memento", "STATS_GATEWAY_PRODUCT_STATS_URLS"
    )


GATEWAY_IDENTITY_URLS = _gateway_identity_urls(os.environ.get(
    "STATS_GATEWAY_IDENTITY_URLS",
    "https://gw.multitool.works,https://gw2.multitool.works",
))
# Managed DeepSeek spend for THIS product. Both hosts are queried and summed:
# an install fails over between them, and each host meters only what it served,
# so the two are disjoint halves of one bill.
GATEWAY_PRODUCT_STATS_URLS = _gateway_product_stats_urls(os.environ.get(
    "STATS_GATEWAY_PRODUCT_STATS_URLS",
    "https://gw.multitool.works,https://gw2.multitool.works",
))
# Read-only, Memento-scoped gateway secret (PRODUCT_STATS_SECRET_MEMENTO on the
# gateway). Deliberately NOT the gateway admin secret, which would also open
# conversation traces and every product's install leaderboard. Unset = the
# spend panel stays off, which is the state on any box without the drop-in.
GATEWAY_PRODUCT_STATS_SECRET = os.environ.get("STATS_GATEWAY_PRODUCT_STATS_SECRET", "")
INSTALL_AUTH_CACHE_SECONDS = max(
    0, min(int(os.environ.get("STATS_INSTALL_AUTH_CACHE_SECONDS", "60")), 60)
)
INSTALL_RATE_LIMIT_PER_MINUTE = max(
    1, int(os.environ.get("STATS_INSTALL_RATE_LIMIT_PER_MINUTE", "12"))
)
AUTH_VALIDATION_RATE_LIMIT_PER_MINUTE = max(
    1, int(os.environ.get("STATS_AUTH_VALIDATION_RATE_LIMIT_PER_MINUTE", "1200"))
)
STATIC_RATE_LIMIT_PER_MINUTE = max(
    1, int(os.environ.get("STATS_STATIC_RATE_LIMIT_PER_MINUTE", "60"))
)
# The spend panel is a slow-moving number behind an unauthenticated GET, so it
# is cached hard and independently rate limited — the dashboard must not be
# usable as an amplifier against the gateway's admin surface.
GATEWAY_SPEND_CACHE_SECONDS = max(
    30, min(int(os.environ.get("STATS_GATEWAY_SPEND_CACHE_SECONDS", "300")), 3600)
)
GATEWAY_SPEND_RATE_LIMIT_PER_MINUTE = max(
    1, int(os.environ.get("STATS_GATEWAY_SPEND_RATE_LIMIT_PER_MINUTE", "20"))
)
RETENTION_DAYS = max(1, int(os.environ.get("STATS_RETENTION_DAYS", "365")))
VERSION_FILE = HERE / "VERSION"
VERSION = VERSION_FILE.read_text().strip() if VERSION_FILE.exists() else "dev"
MAX_BODY_BYTES = 1_000_000
DAY = 86_400.0
REPORTING_TZ = ZoneInfo("Europe/Moscow")

CAPTURE_COMPLETED = {"meeting_ended", "import_audio_completed"}
VALUE_EVENTS = {
    *CAPTURE_COMPLETED,
    "summary_copied",
    "transcript_copied",
    "knowledge_search_completed",
    "meeting_chat_response_completed",
    "collection_chat_response_completed",
    "memory_record_accepted",
    "analytics_report_completed",
}
CAPTURE_STARTED = {"recording_started", "import_audio_started"}
COPY_EVENTS = {"summary_copied", "transcript_copied"}
SUCCESS_PROPERTY_EVENTS = {"import_audio_completed", "summary_generation_completed"}

_db = connect(DB_PATH)
_install_auth_cache: dict[str, tuple[float, str]] = {}
_install_request_times: dict[str, deque[float]] = defaultdict(deque)
_auth_validation_times: deque[float] = deque()
_static_request_times: deque[float] = deque()
_gateway_spend_times: deque[float] = deque()
# days -> (monotonic_expiry, payload)
_gateway_spend_cache: dict[int, tuple[float, dict[str, Any]]] = {}


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: ANN001
        # Never forward an install bearer token beyond the exact configured
        # gateway `/me` URL.
        return None


def _inspect_install_token(url: str, token: str) -> tuple[str, str | None]:
    """Ask the managed gateway to validate a client JWT.

    The stats service never receives the gateway signing key. It accepts only
    `/me` responses for the Memento product and uses the verified device id as
    the event actor, ignoring any actor claimed in the request body.
    """
    request = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {token}", "Accept": "application/json"},
    )
    try:
        opener = urllib.request.build_opener(_NoRedirect)
        with opener.open(request, timeout=5) as response:  # noqa: S310
            payload = json.loads(response.read(MAX_BODY_BYTES))
    except urllib.error.HTTPError as error:
        return ("invalid", None) if error.code in (401, 403) else ("unavailable", None)
    except (OSError, ValueError, json.JSONDecodeError):
        return "unavailable", None
    device = payload.get("deviceId") if isinstance(payload, dict) else None
    if (
        isinstance(payload, dict)
        and payload.get("ok") is True
        and payload.get("product") == "memento"
        and isinstance(device, str)
        and 0 < len(device) <= 200
    ):
        return "valid", device
    return "invalid", None


async def _validate_install_token(token: str) -> tuple[str | None, int]:
    digest = hashlib.sha256(token.encode()).hexdigest()
    cached = _install_auth_cache.get(digest)
    if cached and cached[0] > time.monotonic():
        return cached[1], 200

    # Cache misses are the only requests that fan out to the managed gateways.
    # Bound them globally so random bearer tokens cannot amplify a public ingest
    # flood into an outbound gateway flood. A valid cached install is unaffected.
    if _auth_validation_rate_limited():
        return None, 429

    saw_definitive_rejection = False
    for url in GATEWAY_IDENTITY_URLS:
        state, device = await asyncio.to_thread(_inspect_install_token, url, token)
        if state == "valid" and device:
            if len(_install_auth_cache) >= 10_000:
                _install_auth_cache.clear()
                # The cache is only an availability optimization. Clearing it
                # is safer than allowing unbounded per-install memory growth.
            _install_auth_cache[digest] = (
                time.monotonic() + INSTALL_AUTH_CACHE_SECONDS,
                device,
            )
            return device, 200
        saw_definitive_rejection = saw_definitive_rejection or state == "invalid"
    return None, 401 if saw_definitive_rejection else 503


def _auth_validation_rate_limited() -> bool:
    now = time.monotonic()
    while _auth_validation_times and _auth_validation_times[0] <= now - 60:
        _auth_validation_times.popleft()
    if len(_auth_validation_times) >= AUTH_VALIDATION_RATE_LIMIT_PER_MINUTE:
        return True
    _auth_validation_times.append(now)
    return False


def _install_rate_limited(device: str) -> bool:
    now = time.monotonic()
    requests = _install_request_times[device]
    while requests and requests[0] <= now - 60:
        requests.popleft()
    if len(requests) >= INSTALL_RATE_LIMIT_PER_MINUTE:
        return True
    requests.append(now)
    return False


def _static_rate_limited() -> bool:
    now = time.monotonic()
    while _static_request_times and _static_request_times[0] <= now - 60:
        _static_request_times.popleft()
    if len(_static_request_times) >= STATIC_RATE_LIMIT_PER_MINUTE:
        return True
    _static_request_times.append(now)
    return False


def _gateway_spend_rate_limited() -> bool:
    now = time.monotonic()
    while _gateway_spend_times and _gateway_spend_times[0] <= now - 60:
        _gateway_spend_times.popleft()
    if len(_gateway_spend_times) >= GATEWAY_SPEND_RATE_LIMIT_PER_MINUTE:
        return True
    _gateway_spend_times.append(now)
    return False


def _fetch_product_stats(url: str, secret: str, days: int) -> dict[str, Any] | None:
    """One gateway host's Memento usage aggregate, or None when unavailable.

    Returns None for every failure — unreachable host, non-200, unparseable or
    wrong-shaped body. The caller distinguishes "no host answered" from "a host
    answered zero", which matters: a silent zero would read as "we spent
    nothing today" during an outage.
    """
    request = urllib.request.Request(
        f"{url}?days={days}",
        headers={"x-product-stats-secret": secret, "accept": "application/json"},
        method="GET",
    )
    try:
        opener = urllib.request.build_opener(_NoRedirect)
        with opener.open(request, timeout=6) as response:
            payload = json.loads(response.read(MAX_BODY_BYTES))
    except (urllib.error.HTTPError, OSError, ValueError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict) or payload.get("product") != "memento":
        return None
    return payload


def _num(value: Any) -> float:
    return float(value) if isinstance(value, (int, float)) and not isinstance(value, bool) else 0.0


def _merge_product_stats(parts: list[dict[str, Any]]) -> dict[str, Any]:
    """Sum the gateway hosts' aggregates into one Memento bill.

    Usage rows live on exactly the host that served them, so spend, requests
    and tokens add up cleanly. Install counts deliberately do NOT: an install
    that re-registered after a failover exists in both registries, so summing
    them would overstate the fleet. They are reported per host instead.
    """
    totals = {"requests": 0.0, "inputTokens": 0.0, "outputTokens": 0.0, "costUsd": 0.0}
    today = {"requests": 0.0, "costUsd": 0.0}
    daily: dict[str, dict[str, float]] = {}
    for part in parts:
        part_totals = part.get("totals") or {}
        for key in totals:
            totals[key] += _num(part_totals.get(key))
        part_today = part.get("todayTotals") or {}
        for key in today:
            today[key] += _num(part_today.get(key))
        for row in part.get("daily") or []:
            if not isinstance(row, dict) or not isinstance(row.get("day"), str):
                continue
            bucket = daily.setdefault(
                row["day"],
                {"requests": 0.0, "inputTokens": 0.0, "outputTokens": 0.0, "costUsd": 0.0},
            )
            for key in bucket:
                bucket[key] += _num(row.get(key))
    return {
        "totals": totals,
        "todayTotals": today,
        "daily": [
            {"day": day, **values}
            for day, values in sorted(daily.items(), key=lambda kv: kv[0], reverse=True)
        ],
    }


def _gateway_spend(days: int) -> dict[str, Any] | None:
    """Memento's managed DeepSeek spend, merged across gateway hosts.

    None when nothing is configured or no host answered — the panel then stays
    hidden rather than drawing a confident zero.
    """
    if not GATEWAY_PRODUCT_STATS_SECRET or not GATEWAY_PRODUCT_STATS_URLS:
        return None
    parts = []
    hosts = []
    for url in GATEWAY_PRODUCT_STATS_URLS:
        payload = _fetch_product_stats(url, GATEWAY_PRODUCT_STATS_SECRET, days)
        host = urllib.parse.urlsplit(url).hostname or url
        if payload is None:
            hosts.append({"host": host, "ok": False})
            continue
        parts.append(payload)
        hosts.append({
            "host": host,
            "ok": True,
            "installs": int(_num(payload.get("installs"))),
            "activeToday": int(_num(payload.get("activeToday"))),
            "costUsd": _num((payload.get("totals") or {}).get("costUsd")),
        })
    if not parts:
        return None
    merged = _merge_product_stats(parts)
    merged["days"] = days
    merged["hosts"] = hosts
    # True when at least one host failed: the numbers are then a floor, not a
    # total, and the dashboard says so rather than quietly under-reporting.
    merged["partial"] = any(not h["ok"] for h in hosts)
    merged["updated_at"] = datetime.now(timezone.utc).isoformat()
    return merged


async def _posthog_sync_loop() -> None:
    interval = max(30, int(os.environ.get("POSTHOG_SYNC_INTERVAL_SECONDS", "300")))
    while True:
        try:
            result = await asyncio.to_thread(sync_once, _db)
            if result.get("enabled"):
                print(f"[posthog-sync] {json.dumps(result, sort_keys=True)}", flush=True)
        except Exception as error:  # the dashboard remains live when PostHog is down
            print(f"[posthog-sync] failed: {error}", flush=True)
        await asyncio.sleep(interval)


async def _retention_loop() -> None:
    while True:
        deleted = await asyncio.to_thread(
            delete_events_before,
            _db,
            time.time() - RETENTION_DAYS * DAY,
        )
        if deleted:
            print(f"[retention] deleted {deleted} expired events", flush=True)
        await asyncio.sleep(DAY)


@asynccontextmanager
async def lifespan(_: FastAPI):
    tasks = [asyncio.create_task(_retention_loop())]
    if os.environ.get("POSTHOG_PERSONAL_API_KEY") and os.environ.get("POSTHOG_PROJECT_ID"):
        tasks.append(asyncio.create_task(_posthog_sync_loop()))
    yield
    for task in tasks:
        task.cancel()
    for task in tasks:
        with suppress(asyncio.CancelledError):
            await task


app = FastAPI(lifespan=lifespan)

# --- Traction observer mode --------------------------------------------------
# The Traction hub proxies this dashboard to external observers and stamps
# every such request with X-Traction-Observer. Product metrics are fair game
# for them; infrastructure cost is not — DeepSeek spend is our commercial data,
# not a product KPI. The header only ever REMOVES access, so a direct caller
# setting it gains nothing. Mirrors the same guard in the MultiTool module.
_OBSERVER_BLOCKED_PREFIXES = ("/gateway-spend",)


@app.middleware("http")
async def _observer_guard(request: Request, call_next):  # noqa: ANN001, ANN201
    if request.headers.get("x-traction-observer"):
        if request.method not in ("GET", "HEAD"):
            return JSONResponse({"error": "observer mode is read-only"}, status_code=403)
        path = request.url.path
        if any(
            path == prefix or path.startswith(prefix + "/")
            for prefix in _OBSERVER_BLOCKED_PREFIXES
        ):
            return JSONResponse(
                {"error": "not available in observer mode"}, status_code=403
            )
    return await call_next(request)


def _day_index(timestamp: float) -> int:
    return datetime.fromtimestamp(timestamp, REPORTING_TZ).date().toordinal()


def _window(days: float, now: float | None = None) -> tuple[float, float, str]:
    window_days = max(1, min(int(float(days) + 0.999999), 365))
    current = datetime.fromtimestamp(time.time() if now is None else now, REPORTING_TZ)
    day_start = current.replace(hour=0, minute=0, second=0, microsecond=0)
    since = (day_start - timedelta(days=window_days - 1)).timestamp()
    label = "today MSK" if window_days == 1 else f"{window_days} Moscow dates"
    return float(window_days), since, label


def _props(raw: str | None) -> dict[str, str]:
    try:
        value = json.loads(raw or "{}")
        return value if isinstance(value, dict) else {}
    except json.JSONDecodeError:
        return {}


def _true(value: Any) -> bool:
    return str(value).lower() == "true"


def _successful(name: str, properties: dict[str, str]) -> bool:
    return name not in SUCCESS_PROPERTY_EVENTS or _true(
        properties.get("success", "true")
    )


def _is_value(name: str, properties: dict[str, str]) -> bool:
    return name in VALUE_EVENTS and _successful(name, properties)


def _is_error(name: str, properties: dict[str, str]) -> bool:
    return (
        name == "error"
        or name.endswith("_error")
        or name.endswith(".failed")
        or (name == "meeting_ended" and _true(properties.get("had_fatal_error")))
        or (
            name in {"summary_generation_completed", "import_audio_completed"}
            and not _successful(name, properties)
        )
    )


def _positive_number(properties: dict[str, str], *keys: str) -> tuple[float, bool]:
    saw_invalid = False
    for key in keys:
        if key not in properties:
            continue
        try:
            value = float(properties[key])
        except (TypeError, ValueError):
            saw_invalid = True
            continue
        if value < 0 or value != value or value == float("inf"):
            saw_invalid = True
            continue
        return value, True
    return 0.0, not saw_invalid


def _fmt_hours(seconds: float) -> str:
    if seconds < 3600:
        return f"{seconds / 60:.0f}m"
    return f"{seconds / 3600:.1f}h"


def _fmt_rate(value: float | None) -> str:
    return "—" if value is None else f"{value * 100:.0f}%"


def _iso(ts: float | None) -> str | None:
    return datetime.fromtimestamp(ts, timezone.utc).isoformat(timespec="seconds") if ts else None


def _human_session_count(since: float, until: float) -> int:
    if not HUMAN_EVENT_NAMES:
        return 0
    clause, params = exclusion_clause(excluded_device_ids())
    names = list(HUMAN_EVENT_NAMES)
    placeholders = ",".join("?" for _ in names)
    return _db.execute(
        "SELECT COUNT(*) FROM ("
        " SELECT ts,ts-LAG(ts) OVER (PARTITION BY device_id ORDER BY ts) AS delta"
        " FROM events WHERE ts >= ? AND ts <= ? AND device_id != ''"
        + clause
        + f" AND name IN ({placeholders})"
        + HUMAN_EVENT_EXCLUSION_SQL
        + ") WHERE ts >= ? AND (delta IS NULL OR delta > ?)",
        [
            since - SESSION_GAP_SECONDS,
            until,
            *params,
            *names,
            since,
            SESSION_GAP_SECONDS,
        ],
    ).fetchone()[0]


def _value_event_count(since: float, until: float) -> int:
    clause, params = exclusion_clause(excluded_device_ids())
    value_names = sorted(VALUE_EVENTS)
    success_names = sorted(VALUE_EVENTS & SUCCESS_PROPERTY_EVENTS)
    value_placeholders = ",".join("?" for _ in value_names)
    success_placeholders = ",".join("?" for _ in success_names)
    success_predicate = (
        f"(name NOT IN ({success_placeholders})"
        " OR lower(COALESCE(json_extract(properties,'$.success'),'true')) = 'true')"
        if success_names
        else "1=1"
    )
    return _db.execute(
        "SELECT COUNT(*) FROM events WHERE ts >= ? AND ts <= ? AND device_id != ''"
        + clause
        + f" AND name IN ({value_placeholders}) AND {success_predicate}",
        [since, until, *params, *value_names, *success_names],
    ).fetchone()[0]


@app.get("/health")
async def health() -> dict[str, Any]:
    state = _db.execute(
        "SELECT cursor,updated_at FROM sync_state WHERE source='posthog'"
    ).fetchone()
    page_state = _db.execute(
        "SELECT 1 FROM sync_state WHERE source='posthog_page'"
    ).fetchone()
    return {
        "ok": True,
        "version": VERSION,
        "ingest_enabled": bool(INGEST_TOKEN or GATEWAY_IDENTITY_URLS),
        "ingest_authenticated": bool(INGEST_TOKEN or GATEWAY_IDENTITY_URLS),
        "install_auth": bool(GATEWAY_IDENTITY_URLS),
        "static_auth": bool(INGEST_TOKEN),
        "gateway_spend": bool(GATEWAY_PRODUCT_STATS_SECRET and GATEWAY_PRODUCT_STATS_URLS),
        "retention_days": RETENTION_DAYS,
        "posthog_sync": bool(os.environ.get("POSTHOG_PERSONAL_API_KEY")),
        "posthog_backfill_complete": not bool(page_state),
        "posthog_cursor": _iso(float(state[0])) if state else None,
        "posthog_synced_at": _iso(float(state[1])) if state else None,
    }


@app.post("/events")
async def ingest(request: Request) -> JSONResponse:
    if not INGEST_TOKEN and not GATEWAY_IDENTITY_URLS:
        return JSONResponse({"error": "ingest disabled"}, status_code=503)
    supplied_token = request.headers.get("x-ingest-token", "")
    static_authenticated = bool(
        INGEST_TOKEN and hmac.compare_digest(supplied_token, INGEST_TOKEN)
    )
    verified_device = None
    source = "direct-server" if static_authenticated else "direct-client"
    if static_authenticated:
        if _static_rate_limited():
            return JSONResponse(
                {"error": "rate limited"},
                status_code=429,
                headers={"Retry-After": "60"},
            )
    else:
        authorization = request.headers.get("authorization", "")
        scheme, _, bearer_token = authorization.partition(" ")
        if scheme.lower() != "bearer" or not bearer_token.strip():
            return JSONResponse({"error": "missing install token"}, status_code=401)
        verified_device, status = await _validate_install_token(bearer_token.strip())
        if verified_device is None:
            if status == 503:
                message = "identity service unavailable"
            elif status == 429:
                message = "identity validation rate limited"
            else:
                message = "bad install token"
            headers = {"Retry-After": "60"} if status in (429, 503) else None
            return JSONResponse({"error": message}, status_code=status, headers=headers)
        if _install_rate_limited(verified_device):
            return JSONResponse(
                {"error": "rate limited"},
                status_code=429,
                headers={"Retry-After": "60"},
            )
    try:
        content_length = int(request.headers.get("content-length") or 0)
    except ValueError:
        return JSONResponse({"error": "bad content-length"}, status_code=400)
    if content_length > MAX_BODY_BYTES:
        return JSONResponse({"error": "body too large"}, status_code=413)
    body_raw = await request.body()
    if len(body_raw) > MAX_BODY_BYTES:
        return JSONResponse({"error": "body too large"}, status_code=413)
    try:
        body = json.loads(body_raw)
    except json.JSONDecodeError as error:
        return JSONResponse({"error": f"bad json: {error}"}, status_code=400)
    events = body.get("events", [body]) if isinstance(body, dict) else body
    if not isinstance(events, list) or len(events) > 500:
        return JSONResponse({"error": "expected ≤500 events"}, status_code=400)
    device = body.get("device_id") if isinstance(body, dict) else None
    result = insert_events(
        _db,
        events,
        device,
        source=source,
        authoritative_device=verified_device,
    )
    return JSONResponse({"ok": True, "ingested": result["accepted"], **result})


def compute_product(days: float = 30.0) -> dict[str, Any]:
    now = time.time()
    window_days, since, label = _window(days, now)
    excluded = excluded_device_ids()
    clause, params = exclusion_clause(excluded)
    today = _day_index(now)
    current_date = date.fromordinal(today)
    week_start = today - current_date.weekday()
    month_start = current_date.toordinal() - 29
    # Retention needs at most the latest 90 Moscow dates; longer requested windows
    # still receive their complete selected history.
    history_date = date.fromordinal(today - 90)
    history_since = min(
        since,
        datetime.combine(history_date, datetime.min.time(), REPORTING_TZ).timestamp(),
    )
    rows = _db.execute(
        "SELECT ts,device_id,name,properties,source FROM events"
        " WHERE ts >= ? AND device_id != ''" + clause + " ORDER BY ts",
        [history_since, *params],
    ).fetchall()

    coverage = _db.execute(
        "SELECT COUNT(DISTINCT device_id),MIN(ts),MAX(ts) FROM events"
        " WHERE device_id != ''" + clause,
        params,
    ).fetchone()
    value_names = sorted(VALUE_EVENTS)
    success_value_names = sorted(VALUE_EVENTS & SUCCESS_PROPERTY_EVENTS)
    value_placeholders = ",".join("?" for _ in value_names)
    success_placeholders = ",".join("?" for _ in success_value_names)
    success_predicate = (
        f"(name NOT IN ({success_placeholders})"
        " OR lower(COALESCE(json_extract(properties,'$.success'),'true')) = 'true')"
        if success_value_names
        else "1=1"
    )
    first_value_rows = _db.execute(
        "SELECT device_id,MIN(ts) FROM events WHERE device_id != ''"
        + clause
        + f" AND name IN ({value_placeholders})"
        + f" AND {success_predicate}"
        + " GROUP BY device_id",
        [*params, *value_names, *success_value_names],
    ).fetchall()
    first_value_days = {
        device: _day_index(first_value_ts)
        for device, first_value_ts in first_value_rows
    }

    active_days: dict[str, set[int]] = defaultdict(set)
    value_days: dict[str, set[int]] = defaultdict(set)
    observed: set[str] = set()
    value_devices: set[str] = set()
    capture_started_devices: set[str] = set()
    captured_devices: set[str] = set()
    summary_devices: set[str] = set()
    copy_devices: set[str] = set()
    source_counts: Counter[str] = Counter()
    version_devices: dict[str, set[str]] = defaultdict(set)
    daily: dict[int, dict[str, Any]] = defaultdict(
        lambda: {
            "observed": set(), "value": set(), "recordings": 0, "imports": 0,
            "captured_seconds": 0.0, "summaries": 0, "errors": 0, "events": 0,
        }
    )
    counters: Counter[str] = Counter()
    captured_seconds = 0.0
    invalid_durations = 0
    first_ts = coverage[1]
    last_ts = coverage[2]

    for ts, device, name, raw_properties, source in rows:
        properties = _props(raw_properties)
        day = _day_index(ts)
        active_days[device].add(day)
        if _is_value(name, properties):
            value_days[device].add(day)
        if ts < since or ts > now:
            continue

        observed.add(device)
        source_counts[source or "unknown"] += 1
        version_devices[properties.get("app_version", "unknown")].add(device)
        item = daily[day]
        item["events"] += 1
        item["observed"].add(device)
        if _is_value(name, properties):
            value_devices.add(device)
            item["value"].add(device)
        if name in CAPTURE_STARTED:
            capture_started_devices.add(device)
        if name == "meeting_ended":
            counters["recordings"] += 1
            captured_devices.add(device)
            item["recordings"] += 1
            seconds, valid = _positive_number(
                properties, "total_duration_seconds", "active_duration_seconds"
            )
            invalid_durations += int(not valid)
            captured_seconds += seconds
            item["captured_seconds"] += seconds
            counters["fatal_recordings"] += int(_true(properties.get("had_fatal_error")))
        elif name == "import_audio_started":
            counters["import_starts"] += 1
        elif name == "import_audio_completed" and _successful(name, properties):
            counters["imports"] += 1
            captured_devices.add(device)
            item["imports"] += 1
            seconds, valid = _positive_number(properties, "duration_seconds")
            invalid_durations += int(not valid)
            captured_seconds += seconds
            item["captured_seconds"] += seconds
        elif name == "summary_generation_completed":
            counters["summary_attempts"] += 1
            if _successful(name, properties):
                counters["summaries"] += 1
                summary_devices.add(device)
                item["summaries"] += 1
        if name in COPY_EVENTS:
            counters["copies"] += 1
            copy_devices.add(device)
        if name == "transcription_error":
            counters["transcription_errors"] += 1
        if _is_error(name, properties):
            counters["errors"] += 1
            item["errors"] += 1

    def period_active(source: dict[str, set[int]], low: int) -> int:
        return sum(any(low <= day <= today for day in days_set) for days_set in source.values())

    horizons = (1, 7, 30)
    retention: dict[str, float | None] = {}
    cohort_sizes: dict[str, int] = {}
    for horizon in horizons:
        mature = [
            (device, first_day, value_days.get(device, set()))
            for device, first_day in first_value_days.items()
            if first_day >= today - 90 and first_day + horizon <= today
        ]
        returned = sum(
            any(first_day < day <= first_day + horizon for day in days_set)
            for _, first_day, days_set in mature
        )
        retention[f"d{horizon}"] = returned / len(mature) if mature else None
        cohort_sizes[f"d{horizon}"] = len(mature)

    summary_rate = (
        counters["summaries"] / counters["summary_attempts"]
        if counters["summary_attempts"] else None
    )
    fatal_rate = (
        counters["fatal_recordings"] / counters["recordings"]
        if counters["recordings"] else None
    )
    import_rate = (
        counters["imports"] / counters["import_starts"]
        if counters["import_starts"] else None
    )
    captured = counters["recordings"] + counters["imports"]
    window_start_day = int(since // DAY)
    new_value_devices = sum(
        first_value_days.get(device, today + 1) >= window_start_day
        for device in value_devices
    )
    daily_out = []
    for day in range(today - max(1, int(round(window_days))) + 1, today + 1):
        item = daily[day]
        daily_out.append(
            {
                "date": date.fromordinal(day).isoformat(),
                "dau": len(item["observed"]),
                "value_devices": len(item["value"]),
                "recordings": item["recordings"],
                "imports": item["imports"],
                "captured_seconds": round(item["captured_seconds"], 1),
                "summaries": item["summaries"],
                "errors": item["errors"],
                "events": item["events"],
            }
        )

    return {
        # Preserve sub-second precision: /summary reuses this exact timestamp as
        # its inclusive upper bound. Truncating it to whole seconds made freshly
        # ingested events disappear until the next poll.
        "updated_at": datetime.fromtimestamp(now, timezone.utc).isoformat(),
        "window_days": window_days,
        "window_label": label,
        "identity": {
            "unit": "analytics-enabled anonymous device",
            "observed_devices": len(observed),
            "total_seen_devices": coverage[0],
            "excluded_internal_devices": len(excluded),
        },
        "growth": {
            "dau": period_active(active_days, today),
            "wau": period_active(active_days, week_start),
            "mau": period_active(active_days, month_start),
            "stickiness": (
                period_active(active_days, today) / period_active(active_days, month_start)
                if period_active(active_days, month_start) else None
            ),
            "weekly_value_devices": period_active(value_days, week_start),
            "value_devices_in_window": len(value_devices),
            "new_value_devices": new_value_devices,
            "returning_value_devices": len(value_devices) - new_value_devices,
        },
        "usage": {
            "captured_memories": captured,
            "recordings": counters["recordings"],
            "imports": counters["imports"],
            "captured_seconds": round(captured_seconds, 1),
            "average_capture_seconds": round(captured_seconds / captured, 1) if captured else None,
            "successful_summaries": counters["summaries"],
            "copies": counters["copies"],
            "memories_per_value_device": round(captured / len(value_devices), 2)
            if value_devices else None,
        },
        "funnel": [
            {"step": "Observed", "devices": len(observed)},
            {"step": "Capture started", "devices": len(capture_started_devices)},
            {"step": "Memory completed", "devices": len(captured_devices)},
            {"step": "Summary completed", "devices": len(summary_devices)},
            {"step": "Content copied", "devices": len(copy_devices)},
        ],
        "retention": {"mode": "rolling value retention", "rates": retention, "cohorts": cohort_sizes},
        "weekly_cohorts": compute_retention()["weekly_cohorts"],
        "quality": {
            "fatal_recordings": counters["fatal_recordings"],
            "fatal_recording_rate": fatal_rate,
            "summary_attempts": counters["summary_attempts"],
            "summary_success_rate": summary_rate,
            "import_success_rate": import_rate,
            "transcription_errors": counters["transcription_errors"],
            "errors": counters["errors"],
        },
        "data_quality": {
            "first_event_at": _iso(first_ts),
            "last_event_at": _iso(last_ts),
            "invalid_duration_events": invalid_durations,
            "sources": dict(source_counts),
            "notes": [
                "Opt-in devices only; devices are not people.",
                "Summary adoption is an attempt-count proxy until summary events carry meeting_id_hash.",
                "D1/D7/D30 use mature first-value cohorts from the latest 90 days.",
            ],
        },
        "versions": [
            {"version": version, "devices": len(devices)}
            for version, devices in sorted(
                version_devices.items(), key=lambda item: len(item[1]), reverse=True
            )[:8]
        ],
        "daily": daily_out,
    }


@app.get("/summary")
def summary(days: float = 1.0) -> JSONResponse:
    product = compute_product(days)
    excluded = excluded_device_ids()
    clause, params = exclusion_clause(excluded)
    now = datetime.fromisoformat(product["updated_at"]).timestamp()
    _, since, label = _window(days, now)
    installs = _db.execute(
        "SELECT COUNT(DISTINCT device_id) FROM events WHERE device_id != ''" + clause,
        params,
    ).fetchone()[0]
    window_params: list[Any] = [since, now, *params]
    events = _db.execute(
        "SELECT COUNT(*) FROM events WHERE ts >= ? AND ts <= ?" + clause,
        window_params,
    ).fetchone()[0]
    errors = product["quality"]["errors"]
    observed = product["identity"]["observed_devices"]
    usage = product["usage"]
    retention = product["retention"]["rates"]
    quality = product["quality"]
    current = datetime.fromtimestamp(now, REPORTING_TZ)
    day_start = current.replace(hour=0, minute=0, second=0, microsecond=0).timestamp()
    canonical_dau = product["growth"]["dau"]
    overview = {
        "ever_used": installs,
        "dau": canonical_dau,
        "wau": product["growth"]["wau"],
        "mau": product["growth"]["mau"],
    }
    if canonical_dau:
        overview["sessions_per_dau"] = round(
            _human_session_count(day_start, now) / canonical_dau, 2
        )
        # For Memento a "tool" is a successful product value action: a
        # completed capture/import or an explicit reuse action such as copy,
        # search, chat response, or accepted structured memory.
        overview["tools_per_dau"] = round(
            _value_event_count(day_start, now) / canonical_dau, 2
        )
    payload = {
        "updated_at": product["updated_at"],
        "window_days": product["window_days"],
        "installs": installs,
        "dau": observed,
        "events": events,
        "errors": errors,
        "overview": overview,
        "metrics": [
            {"label": "Weekly value devices", "value": str(product["growth"]["weekly_value_devices"])},
            {"label": f"Captured memories {label}", "value": str(usage["captured_memories"])},
            {"label": f"Captured {label}", "value": _fmt_hours(usage["captured_seconds"])},
            {"label": f"Summaries {label}", "value": str(usage["successful_summaries"])},
            {"label": "Value retention D7", "value": _fmt_rate(retention["d7"])},
            {
                "label": "Fatal recording rate",
                "value": _fmt_rate(quality["fatal_recording_rate"]),
                "good": quality["fatal_recording_rate"] in (None, 0),
            },
            {"label": "Observed installs", "value": str(installs)},
        ] + _retention_cards(),
    }
    # Недельные когорты уже посчитаны внутри compute_product — переиспользуем
    # их, чтобы не гонять второй проход по таблице событий.
    payload["metrics"].extend(
        _canonical_cards(now, {"weekly_cohorts": product.get("weekly_cohorts") or {}})
    )
    return JSONResponse(payload, headers={"Cache-Control": "no-store"})


@app.get("/product")
def product(days: float = 30.0) -> JSONResponse:
    return JSONResponse(compute_product(days), headers={"Cache-Control": "no-store"})


@app.get("/gateway-spend")
async def gateway_spend(days: float = 30.0) -> JSONResponse:
    """Memento's managed DeepSeek spend on the shared gateway.

    Owner-only: blocked in observer mode above. 503 when unconfigured or when
    no gateway host answered — the dashboard hides the panel rather than
    showing a zero that would read as "this costs nothing".
    """
    window = max(1, min(int(float(days) + 0.999999), 365))
    cached = _gateway_spend_cache.get(window)
    if cached and cached[0] > time.monotonic():
        return JSONResponse(cached[1], headers={"Cache-Control": "no-store"})
    if _gateway_spend_rate_limited():
        return JSONResponse(
            {"error": "rate limited"}, status_code=429, headers={"Retry-After": "60"}
        )
    payload = await asyncio.to_thread(_gateway_spend, window)
    if payload is None:
        return JSONResponse(
            {"error": "gateway spend unavailable"},
            status_code=503,
            headers={"Cache-Control": "no-store"},
        )
    # Bounded: the route is unauthenticated and `days` is caller-supplied, so a
    # sweep of distinct windows would otherwise accumulate one entry per value.
    # Evict the oldest insertion rather than clearing everything — a sweep past
    # the ceiling must not throw away the still-valid windows the dashboard is
    # actually using and force them to be re-fetched from the gateway.
    while len(_gateway_spend_cache) >= 16:
        _gateway_spend_cache.pop(next(iter(_gateway_spend_cache)), None)
    _gateway_spend_cache[window] = (
        time.monotonic() + GATEWAY_SPEND_CACHE_SECONDS,
        payload,
    )
    return JSONResponse(payload, headers={"Cache-Control": "no-store"})


@app.get("/dashboard-data")
def dashboard_data(days: float = 30.0) -> JSONResponse:
    data = compute_product(days)
    return JSONResponse({"days": data["daily"]}, headers={"Cache-Control": "no-store"})


@app.get("/retention")
def retention(days: float = 30.0) -> JSONResponse:
    data = compute_product(days)
    return JSONResponse(
        {"growth": data["growth"], "retention": data["retention"]},
        headers={"Cache-Control": "no-store"},
    )


@app.get("/series")
def series(days: float = 90.0) -> JSONResponse:
    data = compute_product(days)
    days_out = [
        {"date": day["date"], "dau": day["dau"], "events": day["events"], "errors": day["errors"]}
        for day in data["daily"]
    ]
    return JSONResponse({"days": days_out}, headers={"Cache-Control": "no-store"})


def compute_retention(now: float | None = None) -> dict:
    """Fleet retention standard (RETENTION_ROLLOUT.md).

    Two complementary views, both per canonical actor (until a product has
    auth, user_id = device_id per the 02.08 pass-through decision):

    - rolling d1/d7/d30 — share of a cohort that returned at least once
      within N days of first use; mature cohorts only, last 90 days,
      size-weighted (MultiTool reference);
    - weekly cohort table — cohort = Moscow ISO week of first appearance,
      distinct returners per week offset (concierge reference).

    Activity = any event on a Moscow date. Cohorts start at the first event
    ever recorded, so young modules simply show short tables.
    """
    now = now or time.time()
    rows = _db.execute(
        "SELECT device_id, ts FROM events WHERE device_id != ''",
    ).fetchall()
    to_day = lambda ts: datetime.fromtimestamp(ts, REPORTING_TZ).date().toordinal()
    active_days: dict[str, set[int]] = {}
    for actor, ts in rows:
        active_days.setdefault(actor, set()).add(to_day(ts))
    today = to_day(now)
    first = {actor: min(days) for actor, days in active_days.items()}

    horizons = (1, 7, 30)
    num = {n: 0 for n in horizons}
    den = {n: 0 for n in horizons}
    for actor, days in active_days.items():
        cohort = first[actor]
        if cohort < today - 90:
            continue
        for n in horizons:
            if cohort + n <= today:  # only cohorts mature enough
                den[n] += 1
                if any(cohort < d <= cohort + n for d in days):
                    num[n] += 1
    rolling = {
        f"d{n}": (num[n] / den[n]) if den[n] else None for n in horizons
    }

    monday = lambda ordinal: ordinal - date.fromordinal(ordinal).weekday()
    this_week = monday(today)
    weeks: dict[str, dict[str, int]] = {}
    sizes: dict[str, int] = {}
    for actor, days in active_days.items():
        cohort_week = monday(first[actor])
        if cohort_week < this_week - 12 * 7:
            continue
        key = date.fromordinal(cohort_week).isoformat()
        sizes[key] = sizes.get(key, 0) + 1
        for offset in {(monday(d) - cohort_week) // 7 for d in days}:
            bucket = weeks.setdefault(key, {})
            bucket[str(offset)] = bucket.get(str(offset), 0) + 1
    return {
        "computed_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "actors": len(active_days),
        "rolling": rolling,
        "weekly_cohorts": {"sizes": sizes, "weeks": weeks},
    }


def _retention_cards() -> list[dict]:
    try:
        rolling = compute_retention()["rolling"]
        return [
            {"label": f"Retention {h.upper()} · rolling, 90d cohorts",
             "value": f"{rolling[h] * 100:.0f}%"}
            for h in ("d1", "d7") if rolling.get(h) is not None
        ]
    except Exception:  # noqa: BLE001 — cards degrade, the core survives
        return []


# «Сессия = использование продукта = активность человека» (Артём/Цев,
# 02.08). Модуль перечисляет СВОИ человеческие события явно: маска вида
# «в имени есть tool» отваливается молча, а тихо завышенные сессии хуже
# отсутствующих. Здесь только то, что эмитится прямо из обработчика жеста.
# Особый случай внутри списка:
#   button_click_start_recording с location=sidebar_auto исключается SQL-
#     предикатом: авто-прослушка сама запускает этот путь без клика;
#   summary_generation_started, custom_prompt_used — рядом живёт
#     setupAutoGeneration (meeting-details/page.tsx); клики по кнопкам
#     уже покрыты button_click_(re)generate_summary;
#   app_started, session_started, page_view_* — жизненный цикл: тот же
#     useEffect отрабатывает и на relaunch() после автообновления;
#   knowledge_search_completed, memory_record_accepted и оба
#     *_chat_response_completed — в клиенте нет эмиттера, только план
#     инструментации (docs/TRACTION_STATS_PLAN.md).
HUMAN_EVENT_NAMES: tuple[str, ...] = (
    "button_click_start_recording",
    "button_click_pause_recording",
    "button_click_resume_recording",
    "button_click_stop_recording",
    "button_click_generate_summary",
    "button_click_regenerate_summary",
    "button_click_stop_summary_generation",
    "button_click_copy_transcript",
    "button_click_toggle_recording_playback",
    "button_click_enhance_transcript",
    "button_click_detect_speakers",
    "button_click_edit_meeting_title",
    "button_click_generate_analytics_report",
    "button_click_cancel_analytics_report",
    "button_click_skip_analytics_questions",
    "button_click_submit_analytics_answers",
    "button_click_skip_analytics_speakers",
    "button_click_submit_analytics_speakers",
    "button_click_reveal_analytics_report",
    "button_click_share_summary_telegram",
    "button_click_save_changes",
    "button_click_find_in_summary",
    "transcript_copied",
    "summary_copied",
    "import_audio_started",
    "enhance_transcript_started",
    "meeting_deleted",
    "microphone_selected",
    "system_audio_selected",
    "language_selected",
    "notification_settings_changed",
    "storage_folder_opened",
    "auto_capture_mode_changed",
    "auto_save_recording_toggled",
    "default_devices_changed",
    "feature_used",
    "settings_changed",
    "analytics_enabled",
    "analytics_disabled",
    "analytics_transparency_viewed",
    "user_id_copied",
)
HUMAN_EVENT_EXCLUSION_SQL = (
    " AND NOT (name = 'button_click_start_recording'"
    " AND COALESCE(json_extract(properties,'$.location'),'') = 'sidebar_auto')"
)
# Машинные события перечисляются так же явно, а не как «всё остальное»:
# дополнение к человеческому списку засасывает нейтральное (обновления,
# версии, технические маркеры) и выдаёт его за работу агента. Здесь —
# конвейер обработки; события приходят из колбэков завершения задач:
# listen('import-complete'), listen('retranscription-complete') и опрос
# статуса LLM. Технические сбои (error, transcription_error) не попадают
# ни в один список. Пустой кортеж = карточки Agent runtime у модуля нет.
MACHINE_EVENT_NAMES: tuple[str, ...] = (
    "summary_generation_completed",
    "import_audio_completed",
    "enhance_transcript_completed",
    "analytics_report_completed",
)
# Разрыв, после которого следующее человеческое событие открывает новую
# сессию (веб-стандарт 30 минут; фрейм Цева — интервалы между user msg,
# «сессия это когда человек касается ноута»).
SESSION_GAP_SECONDS = 5 * 60


def _canonical_cards(now: float, retention: dict | None = None) -> list[dict]:
    """Черновик канона «6 показателей» (Артём/Цева 02.08; ПН решает состав).

    Сессии считаются двумя способами намеренно — в ПН выбирается один:
    - «gap» — интервалы между human-событиями с разрывом 30 минут (фрейм
      Цевы: сессия = когда человек касается ноута);
    - «hourly» — прокси Артёма: уникальные пары (актор, час). Дешевле и
      детерминированнее, но длинный разговор дробится по границам часа.
    Обе смотрят только на HUMAN_EVENT_NAMES, поэтому фоновая обработка не
    надувает сессии; машинное время вынесено в Agent runtime по своему
    списку MACHINE_EVENT_NAMES.
    Плюс Stickiness DAU/MAU (rolling) и Return rate W+1/W+2 из недельных
    когорт. Внутренние устройства отсекаются тем же exclusion_clause, что
    и остальные метрики модуля, иначе карточки разойдутся с ядром.
    Всё деградирует молча, ядро summary неприкосновенно.
    """
    cards: list[dict] = []
    try:
        clause, params = exclusion_clause(excluded_device_ids())
        current = datetime.fromtimestamp(now, REPORTING_TZ)
        day_start = current.replace(hour=0, minute=0, second=0, microsecond=0)
        since = (day_start - timedelta(days=29)).timestamp()
        # Имена — связанные параметры, а не текст запроса; строки событий
        # в Python не вычитываются вовсе: на живых модулях это сотни тысяч
        # строк и +секунды к /summary, из-за которых поллер хаба ловит
        # таймаут и красит модуль в degraded.
        window = " FROM events WHERE ts >= ? AND device_id != ''" + clause
        human = (" AND name IN (" + ",".join("?" * len(HUMAN_EVENT_NAMES)) + ")"
                 if HUMAN_EVENT_NAMES else "")
        names = list(HUMAN_EVENT_NAMES)
        if names:
            # Сессия начинается, когда предыдущее человеческое событие того
            # же актора старше SESSION_GAP_SECONDS (или его вовсе не было).
            gap_sessions = _human_session_count(since, now)
            cards.append({
                "label": "Sessions / day · human, 30-min gap · 30 Moscow dates",
                "value": f"{gap_sessions / 30:.1f}",
            })
            hours = _db.execute(
                "SELECT COUNT(DISTINCT device_id || ':' || CAST(ts / 3600 AS INT))"
                + window + human + HUMAN_EVENT_EXCLUSION_SQL,
                [since, *params, *names]).fetchone()[0]
            cards.append({
                "label": "Sessions / day · human hourly proxy · 30 Moscow dates",
                "value": f"{hours / 30:.1f}",
            })
        if MACHINE_EVENT_NAMES:
            machine = list(MACHINE_EVENT_NAMES)
            agent_hours = _db.execute(
                "SELECT COUNT(DISTINCT device_id || ':' || CAST(ts / 3600 AS INT))"
                + window + " AND name IN ("
                + ",".join("?" * len(machine)) + ")",
                [since, *params, *machine]).fetchone()[0]
            if agent_hours:
                cards.append({
                    "label": "Agent runtime · machine actor-hours / day",
                    "value": f"{agent_hours / 30:.1f}",
                })
        dau, mau = _db.execute(
            "SELECT COUNT(DISTINCT CASE WHEN ts >= ? THEN device_id END),"
            " COUNT(DISTINCT device_id)" + window,
            [day_start.timestamp(), since, *params]).fetchone()
        if mau:
            cards.append({"label": "Stickiness · DAU/MAU rolling",
                          "value": f"{dau / mau * 100:.0f}%"})
    except Exception:  # noqa: BLE001
        pass
    try:
        cohorts = (retention or compute_retention(now))["weekly_cohorts"]
        sizes, weeks = cohorts.get("sizes") or {}, cohorts.get("weeks") or {}
        if sizes:
            current = datetime.fromtimestamp(now, REPORTING_TZ)
            week_start = (current.date()
                          - timedelta(days=current.weekday())).toordinal()
            for offset in (1, 2):
                den = num = 0
                for key, size in sizes.items():
                    # Зрелость: неделя W+offset когорты полностью прожита,
                    # т.е. закончилась до начала текущей недели.
                    cohort = date.fromisoformat(key).toordinal()
                    if cohort + offset * 7 + 7 <= week_start:
                        den += size
                        num += (weeks.get(key) or {}).get(str(offset), 0)
                if den:
                    cards.append({
                        "label": f"Return rate W+{offset} · weekly cohorts",
                        "value": f"{num / den * 100:.0f}%",
                    })
    except Exception:  # noqa: BLE001
        pass
    return cards


@app.get("/")
def dashboard() -> FileResponse:
    return FileResponse(HERE / "index.html")


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT)
