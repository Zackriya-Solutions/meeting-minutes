import { describe, it, expect } from 'vitest';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { ValueOsApiError } from '@/valueos/api/types';

describe('MockValueOsClient (contract behaviors)', () => {
  it('lists the tenants the user belongs to', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    const t = await c.getTenants();
    expect(t.total).toBe(1);
    expect(t.items[0].id).toBe('tenant-acme');
  });

  it('reports entitlement state: active / expired / never', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    expect((await c.getEntitlement('tenant-acme')).active).toBe(true);
    c.setEntitlement('tenant-acme', 'expired');
    const exp = await c.getEntitlement('tenant-acme');
    expect(exp.state).toBe('expired');
    expect(exp.active).toBe(false);
    c.setEntitlement('tenant-acme', 'never');
    expect((await c.getEntitlement('tenant-acme')).state).toBe('never');
  });

  it('blocks reads with 403 feat_agent when the tenant is not entitled', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    c.setEntitlement('tenant-acme', 'expired');
    await expect(c.listLeads('tenant-acme')).rejects.toMatchObject({ status: 403, feature: 'feat_agent' });
  });

  it('throws 401 (re-auth) when unauthenticated', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    c.setAuthenticated(false);
    const err = await c.getTenants().catch((e) => e);
    expect(err).toBeInstanceOf(ValueOsApiError);
    expect((err as ValueOsApiError).isAuth).toBe(true);
  });

  it('searches leads and opportunities server-side', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    expect((await c.listLeads('tenant-acme', { q: 'ada' })).total).toBe(1);
    expect((await c.listLeads('tenant-acme', { q: 'zzz' })).total).toBe(0);
    expect((await c.listOpportunities('tenant-acme', { q: 'q3' })).total).toBe(1);
  });

  it('uploads to an existing target and is idempotent on retry', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    const req = { raw_content: 'hi', digest: 'recap', idempotency_key: 'key-1' };
    const first = await c.uploadTranscript('tenant-acme', 'lead', 'lead-1', req);
    expect(first.idempotent).toBe(false);
    const retry = await c.uploadTranscript('tenant-acme', 'lead', 'lead-1', req);
    expect(retry.idempotent).toBe(true);
    expect(retry.transcript_id).toBe(first.transcript_id); // same ids, no duplicate
  });

  it('rejects upload to a non-existent target with 404', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    await expect(
      c.uploadTranscript('tenant-acme', 'lead', 'nope', { raw_content: 'x', digest: 'y', idempotency_key: 'k' }),
    ).rejects.toMatchObject({ status: 404 });
  });
});
