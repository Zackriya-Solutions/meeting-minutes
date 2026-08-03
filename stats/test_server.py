from __future__ import annotations

import asyncio
import json
import os
import tempfile
import time
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import AsyncMock, patch

from starlette.requests import Request

_TEMP = tempfile.TemporaryDirectory()
os.environ["STATS_DB"] = str(Path(_TEMP.name) / "events.db")
os.environ.pop("POSTHOG_PERSONAL_API_KEY", None)
os.environ.pop("POSTHOG_PROJECT_ID", None)

import server  # noqa: E402
import posthog_sync  # noqa: E402
from storage import delete_events_before, insert_events, sanitize_properties  # noqa: E402


class StatsModuleTests(unittest.TestCase):
    def setUp(self) -> None:
        server._db.execute("DELETE FROM events")
        server._db.execute("DELETE FROM sync_state")
        server._db.commit()
        os.environ.pop("STATS_EXCLUDED_DEVICE_IDS", None)
        server._install_auth_cache.clear()
        server._install_request_times.clear()
        server._auth_validation_times.clear()
        server._static_request_times.clear()

    @staticmethod
    def request(body: object, headers: dict[str, str] | None = None) -> Request:
        raw = json.dumps(body).encode()
        sent = False

        async def receive():
            nonlocal sent
            if sent:
                return {"type": "http.disconnect"}
            sent = True
            return {"type": "http.request", "body": raw, "more_body": False}

        encoded_headers = [(b"content-length", str(len(raw)).encode())]
        encoded_headers.extend(
            (key.lower().encode(), value.encode())
            for key, value in (headers or {}).items()
        )
        return Request({
            "type": "http",
            "method": "POST",
            "path": "/events",
            "headers": encoded_headers,
        }, receive)

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

    def test_retention_deletes_only_expired_events(self) -> None:
        now = time.time()
        insert_events(server._db, [
            {"ts": now - 366 * server.DAY, "device_id": "old", "name": "app_started"},
            {"ts": now - 10, "device_id": "current", "name": "app_started"},
        ])

        deleted = delete_events_before(server._db, now - 365 * server.DAY)

        self.assertEqual(deleted, 1)
        self.assertEqual(
            server._db.execute("SELECT device_id FROM events").fetchone()[0],
            "current",
        )

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
            {"ts": now - 12 * 3600, "device_id": "a", "name": "transcript_copied",
             "properties": {"event_id": "a3"}},
            {"ts": now - 8 * server.DAY, "device_id": "b", "name": "meeting_ended",
             "properties": {"event_id": "b1", "active_duration_seconds": "1800", "had_fatal_error": "true"}},
            {"ts": now - 3600, "device_id": "c", "name": "import_audio_started",
             "properties": {"event_id": "c1", "duration_seconds": "600"}},
            {"ts": now - 3599, "device_id": "c", "name": "import_audio_completed",
             "properties": {"event_id": "c2", "duration_seconds": "600", "success": "true", "app_version": "1.2.3"}},
            {"ts": now - 3598, "device_id": "c", "name": "summary_generation_completed",
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
        self.assertEqual(set(payload["overview"]), {"ever_used", "dau", "wau", "mau"})
        self.assertNotIn("sessions_per_dau", payload["overview"])
        self.assertNotIn("tools_per_dau", payload["overview"])
        self.assertEqual(response.headers["cache-control"], "no-store")

    def test_summary_exposes_canonical_sessions_and_value_actions_per_dau(self) -> None:
        now = time.time()
        insert_events(server._db, [
            {
                "ts": now - 3600,
                "device_id": "active-device",
                "name": "summary_copied",
                "properties": {"event_id": "copy-1"},
            },
            {
                "ts": now - 1,
                "device_id": "active-device",
                "name": "transcript_copied",
                "properties": {"event_id": "copy-2"},
            },
        ])

        overview = json.loads(server.summary(1).body)["overview"]

        self.assertEqual(overview["dau"], 1)
        self.assertEqual(overview["sessions_per_dau"], 2.0)
        self.assertEqual(overview["tools_per_dau"], 2.0)

    def test_automatic_recording_start_is_not_a_human_session(self) -> None:
        now = time.time()
        insert_events(server._db, [{
            "ts": now - 1,
            "device_id": "automatic-device",
            "name": "button_click_start_recording",
            "properties": {"event_id": "auto-start", "location": "sidebar_auto"},
        }])

        overview = json.loads(server.summary(1).body)["overview"]

        self.assertEqual(overview["dau"], 1)
        self.assertEqual(overview["sessions_per_dau"], 0.0)

    def test_overview_uses_moscow_day_iso_week_and_rolling_30_day_mau(self) -> None:
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
        self.assertEqual(growth["mau"], 4)

    def test_ingest_requires_a_per_install_credential(self) -> None:
        response = asyncio.run(server.ingest(self.request({"events": []})))

        self.assertEqual(response.status_code, 401)

    def test_gateway_identity_urls_are_exact_managed_https_hosts(self) -> None:
        self.assertEqual(
            server._gateway_identity_urls(
                "https://gw.multitool.works/,https://gw2.multitool.works"
            ),
            (
                "https://gw.multitool.works/me",
                "https://gw2.multitool.works/me",
            ),
        )
        for value in (
            "http://gw.multitool.works",
            "https://gw.multitool.works.attacker.invalid",
            "https://attacker.invalid",
            "https://user@gw.multitool.works",
            "https://gw.multitool.works/redirect",
            "https://gw.multitool.works:notaport",
        ):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                server._gateway_identity_urls(value)

    def test_health_distinguishes_static_and_install_auth(self) -> None:
        with (
            patch.object(server, "INGEST_TOKEN", "server-secret"),
            patch.object(server, "GATEWAY_IDENTITY_URLS", ()),
        ):
            health = asyncio.run(server.health())

        self.assertTrue(health["ingest_enabled"])
        self.assertTrue(health["ingest_authenticated"])
        self.assertTrue(health["static_auth"])
        self.assertFalse(health["install_auth"])

    def test_gateway_outage_fails_closed_as_unavailable(self) -> None:
        request = self.request(
            {"events": []},
            {"authorization": "Bearer install-jwt"},
        )
        with patch.object(
            server,
            "_validate_install_token",
            new=AsyncMock(return_value=(None, 503)),
        ):
            response = asyncio.run(server.ingest(request))

        self.assertEqual(response.status_code, 503)
        self.assertEqual(json.loads(response.body)["error"], "identity service unavailable")
        self.assertEqual(response.headers["retry-after"], "60")

    def test_fully_disabled_ingest_wins_over_supplied_bearer(self) -> None:
        request = self.request(
            {"events": []},
            {"authorization": "Bearer otherwise-valid"},
        )
        with (
            patch.object(server, "INGEST_TOKEN", ""),
            patch.object(server, "GATEWAY_IDENTITY_URLS", ()),
        ):
            response = asyncio.run(server.ingest(request))

        self.assertEqual(response.status_code, 503)
        self.assertEqual(json.loads(response.body)["error"], "ingest disabled")

    def test_gateway_validation_fanout_is_rate_limited_before_network(self) -> None:
        with (
            patch.object(server, "AUTH_VALIDATION_RATE_LIMIT_PER_MINUTE", 1),
            patch.object(
                server,
                "GATEWAY_IDENTITY_URLS",
                ("https://gw.multitool.works/me",),
            ),
            patch.object(
                server,
                "_inspect_install_token",
                return_value=("invalid", None),
            ) as inspect,
        ):
            first = asyncio.run(server._validate_install_token("invalid-one"))
            second = asyncio.run(server._validate_install_token("invalid-two"))

        self.assertEqual(first, (None, 401))
        self.assertEqual(second, (None, 429))
        self.assertEqual(inspect.call_count, 1)

    def test_per_install_rate_limit_returns_true_after_limit(self) -> None:
        with patch.object(server, "INSTALL_RATE_LIMIT_PER_MINUTE", 2):
            self.assertFalse(server._install_rate_limited("verified-device"))
            self.assertFalse(server._install_rate_limited("verified-device"))
            self.assertTrue(server._install_rate_limited("verified-device"))

    def test_static_ingest_is_also_rate_limited(self) -> None:
        request = self.request(
            {"events": []},
            {"x-ingest-token": "server-secret"},
        )
        with (
            patch.object(server, "INGEST_TOKEN", "server-secret"),
            patch.object(server, "STATIC_RATE_LIMIT_PER_MINUTE", 1),
        ):
            first = asyncio.run(server.ingest(request))
            second = asyncio.run(server.ingest(request))

        self.assertEqual(first.status_code, 200)
        self.assertEqual(second.status_code, 429)
        self.assertEqual(second.headers["retry-after"], "60")

    def test_install_identity_overrides_spoofed_event_device(self) -> None:
        request = self.request(
            {
                "events": [{
                    "ts": time.time(),
                    "device_id": "spoofed-device",
                    "name": "app_started",
                    "properties": {"event_id": "verified-event"},
                }],
            },
            {"authorization": "Bearer install-jwt"},
        )
        with patch.object(
            server,
            "_validate_install_token",
            new=AsyncMock(return_value=("verified-device", 200)),
        ):
            response = asyncio.run(server.ingest(request))

        self.assertEqual(response.status_code, 200)
        self.assertEqual(json.loads(response.body)["ingested"], 1)
        actor, source = server._db.execute(
            "SELECT device_id,source FROM events WHERE event_id='verified-event'"
        ).fetchone()
        self.assertEqual(actor, "verified-device")
        self.assertEqual(source, "direct-client")

    def test_static_token_remains_available_for_server_to_server_ingest(self) -> None:
        request = self.request(
            {"events": [{
                "ts": time.time(),
                "device_id": "trusted-backend-device",
                "name": "app_started",
                "properties": {"event_id": "server-event"},
            }]},
            {"x-ingest-token": "server-secret"},
        )
        with patch.object(server, "INGEST_TOKEN", "server-secret"):
            response = asyncio.run(server.ingest(request))

        self.assertEqual(response.status_code, 200)
        self.assertEqual(
            server._db.execute(
                "SELECT source FROM events WHERE event_id='server-event'"
            ).fetchone()[0],
            "direct-server",
        )

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
