// VALUEOS: ⚠️ MOCK transport — NOT REAL. In-memory implementation of ValueOsClient used
// by the flow until the Phase 3 real transport (tauri-plugin-oauth + Rust reqwest) is
// wired. Deterministic, configurable, and used by our tests. Every method mirrors the
// Part 28 contract's shapes and error codes.
import type { ValueOsClient } from './client';
import {
  ActivityType,
  AgentTenantsResult,
  CreateCallRequest,
  Entitlement,
  EntitlementState,
  Lead,
  ListParams,
  Opportunity,
  Paginated,
  Tenant,
  UploadRequest,
  UploadResult,
  ValueOsApiError,
} from './types';

export interface MockSeed {
  tenants: Tenant[];
  entitlements: Record<string, EntitlementState>; // tenantId -> state
  leads: Record<string, Lead[]>; // tenantId -> leads
  opportunities: Record<string, Opportunity[]>; // tenantId -> opportunities
  /** When false, every call throws 401 (simulates missing/expired token → re-auth). */
  authenticated?: boolean;
}

function paginate<T>(all: T[], params?: ListParams): Paginated<T> {
  const limit = Math.min(200, Math.max(1, params?.limit ?? 50));
  const offset = Math.max(0, params?.offset ?? 0);
  return { items: all.slice(offset, offset + limit), total: all.length, limit, offset };
}

function match(hay: (string | null | undefined)[], q: string): boolean {
  const needle = q.trim().toLowerCase();
  if (!needle) return true;
  return hay.some((h) => (h ?? '').toLowerCase().includes(needle));
}

export class MockValueOsClient implements ValueOsClient {
  private seed: MockSeed;
  /** Records uploads by idempotency key so retries replay (idempotent:true). */
  private uploads = new Map<string, UploadResult>();
  /** Test hook: force the next N calls to throw 503 (retryable). */
  failNext503 = 0;

  constructor(seed: MockSeed) {
    this.seed = { authenticated: true, ...seed };
  }

  setAuthenticated(v: boolean) {
    this.seed.authenticated = v;
  }
  setEntitlement(tenantId: string, state: EntitlementState) {
    this.seed.entitlements[tenantId] = state;
  }

  private guardAuth() {
    if (this.seed.authenticated === false) {
      throw new ValueOsApiError(401, 'No / invalid / expired token');
    }
    if (this.failNext503 > 0) {
      this.failNext503 -= 1;
      throw new ValueOsApiError(503, 'Store temporarily unavailable — retry');
    }
  }

  private guardMember(tenantId: string) {
    if (!this.seed.tenants.some((t) => t.id === tenantId)) {
      // not a member → 403 with no feature/scope, per the contract
      throw new ValueOsApiError(403, 'Not a member of that tenant');
    }
  }

  private guardEntitled(tenantId: string) {
    if ((this.seed.entitlements[tenantId] ?? 'never') !== 'active') {
      throw new ValueOsApiError(403, 'Tenant not entitled to the ValueOS Agent add-on', {
        feature: 'feat_agent',
      });
    }
  }

  async getAgentTenants(): Promise<AgentTenantsResult> {
    this.guardAuth();
    // Server-side filter: ONLY tenants whose add-on is active right now (never/expired and
    // not-a-member are excluded). total_memberships counts ALL of the user's tenants.
    const items = this.seed.tenants
      .filter((t) => (this.seed.entitlements[t.id] ?? 'never') === 'active')
      .map((t) => ({ ...t, state: 'active' as EntitlementState, active: true }));
    return {
      items,
      total: items.length,
      total_memberships: this.seed.tenants.length,
      capability: 'valueos_agent',
      feature: 'feat_agent',
    };
  }

  async getTenants(): Promise<Paginated<Tenant>> {
    this.guardAuth();
    return { items: [...this.seed.tenants], total: this.seed.tenants.length };
  }

  async getEntitlement(tenantId: string): Promise<Entitlement> {
    this.guardAuth();
    this.guardMember(tenantId);
    const state = this.seed.entitlements[tenantId] ?? 'never';
    return { capability: 'valueos_agent', feature: 'feat_agent', state, active: state === 'active' };
  }

  async listLeads(tenantId: string, params?: ListParams): Promise<Paginated<Lead>> {
    this.guardAuth();
    this.guardMember(tenantId);
    this.guardEntitled(tenantId);
    let all = this.seed.leads[tenantId] ?? [];
    if (params?.q) all = all.filter((l) => match([l.label, l.company], params.q!));
    return paginate(all, params);
  }

