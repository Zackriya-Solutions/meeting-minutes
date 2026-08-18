"""Incremental SQLite facts and ready-to-serve period snapshots.

The module deliberately has no FastAPI dependency.  Product stats services can
vendor it unchanged and keep their existing raw event schema and dashboards.
Raw events remain the source of truth; the tables created here are rebuildable
derived state.
"""
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import pathlib
import sqlite3
import threading
import time
import uuid
from collections import deque
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Any, Iterable, Iterator, Sequence
from zoneinfo import ZoneInfo


REPORTING_TZ = ZoneInfo("Europe/Moscow")
UTC = dt.timezone.utc
PERIOD_IDS = (
    "today",
    "yesterday",
    "last_3_dates",
    "last_7_dates",
    "last_30_dates",
    "all_time",
)
CORE_METRICS = (
    ("core.installs", 2),
    ("core.active_actors", 1),
    ("core.events", 1),
    ("core.errors", 2),
)
CORE_METRIC_VERSIONS = dict(CORE_METRICS)


def _iso_now() -> str:
    return dt.datetime.now(UTC).replace(microsecond=0).isoformat()


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def _checksum(value: Any) -> str:
    return hashlib.sha256(_json(value).encode("utf-8")).hexdigest()


def _day_for_timestamp(timestamp: float) -> str:
    return dt.datetime.fromtimestamp(timestamp, REPORTING_TZ).date().isoformat()


@dataclass(frozen=True)
class FactEvent:
    timestamp: float
    actor_id: str
    name: str
    event_id: str = ""
    included: bool = True
    actor_active: bool | None = None
    lifetime_actor: bool | None = None


