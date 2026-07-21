# Memento stats module (Traction)

Product analytics module for the [Traction hub](https://stats.multitool.works)
(tab `/p/memento/`). Built from `traction/stats-module-template` in the
GigaTool repo; contract in `specs/stats-hub.md` §4, onboarding runbook in
`traction/ONBOARDING.md` there.

- **Events in**: the Memento desktop app batches analytics events
  (`device_id` + `name` + `properties`) to `POST /events` with
  `X-Ingest-Token` — see `frontend/src-tauri/src/analytics/traction.rs`.
  Sending respects the in-app analytics opt-in: nothing is sent unless the
  user enabled analytics.
- **Metrics out**: `/summary?days=N` serves the standard core
  (installs/dau/events/errors) for the hub's cross-project Summary, plus
  Memento's showcase `metrics[]` (Meetings, Recorded hrs, Summaries, DAU,
  Avg meeting, Errors, Installs). `index.html` is the in-tab dashboard —
  all URLs relative (the module lives behind `strip_prefix /p/memento`).

## Local run

```bash
cd stats
python3 -m venv .venv && .venv/bin/pip install fastapi uvicorn
.venv/bin/python server.py          # http://127.0.0.1:9901
```

A dev app build (`debug_assertions`) sends events to
`http://127.0.0.1:9901/events` with no token, so local end-to-end testing
needs nothing else.

## Deploy (prod: i167, port 9901)

```bash
REMOTE=max@158.160.163.167 ./deploy.sh
```

Prod layout: `/srv/stats/memento/{app,data}`, systemd unit `stats-memento`
(`stats-memento.service`), ingest token in the unit's drop-in
`env.conf` (`STATS_INGEST_TOKEN`). `VERSION` is stamped by `deploy.sh` and
served by `/health`; the deploy fails if the running version didn't change.
