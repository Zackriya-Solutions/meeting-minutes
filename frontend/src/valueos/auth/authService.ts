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
  /** true iff at least one tenant is actively entitled (gate passes). */
  anyEntitled: boolean;
  all: EntitledTenant[];
  entitled: EntitledTenant[]; // subset with entitlement.active === true
}

export interface AuthService {
  isLoggedIn(): Promise<boolean>;
  login(): Promise<void>;
  logout(): Promise<void>;
  /** Fetch tenants + per-tenant entitlement to drive the gate + later tenant choices. */
  loadEntitlementSummary(): Promise<EntitlementSummary>;
}

export async function summarizeEntitlements(client: ValueOsClient): Promise<EntitlementSummary> {
  const tenants = (await client.getTenants()).items;
  const all: EntitledTenant[] = [];
  for (const tenant of tenants) {
    const entitlement = await client.getEntitlement(tenant.id);
    all.push({ tenant, entitlement });
  }
  const entitled = all.filter((e) => e.entitlement.active);
  return { anyEntitled: entitled.length > 0, all, entitled };
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
