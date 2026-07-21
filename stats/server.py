#!/usr/bin/env python3
"""Memento stats module for the Traction analytics hub.

Built from traction/stats-module-template (GigaTool repo); contract:
specs/stats-hub.md §4. Lives behind `strip_prefix /p/memento` on the hub,
so every URL the dashboard uses is relative.

Routes:
  GET  /            — dashboard (index.html)
  GET  /health      — liveness + deploy version
  GET  /summary     — standard core (?days=N) + Memento metrics[]
  GET  /dashboard-data — daily series for the dashboard chart
  GET  /series      — daily core series (hub history backfill)
  POST /events      — ingest (X-Ingest-Token)

Event schema (sent by the Memento desktop app, see
frontend/src-tauri/src/analytics/traction.rs):
  { "device_id": "...", "events": [ { "ts": 1712..., "name": "meeting_ended",
    "properties": { "total_duration_seconds": "1830.5", ... } } ] }

Metric sources:
  Meetings      — count of `meeting_ended` events (emitted by the Rust core
                  when a recording stops, includes durations)
  Recorded hrs  — sum of total_duration_seconds (fallback active_duration_seconds)
  Summaries     — `summary_generation_completed` with success=true
  DAU           — distinct device_id over the window (core)
  Errors        — `error` + `*_error` events (transcription_error, ...)

Run:  STATS_PORT=9901 python3 server.py
Env:  STATS_PORT (default 9901), STATS_DB (default ./data/events.db),
      STATS_INGEST_TOKEN (empty = no token check, dev only!)
"""
from __future__ import annotations

import json
import os
import sqlite3
import time
from datetime import datetime, timezone
from pathlib import Path

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import FileResponse, JSONResponse

HERE = Path(__file__).resolve().parent
PORT = int(os.environ.get("STATS_PORT", "9901"))
DB_PATH = Path(os.environ.get("STATS_DB", HERE / "data" / "events.db"))
INGEST_TOKEN = os.environ.get("STATS_INGEST_TOKEN", "")
VERSION_FILE = HERE / "VERSION"
VERSION = VERSION_FILE.read_text().strip() if VERSION_FILE.exists() else "dev"

app = FastAPI()

DB_PATH.parent.mkdir(parents=True, exist_ok=True)
_db = sqlite3.connect(DB_PATH, check_same_thread=False)
_db.execute("PRAGMA journal_mode=WAL")
_db.execute(
    "CREATE TABLE IF NOT EXISTS events ("
    " ts REAL NOT NULL, device_id TEXT, name TEXT NOT NULL, properties TEXT)"
)
_db.execute("CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts)")
_db.commit()

# Recorded seconds per meeting_ended row: prefer wall-clock total, fall back
# to active (older events may miss total), 0 if neither parses.
DURATION_SQL = (
    "COALESCE(CAST(json_extract(properties,'$.total_duration_seconds') AS REAL),"
    " CAST(json_extract(properties,'$.active_duration_seconds') AS REAL), 0)"
)
ERROR_NAMES_SQL = "(name = 'error' OR name LIKE '%error')"


@app.get("/health")
def health() -> dict:
    return {"ok": True, "version": VERSION}


@app.post("/events")
async def ingest(request: Request) -> JSONResponse:
    if INGEST_TOKEN and request.headers.get("x-ingest-token") != INGEST_TOKEN:
        return JSONResponse({"error": "bad token"}, status_code=401)
    try:
        body = json.loads(await request.body())
    except json.JSONDecodeError as e:
        return JSONResponse({"error": f"bad json: {e}"}, status_code=400)
    events = body.get("events", [body]) if isinstance(body, dict) else body
    if not isinstance(events, list) or len(events) > 500:
        return JSONResponse({"error": "expected ≤500 events"}, status_code=400)
    device = body.get("device_id") if isinstance(body, dict) else None
    rows = []
    for ev in events:
        if not isinstance(ev, dict) or not ev.get("name"):
            continue
        rows.append((
            float(ev.get("ts") or time.time()),
            str(ev.get("device_id") or device or ""),
            str(ev["name"])[:120],
            json.dumps(ev.get("properties") or {})[:4000],
        ))
    _db.executemany("INSERT INTO events VALUES (?,?,?,?)", rows)
    _db.commit()
    return JSONResponse({"ok": True, "ingested": len(rows)})


def _fmt_hours(seconds: float) -> str:
    if seconds < 3600:
        return f"{seconds / 60:.0f}m"
    return f"{seconds / 3600:.1f}h"


