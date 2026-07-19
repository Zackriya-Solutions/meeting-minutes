# Feature: Local updater (wired to the real ValueOS API)

Prompt-first, **never silent**, **never loses local data**. The updater is bound to the real
ValueOS Agent contract — `updates/check`, the presigned download, and `telemetry` — with the
HTTP layer mocked only in tests.

## Flow

1. **install_id** — on first launch the native side generates a UUIDv4 and persists it in the
   app data dir (`valueos-install-id`); it is reused forever. `valueos_install_id` (mod.rs).
2. **Register + reconcile** (app entry, `AppFlow`): once per install, report telemetry
   `install`; on every launch, if the running version is newer than the last recorded run,
   report `update_success` (from_version → to_version). A long-interval `heartbeat` then sends
   periodic `check` events.
3. **Check** (user-triggered in **Settings → Software updates**): `GET
   /tenants/{tid}/updates/check?platform=&current_version=` (scope `read:releases` +
   `feat_agent`) via `valueos_api_check_update`. The tenant is the one resolved by the
   post-login gate (`/me/agent-tenants`).
4. **Notify + wait** — if `update_available`, the UI shows current vs `latest` (+ notes)
   and a **Download & install** button. `notify_only:true` from the server; we **never**
   auto-install. Nothing happens without the user clicking install.
5. **Download + verify + apply** — on confirm: `valueos_download_update` fetches the presigned
   `download_url` (short-lived ~300s; if expired, re-check), and **verifies the SHA-256
   checksum before the file can be applied** (mismatch → hard reject, `update_failure`).
   `valueos_apply_update` then hands the verified installer to the OS and exits so it can
   replace the app; the user relaunches the new version (which reports `update_success` via
   step 2's reconcile).
6. **Telemetry** — `update_success` (from→to) or `update_failure` (+ `detail`, e.g. "checksum
   mismatch"), through `valueos_api_report_telemetry` (scope `write:telemetry` + `feat_agent`).

## Errors

- `401` → the native `api_get`/`api_post` force one token refresh + retry; if it still fails,
  the updater returns `reauth` and the UI asks the user to sign in again.
- `403 {scope}` → returned as `reauth` with a message to sign in again (the token predates the
  new `read:releases`/`write:telemetry` scopes — a fresh login grants them).
- `403 {feature:"feat_agent"}` → `deEntitled`: stop offering updates for that tenant (the flow
  re-runs `/me/agent-tenants` elsewhere).
- Telemetry is best-effort and never blocks or breaks the user (failures are swallowed).

## Data preservation (guaranteed + tested)

An update + relaunch leaves ALL of these unchanged:

- Cognito tokens (OS keychain, `keyring`) — the updater never logs out or clears them.
- Local config incl. the transcript folder (`valueos.transcriptFolder`).
- The pending-upload queue.
- The `install_id` (persisted app-data file, stable forever).
- All stored transcripts + local history.

This holds **by construction**: the updater module has no reference to tokens/config/queue/
history, and applying an update only opens an installer + exits (it never touches user data).
`valueos/shell-tests/updater.test.ts` asserts it explicitly — a full check→download→apply
cycle plus a version-bump relaunch, after which tokens/config/history are intact and the
`install_id` is stable, with `update_success` reported from→to.

## Architecture — ONE typed client

Release checks and telemetry go through the **same** `ValueOsClient` as tenants/leads/calls
(`api/client.ts` interface → `api/tauriClient.ts` real / `api/mockClient.ts` mock), not the
stock `services/updateService.ts`. The orchestration is a small pure module
(`valueos/updater/updater.ts`, `createUpdater({ client, native, store })`) with the native
surface (`valueos/updater/nativeUpdater.ts`) wrapping the `valueos_*` commands, and it is
injected via the DI (`ValueOsServices.updater`) exactly like every other service.

Native commands (mod.rs, registered in lib.rs): `valueos_install_id`, `valueos_app_info`,
`valueos_api_check_update`, `valueos_api_report_telemetry`, `valueos_download_update`,
`valueos_apply_update`. Scopes are the full set requested at authorize (`openid` + the six
ValueOS agent scopes, incl. `read:releases` + `write:telemetry`) in both the Rust `SCOPES`
const (authoritative) and the TS `VALUEOS_SCOPES`.

## Why not the Tauri updater plugin's manifest flow

The plugin resolves downloads from a **static manifest** at `plugins.updater.endpoints`; its
JS `check()` cannot carry a per-request **presigned** URL from our API contract. So the updater
is driven through our own API (steps above), and the plugin is only a fallback. Its endpoint in
`tauri.conf.json` previously pointed at the **upstream Zackriya project** — a real bug (the app
could self-update to upstream Meetily). **Upstream edit made (authorized):**
`frontend/src-tauri/tauri.conf.json` — `plugins.updater.endpoints` repointed to a ValueOS-owned
URL so a stray plugin check can never pull upstream. `plugins.updater.pubkey` is unused by our
flow (we distribute plain installers via `publish.yml` and verify with SHA-256, not Tauri
signed updater bundles); it would only need a ValueOS-owned minisign key if we ever adopt the
plugin's signed-bundle update path.

## Tests

`valueos/shell-tests/updater.test.ts` (9 tests, HTTP mocked): register-once, version-bump
`update_success`, check availability/up-to-date, download+verify+apply success (checksum passed
to native, no failure event), `update_failure` on checksum mismatch (detail + to_version),
`401`→reauth / `403 feat_agent`→deEntitled mapping, and the data-preservation cycle above.
