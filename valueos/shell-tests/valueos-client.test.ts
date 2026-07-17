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

  it('agent-tenants is the gate: returns ONLY active workspaces + total_memberships', async () => {
    const c = new MockValueOsClient(defaultMockSeed());
    const a = await c.getAgentTenants();
    expect(a.items).toHaveLength(1);
    expect(a.items[0].id).toBe('tenant-acme');
    expect(a.items[0].active).toBe(true);
    expect(a.total).toBe(1);
    expect(a.total_memberships).toBe(1);
    // Lapsing the add-on removes it from the gate, but membership still counts.
    c.setEntitlement('tenant-acme', 'expired');
    const b = await c.getAgentTenants();
    expect(b.items).toHaveLength(0);
    expect(b.total).toBe(0);
    expect(b.total_memberships).toBe(1);
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

  describe('createCall (composite: call + transcript in one op)', () => {
    const call = (over = {}) => ({
      name: 'Discovery with Ada',
      lead_id: 'lead-1',
      raw_content: 'hi',
      digest: 'recap',
      idempotency_key: 'ck-1',
      ...over,
    });

    it('creates a call linked to a lead, idempotent on retry with the same key', async () => {
      const c = new MockValueOsClient(defaultMockSeed());
      const first = await c.createCall('tenant-acme', call());
      expect(first.idempotent).toBe(false);
      const retry = await c.createCall('tenant-acme', call());
      expect(retry.idempotent).toBe(true);
      expect(retry.transcript_id).toBe(first.transcript_id);
    });

    it('rejects when both or neither of lead_id/opportunity_id are set (XOR → 422 fields.link)', async () => {
      const c = new MockValueOsClient(defaultMockSeed());
      await expect(c.createCall('tenant-acme', call({ opportunity_id: 'opp-1' }))).rejects.toMatchObject({
        status: 422,
        fields: { link: expect.any(String) },
      });
      await expect(c.createCall('tenant-acme', call({ lead_id: undefined }))).rejects.toMatchObject({ status: 422 });
    });

    it('rejects a non-existent linked record with 404', async () => {
      const c = new MockValueOsClient(defaultMockSeed());
      await expect(c.createCall('tenant-acme', call({ lead_id: 'nope' }))).rejects.toMatchObject({ status: 404 });
    });

    it('403 feat_agent when the tenant is not entitled', async () => {
      const c = new MockValueOsClient(defaultMockSeed());
      c.setEntitlement('tenant-acme', 'never');
      await expect(c.createCall('tenant-acme', call())).rejects.toMatchObject({ status: 403, feature: 'feat_agent' });
    });
  });
});
