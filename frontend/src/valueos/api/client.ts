// VALUEOS: the typed ValueOS Agent API client interface. Screens depend ONLY on this
// interface; concrete transports are the mock (mockClient.ts) and — Phase 3 — a real
// transport that calls Rust/plugin commands. This keeps the flow testable and lets us
// wire the native transport later without touching any screen.
import type {
  ActivityType,
  AgentTenantsResult,
  CreateCallRequest,
  Entitlement,
  Lead,
  ListParams,
  Opportunity,
  Paginated,
  Tenant,
  TelemetryEvent,
  UpdateCheckResult,
  UploadRequest,
  UploadResult,
} from './types';

export interface ValueOsClient {
  /** The post-login GATE (contract §2): the workspaces where the agent add-on is ACTIVE
   *  right now — the ONLY tenants the app may offer. scope read:tenants */
  getAgentTenants(): Promise<AgentTenantsResult>;
  /** All tenants the user is a member of (entitled or not). Not the gate; kept for
   *  completeness. scope read:tenants */
  getTenants(): Promise<Paginated<Tenant>>;
  /** Agent entitlement detail for a tenant (active | expired | never); optional — the gate
   *  is getAgentTenants(). scope read:tenants */
  getEntitlement(tenantId: string): Promise<Entitlement>;
  /** Existing leads in a permitted, entitled tenant (read-only, searchable). scope read:leads */
  listLeads(tenantId: string, params?: ListParams): Promise<Paginated<Lead>>;
  /** Existing opportunities (read-only, searchable). scope read:opportunities */
  listOpportunities(tenantId: string, params?: ListParams): Promise<Paginated<Opportunity>>;
  /** PRIMARY write path: create a call activity AND attach its transcript+digest in one
   *  atomic op (link in the body — XOR lead_id/opportunity_id). scope write:transcripts */
  createCall(tenantId: string, req: CreateCallRequest): Promise<UploadResult>;
  /** Fallback write path: attach transcript + digest to an EXISTING lead/opportunity (link
   *  in the path, idempotency_key required). Prefer createCall(). scope write:transcripts */
  uploadTranscript(
    tenantId: string,
    activityType: ActivityType,
    targetId: string,
    req: UploadRequest,
  ): Promise<UploadResult>;
  /** Check for an agent update (scope read:releases + feat_agent). Notify-only — the caller
   *  never auto-installs; it prompts the user first. */
  checkUpdate(tenantId: string, platform: string, currentVersion: string): Promise<UpdateCheckResult>;
  /** Report an updater telemetry event (scope write:telemetry + feat_agent). */
  reportTelemetry(tenantId: string, event: TelemetryEvent): Promise<void>;
}
