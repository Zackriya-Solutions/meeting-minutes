// VALUEOS: auth orchestration (login + entitlement summary). Phase 2 uses a mock login;
// Phase 3 replaces createMockAuthService with a real one that runs the PKCE browser flow
// (open_external_url + tauri-plugin-oauth loopback) and stores tokens in stronghold —
// behind this same interface, so screens never change.
import type { ValueOsClient } from '../api/client';
import type { Entitlement, Tenant } from '../api/types';
import { isExpired, TokenSet, TokenStore } from './tokenStore';

export interface EntitledTenant {
  tenant: Tenant;
  entitlement: Entitlement;
}

export interface EntitlementSummary {
  /** true iff at least one workspace has the agent add-on active (the gate passes). */
  anyEntitled: boolean;
  /** the entitled workspaces (from /me/agent-tenants) — the ONLY tenants to offer. */
  entitled: EntitledTenant[];
  /** how many workspaces the user is a member of at all — drives the block wording
   *  (0 = "no workspace"; >0 = "no add-on"). */
  totalMemberships: number;
}

export interface AuthService {
  isLoggedIn(): Promise<boolean>;
  login(): Promise<void>;
  logout(): Promise<void>;
  /** Run the post-login gate (GET /me/agent-tenants) to drive the entitlement block and
   *  the later tenant picker. */
  loadEntitlementSummary(): Promise<EntitlementSummary>;
}

export async function summarizeEntitlements(client: ValueOsClient): Promise<EntitlementSummary> {
  // Contract §2: /me/agent-tenants is the SINGLE post-login gate. It returns ONLY the
  // workspaces whose agent add-on is active right now (not-a-member / never / expired are
  // filtered server-side), plus total_memberships. We do NOT enumerate /me/entitlements
  // per tenant, and we do NOT rely on catching 403s to decide the gate.
  const res = await client.getAgentTenants();
  const entitled: EntitledTenant[] = res.items.map((t) => ({
    tenant: { id: t.id, name: t.name, role: t.role, roles: t.roles },
    entitlement: {
      capability: 'valueos_agent',
      feature: 'feat_agent',
      state: t.state,
      active: t.active,
    },
  }));
  return { anyEntitled: entitled.length > 0, entitled, totalMemberships: res.total_memberships };
}

/**
 * ⚠️ MOCK auth — login immediately succeeds and writes a fake token carrying ONLY the four
 * agent scopes. Real PKCE/browser/keychain login arrives in Phase 3 behind this interface.
 */
export function createMockAuthService(
  client: ValueOsClient,
  store: TokenStore,
  now: () => number = () => Date.now(),
): AuthService {
  return {
    async isLoggedIn() {
      const t = await store.load();
      return !!t && !isExpired(t, now());
    },
    async login() {
      const tokens: TokenSet = {
        accessToken: 'mock-access-token',
        refreshToken: 'mock-refresh-token',
        expiresAt: now() + 60 * 60 * 1000,
        scopes: [
          'valueos/read:tenants',
          'valueos/read:leads',
          'valueos/read:opportunities',
          'valueos/write:transcripts',
        ],
      };
      await store.save(tokens);
    },
    async logout() {
      await store.clear();
    },
    loadEntitlementSummary() {
      return summarizeEntitlements(client);
    },
  };
}
