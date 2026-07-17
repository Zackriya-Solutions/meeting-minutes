import { describe, it, expect, vi } from 'vitest';
import { createUpdater, type UpdaterNative, type UpdaterStore } from '@/valueos/updater/updater';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { createMockConfigService } from '@/valueos/config/configService';
import { InMemoryTokenStore } from '@/valueos/auth/tokenStore';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';
import { InMemoryTranscriptHistory } from '@/valueos/history/transcriptHistory';
import type { UpdateCheckResult } from '@/valueos/api/types';

// VALUEOS WS4: updater orchestration against the REAL contract shapes, with the HTTP layer
// mocked (MockValueOsClient). Covers the telemetry lifecycle (registered/check/update_success/
// update_failure), notify-only download+verify+apply, error mapping (401→reauth, 403
// feat_agent→deEntitled), and an explicit data-preservation guarantee.

const T = 'tenant-acme';

function memStore(): UpdaterStore {
  const m = new Map<string, string>();
  return { get: (k) => m.get(k) ?? null, set: (k, v) => void m.set(k, v) };
}
function fakeNative(
  over: { version?: string; installId?: string; download?: UpdaterNative['download']; apply?: UpdaterNative['apply'] } = {},
): UpdaterNative {
  return {
    appInfo: async () => ({ platform: 'macos', version: over.version ?? '1.0.0' }),
    installId: async () => over.installId ?? 'install-abc',
    download: over.download ?? (async () => '/tmp/valueos-update.dmg'),
    apply: over.apply ?? (async () => {}),
  };
}

describe('updater lifecycle', () => {
  it('registers once, then no-ops on subsequent runs of the same install/version', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const store = memStore();
    await createUpdater({ client, native: fakeNative(), store }).registerAndReconcile(T);
    await createUpdater({ client, native: fakeNative(), store }).registerAndReconcile(T);
    const registered = client.telemetry.filter((e) => e.event_type === 'registered');
    expect(registered).toHaveLength(1);
    expect(registered[0].install_id).toBe('install-abc');
    expect(registered[0].current_version).toBe('1.0.0');
  });

  it('reports update_success on the first run after a version bump (from → to)', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const store = memStore();
    await createUpdater({ client, native: fakeNative({ version: '1.0.0' }), store }).registerAndReconcile(T);
    await createUpdater({ client, native: fakeNative({ version: '1.1.0' }), store }).registerAndReconcile(T);
    const success = client.telemetry.find((e) => e.event_type === 'update_success');
    expect(success).toBeTruthy();
    expect(success!.from_version).toBe('1.0.0');
    expect(success!.to_version).toBe('1.1.0');
  });
});

describe('updater check + apply', () => {
  it('checkForUpdates emits a check event and surfaces availability', async () => {
    const seed = defaultMockSeed();
    seed.updateCheck = {
      update_available: true,
      latest_version: '2.0.0',
      download_url: 'https://x/app.dmg',
      sha256: 'abc',
      notify_only: true,
    };
    const client = new MockValueOsClient(seed);
    const out = await createUpdater({ client, native: fakeNative(), store: memStore() }).checkForUpdates(T);
    expect(out.status).toBe('available');
    expect(out.result?.latest_version).toBe('2.0.0');
    expect(client.telemetry.some((e) => e.event_type === 'check')).toBe(true);
  });

  it('reports up-to-date when no update is available', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const out = await createUpdater({ client, native: fakeNative(), store: memStore() }).checkForUpdates(T);
    expect(out.status).toBe('up-to-date');
  });

  it('downloads (with checksum) + applies, reporting NO failure on success', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const download = vi.fn(async () => '/tmp/app.dmg');
    const apply = vi.fn(async () => {});
    const u = createUpdater({ client, native: fakeNative({ download, apply }), store: memStore() });
    const result: UpdateCheckResult = {
      update_available: true,
      latest_version: '2.0.0',
      download_url: 'https://x/app.dmg',
      sha256: 'deadbeef',
    };
    const out = await u.downloadAndApply(T, result);
    expect(out.status).toBe('applying');
    expect(download).toHaveBeenCalledWith('https://x/app.dmg', 'deadbeef'); // checksum passed to native
    expect(apply).toHaveBeenCalledWith('/tmp/app.dmg');
    expect(client.telemetry.some((e) => e.event_type === 'update_failure')).toBe(false);
  });

  it('reports update_failure (with detail + to_version) when download/apply throws', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const download = vi.fn(async () => {
      throw new Error('checksum mismatch — update rejected');
    });
    const u = createUpdater({ client, native: fakeNative({ download }), store: memStore() });
    const out = await u.downloadAndApply(T, {
      update_available: true,
      latest_version: '2.0.0',
      download_url: 'https://x/app.dmg',
      sha256: 'expected',
    });
    expect(out.status).toBe('error');
    const fail = client.telemetry.find((e) => e.event_type === 'update_failure');
    expect(fail).toBeTruthy();
    expect(fail!.detail).toMatch(/checksum/i);
    expect(fail!.to_version).toBe('2.0.0');
  });
});

