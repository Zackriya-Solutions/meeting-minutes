#!/usr/bin/env python3
"""Idempotently copy opted-in Memento events from PostHog into Traction.

The personal key must be read-only and scoped to the Memento project. Event
properties pass through storage.SAFE_PROPERTY_KEYS before they touch SQLite.
"""
from __future__ import annotations

import json
import os
import time
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from storage import connect, insert_events, parse_timestamp

DEFAULT_HOST = "https://us.posthog.com"


def _iso(ts: float) -> str:
    return datetime.fromtimestamp(ts, timezone.utc).isoformat().replace("+00:00", "Z")


def _request_json(url: str, key: str, allowed_host: str) -> dict[str, Any]:
    parsed = urllib.parse.urlparse(url)
    expected = urllib.parse.urlparse(allowed_host)
    if parsed.scheme != "https" or parsed.netloc != expected.netloc:
        raise RuntimeError("PostHog pagination URL left the configured HTTPS host")
    request = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {key}", "Accept": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=20) as response:
        return json.loads(response.read(8 * 1024 * 1024))


def sync_once(db) -> dict[str, Any]:
    key = os.environ.get("POSTHOG_PERSONAL_API_KEY", "")
    project = os.environ.get("POSTHOG_PROJECT_ID", "")
    host = os.environ.get("POSTHOG_HOST", DEFAULT_HOST).rstrip("/")
    if not key or not project:
        return {"enabled": False, "inserted": 0, "duplicates": 0, "rejected": 0}

    state = db.execute(
        "SELECT cursor FROM sync_state WHERE source='posthog'"
    ).fetchone()
    page_state = db.execute(
        "SELECT cursor FROM sync_state WHERE source='posthog_page'"
    ).fetchone()
    if page_state:
        checkpoint = json.loads(page_state[0])
        url = str(checkpoint["next_url"])
        max_seen = float(checkpoint["max_seen"])
    else:
        overlap = max(0, int(os.environ.get("POSTHOG_SYNC_OVERLAP_SECONDS", "300")))
        if state:
            after_ts = max(1_262_304_000.0, float(state[0]) - overlap)
        else:
            backfill_days = max(1, int(os.environ.get("POSTHOG_BACKFILL_DAYS", "365")))
            after_ts = time.time() - backfill_days * 86_400
        query = urllib.parse.urlencode({"after": _iso(after_ts), "limit": 100})
        url = f"{host}/api/projects/{urllib.parse.quote(project, safe='')}/events/?{query}"
        max_seen = float(state[0]) if state else after_ts

    max_pages = max(1, int(os.environ.get("POSTHOG_SYNC_MAX_PAGES", "2000")))
    totals = {"inserted": 0, "duplicates": 0, "rejected": 0}
    pages = 0

    while url and pages < max_pages:
        payload = _request_json(url, key, host)
        results = payload.get("results") or []
        if not isinstance(results, list):
            raise RuntimeError("PostHog events response has no results list")
        normalized = []
        for event in results:
            if not isinstance(event, dict):
                continue
            properties = event.get("properties") if isinstance(event.get("properties"), dict) else {}
            normalized.append(
                {
                    "event": event.get("event"),
                    "timestamp": event.get("timestamp"),
                    "distinct_id": event.get("distinct_id") or properties.get("distinct_id")
                    or properties.get("$distinct_id"),
                    "uuid": event.get("uuid") or event.get("id"),
                    "properties": properties,
                }
            )
            ts = parse_timestamp(event.get("timestamp"))
            if ts is not None:
                max_seen = max(max_seen, ts)
        result = insert_events(db, normalized, source="posthog")
        for key_name in totals:
            totals[key_name] += result[key_name]
        pages += 1
        next_url = payload.get("next")
        url = urllib.parse.urljoin(host + "/", next_url) if next_url else ""

    if url:
        # Preserve the opaque next-page URL rather than advancing the timestamp
        # cursor: PostHog may paginate newest-first, so max_seen alone could
        # skip older pages in a large backfill.
        checkpoint = json.dumps({"next_url": url, "max_seen": max_seen})
        db.execute(
            "INSERT INTO sync_state(source,cursor,updated_at) VALUES('posthog_page',?,?)"
            " ON CONFLICT(source) DO UPDATE SET cursor=excluded.cursor,updated_at=excluded.updated_at",
            (checkpoint, time.time()),
        )
        db.commit()
        return {
            "enabled": True,
            "pages": pages,
            "cursor": _iso(max_seen),
            "backfill_complete": False,
            **totals,
        }

    db.execute(
        "INSERT INTO sync_state(source,cursor,updated_at) VALUES('posthog',?,?)"
        " ON CONFLICT(source) DO UPDATE SET cursor=excluded.cursor,updated_at=excluded.updated_at",
        (str(max_seen), time.time()),
    )
    db.execute("DELETE FROM sync_state WHERE source='posthog_page'")
    db.commit()
    return {
        "enabled": True,
        "pages": pages,
        "cursor": _iso(max_seen),
        "backfill_complete": True,
        **totals,
    }


def main() -> None:
    db_path = Path(os.environ.get("STATS_DB", Path(__file__).parent / "data" / "events.db"))
    result = sync_once(connect(db_path))
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
