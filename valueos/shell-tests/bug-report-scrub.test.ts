import { describe, it, expect } from 'vitest';
import { scrubText, scrubLogs, scrubBundle } from '@/valueos/bugreport/scrub';
import type { BugReportBundle } from '@/valueos/bugreport/types';

// VALUEOS WS3: the scrub MUST remove all auth material + PII before a bundle leaves the machine.
// These are the known-sensitive patterns the prompt requires to be absent.

const JWT = 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3In0.abcDEFghijKLmnopQRstuv';

describe('scrubText — auth material + PII', () => {
  it('redacts a Bearer header + JWT', () => {
    const s = scrubText(`Authorization: Bearer ${JWT}`);
    expect(s).not.toContain(JWT);
    expect(s).toContain('[REDACTED');
  });

  it('redacts token key/values (quoted JSON and unquoted kv)', () => {
    const s = scrubText('"access_token":"supersecretvalue" and token=anothersecret123 and code_verifier=pkceXYZ');
    expect(s).not.toContain('supersecretvalue');
    expect(s).not.toContain('anothersecret123');
    expect(s).not.toContain('pkceXYZ');
  });

  it('redacts presigned URL query strings', () => {
    const url = 'https://d2luofz0a4v7f3.cloudfront.net/agent-releases/app.dmg?X-Amz-Signature=deadbeefsig&X-Amz-Credential=AKIAEXAMPLE';
    const s = scrubText(url);
    expect(s).not.toContain('deadbeefsig');
    expect(s).not.toContain('AKIAEXAMPLE');
    expect(s).toContain('[REDACTED_PRESIGNED]');
  });

  it('redacts emails (PII)', () => {
    const s = scrubText('reach me at g.perrone@value-accelerator.io anytime');
    expect(s).not.toContain('g.perrone@value-accelerator.io');
    expect(s).toContain('[EMAIL]');
  });
});

describe('scrubLogs — transcript content', () => {
  it('drops lines that may contain transcript text, keeps the rest', () => {
    const logs = 'app started ok\n2026 [log] 📝 Latest transcript: we discussed the pricing plan\ntranscription engine ready';
    const s = scrubLogs(logs);
    expect(s).not.toContain('we discussed the pricing plan');
    expect(s).toContain('line omitted');
    expect(s).toContain('app started ok');
  });
});

describe('scrubBundle', () => {
  it('scrubs description + logs and leaves structured metadata intact', () => {
    const bundle: BugReportBundle = {
      description: `token=abc123secret and me@x.com`,
      metadata: {
        app_version: '0.0.1',
        build: 'b',
        platform: 'macos',
        install_id: 'iid-123',
        timestamp: '2026-07-18T00:00:00Z',
        timezone: 'UTC',
      },
      scrubbed_logs: `Authorization: Bearer ${JWT}`,
      idempotency_key: 'k',
    };
    const out = scrubBundle(bundle);
    expect(out.description).not.toContain('abc123secret');
    expect(out.description).toContain('[EMAIL]');
    expect(out.scrubbed_logs).not.toContain(JWT);
    expect(out.metadata.install_id).toBe('iid-123'); // metadata is structured + safe → untouched
  });
});
