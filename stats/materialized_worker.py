"""Shared non-blocking coordinator for a product-local materialized journal."""
from __future__ import annotations

import datetime as dt
import threading
import traceback
from collections import deque
from typing import Any, Callable, Iterable

from materialized import FactEvent, MaterializedStats


class MaterializedWorker:
    """Keep a rebuildable metrics DB current without blocking HTTP ingest."""

    def __init__(
        self,
        engine: MaterializedStats,
        events_factory: Callable[[], Iterable[FactEvent]],
        raw_state: Callable[[], tuple[int, float | None]],
        *,
        error_names: frozenset[str],
        code_revision: str = "",
        tick_seconds: float = 15.0,
        preserve_history: bool = False,
    ) -> None:
        self.engine = engine
        self.events_factory = events_factory
        self.raw_state = raw_state
        self.error_names = error_names
        self.code_revision = code_revision
        self.tick_seconds = max(1.0, float(tick_seconds))
        self.preserve_history = preserve_history
        self.ready = threading.Event()
        self._wakeup = threading.Event()
        self._rebuild_requested = threading.Event()
        self._pending: deque[tuple[str, Any]] = deque()
        self._pending_lock = threading.Lock()
        self._thread: threading.Thread | None = None
        self._start_lock = threading.Lock()

    def start(self) -> None:
        with self._start_lock:
            if self._thread is not None and self._thread.is_alive():
                return
            self._thread = threading.Thread(
                target=self._loop, name="stats-materializer", daemon=True
            )
            self._thread.start()

    def enqueue(self, facts: Iterable[FactEvent], *, rebuild: bool = False) -> None:
        rows = list(facts)
        if rows:
            with self._pending_lock:
                self._pending.append(("events", rows))
        if rebuild:
            self._rebuild_requested.set()
        self._wakeup.set()

    def request_rebuild(self) -> None:
        self.enqueue((), rebuild=True)

    def request_actor_delete(self, actor_id: str) -> None:
        if not actor_id:
            return
        with self._pending_lock:
            self._pending.append(("delete_actor", actor_id))
        self._wakeup.set()

    def rebuild_now(self) -> None:
        self.engine.rebuild(
            self.events_factory(),
            error_names=self.error_names,
            code_revision=self.code_revision,
        )
        self.ready.set()

    def ensure_now(self) -> None:
        raw_count, raw_watermark = self.raw_state()
        telemetry = self.engine.telemetry()
        derived_count = int(telemetry.get("processed_events") or 0)
        derived_watermark = telemetry.get("source_watermark")
        derived_ts = (
            dt.datetime.fromisoformat(derived_watermark).timestamp()
            if derived_watermark else None
        )
        if self.preserve_history and telemetry.get("last_run"):
            batch: list[FactEvent] = []
            for event in self.events_factory():
                batch.append(event)
                if len(batch) >= 5000:
                    self.engine.record_events(batch, error_names=self.error_names)
                    batch.clear()
            if batch:
                self.engine.record_events(batch, error_names=self.error_names)
            if self.engine.telemetry()["dirty_dates"]:
                self.engine.refresh(kind="catchup", code_revision=self.code_revision)
            self.ready.set()
        elif (
            raw_count != derived_count
            or (raw_watermark is not None and
                (derived_ts is None or derived_ts + 1 < raw_watermark))
        ):
            self.rebuild_now()
        elif not telemetry.get("last_run"):
            self.engine.refresh(kind="bootstrap", code_revision=self.code_revision)
            self.ready.set()
        else:
            self.ready.set()

    def _drain(self) -> None:
        while True:
            with self._pending_lock:
                if not self._pending:
                    return
                kind, payload = self._pending.popleft()
            if kind == "events":
                self.engine.record_events(payload, error_names=self.error_names)
            elif kind == "delete_actor":
                self.engine.delete_actor(str(payload))

    def _loop(self) -> None:
        try:
            self.ensure_now()
        except Exception:  # noqa: BLE001 — legacy stats stay available
            traceback.print_exc()
        while True:
            self._wakeup.wait(self.tick_seconds)
            self._wakeup.clear()
            try:
                if self._rebuild_requested.is_set():
                    # Clear before work so a concurrent mutation is not lost.
                    self._rebuild_requested.clear()
                    # The raw journal is authoritative for a rebuild. Pending
                    # pre-mutation facts would otherwise resurrect rows that
                    # a privacy or migration rollback just removed.
                    with self._pending_lock:
                        self._pending.clear()
                    self.rebuild_now()
                self._drain()
                if self.ready.is_set() and self.engine.telemetry()["dirty_dates"]:
                    self.engine.refresh(code_revision=self.code_revision)
            except Exception:  # noqa: BLE001 — retry after the bounded tick
                traceback.print_exc()
                self._wakeup.set()

    def health(self) -> dict:
        with self._pending_lock:
            pending_batches = len(self._pending)
            pending_events = sum(
                len(payload) for kind, payload in self._pending
                if kind == "events"
            )
        return {
            "ready": self.ready.is_set(),
            "worker_alive": bool(self._thread and self._thread.is_alive()),
            "tick_seconds": self.tick_seconds,
            "pending_batches": pending_batches,
            "pending_events": pending_events,
            "rebuild_requested": self._rebuild_requested.is_set(),
            **self.engine.telemetry(),
        }
