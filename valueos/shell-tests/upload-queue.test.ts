import { describe, it, expect } from 'vitest';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';

function makeItem(id: string) {
  return {
    id,
    tenantId: 'tenant-acme',
    activityType: 'lead' as const,
    targetId: 'lead-1',
    transcriptPath: `/tmp/${id}.txt`,
    request: { raw_content: 'hello', digest: 'recap', idempotency_key: id },
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
});
