import { describe, it, expect, vi, beforeEach } from 'vitest';

// Control the native invoke by mocking our single indirection module.
const inv = vi.hoisted(() => ({ fn: vi.fn() }));
vi.mock('@/valueos/transport/tauri', () => ({
  invoke: (...args: unknown[]) => inv.fn(...args),
}));

import { TauriValueOsClient } from '@/valueos/api/tauriClient';
import { createTauriAuthService } from '@/valueos/auth/tauriAuthService';
import { createTauriConfigService } from '@/valueos/config/tauriConfigService';
import { callValueOs } from '@/valueos/transport/invoke';
import { ValueOsApiError } from '@/valueos/api/types';

beforeEach(() => inv.fn.mockReset());

describe('real transport (invoke wrappers)', () => {
  it('client sends the correct commands + snake_case args', async () => {
    inv.fn.mockResolvedValue({ items: [], total: 0 });
    const c = new TauriValueOsClient();

    await c.getAgentTenants();
    expect(inv.fn).toHaveBeenLastCalledWith('valueos_api_get_agent_tenants', undefined);

    // Tauri v2 wants camelCase arg keys; the Rust side maps them to snake_case params.
    await c.listLeads('t1', { q: 'ada', limit: 10, offset: 5 });
    expect(inv.fn).toHaveBeenLastCalledWith('valueos_api_list_leads', {
      tenantId: 't1',
      q: 'ada',
      limit: 10,
      offset: 5,
    });

    inv.fn.mockResolvedValueOnce({ idempotent: false });
    await c.uploadTranscript('t1', 'opportunity', 'o9', {
      raw_content: 'x',
      digest: 'y',
      idempotency_key: 'k',
    });
    expect(inv.fn).toHaveBeenLastCalledWith('valueos_api_upload_transcript', {
      tenantId: 't1',
      activityType: 'opportunity',
      targetId: 'o9',
      // the request VALUE stays snake_case — it's the API body, not a Tauri arg
      request: { raw_content: 'x', digest: 'y', idempotency_key: 'k' },
    });
  });

  it('auth triggers the native login/logout commands', async () => {
    inv.fn.mockResolvedValue(undefined);
    const client = new TauriValueOsClient();
    const auth = createTauriAuthService(client);
    await auth.login();
    expect(inv.fn).toHaveBeenCalledWith('valueos_login', undefined);
    await auth.logout();
    expect(inv.fn).toHaveBeenCalledWith('valueos_logout', undefined);
  });

  it('config routes picker/validate/write to native commands', async () => {
    localStorage.clear();
    const config = createTauriConfigService();
    inv.fn.mockResolvedValueOnce('/picked/folder');
    expect(await config.pickFolder()).toBe('/picked/folder');
    expect(inv.fn).toHaveBeenLastCalledWith('valueos_pick_folder', undefined);
    await config.setTranscriptFolder('/picked/folder');
    inv.fn.mockResolvedValueOnce('/picked/folder/call.txt');
    const path = await config.writeTranscriptFile('call.txt', 'body');
    expect(path).toBe('/picked/folder/call.txt');
    expect(inv.fn).toHaveBeenLastCalledWith('valueos_write_transcript_file', {
      folder: '/picked/folder',
      fileName: 'call.txt',
      content: 'body',
    });
  });

  it('maps native error payloads to ValueOsApiError', async () => {
    inv.fn.mockRejectedValueOnce({ status: 403, feature: 'feat_agent', message: 'no add-on' });
    const e1 = await callValueOs('valueos_api_get_tenants').catch((e) => e);
    expect(e1).toBeInstanceOf(ValueOsApiError);
    expect((e1 as ValueOsApiError).isNotEntitled).toBe(true);

    inv.fn.mockRejectedValueOnce({ status: 401, message: 'expired' });
    const e2 = await callValueOs('x').catch((e) => e);
    expect((e2 as ValueOsApiError).isAuth).toBe(true);

    inv.fn.mockRejectedValueOnce('network down');
    const e3 = await callValueOs('x').catch((e) => e);
    expect((e3 as ValueOsApiError).status).toBe(0); // transport failure → retryable, not auth
  });
});
