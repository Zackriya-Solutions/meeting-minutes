// VALUEOS: Types for the ValueOS Agent API (/api/agent/v1) — source of truth is the
// pasted contract "ValueOS Agent API (Part 28)". Kept in OUR namespace; no upstream types.

/** The OAuth2 scopes the agent client requests (VALUEOS_AGENT_API.md §1): standard `openid`
 *  plus the six ValueOS agent scopes. The token can never exceed this set. */
export const VALUEOS_SCOPES = [
  'openid',
  'valueos/read:tenants',
  'valueos/read:leads',
  'valueos/read:opportunities',
  'valueos/write:transcripts',
  'valueos/read:releases',
  'valueos/write:telemetry',
  'valueos/write:bug-reports',
] as const;
export type ValueOsScope = (typeof VALUEOS_SCOPES)[number];

/** Cognito / API configuration. Real values come from Terraform outputs (see FEATURE-flow.md);
 *  until wired they are placeholders — the mock transport ignores them. */
export interface ValueOsConfig {
  region: string; // e.g. eu-central-2
  clientId: string; // cognito_agent_client_id (public, no secret)
  hostedUiBase: string; // https://<prefix>.auth.eu-central-2.amazoncognito.com
  apiBase: string; // https://<host>/api/agent/v1
  scopes: readonly ValueOsScope[];
  callbackPorts: number[]; // loopback ports registered in Cognito (default 8765, 14321)
}

export interface Tenant {
  id: string;
  name: string;
  role: string;
  roles: string[];
}

export type EntitlementState = 'active' | 'expired' | 'never';
export interface Entitlement {
  capability: 'valueos_agent';
  feature: 'feat_agent';
  state: EntitlementState;
  active: boolean;
}

/** One workspace where the ValueOS Agent capability is ACTIVE right now (an item of
 *  GET /me/agent-tenants). Superset of Tenant carrying the live entitlement state. */
export interface AgentTenant extends Tenant {
  state: EntitlementState;
  active: boolean;
}

/** GET /me/agent-tenants — the post-login gate (contract §2). `items` are the ONLY
 *  workspaces the agent may operate in; `total_memberships` distinguishes "member of
 *  nothing" from "member of workspaces that lack the add-on" so the block can be worded. */
export interface AgentTenantsResult {
  items: AgentTenant[];
  total: number;
  total_memberships: number;
  capability?: 'valueos_agent';
  feature?: 'feat_agent';
}

export interface Lead {
  id: string;
  label: string; // full name, else company, else email
  status: string;
  lead_type: string | null;
  lead_source: string | null;
  company: string | null;
  owner_id: string | null;
  converted: boolean;
  created_at: string;
}

export interface Opportunity {
  id: string;
  label: string;
  stage: string;
  status: string;
  close_date: string | null;
  amount: number | null;
  currency: string | null;
  account_id: string | null;
  owner_id: string | null;
  created_at: string;
}

/** 'lead' | 'opportunity' — the activity type the transcript attaches to. */
export type ActivityType = 'lead' | 'opportunity';

export interface Paginated<T> {
  items: T[];
  total: number;
  limit?: number;
  offset?: number;
}

export interface ListParams {
  q?: string; // free-text search (server-side)
  limit?: number; // 1–200, default 50
  offset?: number; // default 0
}

/** Allowed content types per the contract. */
export type TranscriptContentType =
  | 'text/plain'
  | 'text/vtt'
  | 'text/markdown'
  | 'text/csv'
  | 'application/json'
  | 'application/octet-stream';

export interface UploadRequest {
  raw_content: string; // the transcript text (required)
  digest: string; // the high-level recap (required)
  idempotency_key: string; // client-unique (required); retry with same key does not duplicate
  file_name?: string; // default transcript.txt
  content_type?: TranscriptContentType;
  title?: string;
}

/** The transcript sub-object of POST /calls — a REQUIRED NESTED object (VALUEOS_AGENT_API.md
 *  §4 explicitly: "do NOT flatten these"). raw_content + digest are required; digest is
 *  generated LOCALLY by the agent (ValueOS does not generate it). */
export interface CallTranscript {
  raw_content: string; // required — the transcript text
  digest: string; // required — the high-level recap (agent-generated)
  digest_source?: string; // default 'ai_generated'
  content_type?: TranscriptContentType; // must be in the allowed set (else 422)
  file_name?: string; // default transcript.txt
  title?: string; // defaults to name/file_name
}

/** Body for POST /api/agent/v1/tenants/{tenantId}/calls (VALUEOS_AGENT_API.md §4) — creates a
 *  call activity AND attaches its transcript+digest in one atomic op. The transcript is a
 *  NESTED object (do NOT flatten). The link is EXACTLY ONE of lead_id / opportunity_id (XOR;
 *  both or neither → 422 {fields.link}). */
export interface CreateCallRequest {
  name: string; // required — the call activity's title (user-chosen at capture time)
  lead_id?: string; // XOR with opportunity_id
  opportunity_id?: string;
  transcript: CallTranscript; // required, nested
  occurred_at?: string; // optional (must start yyyy-mm-dd, else ignored)
  notes?: string; // optional → stored on the call activity
  idempotency_key?: string; // optional here; if sent, reuse the SAME key on every retry
}

export interface UploadResult {
  idempotent: boolean;
  activity_id: string;
  transcript_id: string;
  file_id: string | null;
  s3_stored?: boolean;
}

/** GET /tenants/{tid}/updates/check (VALUEOS_AGENT_API.md §5) — NOTIFY-ONLY (never
 *  auto-install). `download_url` is a short-lived (~5-min) presigned GET, null when no update.
 *  No build → update_available:false, latest:null. */
export interface UpdateCheckResult {
  update_available: boolean;
  current?: string | null;
  latest: string | null;
  platform?: string | null;
  download_url?: string | null;
  expires_in?: number | null;
  /** Optional integrity checksum the agent verifies before applying, when the server sends it. */
  sha256?: string | null;
  notes?: string | null;
}

/** POST /tenants/{tid}/telemetry (VALUEOS_AGENT_API.md §6) — event_type lifecycle. */
export type TelemetryEventType = 'install' | 'check' | 'update_success' | 'update_failure';
export interface TelemetryEvent {
  install_id: string; // agent-generated, persisted locally, stable forever
  platform: string;
  current_version: string;
  event_type: TelemetryEventType;
  from_version?: string;
  to_version?: string;
  detail?: string;
}

/** Structured error mirroring the contract's error envelope + codes. */
export class ValueOsApiError extends Error {
  status: number;
  scope?: string;
  feature?: string;
  fields?: Record<string, string>;
  constructor(
    status: number,
    message: string,
    extra?: { scope?: string; feature?: string; fields?: Record<string, string> },
  ) {
    super(message);
    this.name = 'ValueOsApiError';
    this.status = status;
    this.scope = extra?.scope;
    this.feature = extra?.feature;
    this.fields = extra?.fields;
  }
  /** 401 / expired token → the flow must re-auth. */
  get isAuth(): boolean {
    return this.status === 401;
  }
  /** Tenant lacks the agent add-on (403 + feat_agent) → entitlement block. */
  get isNotEntitled(): boolean {
    return this.status === 403 && this.feature === 'feat_agent';
  }
  /** Store temporarily unavailable → retryable. */
  get isRetryable(): boolean {
    return this.status === 503;
  }
}
