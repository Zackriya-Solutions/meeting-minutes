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
   `frontend/src-tauri/src/analytics/traction.rs`. Release clients authenticate
   with their per-install Memento gateway JWT. The collector validates it via
   the gateway's `/me` endpoint and uses the verified install id as the actor.
   Validation targets are restricted to the exact managed HTTPS hosts.
   Successful validation is cached briefly, with a bounded revocation window;
   cache misses are globally limited before any outbound gateway lookup.
2. `posthog_sync.py` is a legacy migration path. It can backfill an old PostHog
   project only if Memento obtains a read-scoped personal key and numeric
   project id; production currently has neither configured.

New clients stamp one `event_id` for idempotent retries. SQLite has a partial
unique index on that id. Imported PostHog history uses the PostHog event UUID.
Both ingest paths apply a strict property allowlist; raw errors, titles, paths,
prompts, and meeting content are discarded. Events older than 365 days are
deleted by the service.

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
- `POST /events` — idempotent event ingest. Desktop: `Authorization: Bearer
  <per-install JWT>`; trusted backend migration jobs: `X-Ingest-Token`.

The static `X-Ingest-Token` path deliberately trusts the migration job's
`device_id`; it does not perform per-install identity verification. Keep this
credential only on the server, rotate it independently, and never embed it in
the desktop application. It is separately rate-limited and exposed as
`static_auth` in `/health`; normal clients always use a verified gateway JWT.

## Local run and tests

```bash
cd stats
python3 -m venv .venv
.venv/bin/pip install fastapi uvicorn
.venv/bin/python -m unittest -v test_server.py
STATS_INGEST_TOKEN=devtoken .venv/bin/python server.py
                                            # http://127.0.0.1:9901
```

Debug desktop builds send directly to `http://127.0.0.1:9901/events`. Set
`MEMENTO_STATS_INGEST_TOKEN=devtoken` for the desktop process and the matching
`STATS_INGEST_TOKEN=devtoken` for the local server; unauthenticated ingest is
not supported.
Release builds reuse `MEMENTO_REGISTRATION_KEY`, already required for the
managed gateway paths; no analytics ingest secret is compiled into the binary.
The ignored Rust e2e test can be run while the server is up:

```bash
cd frontend/src-tauri
cargo test -p meetily traction -- --ignored
```

## Optional legacy PostHog migration

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
This is not the primary collection path and does not justify enabling or
retaining the old PostHog client in Memento.
Configuration:

- `POSTHOG_BACKFILL_DAYS` — initial lookback, default `365`;
- `POSTHOG_SYNC_INTERVAL_SECONDS` — background cadence, default `300`;
- `POSTHOG_SYNC_OVERLAP_SECONDS` — overlap for late events, default `300`;
- `POSTHOG_SYNC_MAX_PAGES` — safety cap, default `2000`.
- `STATS_GATEWAY_IDENTITY_URLS` — comma-separated managed gateway bases used
  to validate install JWTs; defaults to `gw.multitool.works` and its fallback.
- `STATS_INSTALL_AUTH_CACHE_SECONDS` — successful validation cache, default
  `60`, maximum `60` seconds. This covers the client's normal 30-second flush
  cadence while bounding the accepted-token revocation window to one minute.
- `STATS_INSTALL_RATE_LIMIT_PER_MINUTE` — accepted client batches per verified
  install, default `12`.
- `STATS_AUTH_VALIDATION_RATE_LIMIT_PER_MINUTE` — global cap on outbound
  identity lookups caused by cache misses, default `1200`. This is an emergency
  abuse ceiling rather than a normal traffic limit, including restart bursts.
- `STATS_STATIC_RATE_LIMIT_PER_MINUTE` — accepted batches authenticated with
  the trusted server-to-server ingest token, default `60`.
- `STATS_RETENTION_DAYS` — event retention, default `365` days.

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
