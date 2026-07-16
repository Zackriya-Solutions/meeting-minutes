# Feature: ValueOS Full Flow

Chains the branded shell into the complete agent flow:

**get-started → browser/PKCE login (+entitlement gate) → model download (unchanged) →
configuration (transcript folder) → BLOCKING tenant + lead/opportunity selection →
record → store + high-level digest + authorized upload.**

Built in our namespace (`frontend/src/valueos/`, later `src-tauri/src/valueos/`), reusing
upstream via imports. Delivered in phases; this doc is the living source of truth.

## Status

| Phase | Scope | Upstream edits | State |
|------|-------|----------------|-------|
| **1** | Typed ValueOS client + PKCE + token store + digest + upload queue, all against interfaces + **mocks** | **none** | ✅ done, tested |
| **2** | Screens + `ValueOsShell` wiring (login → entitlement → config → capture → finalize), mock-backed | none | ⏳ next |
| **3** | Real transport: `tauri-plugin-oauth` (loopback) + `tauri-plugin-stronghold` (token vault) + real ValueOS HTTP; swap mocks for real behind the same interfaces | **small, marked** (see below) — needs approval before applied | ⏳ pending |

## Identity, scopes, least privilege

Login is OAuth2 **Authorization Code + PKCE** (public client, no secret) through the
**dedicated agent Cognito app client**, requesting ONLY these four scopes so the token can
never exceed them:

```
valueos/read:tenants  valueos/read:leads  valueos/read:opportunities  valueos/write:transcripts
```

PKCE S256 is computed in the webview (Web Crypto, `auth/pkce.ts`); the browser opens to the
Cognito authorize URL; the code returns via a **loopback** redirect
(`http://127.0.0.1:8765/callback`, or 14321). Tokens persist in secure storage and refresh;
**401/403 → re-auth**. After login, the **entitlement gate** checks the tenant's agent
add-on: `expired`/`never` → hard block with a CTA to <https://www.value-accelerator.io>;
`active` → continue. No bypass.

## The blocking-metadata rule (capture)

Recording **cannot start** until ALL of these are chosen — no "start now, fill later":
1. **Tenant** (from `read:tenants`),
2. **Activity type** — `lead` or `opportunity`,
3. the **specific existing lead/opportunity** (scoped to the tenant; searchable; agent
   attaches to EXISTING targets only, never creates them).

The START control enables only when tenant + type + target are all set. That selection is
the single upload destination for BOTH the transcript and the digest.

## Configuration

`configuration` step stores the **transcript folder** (persisted key
`valueos.transcriptFolder`; must be writable). Required before capture is possible.

## Finalize

On finish: write the transcript file to the configured folder; generate the **digest**
(readable HIGH-LEVEL recap, not a hash); upload **both** to the pre-selected tenant +
lead/opportunity with a client `idempotency_key`. Resilience via the pending-upload queue:
401 → re-auth; network/503 → keep the local file + retry; success (incl. idempotent
replay) → dequeue. **Write-only** (transcripts are never read back).

## Reused / imported vs copied vs mocked

- **Reused (import, no copy):** branded shell + model-download (`useOnboarding`);
  transcription — `useRecordingStart`, `useTranscripts` (finished transcript = `transcripts[]`),
  `useRecordingState`, `recordingService.stopRecording`; browser open — existing
  `open_external_url` command. Digest (Phase 3) reuses upstream local summarization
  (`crate::summary::llm_client::generate_summary`, or `api_process_transcript`) behind
  `digest/digest.ts`.
- **Copied:** none.
- **Mocked (Phase 1, clearly marked `⚠️ MOCK`):** `api/mockClient.ts` (ValueOS API),
  `auth/tokenStore.ts` `InMemoryTokenStore`, `digest/digest.ts` `MockDigestGenerator`,
  `upload/pendingQueue.ts` `InMemoryPendingUploadStore`. Real config values (client_id,
  hosted-UI domain, API base) come from Terraform outputs and are placeholders until wired.

## Phase-1 files (`frontend/src/valueos/`)

| File | Role |
|------|------|
| `api/types.ts` | Contract types (Tenant, Entitlement, Lead, Opportunity, Upload…) + `ValueOsApiError` (isAuth/isNotEntitled/isRetryable) + scopes |
| `api/client.ts` | `ValueOsClient` interface (the 5 API ops) |
| `api/mockClient.ts` | ⚠️ MOCK in-memory client (entitlement states, search, idempotent upload) |
| `auth/pkce.ts` | PKCE S256 + authorize-URL builder (Web Crypto) |
| `auth/tokenStore.ts` | `TokenStore` interface + in-memory mock |
| `digest/digest.ts` | `DigestGenerator` interface + mock recap |
| `upload/pendingQueue.ts` | Retry queue — never loses data |

Tests: `valueos/shell-tests/{pkce,valueos-client,upload-queue,digest}.test.ts` (+ existing
shell tests) — 23 passing, run in CI (`valueos-tests.yml`).

## Phase 3 upstream edits (proposed, NOT yet applied — needs approval)

Per your choice to use Tauri plugins:
- `frontend/src-tauri/Cargo.toml`: add `tauri-plugin-oauth`, `tauri-plugin-stronghold`.
- `frontend/src-tauri/src/lib.rs`: `// VALUEOS:` `.plugin(...)` registrations for both.
- `frontend/package.json`: matching JS deps.
- `frontend/src-tauri/tauri.conf.json`: `// VALUEOS:` CSP entries for the Cognito + ValueOS
  API hosts, plus the two plugin capabilities.

The exact diff will be presented for approval before anything upstream is touched.