class MaterializedStats:
    """Owns rebuildable facts and versioned period values in one SQLite file."""

    def __init__(self, database_path: pathlib.Path | str) -> None:
        self.database_path = pathlib.Path(database_path)
        self.database_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        self._telemetry_lock = threading.Lock()
        self._write_wait_samples_ms: deque[float] = deque(maxlen=256)
        self._sqlite_busy_errors = 0
        self._init_database()

    @contextmanager
    def _connect(self) -> Iterator[sqlite3.Connection]:
        connection = sqlite3.connect(self.database_path, timeout=10)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("PRAGMA busy_timeout=10000")
        connection.execute("PRAGMA foreign_keys=ON")
        try:
            yield connection
            connection.commit()
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    @contextmanager
    def _write_connect(self) -> Iterator[sqlite3.Connection]:
        """Acquire SQLite's cross-process writer lock and measure its wait."""
        with self._connect() as connection:
            started = time.perf_counter()
            try:
                connection.execute("BEGIN IMMEDIATE")
            except sqlite3.OperationalError as exc:
                if "busy" in str(exc).lower() or "locked" in str(exc).lower():
                    with self._telemetry_lock:
                        self._sqlite_busy_errors += 1
                raise
            wait_ms = (time.perf_counter() - started) * 1000
            with self._telemetry_lock:
                self._write_wait_samples_ms.append(wait_ms)
            yield connection

    def _init_database(self) -> None:
        with self._connect() as connection:
            connection.executescript(
                """
                CREATE TABLE IF NOT EXISTS stats_actor_dates (
                    day TEXT NOT NULL,
                    actor_id TEXT NOT NULL,
                    PRIMARY KEY(day, actor_id)
                ) WITHOUT ROWID;
                CREATE INDEX IF NOT EXISTS stats_actor_dates_actor
                    ON stats_actor_dates(actor_id, day);

                CREATE TABLE IF NOT EXISTS stats_processed_events (
                    event_id TEXT PRIMARY KEY,
                    event_ts REAL NOT NULL,
                    actor_id TEXT NOT NULL DEFAULT '',
                    day TEXT NOT NULL DEFAULT '',
                    included INTEGER NOT NULL DEFAULT 1,
                    is_error INTEGER NOT NULL DEFAULT 0
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_actor_lifetime (
                    actor_id TEXT PRIMARY KEY,
                    first_day TEXT NOT NULL,
                    last_day TEXT NOT NULL
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_daily_counts (
                    day TEXT PRIMARY KEY,
                    events INTEGER NOT NULL DEFAULT 0 CHECK(events >= 0),
                    errors INTEGER NOT NULL DEFAULT 0 CHECK(errors >= 0),
                    source_watermark REAL
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_dirty_dates (
                    day TEXT PRIMARY KEY,
                    marked_at TEXT NOT NULL
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_metric_runs (
                    run_id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    range_start TEXT,
                    range_end TEXT,
                    input_rows INTEGER NOT NULL DEFAULT 0,
                    output_rows INTEGER NOT NULL DEFAULT 0,
                    duration_ms REAL,
                    checksum TEXT,
                    code_revision TEXT NOT NULL DEFAULT '',
                    error TEXT
                );
                CREATE INDEX IF NOT EXISTS stats_metric_runs_finished
                    ON stats_metric_runs(finished_at DESC);

                CREATE TABLE IF NOT EXISTS stats_metric_values (
                    period_id TEXT NOT NULL,
                    metric_id TEXT NOT NULL,
                    metric_version INTEGER NOT NULL,
                    status TEXT NOT NULL CHECK(status IN
                        ('shadow','validated','published','deprecated')),
                    value_json TEXT NOT NULL,
                    display_value TEXT,
                    period_start TEXT,
                    period_end TEXT NOT NULL,
                    is_partial INTEGER NOT NULL,
                    computed_at TEXT NOT NULL,
                    source_watermark TEXT,
                    run_id TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    PRIMARY KEY(period_id, metric_id, metric_version, status),
                    FOREIGN KEY(run_id) REFERENCES stats_metric_runs(run_id)
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_metric_publications (
                    metric_id TEXT PRIMARY KEY,
                    metric_version INTEGER NOT NULL,
                    published_at TEXT NOT NULL,
                    run_id TEXT NOT NULL
                ) WITHOUT ROWID;

                CREATE TABLE IF NOT EXISTS stats_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                ) WITHOUT ROWID;
                """
            )
            processed_columns = {
                row[1] for row in connection.execute(
                    "PRAGMA table_info(stats_processed_events)"
                )
            }
            for name, definition in (
                ("actor_id", "TEXT NOT NULL DEFAULT ''"),
                ("day", "TEXT NOT NULL DEFAULT ''"),
                ("included", "INTEGER NOT NULL DEFAULT 1"),
                ("is_error", "INTEGER NOT NULL DEFAULT 0"),
            ):
                if name not in processed_columns:
                    connection.execute(
                        f"ALTER TABLE stats_processed_events "
                        f"ADD COLUMN {name} {definition}"
                    )
            now = _iso_now()
            for metric_id, version in CORE_METRICS:
                connection.execute(
                    """INSERT OR IGNORE INTO stats_metric_publications
                       (metric_id, metric_version, published_at, run_id)
                       VALUES (?, ?, ?, 'bootstrap')""",
                    (metric_id, version, now),
                )

    @staticmethod
    def normalize_events(events: Iterable[Sequence[Any] | FactEvent]) -> list[FactEvent]:
        result: list[FactEvent] = []
        for raw in events:
            if isinstance(raw, FactEvent):
                item = raw
            else:
                if len(raw) not in (3, 4, 5, 6, 7):
                    raise ValueError(
                        "fact event needs timestamp, actor_id, name and optional event_id"
                    )
                item = FactEvent(
                    float(raw[0]), str(raw[1] or ""), str(raw[2]),
                    str(raw[3]) if len(raw) >= 4 else "",
                    bool(raw[4]) if len(raw) >= 5 else True,
                    bool(raw[5]) if len(raw) >= 6 else None,
                    bool(raw[6]) if len(raw) >= 7 else None,
                )
            if not item.name or not (0 < item.timestamp < 32503680000):
                raise ValueError("fact event has invalid timestamp or name")
            if not item.event_id:
                item = FactEvent(
                    item.timestamp, item.actor_id, item.name,
                    "derived:" + _checksum(
                        (item.timestamp, item.actor_id, item.name)
                    ),
                    item.included,
                    item.actor_active,
                    item.lifetime_actor,
                )
            result.append(item)
        return result

    def record_events(
        self,
        events: Iterable[Sequence[Any] | FactEvent],
        *,
        error_names: frozenset[str] = frozenset({"error"}),
    ) -> int:
        """Increment facts after raw ingest; safe to replay only after a rebuild.

        Normal online ingest must call this once for rows that were actually
        inserted into the raw event journal.  Sources with event-id deduplication
        should pass only newly inserted rows.
        """
        rows = self.normalize_events(events)
        if not rows:
            return 0
        marked_at = _iso_now()
        with self._lock, self._write_connect() as connection:
            inserted: list[FactEvent] = []
            for event in rows:
                day = _day_for_timestamp(event.timestamp)
                cursor = connection.execute(
                    "INSERT OR IGNORE INTO stats_processed_events"
                    "(event_id,event_ts,actor_id,day,included,is_error) "
                    "VALUES (?,?,?,?,?,?)",
                    (
                        event.event_id, event.timestamp, event.actor_id, day,
                        int(event.included),
                        int(event.included and event.name in error_names),
                    ),
                )
                if cursor.rowcount:
                    inserted.append(event)
            if not inserted:
                return 0
            by_day: dict[str, dict[str, Any]] = {}
            actors: dict[str, tuple[str, str]] = {}
            for event in inserted:
                day = _day_for_timestamp(event.timestamp)
                actor_active = (
                    event.included if event.actor_active is None else event.actor_active
                )
                lifetime_actor = (
                    actor_active
                    if event.lifetime_actor is None else event.lifetime_actor
                )
                if event.actor_id and lifetime_actor:
                    first, last = actors.get(event.actor_id, (day, day))
                    actors[event.actor_id] = (min(first, day), max(last, day))
                if event.included:
                    state = by_day.setdefault(
                        day, {"events": 0, "errors": 0, "max_ts": 0.0}
                    )
                    state["events"] += 1
                    state["errors"] += int(event.name in error_names)
                    state["max_ts"] = max(state["max_ts"], event.timestamp)
                if event.actor_id and actor_active:
                    connection.execute(
                        "INSERT OR IGNORE INTO stats_actor_dates(day,actor_id) VALUES (?,?)",
                        (day, event.actor_id),
                    )
            for actor_id, (first, last) in actors.items():
                connection.execute(
                    """INSERT INTO stats_actor_lifetime(actor_id,first_day,last_day)
                       VALUES (?,?,?) ON CONFLICT(actor_id) DO UPDATE SET
                       first_day=MIN(first_day,excluded.first_day),
                       last_day=MAX(last_day,excluded.last_day)""",
                    (actor_id, first, last),
                )
            for day, state in by_day.items():
                connection.execute(
                    """INSERT INTO stats_daily_counts(day,events,errors,source_watermark)
                       VALUES (?,?,?,?) ON CONFLICT(day) DO UPDATE SET
                       events=events+excluded.events,
                       errors=errors+excluded.errors,
                       source_watermark=MAX(source_watermark,excluded.source_watermark)""",
                    (day, state["events"], state["errors"], state["max_ts"]),
                )
                connection.execute(
                    """INSERT INTO stats_dirty_dates(day,marked_at) VALUES (?,?)
                       ON CONFLICT(day) DO UPDATE SET marked_at=excluded.marked_at""",
                    (day, marked_at),
                )
            connection.execute(
                """INSERT INTO stats_meta(key,value) VALUES ('last_ingested_at',?)
                   ON CONFLICT(key) DO UPDATE SET value=excluded.value""",
                (marked_at,),
            )
        return len(inserted)

    def delete_actor(self, actor_id: str) -> int:
        """Remove one actor from derived facts without rebuilding retained raw.

        The processed-event journal stores the contribution needed to subtract
        additive counts. This keeps unrelated lifetime history intact when the
        raw replay window is shorter than All time.
        """
        if not actor_id:
            return 0
        marked_at = _iso_now()
        with self._lock, self._write_connect() as connection:
            contributions = connection.execute(
                "SELECT day,SUM(included),SUM(is_error),COUNT(*) "
                "FROM stats_processed_events WHERE actor_id=? GROUP BY day",
                (actor_id,),
            ).fetchall()
            dirty_days = {str(row[0]) for row in contributions if row[0]}
            dirty_days.update(
                str(row[0]) for row in connection.execute(
                    "SELECT day FROM stats_actor_dates WHERE actor_id=?",
                    (actor_id,),
                ) if row[0]
            )
            for day, events, errors, _count in contributions:
                if day:
                    connection.execute(
                        "UPDATE stats_daily_counts SET "
                        "events=MAX(0,events-?),errors=MAX(0,errors-?) WHERE day=?",
                        (int(events or 0), int(errors or 0), day),
                    )
            for day in dirty_days:
                connection.execute(
                    "INSERT INTO stats_dirty_dates(day,marked_at) VALUES (?,?) "
                    "ON CONFLICT(day) DO UPDATE SET marked_at=excluded.marked_at",
                    (day, marked_at),
                )
            connection.execute(
                "DELETE FROM stats_actor_dates WHERE actor_id=?", (actor_id,)
            )
            connection.execute(
                "DELETE FROM stats_actor_lifetime WHERE actor_id=?", (actor_id,)
            )
            cursor = connection.execute(
                "DELETE FROM stats_processed_events WHERE actor_id=?", (actor_id,)
            )
        return max(0, int(cursor.rowcount or 0))

    @staticmethod
    def _period_bounds(today: dt.date) -> dict[str, tuple[str | None, str, bool]]:
        tomorrow = today + dt.timedelta(days=1)
        return {
            "today": (today.isoformat(), tomorrow.isoformat(), True),
            "yesterday": (
                (today - dt.timedelta(days=1)).isoformat(), today.isoformat(), False
            ),
            "last_3_dates": (
                (today - dt.timedelta(days=2)).isoformat(), tomorrow.isoformat(), True
            ),
            "last_7_dates": (
                (today - dt.timedelta(days=6)).isoformat(), tomorrow.isoformat(), True
            ),
            "last_30_dates": (
                (today - dt.timedelta(days=29)).isoformat(), tomorrow.isoformat(), True
            ),
            "all_time": (None, tomorrow.isoformat(), True),
        }

    @staticmethod
    def _period_values(
        connection: sqlite3.Connection,
        start: str | None,
        end: str,
    ) -> dict[str, int]:
        where = "day < ?" if start is None else "day >= ? AND day < ?"
        params: tuple[str, ...] = (end,) if start is None else (start, end)
        active_actors = connection.execute(
            f"SELECT COUNT(DISTINCT actor_id) FROM stats_actor_dates WHERE {where}",
            params,
        ).fetchone()[0]
        active_actor_days = connection.execute(
            f"SELECT COUNT(*) FROM stats_actor_dates WHERE {where}", params
        ).fetchone()[0]
        counts = connection.execute(
            f"SELECT COALESCE(SUM(events),0),COALESCE(SUM(errors),0) "
            f"FROM stats_daily_counts WHERE {where}",
            params,
        ).fetchone()
        installs = connection.execute(
            "SELECT COUNT(*) FROM stats_actor_lifetime"
        ).fetchone()[0]
        denominator_start = start
        if denominator_start is None:
            first = connection.execute(
                "SELECT MIN(day) FROM ("
                " SELECT day FROM stats_actor_dates UNION ALL"
                " SELECT day FROM stats_daily_counts)"
            ).fetchone()[0]
            denominator_start = str(first) if first is not None else None
        date_count = (
            max(1, (
                dt.date.fromisoformat(end)
                - dt.date.fromisoformat(denominator_start)
            ).days)
            if denominator_start is not None else 0
        )
        return {
            "installs": int(installs),
            "active_actors": int(active_actors),
            "avg_daily_active_actors": (
                round(int(active_actor_days) / date_count, 2) if date_count else 0.0
            ),
            "events": int(counts[0]),
            "errors": int(counts[1]),
        }

    def refresh(
        self,
        *,
        now: float | None = None,
        kind: str = "incremental",
        code_revision: str = "",
    ) -> dict[str, Any]:
        """Atomically replace published core values for every canonical period."""
        started = time.perf_counter()
        started_at = _iso_now()
        run_id = uuid.uuid4().hex
        current = dt.datetime.fromtimestamp(now or time.time(), REPORTING_TZ)
        bounds = self._period_bounds(current.date())
        with self._lock, self._write_connect() as connection:
            dirty = [row[0] for row in connection.execute(
                "SELECT day FROM stats_dirty_dates ORDER BY day"
            )]
            connection.execute(
                """INSERT INTO stats_metric_runs
                   (run_id,kind,status,started_at,range_start,range_end,code_revision)
                   VALUES (?,?,?,?,?,?,?)""",
                (
                    run_id, kind, "running", started_at,
                    min(dirty, default=None), max(dirty, default=None), code_revision,
                ),
            )
            periods: dict[str, dict[str, Any]] = {}
            for period_id in PERIOD_IDS:
                start, end, partial = bounds[period_id]
                values = self._period_values(connection, start, end)
                periods[period_id] = {
                    "id": period_id,
                    "period_start": start,
                    "period_end": end,
                    "is_partial": partial,
                    **values,
                }

            overview = {
                "ever_used": periods["all_time"]["installs"],
                "dau": periods["today"]["active_actors"],
                "wau": periods["last_7_dates"]["active_actors"],
                "mau": periods["last_30_dates"]["active_actors"],
            }
            source_ts = connection.execute(
                "SELECT MAX(source_watermark) FROM stats_daily_counts"
            ).fetchone()[0]
            source_watermark = (
                dt.datetime.fromtimestamp(source_ts, UTC).replace(microsecond=0).isoformat()
                if source_ts is not None else None
            )
            ingest_row = connection.execute(
                "SELECT value FROM stats_meta WHERE key='last_ingested_at'"
            ).fetchone()
            ingest_watermark = ingest_row[0] if ingest_row is not None else None
            computed_at = _iso_now()
            output_rows = 0
            for metric_id, metric_version in CORE_METRICS:
                connection.execute(
                    "DELETE FROM stats_metric_values WHERE metric_id=? "
                    "AND metric_version<>? AND status='deprecated'",
                    (metric_id, metric_version),
                )
                connection.execute(
                    "UPDATE stats_metric_values SET status='deprecated' "
                    "WHERE metric_id=? AND metric_version<>? AND status='published'",
                    (metric_id, metric_version),
                )
            for period_id, period in periods.items():
                values = {
                    "core.installs": period["installs"],
                    "core.active_actors": period["active_actors"],
                    "core.events": period["events"],
                    "core.errors": period["errors"],
                }
                for metric_id, value in values.items():
                    metric_version = CORE_METRIC_VERSIONS[metric_id]
                    metric_checksum = _checksum({
                        "period_id": period_id,
                        "metric_id": metric_id,
                        "version": metric_version,
                        "value": value,
                    })
                    connection.execute(
                        """INSERT INTO stats_metric_values
                           (period_id,metric_id,metric_version,status,value_json,
                            display_value,period_start,period_end,is_partial,
                            computed_at,source_watermark,run_id,checksum)
                           VALUES (?,?,?,'published',?,?,?,?,?,?,?,?,?)
                           ON CONFLICT(period_id,metric_id,metric_version,status)
                           DO UPDATE SET value_json=excluded.value_json,
                             display_value=excluded.display_value,
                             period_start=excluded.period_start,
                             period_end=excluded.period_end,
                             is_partial=excluded.is_partial,
                             computed_at=excluded.computed_at,
                             source_watermark=excluded.source_watermark,
                             run_id=excluded.run_id,checksum=excluded.checksum""",
                        (
                            period_id, metric_id, metric_version,
                            _json(value), str(value),
                            period["period_start"], period["period_end"],
                            int(period["is_partial"]), computed_at,
                            source_watermark, run_id, metric_checksum,
                        ),
                    )
                    output_rows += 1
            for metric_id, metric_version in CORE_METRICS:
                connection.execute(
                    """INSERT INTO stats_metric_publications
                       (metric_id,metric_version,published_at,run_id)
                       VALUES (?,?,?,?) ON CONFLICT(metric_id) DO UPDATE SET
                       metric_version=excluded.metric_version,
                       published_at=excluded.published_at,run_id=excluded.run_id""",
                    (metric_id, metric_version, computed_at, run_id),
                )
            payload_checksum = _checksum({"periods": periods, "overview": overview})
            duration_ms = (time.perf_counter() - started) * 1000
            connection.execute("DELETE FROM stats_dirty_dates")
            connection.execute(
                """UPDATE stats_metric_runs SET status='published',finished_at=?,
                   input_rows=?,output_rows=?,duration_ms=?,checksum=? WHERE run_id=?""",
                (
                    computed_at, len(dirty), output_rows, duration_ms,
                    payload_checksum, run_id,
                ),
            )
            connection.execute(
                """INSERT INTO stats_meta(key,value) VALUES ('latest_snapshot',?)
                   ON CONFLICT(key) DO UPDATE SET value=excluded.value""",
                (_json({
                    "schema_version": 1,
                    "timezone": "Europe/Moscow",
                    "computed_at": computed_at,
                    "source_watermark": source_watermark,
                    "ingest_watermark": ingest_watermark,
                    "run_id": run_id,
                    "checksum": payload_checksum,
                    "metric_versions": CORE_METRIC_VERSIONS,
                    "overview": overview,
                    "periods": periods,
                }),),
            )
        return self.snapshot()

    def snapshot(self) -> dict[str, Any]:
        """Return the last complete snapshot without running aggregation."""
        with self._connect() as connection:
            row = connection.execute(
                "SELECT value FROM stats_meta WHERE key='latest_snapshot'"
            ).fetchone()
        if row is None:
            return self.refresh(kind="bootstrap")
        return json.loads(row[0])

    def stage_metric_version(
        self,
        metric_id: str,
        metric_version: int,
        values: dict[str, int | float],
        *,
        code_revision: str = "",
    ) -> str:
        """Write a complete shadow revision without changing serving aliases."""
        if not metric_id or metric_version < 1 or set(values) != set(PERIOD_IDS):
            raise ValueError("shadow metric needs an id, version and all periods")
        if any(
            isinstance(value, bool) or not isinstance(value, (int, float))
            for value in values.values()
        ):
            raise ValueError("shadow metric values must be numeric")
        run_id = uuid.uuid4().hex
        started_at = _iso_now()
        with self._lock, self._write_connect() as connection:
            snapshot = json.loads(connection.execute(
                "SELECT value FROM stats_meta WHERE key='latest_snapshot'"
            ).fetchone()[0])
            connection.execute(
                """INSERT INTO stats_metric_runs
                   (run_id,kind,status,started_at,code_revision)
                   VALUES (?, 'shadow', 'shadow', ?, ?)""",
                (run_id, started_at, code_revision),
            )
            connection.execute(
                "DELETE FROM stats_metric_values WHERE metric_id=? "
                "AND metric_version=? AND status IN ('shadow','validated')",
                (metric_id, metric_version),
            )
            for period_id in PERIOD_IDS:
                period = snapshot["periods"][period_id]
                value = values[period_id]
                checksum = _checksum({
                    "period_id": period_id, "metric_id": metric_id,
                    "version": metric_version, "value": value,
                })
                connection.execute(
                    """INSERT INTO stats_metric_values
                       (period_id,metric_id,metric_version,status,value_json,
                        display_value,period_start,period_end,is_partial,
                        computed_at,source_watermark,run_id,checksum)
                       VALUES (?,?,?,'shadow',?,?,?,?,?,?,?,?,?)""",
                    (
                        period_id, metric_id, metric_version, _json(value),
                        str(value), period["period_start"], period["period_end"],
                        int(period["is_partial"]), started_at,
                        snapshot.get("source_watermark"), run_id, checksum,
                    ),
                )
            connection.execute(
                """UPDATE stats_metric_runs SET finished_at=?,input_rows=?,
                   output_rows=?,duration_ms=0,checksum=? WHERE run_id=?""",
                (
                    started_at, len(values), len(values),
                    _checksum(values), run_id,
                ),
            )
        return run_id

    def validate_metric_version(self, metric_id: str, metric_version: int) -> None:
        """Mark a complete shadow revision eligible for atomic publication."""
        with self._lock, self._write_connect() as connection:
            count = connection.execute(
                """SELECT COUNT(*) FROM stats_metric_values WHERE metric_id=?
                   AND metric_version=? AND status='shadow'""",
                (metric_id, metric_version),
            ).fetchone()[0]
            if int(count) != len(PERIOD_IDS):
                raise ValueError("metric version needs a complete shadow")
            connection.execute(
                """UPDATE stats_metric_values SET status='validated'
                   WHERE metric_id=? AND metric_version=? AND status='shadow'""",
                (metric_id, metric_version),
            )

    def publish_version(self, metric_id: str, metric_version: int, run_id: str) -> None:
        """Atomically move one published alias after shadow validation."""
        if not metric_id or metric_version < 1:
            raise ValueError("invalid metric publication")
        with self._lock, self._write_connect() as connection:
            candidate = connection.execute(
                """SELECT COUNT(*) FROM stats_metric_values WHERE metric_id=?
                   AND metric_version=? AND status IN ('validated','published')""",
                (metric_id, metric_version),
            ).fetchone()[0]
            if int(candidate) != len(PERIOD_IDS):
                raise ValueError("metric version has no complete validated values")
            old = connection.execute(
                "SELECT metric_version FROM stats_metric_publications WHERE metric_id=?",
                (metric_id,),
            ).fetchone()
            if old is not None and int(old[0]) != metric_version:
                connection.execute(
                    "DELETE FROM stats_metric_values WHERE metric_id=? "
                    "AND metric_version=? AND status='deprecated'",
                    (metric_id, int(old[0])),
                )
                connection.execute(
                    "UPDATE stats_metric_values SET status='deprecated' "
                    "WHERE metric_id=? AND metric_version=? AND status='published'",
                    (metric_id, int(old[0])),
                )
            connection.execute(
                "UPDATE stats_metric_values SET status='published' "
                "WHERE metric_id=? AND metric_version=? AND status='validated'",
                (metric_id, metric_version),
            )
            connection.execute(
                """INSERT INTO stats_metric_publications
                   (metric_id,metric_version,published_at,run_id) VALUES (?,?,?,?)
                   ON CONFLICT(metric_id) DO UPDATE SET
                     metric_version=excluded.metric_version,
                     published_at=excluded.published_at,run_id=excluded.run_id""",
                (metric_id, metric_version, _iso_now(), run_id),
            )
            row = connection.execute(
                "SELECT value FROM stats_meta WHERE key='latest_snapshot'"
            ).fetchone()
            field = {
                "core.installs": "installs",
                "core.active_actors": "active_actors",
                "core.events": "events",
                "core.errors": "errors",
            }.get(metric_id)
            if row is not None and field:
                snapshot = json.loads(row[0])
                for period_id, value_json in connection.execute(
                    """SELECT period_id,value_json FROM stats_metric_values
                       WHERE metric_id=? AND metric_version=? AND status='published'""",
                    (metric_id, metric_version),
                ):
                    snapshot["periods"][period_id][field] = json.loads(value_json)
                snapshot.setdefault("metric_versions", {})[metric_id] = metric_version
                snapshot["overview"] = {
                    "ever_used": snapshot["periods"]["all_time"]["installs"],
                    "dau": snapshot["periods"]["today"]["active_actors"],
                    "wau": snapshot["periods"]["last_7_dates"]["active_actors"],
                    "mau": snapshot["periods"]["last_30_dates"]["active_actors"],
                }
                snapshot["run_id"] = run_id
                snapshot["computed_at"] = _iso_now()
                snapshot["checksum"] = _checksum({
                    "periods": snapshot["periods"],
                    "overview": snapshot["overview"],
                    "metric_versions": snapshot["metric_versions"],
                })
                connection.execute(
                    "UPDATE stats_meta SET value=? WHERE key='latest_snapshot'",
                    (_json(snapshot),),
                )

    def telemetry(self) -> dict[str, Any]:
        with self._connect() as connection:
            latest = connection.execute(
                """SELECT * FROM stats_metric_runs
                   WHERE status != 'running' ORDER BY finished_at DESC LIMIT 1"""
            ).fetchone()
            dirty = connection.execute(
                "SELECT COUNT(*),MIN(day),MAX(day) FROM stats_dirty_dates"
            ).fetchone()
            watermark = connection.execute(
                "SELECT MAX(source_watermark) FROM stats_daily_counts"
            ).fetchone()[0]
            processed_events = connection.execute(
                "SELECT COUNT(*) FROM stats_processed_events"
            ).fetchone()[0]
            snapshot_row = connection.execute(
                "SELECT value FROM stats_meta WHERE key='latest_snapshot'"
            ).fetchone()
        now = time.time()
        wal_path = pathlib.Path(str(self.database_path) + "-wal")
        snapshot = json.loads(snapshot_row[0]) if snapshot_row is not None else {}
        computed_at = snapshot.get("computed_at")
        ingest_watermark = snapshot.get("ingest_watermark")
        visible_latency = None
        if computed_at and ingest_watermark:
            visible_latency = max(
                0.0,
                dt.datetime.fromisoformat(computed_at).timestamp()
                - dt.datetime.fromisoformat(ingest_watermark).timestamp(),
            )
        with self._telemetry_lock:
            waits = sorted(self._write_wait_samples_ms)
            busy_errors = self._sqlite_busy_errors
        p95_index = max(0, min(len(waits) - 1, int(len(waits) * .95)))
        return {
            "dirty_dates": int(dirty[0]),
            "processed_events": int(processed_events),
            "dirty_from": dirty[1],
            "dirty_to": dirty[2],
            "source_watermark": (
                dt.datetime.fromtimestamp(watermark, UTC).replace(microsecond=0).isoformat()
                if watermark is not None else None
            ),
            "source_age_seconds": max(0.0, now - watermark) if watermark else None,
            "snapshot_age_seconds": max(
                0.0, now - dt.datetime.fromisoformat(computed_at).timestamp()
            ) if computed_at else None,
            "last_visible_latency_seconds": visible_latency,
            "database_bytes": self.database_path.stat().st_size
            if self.database_path.exists() else 0,
            "wal_bytes": wal_path.stat().st_size if wal_path.exists() else 0,
            "sqlite_version": sqlite3.sqlite_version,
            "write_batches_observed": len(waits),
            "write_lock_wait_p95_ms": waits[p95_index] if waits else None,
            "write_lock_wait_max_ms": max(waits) if waits else None,
            "sqlite_busy_errors": busy_errors,
            "last_run": dict(latest) if latest is not None else None,
        }

    def rebuild(
        self,
        events: Iterable[Sequence[Any] | FactEvent],
        *,
        dry_run: bool = False,
        error_names: frozenset[str] = frozenset({"error"}),
        code_revision: str = "",
    ) -> dict[str, Any]:
        """Rebuild all derived facts from an iterator of canonical raw rows."""
        digest = hashlib.sha256()
        input_rows = 0
        first_day: str | None = None
        last_day: str | None = None
        batch: list[FactEvent] = []
        if not dry_run:
            with self._lock, self._write_connect() as connection:
                connection.execute("DELETE FROM stats_actor_dates")
                connection.execute("DELETE FROM stats_processed_events")
                connection.execute("DELETE FROM stats_actor_lifetime")
                connection.execute("DELETE FROM stats_daily_counts")
                connection.execute("DELETE FROM stats_dirty_dates")
                connection.execute("DELETE FROM stats_meta WHERE key='last_ingested_at'")
        for raw in events:
            item = self.normalize_events([raw])[0]
            day = _day_for_timestamp(item.timestamp)
            first_day = day if first_day is None else min(first_day, day)
            last_day = day if last_day is None else max(last_day, day)
            digest.update(_json(
                (item.timestamp, item.actor_id, item.name, item.event_id,
                 item.included, item.actor_active, item.lifetime_actor)
            ).encode())
            digest.update(b"\n")
            input_rows += 1
            if not dry_run:
                batch.append(item)
                if len(batch) >= 5000:
                    self.record_events(batch, error_names=error_names)
                    batch.clear()
        if batch:
            self.record_events(batch, error_names=error_names)
        preview = {
            "input_rows": input_rows,
            "from": first_day,
            "to": last_day,
            "checksum": digest.hexdigest(),
            "dry_run": dry_run,
        }
        if dry_run:
            return preview
        snapshot = self.refresh(kind="backfill", code_revision=code_revision)
        return {**preview, "run_id": snapshot["run_id"], "dry_run": False}