  async listOpportunities(tenantId: string, params?: ListParams): Promise<Paginated<Opportunity>> {
    this.guardAuth();
    this.guardMember(tenantId);
    this.guardEntitled(tenantId);
    let all = this.seed.opportunities[tenantId] ?? [];
    if (params?.q) all = all.filter((o) => match([o.label, o.stage], params.q!));
    return paginate(all, params);
  }

  /** PRIMARY: composite create-call-with-transcript. XOR link in the body. */
  async createCall(tenantId: string, req: CreateCallRequest): Promise<UploadResult> {
    this.guardAuth();
    this.guardMember(tenantId);
    this.guardEntitled(tenantId);

    const hasLead = !!req.lead_id;
    const hasOpp = !!req.opportunity_id;
    const fields: Record<string, string> = {};
    if (!req.name) fields.name = 'required';
    if (!req.transcript?.raw_content) fields.raw_content = 'required';
    if (!req.transcript?.digest) fields.digest = 'required';
    if (hasLead === hasOpp) fields.link = 'exactly one of lead_id / opportunity_id is required';
    if (Object.keys(fields).length) throw new ValueOsApiError(422, 'Invalid request body', { fields });

    const targetId = (req.lead_id ?? req.opportunity_id)!;
    const targets = hasLead ? this.seed.leads[tenantId] : this.seed.opportunities[tenantId];
    if (!targets?.some((t) => t.id === targetId)) {
      throw new ValueOsApiError(404, 'Referenced lead/opportunity does not exist');
    }

    // Idempotency (only when a key is supplied): a retry replays the same ids.
    const key = req.idempotency_key;
    if (key) {
      const existing = this.uploads.get(key);
      if (existing) return { ...existing, idempotent: true };
    }
    const seed = key ?? `${targetId}-${this.uploads.size}`;
    const result: UploadResult = {
      idempotent: false,
      activity_id: `act-${seed}`,
      transcript_id: `tr-${seed}`,
      file_id: `file-${seed}`,
      s3_stored: true,
    };
    if (key) this.uploads.set(key, result);
    return result;
  }

  async uploadTranscript(
    tenantId: string,
    activityType: ActivityType,
    targetId: string,
    req: UploadRequest,
  ): Promise<UploadResult> {
    this.guardAuth();
    this.guardMember(tenantId);
    this.guardEntitled(tenantId);
    if (!req.raw_content || !req.digest || !req.idempotency_key) {
      throw new ValueOsApiError(422, 'Invalid request body', {
        fields: {
          ...(req.raw_content ? {} : { raw_content: 'required' }),
          ...(req.digest ? {} : { digest: 'required' }),
          ...(req.idempotency_key ? {} : { idempotency_key: 'required' }),
        },
      });
    }
    const targets =
      activityType === 'lead' ? this.seed.leads[tenantId] : this.seed.opportunities[tenantId];
    if (!targets?.some((t) => t.id === targetId)) {
      throw new ValueOsApiError(404, 'Referenced lead/opportunity does not exist');
    }
    // Idempotency: a retry with the same key replays the same ids and creates no duplicate.
    const existing = this.uploads.get(req.idempotency_key);
    if (existing) return { ...existing, idempotent: true };
    const result: UploadResult = {
      idempotent: false,
      activity_id: `act-${req.idempotency_key}`,
      transcript_id: `tr-${req.idempotency_key}`,
      file_id: `file-${req.idempotency_key}`,
      s3_stored: true,
    };
    this.uploads.set(req.idempotency_key, result);
    return result;
  }
}

/** Convenience seed for tests/dev: one active tenant with a couple of targets. */
export function defaultMockSeed(): MockSeed {
  return {
    authenticated: true,
    tenants: [{ id: 'tenant-acme', name: 'Acme GmbH', role: 'sales_user', roles: ['sales_user'] }],
    entitlements: { 'tenant-acme': 'active' },
    leads: {
      'tenant-acme': [
        {
          id: 'lead-1',
          label: 'Ada Lovelace',
          status: 'new',
          lead_type: 'inbound_lead',
          lead_source: null,
          company: 'Acme GmbH',
          owner_id: null,
          converted: false,
          created_at: '2026-01-01T00:00:00Z',
        },
      ],
    },
    opportunities: {
      'tenant-acme': [
        {
          id: 'opp-1',
          label: 'Acme Q3 Deal',
          stage: 'Discovery',
          status: 'open',
          close_date: '2026-08-15',
          amount: 50000,
          currency: 'EUR',
          account_id: null,
          owner_id: null,
          created_at: '2026-01-01T00:00:00Z',
        },
      ],
    },
  };
}
