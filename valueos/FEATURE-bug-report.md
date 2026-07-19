# Feature: In-app bug reporting

Let a user report a bug from inside the agent — a free-text description plus scrubbed local
logs and diagnostic metadata — so Value Accelerator can debug, with **sensitive data removed
before anything leaves the machine**. All in our namespace (`frontend/src/valueos/bugreport/`);
no upstream files modified.

## UI

**Settings → Help → "Report a bug"** opens a modal with a description box and a **visible note
of exactly what will be attached** (so the user consents):
- Their description.
- Recent app logs (scrubbed).
- App version & build, OS/platform & architecture, install id, current workspace, time zone.
- An explicit line: *your recordings and transcript text are never attached.*

On submit: progress → success (with a reference) or failure. **On failure it offers "Save
locally"** so the bundle isn't lost. A stable `idempotency_key` is generated once per submission
and reused on retry.

## Bundle contents (`buildBugReport.ts`)

- **description** — the user's text.
- **metadata** (`BugReportMetadata`) — app_version, build, platform, os_version?, arch,
  install_id (from the telemetry install id, `valueos_install_id`), tenant_id (only when a
  workspace is selected), timestamp, timezone, engine.
- **scrubbed_logs** — recent app output. The app has no persistent log file (`env_logger`
  writes to stderr), so `logBuffer.ts` patches `console.*` once (at the app root) into a bounded
  ring buffer (≤2000 lines / ~0.5 MB) — no gigabytes shipped.

## Scrubbing (`scrub.ts`) — required guarantee, fail-closed

A single pass runs over the whole bundle (logs + description) **before it can be sent or saved**:
- **Auth material redacted:** JWTs, `Authorization: Bearer …`, and `access_token` /
  `refresh_token` / `id_token` / `token` / `code` / `code_verifier` / `code_challenge` /
  `x-api-key` / `api_key` / `password` / `secret` (quoted JSON *and* `key=value`).
- **Presigned URLs** (S3/CloudFront `X-Amz-Signature`/`Signature`/…) → query string redacted.
- **PII:** emails → `[EMAIL]`.
- **Transcript content:** raw transcript text / audio are **never** put in the bundle; and any
  log line that may contain captured speech (matches transcript markers) is dropped
  (`[line omitted …]`). Structured metadata is safe and left intact.

Fail-closed: when unsure, redact. **Tested** (`bug-report-scrub.test.ts`): a fake token, a
Bearer header, a presigned signature, and a sample email are all **absent** from the output; a
transcript log line is dropped.

## Destination — ValueOS endpoint (contract NOT yet available)

Built against a **typed interface + mock**, exactly like the updater was before its contract:

```ts
interface BugReportService { submit(bundle: BugReportBundle): Promise<{ reportId }> }
```

- **Mock** (`MockBugReportService`) — authoritative for tests; records submissions, can be set
  to fail.
- **Interim default** in the app (`LocalFileBugReportService`) — **saves the scrubbed bundle
  locally** (native `valueos_save_bug_report` → app-data `bug-reports/report-<uuid>.json`) and
  returns a local id, so real reports are captured until the endpoint exists.

**All contract assumptions are isolated in `service.ts`** and clearly marked
`⚠️ VALUEOS-CONTRACT-TBD`. The assumed real shape (swap-in is a one-file change):
```
POST /api/agent/v1/tenants/{tenantId}/bug-reports   (Bearer access token, tenant-scoped)
body { description, metadata, logs, idempotency_key }  ->  { reportId }
```
Paste the real contract and only `service.ts` changes.

## Local-save fallback

`saveBugReportLocally(bundle)` writes the scrubbed bundle to disk (best-effort). It backs both
the interim default transport and the on-failure "Save locally" action, so a report is never
lost even offline.

## Tests

- `bug-report-scrub.test.ts` — the scrubbing guarantees (auth material, presigned, email,
  transcript lines).
- `bug-report-flow.test.ts` — assemble → scrub → submit via the mock; failure surfaces.
- `shell.test.tsx` — end-to-end from the UI: open the dialog in Settings, submit, and the mock
  records a **scrubbed** bundle (email redacted) with the current tenant.