def sqlite_events(
    database_path: pathlib.Path | str,
    *,
    table: str = "events",
    timestamp_column: str = "ts",
    actor_column: str = "device_id",
    name_column: str = "name",
    event_id_column: str | None = "event_id",
    start_timestamp: float | None = None,
    end_timestamp: float | None = None,
) -> Iterator[FactEvent]:
    """Stream canonical facts from a trusted, locally configured schema."""
    identifiers = (table, timestamp_column, actor_column, name_column)
    if any(not value.replace("_", "").isalnum() for value in identifiers):
        raise ValueError("invalid SQLite identifier")
    if event_id_column and not event_id_column.replace("_", "").isalnum():
        raise ValueError("invalid SQLite identifier")
    clauses: list[str] = []
    params: list[float] = []
    if start_timestamp is not None:
        clauses.append(f"{timestamp_column} >= ?")
        params.append(start_timestamp)
    if end_timestamp is not None:
        clauses.append(f"{timestamp_column} < ?")
        params.append(end_timestamp)
    where = " WHERE " + " AND ".join(clauses) if clauses else ""
    connection = sqlite3.connect(database_path)
    try:
        columns = {
            row[1] for row in connection.execute(f"PRAGMA table_info({table})")
        }
        event_id_sql = (
            f",{event_id_column}" if event_id_column in columns else ",NULL"
        )
        cursor = connection.execute(
            f"SELECT rowid,{timestamp_column},{actor_column},{name_column}"
            f"{event_id_sql} "
            f"FROM {table}{where} ORDER BY {timestamp_column}",
            params,
        )
        for rowid, timestamp, actor_id, name, event_id in cursor:
            yield FactEvent(
                float(timestamp), str(actor_id or ""), str(name),
                str(event_id or f"raw:{rowid}"),
            )
    finally:
        connection.close()
