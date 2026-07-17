import { describe, it, expect } from 'vitest';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';

function makeItem(id: string, leadId = 'lead-1') {
  return {
    id,
    tenantId: 'tenant-acme',
    transcriptPath: `/tmp/${id}.txt`,
    request: {
      name: 'Call with Ada Lovelace',
      lead_id: leadId,
      transcript: { raw_content: 'hello', digest: 'recap' },
      idempotency_key: id,
    },
  };
}

describe('PendingUploadQueue (never lose data)', () => {
  it('uploads and removes on success', async () => {
    const q = new PendingUploadQueue(new MockValueOsClient(defaultMockSeed()), new InMemoryPendingUploadStore());
    await q.enqueue(makeItem('k1'));
    const out = await q.flush();
    expect(out.uploaded).toEqual(['k1']);
    expect(await q.count()).toBe(0);
  });

  it('RETAINS the item and increments attempts on a retryable (503) failure', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const store = new InMemoryPendingUploadStore();
    const q = new PendingUploadQueue(client, store);
    await q.enqueue(makeItem('k1'));
    client.failNext503 = 1; // first upload attempt fails 503
    const out = await q.flush();
    expect(out.uploaded).toEqual([]);
    expect(out.retained).toEqual(['k1']);
    expect(await q.count()).toBe(1); // NOT dropped
    expect((await store.list())[0].attempts).toBe(1);
    // a later flush (store healthy) succeeds
    const out2 = await q.flush();
    expect(out2.uploaded).toEqual(['k1']);
    expect(await q.count()).toBe(0);
  });

  it('stops and signals re-auth on 401, keeping items', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const q = new PendingUploadQueue(client, new InMemoryPendingUploadStore());
    await q.enqueue(makeItem('k1'));
    await q.enqueue(makeItem('k2'));
    client.setAuthenticated(false);
    const out = await q.flush();
    expect(out.needsReauth).toBe(true);
    expect(out.uploaded).toEqual([]);
    expect(await q.count()).toBe(2); // nothing lost
  });

  it('quarantines a 403 feat_agent (de-entitled) and reports the tenant to re-gate', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const store = new InMemoryPendingUploadStore();
    const q = new PendingUploadQueue(client, store);
    await q.enqueue(makeItem('k1'));
    client.setEntitlement('tenant-acme', 'expired'); // add-on lost mid-session
    const out = await q.flush();
    expect(out.deEntitled).toEqual(['tenant-acme']);
    expect(out.failed.map((f) => f.id)).toEqual(['k1']);
    expect(out.retained).toEqual([]);
    expect(await q.count()).toBe(0); // quarantined — NOT a poison pill retried forever
  });

  it('fails terminally (no endless retry) on a 404, instead of retaining', async () => {
    const client = new MockValueOsClient(defaultMockSeed());
    const store = new InMemoryPendingUploadStore();
    const q = new PendingUploadQueue(client, store);
    await q.enqueue(makeItem('k1', 'does-not-exist')); // lead_id that doesn't exist → 404
    const out = await q.flush();
    expect(out.failed.map((f) => f.id)).toEqual(['k1']);
    expect(out.failed[0].status).toBe(404);
    expect(out.retained).toEqual([]);
    expect(await q.count()).toBe(0); // terminal → dropped from the retry loop
  });
});