def _window(days: float) -> tuple[float, float, str]:
    window_days = max(0.04, min(float(days), 365.0))
    since = time.time() - window_days * 86400.0
    win = "24h" if abs(window_days - 1.0) < 1e-9 else f"{window_days:g}d"
    return window_days, since, win


@app.get("/summary")
def summary(days: float = 1.0) -> JSONResponse:
    """Core (installs/dau/events/errors) feeds the cross-project Summary
    page; metrics[] is Memento's showcase on its Overview card."""
    window_days, since, win = _window(days)

    def q(sql: str, *args) -> float:
        return _db.execute(sql, args).fetchone()[0]

    installs = q("SELECT COUNT(DISTINCT device_id) FROM events WHERE device_id != ''")
    dau = q(
        "SELECT COUNT(DISTINCT device_id) FROM events WHERE ts > ? AND device_id != ''",
        since,
    )
    events = q("SELECT COUNT(*) FROM events WHERE ts > ?", since)
    errors = q(f"SELECT COUNT(*) FROM events WHERE ts > ? AND {ERROR_NAMES_SQL}", since)
    meetings = q(
        "SELECT COUNT(*) FROM events WHERE ts > ? AND name = 'meeting_ended'", since
    )
    recorded_s = q(
        f"SELECT COALESCE(SUM({DURATION_SQL}), 0) FROM events"
        " WHERE ts > ? AND name = 'meeting_ended'",
        since,
    )
    summaries = q(
        "SELECT COUNT(*) FROM events WHERE ts > ?"
        " AND name = 'summary_generation_completed'"
        " AND json_extract(properties,'$.success') = 'true'",
        since,
    )

    metrics = [
        {"label": f"Meetings {win}", "value": str(meetings)},
        {"label": f"Recorded {win}", "value": _fmt_hours(recorded_s)},
        {"label": f"Summaries {win}", "value": str(summaries)},
        {"label": "DAU" if win == "24h" else f"Devices {win}", "value": str(dau)},
        {
            "label": "Avg meeting",
            "value": _fmt_hours(recorded_s / meetings) if meetings else "—",
        },
        {"label": f"Errors {win}", "value": str(errors), "good": errors == 0},
        {"label": "Installs", "value": str(installs)},
    ]

    payload = {
        "updated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "window_days": window_days,
        "installs": installs,
        "dau": dau,
        "events": events,
        "errors": errors,
        "metrics": metrics,
    }
    # no-store обязателен: закэшированный 200 маскирует мёртвый модуль.
    return JSONResponse(payload, headers={"Cache-Control": "no-store"})


def _daily_rows(since: float) -> list[dict]:
    rows = _db.execute(
        "SELECT date(ts, 'unixepoch') AS day,"
        " SUM(name = 'meeting_ended'),"
        f" SUM(CASE WHEN name = 'meeting_ended' THEN {DURATION_SQL} ELSE 0 END),"
        " SUM(name = 'summary_generation_completed'"
        "     AND json_extract(properties,'$.success') = 'true'),"
        " COUNT(DISTINCT CASE WHEN device_id != '' THEN device_id END),"
        " COUNT(*),"
        f" SUM({ERROR_NAMES_SQL})"
        " FROM events WHERE ts > ? GROUP BY day ORDER BY day",
        (since,),
    ).fetchall()
    return [
        {
            "date": r[0], "meetings": r[1] or 0,
            "recorded_seconds": round(r[2] or 0.0, 1),
            "summaries": r[3] or 0, "dau": r[4], "events": r[5], "errors": r[6] or 0,
        }
        for r in rows
    ]


@app.get("/dashboard-data")
def dashboard_data(days: float = 30.0) -> JSONResponse:
    _, since, _ = _window(days)
    return JSONResponse(
        {"days": _daily_rows(since)}, headers={"Cache-Control": "no-store"}
    )


@app.get("/series")
def series(days: float = 90.0) -> JSONResponse:
    """Daily core series — only for hub Summary history backfill (spec §2.3)."""
    _, since, _ = _window(days)
    days_out = [
        {"date": d["date"], "dau": d["dau"], "events": d["events"], "errors": d["errors"]}
        for d in _daily_rows(since)
    ]
    return JSONResponse({"days": days_out}, headers={"Cache-Control": "no-store"})


@app.get("/")
def dashboard() -> FileResponse:
    return FileResponse(HERE / "index.html")


if __name__ == "__main__":
    # Loopback only: the outside world reaches modules through Caddy (spec §4).
    uvicorn.run(app, host="127.0.0.1", port=PORT)
