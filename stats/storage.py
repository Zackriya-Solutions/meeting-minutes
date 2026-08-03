"""Shared, privacy-minimizing event storage for the Memento stats module."""
from __future__ import annotations

import json
import math
import os
import re
import sqlite3
import threading
import time
from datetime import datetime
from pathlib import Path
from typing import Any, Iterable

SAFE_PROPERTY_KEYS = frozenset(
    line.strip()
    for line in Path(__file__).with_name("safe_properties.txt").read_text().splitlines()
    if line.strip() and not line.lstrip().startswith("#")
)

MAX_PROPERTY_VALUE_LENGTH = 240
MAX_DEVICE_ID_LENGTH = 200
MAX_EVENT_NAME_LENGTH = 120
EVENT_NAME_RE = re.compile(r"^[A-Za-z0-9_$.-]+$")
WRITE_LOCK = threading.Lock()


def connect(path: Path | str) -> sqlite3.Connection:
    db_path = Path(path)
    db_path.parent.mkdir(parents=True, exist_ok=True)
    db = sqlite3.connect(db_path, check_same_thread=False)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA busy_timeout=5000")
    initialize(db)
    return db


def initialize(db: sqlite3.Connection) -> None:
    db.execute(
        "CREATE TABLE IF NOT EXISTS events ("
        " ts REAL NOT NULL, device_id TEXT, name TEXT NOT NULL, properties TEXT,"
        " event_id TEXT, source TEXT NOT NULL DEFAULT 'direct',"
        " ingested_at REAL NOT NULL DEFAULT 0)"
    )
    columns = {row[1] for row in db.execute("PRAGMA table_info(events)")}
    additions = {
        "event_id": "TEXT",
        "source": "TEXT NOT NULL DEFAULT 'direct'",
        "ingested_at": "REAL NOT NULL DEFAULT 0",
    }
    for column, sql_type in additions.items():
        if column not in columns:
            db.execute(f"ALTER TABLE events ADD COLUMN {column} {sql_type}")
    db.execute("CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts)")
    db.execute("CREATE INDEX IF NOT EXISTS idx_events_device_ts ON events(device_id, ts)")
    db.execute("CREATE INDEX IF NOT EXISTS idx_events_name_ts ON events(name, ts)")
    db.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_events_event_id"
        " ON events(event_id) WHERE event_id IS NOT NULL AND event_id != ''"
    )
    db.execute(
        "CREATE TABLE IF NOT EXISTS sync_state ("
        " source TEXT PRIMARY KEY, cursor TEXT NOT NULL, updated_at REAL NOT NULL)"
    )
    db.commit()


def sanitize_properties(value: Any) -> dict[str, str]:
    if not isinstance(value, dict):
        return {}
    safe: dict[str, str] = {}
    for key, raw in value.items():
        if key not in SAFE_PROPERTY_KEYS or raw is None:
            continue
        if isinstance(raw, bool):
            text = "true" if raw else "false"
        elif isinstance(raw, (str, int, float)):
            text = str(raw)
        else:
            continue
        safe[key] = text[:MAX_PROPERTY_VALUE_LENGTH]
    return safe


def parse_timestamp(value: Any) -> float | None:
    try:
        ts = float(value)
    except (TypeError, ValueError):
        if not isinstance(value, str):
            return None
        try:
            ts = datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
        except ValueError:
            return None
    if not math.isfinite(ts) or ts < 1_262_304_000 or ts > time.time() + 86_400:
        return None
    return ts


def insert_events(
    db: sqlite3.Connection,
    events: Any,
    envelope_device: str | None = None,
    source: str = "direct",
    authoritative_device: str | None = None,
) -> dict[str, int]:
    if not isinstance(events, list):
        return {"accepted": 0, "inserted": 0, "duplicates": 0, "rejected": 1}
    rows: list[tuple[float, str, str, str, str | None, str, float]] = []
    rejected = 0
    for event in events:
        if not isinstance(event, dict):
            rejected += 1
            continue
        name = str(event.get("name") or event.get("event") or "")[:MAX_EVENT_NAME_LENGTH]
        device_id = str(
            authoritative_device
            or event.get("device_id")
            or event.get("distinct_id")
            or envelope_device
            or ""
        )[:MAX_DEVICE_ID_LENGTH]
        ts = parse_timestamp(event.get("ts") or event.get("timestamp"))
        if not name or not EVENT_NAME_RE.fullmatch(name) or ts is None:
            rejected += 1
            continue
        properties = sanitize_properties(event.get("properties"))
        event_id_value = (
            event.get("event_id")
            or properties.get("event_id")
            or event.get("uuid")
            or event.get("id")
        )
        event_id = str(event_id_value)[:200] if event_id_value else None
        rows.append(
            (
                ts,
                device_id,
                name,
                json.dumps(properties, separators=(",", ":"), sort_keys=True),
                event_id,
                source[:32],
                time.time(),
            )
        )

    with WRITE_LOCK:
        before = db.total_changes
        db.executemany(
            "INSERT OR IGNORE INTO events"
            " (ts,device_id,name,properties,event_id,source,ingested_at)"
            " VALUES (?,?,?,?,?,?,?)",
            rows,
        )
        db.commit()
        inserted = db.total_changes - before
    return {
        "accepted": len(rows),
        "inserted": inserted,
        "duplicates": len(rows) - inserted,
        "rejected": rejected,
    }


def delete_events_before(db: sqlite3.Connection, cutoff: float) -> int:
    """Delete analytics events outside the documented retention window."""
    with WRITE_LOCK:
        before = db.total_changes
        db.execute("DELETE FROM events WHERE ts < ?", (cutoff,))
        db.commit()
        return db.total_changes - before


def excluded_device_ids() -> set[str]:
    values: list[str] = []
    raw = os.environ.get("STATS_EXCLUDED_DEVICE_IDS", "")
    values.extend(re.split(r"[\s,]+", raw))
    path = Path(
        os.environ.get(
            "STATS_EXCLUDED_DEVICE_IDS_FILE",
            Path(os.environ.get("STATS_DB", "events.db")).with_name("excluded-devices.txt"),
        )
    )
    try:
        values.extend(line.split("#", 1)[0].strip() for line in path.read_text().splitlines())
    except FileNotFoundError:
        pass
    return {value for value in values if value}


def exclusion_clause(excluded: Iterable[str], prefix: str = "") -> tuple[str, list[str]]:
    ids = sorted(set(excluded))
    if not ids:
        return "", []
    column = f"{prefix}device_id"
    return f" AND {column} NOT IN ({','.join('?' for _ in ids)})", ids
