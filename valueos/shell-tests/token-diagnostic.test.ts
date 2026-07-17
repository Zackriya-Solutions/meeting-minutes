import { describe, it, expect, vi, beforeEach } from 'vitest';

// Control the native invoke by mocking the single indirection module.
const inv = vi.hoisted(() => ({ fn: vi.fn() }));
vi.mock('@/valueos/transport/tauri', () => ({
  invoke: (...args: unknown[]) => inv.fn(...args),
}));

import { getAccessTokenClaims } from '@/valueos/debug/tokenClaims';

beforeEach(() => inv.fn.mockReset());

describe('token-claims diagnostic', () => {
  it('fetches the access token CLAIMS via the native command (no token/secret)', async () => {
    inv.fn.mockResolvedValueOnce({
      client_id: '3kjnt13ct6k25u2hkvqatkfrrm',
      token_use: 'access',
      scope: 'valueos/read:tenants valueos/write:transcripts',
    });
    const claims = await getAccessTokenClaims();
    expect(inv.fn).toHaveBeenCalledWith('valueos_debug_token_claims', undefined);
    expect(claims).toMatchObject({ token_use: 'access' });
  });

  it('returns null when not logged in', async () => {
    inv.fn.mockResolvedValueOnce(null);
    expect(await getAccessTokenClaims()).toBeNull();
  });
});
