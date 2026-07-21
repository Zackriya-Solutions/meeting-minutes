# Memento stats module (Traction)

Product-statistics module for the [Traction hub](https://stats.multitool.works)
at `/p/memento/`. Metric definitions and the client-instrumentation backlog are
in [`docs/TRACTION_STATS_PLAN.md`](../docs/TRACTION_STATS_PLAN.md).

The dashboard is deliberately value-led:

- **Weekly Value Devices**: opted-in anonymous devices that completed a
  recording/import or reused a transcript/summary in the trailing 7 days;
- captured memories and hours;
- device-level activation reach and rolling value retention D1/D7/D30;
- recording, transcription, import, and summary reliability guardrails;
- coverage/source notes, app-version adoption, and reversible internal-device
  exclusions.

`device_id` means an opted-in installation, not a person or account. Opted-out
devices are intentionally invisible, so this module cannot report analytics
opt-in rate or total product population.

## Sources

1. The desktop app batches consent-gated events to `POST /events` through
   `frontend/src-tauri/src/analytics/traction.rs`.
2. `posthog_sync.py` can backfill and continuously sync the same events already
   emitted by released clients to PostHog. This provides history without a new
   desktop release.

New clients stamp one `event_id` into both deliveries. SQLite has a partial
unique index on that id, so retries and the two sources cannot double-count the
same event. Old PostHog history uses the PostHog event UUID. Both ingest paths
apply a strict property allowlist; raw errors, titles, paths, prompts, and
meeting content are discarded.

## Routes

- `GET /` — product dashboard; all URLs are relative for Traction's
  `strip_prefix /p/memento` proxy.
- `GET /health` — liveness, deploy version, and PostHog-sync freshness.
- `GET /summary?days=N` — Traction standard core plus Memento showcase metrics.
- `GET /product?days=N` — KPI, funnel, retention, quality, versions, and daily
  series used by the dashboard.
- `GET /retention?days=N` — compact growth/retention response.
- `GET /dashboard-data?days=N` — compatibility daily series.
- `GET /series?days=N` — Traction hub history backfill.
- `POST /events` — idempotent event ingest (`X-Ingest-Token`).

## Local run and tests

```bash
cd stats
python3 -m venv .venv
.venv/bin/pip install fastapi uvicorn
.venv/bin/python -m unittest -v test_server.py
STATS_ALLOW_UNAUTHENTICATED_INGEST=1 .venv/bin/python server.py
                                            # http://127.0.0.1:9901
```

Debug desktop builds send directly to `http://127.0.0.1:9901/events` without a
token, so local server ingest must be enabled explicitly as shown above.
Without `STATS_INGEST_TOKEN` or that development-only flag, `POST /events`
fails closed with 503. Release builds require `MEMENTO_STATS_INGEST_TOKEN` in the build
environment; there is intentionally no committed fallback. Rotate the
currently exposed server token before enabling the next release. The ignored
Rust e2e test can be run while the server is up:

```bash
cd frontend/src-tauri
cargo test -p meetily traction -- --ignored
```

## Existing-release PostHog sync

Use a read-only personal API key scoped to the Memento project. The public
PostHog ingestion key embedded in the app cannot read events.

```bash
export POSTHOG_PERSONAL_API_KEY=phx_...
export POSTHOG_PROJECT_ID=12345
export POSTHOG_HOST=https://us.posthog.com
export STATS_DB="$PWD/data/events.db"
python3 posthog_sync.py
```

The service runs the same sync every five minutes when both credentials exist.
Configuration:

- `POSTHOG_BACKFILL_DAYS` — initial lookback, default `365`;
- `POSTHOG_SYNC_INTERVAL_SECONDS` — background cadence, default `300`;
- `POSTHOG_SYNC_OVERLAP_SECONDS` — overlap for late events, default `300`;
- `POSTHOG_SYNC_MAX_PAGES` — safety cap, default `2000`.

Store credentials only in
`/etc/systemd/system/stats-memento.service.d/env.conf` (mode `0600`). Never
commit them or pass them through `deploy.sh` arguments.

## Internal/test device exclusion

Events remain stored and are filtered on every aggregate, so exclusion is
retroactive and reversible. Configure comma/space-separated ids with
`STATS_EXCLUDED_DEVICE_IDS`, or one id per line in
`/srv/stats/memento/data/excluded-devices.txt` (override with
`STATS_EXCLUDED_DEVICE_IDS_FILE`). Lines may contain `#` comments.

## Deploy (i167, port 9901)

```bash
REMOTE=max@158.160.163.167 ./deploy.sh
```

Production layout is `/srv/stats/memento/{app,data}` with systemd unit
`stats-memento`. `deploy.sh` stamps `VERSION`, restarts only this module, and
fails unless `/health` reports the new version and `/summary` responds.
