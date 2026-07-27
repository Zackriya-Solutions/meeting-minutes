from __future__ import annotations

import asyncio
import json
import os
import tempfile
import time
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

_TEMP = tempfile.TemporaryDirectory()
os.environ["STATS_DB"] = str(Path(_TEMP.name) / "events.db")
os.environ.pop("POSTHOG_PERSONAL_API_KEY", None)
os.environ.pop("POSTHOG_PROJECT_ID", None)

import server  # noqa: E402
import posthog_sync  # noqa: E402
from storage import insert_events, sanitize_properties  # noqa: E402


class StatsModuleTests(unittest.TestCase):
    def setUp(self) -> None:
        server._db.execute("DELETE FROM events")
        server._db.execute("DELETE FROM sync_state")
        server._db.commit()
        os.environ.pop("STATS_EXCLUDED_DEVICE_IDS", None)

    def test_storage_deduplicates_shared_event_id_across_sources(self) -> None:
        event = {
            "ts": time.time(),
            "device_id": "device-a",
            "name": "meeting_ended",
            "properties": {"event_id": "same-event", "total_duration_seconds": "42"},
        }
        first = insert_events(server._db, [event], source="direct")
        event["uuid"] = "posthog-uuid"
        second = insert_events(server._db, [event], source="posthog")

        self.assertEqual(first["inserted"], 1)
        self.assertEqual(second["inserted"], 0)
        self.assertEqual(second["duplicates"], 1)
        self.assertEqual(server._db.execute("SELECT COUNT(*) FROM events").fetchone()[0], 1)

    def test_property_allowlist_drops_content_and_raw_failures(self) -> None:
        safe = sanitize_properties(
            {
                "event_id": "event-1",
                "duration_seconds": 42,
                "success": True,
                "meeting_title": "Board Strategy",
                "error_message": "/private/customer/meeting.wav",
                "$current_url": "file:///private/path",
            }
        )

        self.assertEqual(safe["event_id"], "event-1")
        self.assertEqual(safe["duration_seconds"], "42")
        self.assertEqual(safe["success"], "true")
        self.assertNotIn("meeting_title", safe)
        self.assertNotIn("error_message", safe)
        self.assertNotIn("$current_url", safe)

    def test_product_metrics_use_value_events_and_mature_cohorts(self) -> None:
        now = time.time()
        events = [
            {"ts": now - 8 * server.DAY, "device_id": "a", "name": "meeting_ended",
             "properties": {"event_id": "a1", "total_duration_seconds": "3600", "had_fatal_error": "false"}},
            {"ts": now - 2 * server.DAY, "device_id": "a", "name": "summary_copied",
             "properties": {"event_id": "a2"}},
            {"ts": now - 8 * server.DAY, "device_id": "b", "name": "meeting_ended",
             "properties": {"event_id": "b1", "active_duration_seconds": "1800", "had_fatal_error": "true"}},
            {"ts": now - server.DAY, "device_id": "c", "name": "import_audio_started",
             "properties": {"event_id": "c1", "duration_seconds": "600"}},
            {"ts": now - server.DAY + 1, "device_id": "c", "name": "import_audio_completed",
             "properties": {"event_id": "c2", "duration_seconds": "600", "success": "true", "app_version": "1.2.3"}},
            {"ts": now - server.DAY + 2, "device_id": "c", "name": "summary_generation_completed",
             "properties": {"event_id": "c3", "success": "true", "app_version": "1.2.3"}},
        ]
        result = insert_events(server._db, events, source="posthog")
        self.assertEqual(result["inserted"], len(events))

        product = server.compute_product(30)

        self.assertEqual(product["usage"]["captured_memories"], 3)
        self.assertEqual(product["usage"]["captured_seconds"], 6000)
        self.assertEqual(product["usage"]["successful_summaries"], 1)
        self.assertEqual(product["quality"]["fatal_recordings"], 1)
        self.assertEqual(product["quality"]["fatal_recording_rate"], 0.5)
        self.assertEqual(product["retention"]["cohorts"]["d7"], 2)
        self.assertEqual(product["retention"]["rates"]["d7"], 0.5)
        self.assertEqual(product["growth"]["weekly_value_devices"], 2)

    def test_internal_devices_are_filtered_retroactively(self) -> None:
        now = time.time()
        insert_events(
            server._db,
            [
                {"ts": now, "device_id": "internal", "name": "meeting_ended", "properties": {"event_id": "i"}},
                {"ts": now, "device_id": "customer", "name": "meeting_ended", "properties": {"event_id": "c"}},
            ],
        )
        os.environ["STATS_EXCLUDED_DEVICE_IDS"] = "internal"

        product = server.compute_product(7)

        self.assertEqual(product["identity"]["observed_devices"], 1)
        self.assertEqual(product["usage"]["captured_memories"], 1)

    def test_summary_keeps_traction_contract_and_no_store(self) -> None:
        response = server.summary(7)
        payload = json.loads(response.body)

        self.assertEqual(payload["window_days"], 7)
        self.assertIn("installs", payload)
        self.assertIn("dau", payload)
        self.assertIn("events", payload)
        self.assertIn("errors", payload)
        self.assertEqual(
            set(payload["overview"]), {"ever_used", "dau", "wau", "mau"}
        )
        self.assertNotIn("sessions_per_dau", payload["overview"])
        self.assertNotIn("tools_per_dau", payload["overview"])
        self.assertEqual(response.headers["cache-control"], "no-store")

    def test_overview_uses_moscow_day_iso_week_and_calendar_month(self) -> None:
        now = datetime(2026, 7, 8, 12, 0, tzinfo=timezone.utc).timestamp()
        events = [
            {"ts": datetime(2026, 7, 7, 21, 1, tzinfo=timezone.utc).timestamp(),
             "device_id": "today", "name": "meeting_ended",
             "properties": {"event_id": "today"}},
            {"ts": datetime(2026, 7, 6, 8, 0, tzinfo=timezone.utc).timestamp(),
             "device_id": "week", "name": "meeting_ended",
             "properties": {"event_id": "week"}},
            {"ts": datetime(2026, 7, 5, 8, 0, tzinfo=timezone.utc).timestamp(),
             "device_id": "month", "name": "meeting_ended",
             "properties": {"event_id": "month"}},
            {"ts": datetime(2026, 6, 30, 20, 59, tzinfo=timezone.utc).timestamp(),
             "device_id": "old", "name": "meeting_ended",
             "properties": {"event_id": "old"}},
        ]
        insert_events(server._db, events)

        with patch("server.time.time", return_value=now):
            growth = server.compute_product(1)["growth"]

        self.assertEqual(growth["dau"], 1)
        self.assertEqual(growth["wau"], 2)
        self.assertEqual(growth["mau"], 3)

    def test_ingest_fails_closed_without_server_configuration(self) -> None:
        response = asyncio.run(server.ingest(None))

        self.assertEqual(response.status_code, 503)

    def test_summary_without_success_is_not_also_an_error(self) -> None:
        insert_events(
            server._db,
            [{
                "ts": time.time() - server.DAY,
                "device_id": "summary-device",
                "name": "summary_generation_completed",
                "properties": {"event_id": "summary-without-success"},
            }],
        )

        product = server.compute_product(7)

        self.assertEqual(product["usage"]["successful_summaries"], 1)
        self.assertEqual(product["quality"]["summary_attempts"], 1)
        self.assertEqual(product["quality"]["errors"], 0)

    def test_posthog_page_cap_resumes_from_checkpoint(self) -> None:
        now = time.time()
        first_page = {
            "results": [{
                "event": "meeting_ended",
                "timestamp": now - 20,
                "distinct_id": "posthog-a",
                "uuid": "posthog-event-a",
                "properties": {"total_duration_seconds": 20},
            }],
            "next": "/api/projects/7/events/?page=2",
        }
        second_page = {
            "results": [{
                "event": "meeting_ended",
                "timestamp": now - 40,
                "distinct_id": "posthog-b",
                "uuid": "posthog-event-b",
                "properties": {"total_duration_seconds": 40},
            }],
            "next": None,
        }
        env = {
            "POSTHOG_PERSONAL_API_KEY": "test-read-key",
            "POSTHOG_PROJECT_ID": "7",
            "POSTHOG_SYNC_MAX_PAGES": "1",
        }
        with patch.dict(os.environ, env), patch.object(
            posthog_sync, "_request_json", side_effect=[first_page, second_page]
        ) as request:
            first = posthog_sync.sync_once(server._db)
            second = posthog_sync.sync_once(server._db)

        self.assertFalse(first["backfill_complete"])
        self.assertTrue(second["backfill_complete"])
        self.assertIn("page=2", request.call_args_list[1].args[0])
        self.assertIsNone(
            server._db.execute(
                "SELECT cursor FROM sync_state WHERE source='posthog_page'"
            ).fetchone()
        )
        self.assertEqual(
            server._db.execute("SELECT COUNT(*) FROM events").fetchone()[0],
            2,
        )


if __name__ == "__main__":
    unittest.main()
