import { describe, it, expect, vi, beforeEach } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// VALUEOS: the REAL bug-report transport (ApiBugReportService) POSTs the mapped body via the
// native `valueos_api_report_bug` command. Here we mock that native call to assert the mapping,
// the fail-closed local fallback, error surfacing, and the required scope wiring.
const state = vi.hoisted(() => ({
  calls: [] as { cmd: string; args: any }[],
  impl: (async () => undefined) as (cmd: string, args: any) => Promise<any>,
}));

vi.mock('@/valueos/transport/invoke', () => ({
  callValueOs: (cmd: string, args: any) => {
    state.calls.push({ cmd, args });
    return state.impl(cmd, args);
  },
}));

import { ApiBugReportService } from '@/valueos/bugreport/service';
import { ValueOsApiError, VALUEOS_SCOPES } from '@/valueos/api/types';
import type { BugReportBundle } from '@/valueos/bugreport/types';

function makeBundle(over: Partial<BugReportBundle> = {}): BugReportBundle {
  return {
    description: 'Sync crashed uploading a call transcript\nsecond line',
    metadata: {
      app_version: '0.0.1',
      build: 'build abc',
      platform: 'macos',
      arch: 'aarch64',
      install_id: 'iid-1',
      timestamp: '2026-07-18T12:00:00Z',
      timezone: 'UTC',
      engine: 'parakeet',
      tenant_id: 'tenant-acme',
    },
    scrubbed_logs: 'TypeError: cannot read properties of undefined',
    idempotency_key: 'idem-1',
    ...over,
  };
}

beforeEach(() => {
  state.calls = [];
  state.impl = async () => undefined;
});

describe('ApiBugReportService (real bug-report transport)', () => {
  it('POSTs the mapped body to valueos_api_report_bug and returns the issue reference', async () => {
    state.impl = async (cmd) =>
      cmd === 'valueos_api_report_bug'
        ? { issue_number: 123, issue_url: 'https://github.com/va/bugs/issues/123' }
        : undefined;

    const res = await new ApiBugReportService().submit(makeBundle());

    expect(state.calls.map((c) => c.cmd)).toEqual(['valueos_api_report_bug']);
    const body = state.calls[0].args.report;
    expect(body.description).toContain('Sync crashed');
    expect(body.title).toBe('Sync crashed uploading a call transcript'); // first line, <= 80 chars
    expect(body.version).toBe('0.0.1');
    expect(body.platform).toBe('macos');
    expect(body.logs).toContain('TypeError');
    expect(body.context.tenant_id).toBe('tenant-acme');
    expect(body.context.idempotency_key).toBe('idem-1');

    expect(res.issueNumber).toBe(123);
    expect(res.issueUrl).toContain('/issues/123');
    expect(res.reportId).toBe('#123');
    expect(res.savedLocally).toBeFalsy();
  });

  it('surfaces a 403 missing-scope error and does NOT fall back to local save', async () => {
    state.impl = async (cmd) => {
      if (cmd === 'valueos_api_report_bug')
        throw new ValueOsApiError(403, 'missing scope', { scope: 'valueos/write:bug-reports' });
      return '/appdata/bug-reports/report-1.json';
    };
    await expect(new ApiBugReportService().submit(makeBundle())).rejects.toBeInstanceOf(ValueOsApiError);
    expect(state.calls.map((c) => c.cmd)).toEqual(['valueos_api_report_bug']); // no local save on 4xx
  });

  it('falls back to a local save on a 5xx/transport failure so nothing is lost', async () => {
    state.impl = async (cmd) => {
      if (cmd === 'valueos_api_report_bug') throw new ValueOsApiError(502, 'github failed after retries');
      if (cmd === 'valueos_save_bug_report') return '/appdata/bug-reports/report-1.json';
      return undefined;
    };
    const res = await new ApiBugReportService().submit(makeBundle());
    expect(res.savedLocally).toBe(true);
    expect(res.localPath).toContain('report-1.json');
    expect(state.calls.map((c) => c.cmd)).toEqual(['valueos_api_report_bug', 'valueos_save_bug_report']);
  });

  it('requests write:bug-reports in BOTH the TS scope list and the Rust authorize SCOPES const', () => {
    expect(VALUEOS_SCOPES).toContain('valueos/write:bug-reports');
    // The scope must be in the AUTHORIZE request (not merely allowed on the client) or POST
    // /bug-reports returns 403 {scope}. The Rust SCOPES const is the authoritative authorize set.
    const here = path.dirname(fileURLToPath(import.meta.url));
    const modrs = readFileSync(path.resolve(here, '../../frontend/src-tauri/src/valueos/mod.rs'), 'utf8');
    const scopesLine = modrs.match(/const SCOPES:\s*&str\s*=\s*"([^"]+)"/)?.[1] ?? '';
    expect(scopesLine).toContain('valueos/write:bug-reports');
  });
});