describe('updater error mapping', () => {
  it('maps 403 feat_agent → deEntitled', async () => {
    const seed = defaultMockSeed();
    seed.entitlements[T] = 'expired';
    const client = new MockValueOsClient(seed);
    const out = await createUpdater({ client, native: fakeNative(), store: memStore() }).checkForUpdates(T);
    expect(out.status).toBe('deEntitled');
  });

  it('maps 401 → reauth', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    client.setAuthenticated(false);
    const out = await createUpdater({ client, native: fakeNative(), store: memStore() }).checkForUpdates(T);
    expect(out.status).toBe('reauth');
  });
});

describe('updater data preservation', () => {
  it('an update cycle never touches tokens/config/queue/history, and install_id is stable', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    // a "world" of user data the updater must never disturb
    const tokenStore = new InMemoryTokenStore();
    await tokenStore.save({ accessToken: 'tok', refreshToken: 'ref', expiresAt: 4102444800000, scopes: [] });
    const config = createMockConfigService({ initialFolder: '/Users/me/VA Transcripts', writable: true });
    const queue = new PendingUploadQueue(client, new InMemoryPendingUploadStore());
    const history = new InMemoryTranscriptHistory();
    await history.add({
      id: 'k1',
      targetLabel: 'Ada',
      tenantId: T,
      activityType: 'lead',
      targetId: 'lead-1',
      createdAt: 1,
      path: '/Users/me/VA Transcripts/ada.txt',
      uploadStatus: 'uploaded',
      transcript: 'kept',
    });

    const store = memStore(); // survives updates (per-install data)
    const PERSISTED_ID = 'install-stable-across-updates';
    // v1.0.0 install + a confirmed update, then a "relaunch" on v1.1.0
    await createUpdater({ client, native: fakeNative({ version: '1.0.0', installId: PERSISTED_ID }), store }).registerAndReconcile(T);
    const applied = await createUpdater({
      client,
      native: fakeNative({ version: '1.0.0', installId: PERSISTED_ID }),
      store,
    }).downloadAndApply(T, { update_available: true, latest_version: '1.1.0', download_url: 'https://x/app.dmg' });
    expect(applied.status).toBe('applying');
    await createUpdater({ client, native: fakeNative({ version: '1.1.0', installId: PERSISTED_ID }), store }).registerAndReconcile(T);

    // user data intact
    expect(await tokenStore.load()).not.toBeNull();
    expect(await config.getTranscriptFolder()).toBe('/Users/me/VA Transcripts');
    expect((await history.list())).toHaveLength(1);
    expect((await history.list())[0].transcript).toBe('kept');
    expect(queue).toBeTruthy(); // pending queue store untouched (no clear happened)
    // install_id stable + success reported with the same id
    const success = client.telemetry.find((e) => e.event_type === 'update_success');
    expect(success?.install_id).toBe(PERSISTED_ID);
    expect(success?.from_version).toBe('1.0.0');
    expect(success?.to_version).toBe('1.1.0');
  });
});
