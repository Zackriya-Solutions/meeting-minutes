# Memento product statistics in Traction

Status: implemented dashboard contract. Grain and definitions below are the contract for
the Memento dashboard; changing a definition requires changing its label or
version, not silently changing the query.

## Decision the dashboard must support

The weekly review should answer four questions:

1. Are more opted-in devices receiving value from Memento?
2. Do they return for that value?
3. Where does the activation path break?
4. Is growth being bought with worse recording, transcription, or summary
   reliability?

`device_id` is an anonymous installation identifier, not a person or account.
All product metrics cover only devices that explicitly enabled analytics. The
dashboard must never label them as all users or use them to estimate analytics
opt-in rate: opted-out installations are deliberately invisible.

## KPI hierarchy

### Primary outcome

**Weekly Value Devices (WVD)** — distinct opted-in devices with at least one
value event in the trailing 7 days.

Value events currently available:

- a recording completed (`meeting_ended`);
- an audio import completed (`import_audio_completed`, `success=true`);
- a summary or transcript was copied (`summary_copied`,
  `transcript_copied`).

Future value events, once instrumented, include a successful knowledge search,
a meeting/collection chat answer, and an accepted structured-memory record.
WVD is preferred to generic WAU because opening Settings or changing a device
should not count as receiving product value.

### Drivers

- **Captured memories** — completed recordings + successful imports. Keep the
  two sources visible because their reliability and intent differ.
- **Captured hours** — recording wall-clock seconds + imported audio seconds.
- **Summaries completed** — successful summary completions. Until summary
  events carry a meeting id, this is a completion count rather than an exact
  share of meetings; retries can make the proxy exceed 100%.
- **Value frequency** — captured memories per value device in the selected
  window.
- **Activation funnel (device grain)** — observed → capture started → memory
  completed → summary completed → content copied. Each step is a distinct
  device count in the same window, not a sequence claim.
- **Rolling D1/D7/D30 value retention** — share of mature first-value cohorts
  that produced another value event within N days after their first value day.
  This is bounded/rolling retention, not exact-day retention.

### Guardrails

- **Fatal recording rate** — `meeting_ended(had_fatal_error=true)` / completed
  recordings.
- **Summary success rate** — successful / all summary completion events.
- **Transcription errors** — affected devices and event count, not raw error
  messages.
- **Import success rate** — completed imports / import starts when both events
  are available.
- **Freshness and coverage** — last event time, first event time, source mix,
  anonymous rows, invalid durations, and excluded internal devices.

## What is available without a new desktop release

The production Traction database is empty, and its service journal contains no
client `POST /events`. Existing desktop releases did not contain the Traction
ingest token, so their first-party sink disabled itself. They cannot be made to
start direct delivery by a server-only change.

Older opted-in builds may have sent events to the upstream Meetily PostHog US
project. That history is available without a release only if Memento receives a
read-scoped personal key and numeric project id from that project's owner. We
do not currently have either credential, so PostHog history is a conditional
migration source, not an active data source. If access is obtained, it can
unlock:

- observed DAU/WAU/MAU, new/returning devices, and generic stickiness;
- WVD and value retention from the already-instrumented value events;
- recordings, imports, hours, average duration, and value frequency;
- summary count/success/latency proxy;
- fatal, transcription, import, and generic error counts;
- device-level activation funnel;
- provider/model/app-version breakdowns where the property already exists.

The optional sync uses a read-only scoped PostHog personal API key stored only
in the server's systemd drop-in. The public project ingestion key is not
sufficient and must not be treated as read access.

## Delivery contract for the next desktop release

- Analytics remains explicit opt-in and off by default. The V2 migration that
  silently enabled it is treated as invalid consent and reset once.
- The PostHog client and upstream US project token are removed.
- The app sends allowlisted event batches directly to the Memento Traction
  module using its per-install managed-gateway JWT.
- The collector validates that JWT against the gateway, requires
  `product=memento`, and replaces every caller-supplied actor with the verified
  install id. A shared server ingest token is never embedded in the desktop.
- The static `X-Ingest-Token` path remains only for trusted server-to-server
  migration jobs.

## What requires a client change

| Gap | Required event/property | Why it cannot be reconstructed honestly |
| --- | --- | --- |
| Exact summary adoption by meeting | `meeting_id_hash` on summary events | Current summary events are attempt-grain and cannot be joined to a meeting. |
| Search and Memento-chat value | success events with scope and latency bucket | Page views do not prove a useful result was returned. |
| Structured-memory adoption | generated/reviewed/accepted events for Standup, 1:1, Interview | Local database state never leaves the device. |
| Diarization adoption and quality | requested/completed/failed plus speaker-count buckets | Existing events only expose transcript segment count. |
| Auto-detection funnel | detected → prompt shown → accepted/dismissed → recording started | Settings toggles do not measure real detections. |
| Exact first-value latency | stable install timestamp + first value event | First observed analytics event can happen long after install/opt-in. |
| Historical local archive | one consent-gated aggregate `analytics_snapshot_v1` | Traction cannot see meetings created before analytics was enabled. |
| Perceived output quality | explicit thumbs up/down or reason buckets | Success status only means the pipeline completed. |

Do not collect transcript text, meeting titles, file paths, device names, raw
errors, prompts, summary content, participant names, or stable cross-product
identifiers. When meeting-level joins are needed, send a product-local salted
hash rather than the database id.

## Data-quality rules

- Every new first-party event has one `event_id`. The collector uses it for
  retry idempotency. A legacy PostHog import carrying the same id is also
  deduplicated.
- Existing PostHog events use their PostHog UUID during historical import.
- Internal/test devices are ingested but filtered at query time, making the
  exclusion retroactive and reversible.
- Duration inputs must be finite and non-negative; invalid values contribute
  to a quality warning, never to hours.
- Moscow dates are used for daily activity, ISO-week WAU, and cohorts. MAU is
  rolling 30 Moscow dates; the distinction is shown in labels/tooltips.
- A metric based on a proxy is labelled as a proxy in the dashboard until the
  missing client property ships and enough coverage accumulates.

## Delivery phases

1. **Now, server-side:** per-install authentication, idempotent storage,
   exclusions, WVD, device funnel, value retention, quality panel, canonical
   Sessions/DAU and Tools/DAU, rate limiting, and 365-day retention.
2. **Next desktop release:** first-party delivery, removal of PostHog, explicit
   opt-in migration, event ids, release channel/build profile, and privacy
   allowlist. Exact meeting-level summary joins and missing value events remain
   follow-ups.
3. **After coverage matures:** version/provider reliability comparisons and
   targets. Do not set numerical targets before at least four complete weeks of
   stable, deduplicated data are available.
