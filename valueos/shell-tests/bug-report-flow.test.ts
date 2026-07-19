import { describe, it, expect } from 'vitest';
import { buildBugReport, type MetadataSource } from '@/valueos/bugreport/buildBugReport';
import { MockBugReportService } from '@/valueos/bugreport/service';

// VALUEOS WS3: the whole flow — assemble → scrub → send — offline via the mock.
const meta: MetadataSource = {
  appInfo: async () => ({ platform: 'macos', version: '0.0.1' }),
  installId: async () => 'iid-123',
  build: 'build abc',
  engine: 'parakeet',
  arch: 'aarch64',
};

const JWT = 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhYmMifQ.abcDEFghijKLmnopQR';

describe('buildBugReport + submit', () => {
  it('assembles metadata, scrubs the bundle, and the mock records it', async () => {
    const svc = new MockBugReportService();
    const bundle = await buildBugReport({
      description: `it broke; my token=${JWT} and email me@example.com`,
      tenantId: 'tenant-acme',
      meta,
      logs: `Authorization: Bearer ${JWT}\n📝 Latest transcript: confidential speech here`,
      idempotencyKey: 'idem-1',
      now: () => Date.parse('2026-07-18T12:00:00Z'),
    });

    // scrubbed BEFORE it can be sent
    expect(bundle.description).not.toContain(JWT);
    expect(bundle.description).toContain('[EMAIL]');
    expect(bundle.scrubbed_logs).not.toContain('confidential speech here');
    expect(bundle.scrubbed_logs).not.toContain(JWT);

    // metadata assembled
    expect(bundle.metadata).toMatchObject({
      app_version: '0.0.1',
      platform: 'macos',
      install_id: 'iid-123',
      tenant_id: 'tenant-acme',
      engine: 'parakeet',
      arch: 'aarch64',
    });
    expect(bundle.metadata.timestamp).toBe('2026-07-18T12:00:00.000Z');
    expect(bundle.idempotency_key).toBe('idem-1');

    const res = await svc.submit(bundle);
    expect(res.reportId).toContain('mock-');
    expect(svc.submissions).toHaveLength(1);
    expect(svc.submissions[0].idempotency_key).toBe('idem-1');
  });

  it('submission failure surfaces (so the UI can offer local save)', async () => {
    const svc = new MockBugReportService();
    svc.failNext = true;
    await expect(svc.submit({ idempotency_key: 'x' } as never)).rejects.toThrow();
    expect(svc.submissions).toHaveLength(0);
  });
});
