// VALUEOS WS4: the updater orchestration — prompt-first, notify-only, never loses local data.
// Pure logic over injected dependencies (client + native + a small key/value store), so it is
// fully unit-testable with the HTTP layer mocked. The real bindings (native invoke + local
// storage) are wired in nativeUpdater.ts. Flow:
//   register-once + reconcile (report update_success if the version increased since last run)
//   → user-triggered check (telemetry 'check' + GET updates/check)
//   → prompt (caller) → download (presigned) + checksum-verify + apply (open installer, exit)
//   → telemetry update_success (next launch, via reconcile) / update_failure (on error).
import type { ValueOsClient } from '../api/client';
import { ValueOsApiError, type TelemetryEventType, type UpdateCheckResult } from '../api/types';

/** Thin native surface (backed by valueos_* commands in production; faked in tests). */
export interface UpdaterNative {
  appInfo(): Promise<{ platform: string; version: string }>;
  installId(): Promise<string>;
  /** Download the presigned URL, verify the checksum if provided, return the staged path. */
  download(url: string, sha256?: string | null): Promise<string>;
  /** Hand the verified installer to the OS and quit so it can apply; the user relaunches. */
  apply(path: string): Promise<void>;
}

/** Small persisted key/value store (localStorage in production; in-memory in tests). */
export interface UpdaterStore {
  get(key: string): string | null;
  set(key: string, value: string): void;
}

export type UpdateStatusKind = 'up-to-date' | 'available' | 'applying' | 'error' | 'reauth' | 'deEntitled';
export interface UpdateOutcome {
  status: UpdateStatusKind;
  result?: UpdateCheckResult;
  error?: string;
}

export interface Updater {
  /** Once per install: report `registered`; every launch: report `update_success` if we
   *  upgraded since the last run (from_version → to_version). */
  registerAndReconcile(tenantId: string): Promise<void>;
  /** Periodic liveness ping (`check` event) with no UI. */
  heartbeat(tenantId: string): Promise<void>;
  /** User-triggered check. Emits `check`, then returns whether an update is available. */
  checkForUpdates(tenantId: string): Promise<UpdateOutcome>;
  /** After the user confirms: download + verify + apply. Reports `update_failure` on error. */
  downloadAndApply(tenantId: string, result: UpdateCheckResult): Promise<UpdateOutcome>;
}

const K = {
  registered: 'valueos.updater.registered',
  lastVersion: 'valueos.updater.lastVersion',
};

export function createUpdater(deps: {
  client: Pick<ValueOsClient, 'checkUpdate' | 'reportTelemetry'>;
  native: UpdaterNative;
  store: UpdaterStore;
}): Updater {
  const { client, native, store } = deps;

  async function telemetry(
    tenantId: string,
    event_type: TelemetryEventType,
    extra?: { from_version?: string; to_version?: string; detail?: string },
  ): Promise<void> {
    // Telemetry is best-effort — a failure here must never block or break the user.
    try {
      const [{ platform, version }, install_id] = await Promise.all([native.appInfo(), native.installId()]);
      await client.reportTelemetry(tenantId, {
        install_id,
        platform,
        current_version: version,
        event_type,
        ...extra,
      });
    } catch {
      /* swallow */
    }
  }

  function mapErr(e: unknown, fallback: UpdateStatusKind = 'error'): UpdateOutcome {
    if (e instanceof ValueOsApiError) {
      if (e.isAuth) return { status: 'reauth', error: e.message };
      if (e.isNotEntitled) return { status: 'deEntitled', error: e.message };
      if (e.status === 403 && e.scope) return { status: 'reauth', error: `Sign in again to grant ${e.scope}.` };
    }
    return { status: fallback, error: (e as Error)?.message ?? 'Update error' };
  }

  return {
    async registerAndReconcile(tenantId) {
      const { version } = await native.appInfo();
      const last = store.get(K.lastVersion);
      if (last && last !== version) {
        // We came up on a new version → the previous update succeeded.
        await telemetry(tenantId, 'update_success', { from_version: last, to_version: version });
      }
      store.set(K.lastVersion, version);
      if (store.get(K.registered) !== '1') {
        await telemetry(tenantId, 'install');
        store.set(K.registered, '1');
      }
    },

    async heartbeat(tenantId) {
      await telemetry(tenantId, 'check');
    },

    async checkForUpdates(tenantId) {
      await telemetry(tenantId, 'check');
      try {
        const { platform, version } = await native.appInfo();
        const result = await client.checkUpdate(tenantId, platform, version);
        return { status: result.update_available ? 'available' : 'up-to-date', result };
      } catch (e) {
        return mapErr(e);
      }
    },

    async downloadAndApply(tenantId, result) {
      if (!result.download_url) {
        return { status: 'error', error: 'This update has no download link — check again.' };
      }
      try {
        const path = await native.download(result.download_url, result.sha256 ?? undefined);
        await native.apply(path); // opens the verified installer and exits; user relaunches
        return { status: 'applying', result };
      } catch (e) {
        await telemetry(tenantId, 'update_failure', {
          to_version: result.latest ?? undefined,
          detail: (e as Error)?.message,
        });
        return mapErr(e);
      }
    },
  };
}
