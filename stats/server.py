#!/usr/bin/env python3
"""Memento product statistics module for the Traction hub."""
from __future__ import annotations

import asyncio
import hmac
import json
import os
import time
from collections import Counter, defaultdict
from contextlib import asynccontextmanager, suppress
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import FileResponse, JSONResponse

from posthog_sync import sync_once
from storage import connect, excluded_device_ids, exclusion_clause, insert_events

HERE = Path(__file__).resolve().parent
PORT = int(os.environ.get("STATS_PORT", "9901"))
DB_PATH = Path(os.environ.get("STATS_DB", HERE / "data" / "events.db"))
INGEST_TOKEN = os.environ.get("STATS_INGEST_TOKEN", "")
ALLOW_UNAUTHENTICATED_INGEST = (
    os.environ.get("STATS_ALLOW_UNAUTHENTICATED_INGEST", "") == "1"
)
VERSION_FILE = HERE / "VERSION"
VERSION = VERSION_FILE.read_text().strip() if VERSION_FILE.exists() else "dev"
MAX_BODY_BYTES = 1_000_000
DAY = 86_400.0

CAPTURE_COMPLETED = {"meeting_ended", "import_audio_completed"}
VALUE_EVENTS = {
    *CAPTURE_COMPLETED,
    "summary_copied",
    "transcript_copied",
    "knowledge_search_completed",
    "meeting_chat_response_completed",
    "collection_chat_response_completed",
    "memory_record_accepted",
}
CAPTURE_STARTED = {"recording_started", "import_audio_started"}
COPY_EVENTS = {"summary_copied", "transcript_copied"}
SUCCESS_PROPERTY_EVENTS = {"import_audio_completed", "summary_generation_completed"}

_db = connect(DB_PATH)


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


@asynccontextmanager
async def lifespan(_: FastAPI):
    task = None
    if os.environ.get("POSTHOG_PERSONAL_API_KEY") and os.environ.get("POSTHOG_PROJECT_ID"):
        task = asyncio.create_task(_posthog_sync_loop())
    yield
    if task:
        task.cancel()
        with suppress(asyncio.CancelledError):
            await task


app = FastAPI(lifespan=lifespan)


def _window(days: float) -> tuple[float, float, str]:
    window_days = max(0.04, min(float(days), 365.0))
    since = time.time() - window_days * DAY
    label = "24h" if abs(window_days - 1.0) < 1e-9 else f"{window_days:g}d"
    return window_days, since, label


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
        "ingest_enabled": bool(INGEST_TOKEN or ALLOW_UNAUTHENTICATED_INGEST),
        "ingest_authenticated": bool(INGEST_TOKEN),
        "posthog_sync": bool(os.environ.get("POSTHOG_PERSONAL_API_KEY")),
        "posthog_backfill_complete": not bool(page_state),
        "posthog_cursor": _iso(float(state[0])) if state else None,
        "posthog_synced_at": _iso(float(state[1])) if state else None,
    }


@app.post("/events")
async def ingest(request: Request) -> JSONResponse:
    if not INGEST_TOKEN and not ALLOW_UNAUTHENTICATED_INGEST:
        return JSONResponse({"error": "ingest disabled"}, status_code=503)
    supplied_token = request.headers.get("x-ingest-token", "")
    if INGEST_TOKEN and not hmac.compare_digest(supplied_token, INGEST_TOKEN):
        return JSONResponse({"error": "bad token"}, status_code=401)
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
    result = insert_events(_db, events, device, source="direct")
    return JSONResponse({"ok": True, **result})


def compute_product(days: float = 30.0) -> dict[str, Any]:
    window_days, since, label = _window(days)
    excluded = excluded_device_ids()
    clause, params = exclusion_clause(excluded)
    today = int(time.time() // DAY)
    # Retention needs at most the latest 90 UTC days; longer requested windows
    # still receive their complete selected history.
    history_since = min(since, (today - 90) * DAY)
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
        device: int(first_value_ts // DAY)
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
        day = int(ts // DAY)
        active_days[device].add(day)
        if _is_value(name, properties):
            value_days[device].add(day)
        if ts <= since:
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

    def rolling_active(source: dict[str, set[int]], span: int) -> int:
        low = today - span + 1
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
                "date": datetime.fromtimestamp(day * DAY, timezone.utc).date().isoformat(),
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
        "updated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "window_days": window_days,
        "window_label": label,
        "identity": {
            "unit": "opted-in anonymous device",
            "observed_devices": len(observed),
            "total_seen_devices": coverage[0],
            "excluded_internal_devices": len(excluded),
        },
        "growth": {
            "dau_utc": rolling_active(active_days, 1),
            "wau_rolling_7d": rolling_active(active_days, 7),
            "mau_rolling_30d": rolling_active(active_days, 30),
            "stickiness": (
                rolling_active(active_days, 1) / rolling_active(active_days, 30)
                if rolling_active(active_days, 30) else None
            ),
            "weekly_value_devices": rolling_active(value_days, 7),
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
    _, since, label = _window(days)
    installs = _db.execute(
        "SELECT COUNT(DISTINCT device_id) FROM events WHERE device_id != ''" + clause,
        params,
    ).fetchone()[0]
    window_params: list[Any] = [since, *params]
    events = _db.execute(
        "SELECT COUNT(*) FROM events WHERE ts > ?" + clause, window_params
    ).fetchone()[0]
    errors = product["quality"]["errors"]
    observed = product["identity"]["observed_devices"]
    usage = product["usage"]
    retention = product["retention"]["rates"]
    quality = product["quality"]
    payload = {
        "updated_at": product["updated_at"],
        "window_days": product["window_days"],
        "installs": installs,
        "dau": observed,
        "events": events,
        "errors": errors,
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
        ],
    }
    return JSONResponse(payload, headers={"Cache-Control": "no-store"})


@app.get("/product")
def product(days: float = 30.0) -> JSONResponse:
    return JSONResponse(compute_product(days), headers={"Cache-Control": "no-store"})


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


@app.get("/")
def dashboard() -> FileResponse:
    return FileResponse(HERE / "index.html")


if __name__ == "__main__":
    uvicorn.run(app, host="127.0.0.1", port=PORT)
