// VALUEOS: REAL ValueOsClient — thin wrappers over native valueos_api_* commands. The Rust
// module (Phase 3) makes the authenticated HTTP calls with the keychain-held token, so the
// webview never sees the token and no CSP change is needed. Same interface as the mock.
import type { ValueOsClient } from './client';
import {
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
import { callValueOs } from '../transport/invoke';

// NOTE: Tauri v2 expects command args in camelCase on the JS side and maps them to the
// snake_case Rust parameters (e.g. `tenantId` → `tenant_id`). Sending snake_case makes a
// REQUIRED Rust arg look missing ("missing required key tenantId"). So every key below is
// camelCase. (The upload `request` VALUE stays snake_case — it's the API body, passed
// through as an opaque JSON object, not renamed by Tauri.)
export class TauriValueOsClient implements ValueOsClient {
  getAgentTenants(): Promise<AgentTenantsResult> {
    return callValueOs('valueos_api_get_agent_tenants');
  }
  getTenants(): Promise<Paginated<Tenant>> {
    return callValueOs('valueos_api_get_tenants');
  }
  getEntitlement(tenantId: string): Promise<Entitlement> {
    return callValueOs('valueos_api_get_entitlement', { tenantId });
  }
  listLeads(tenantId: string, params?: ListParams): Promise<Paginated<Lead>> {
    return callValueOs('valueos_api_list_leads', {
      tenantId,
      q: params?.q,
      limit: params?.limit,
      offset: params?.offset,
    });
  }
  listOpportunities(tenantId: string, params?: ListParams): Promise<Paginated<Opportunity>> {
    return callValueOs('valueos_api_list_opportunities', {
      tenantId,
      q: params?.q,
      limit: params?.limit,
      offset: params?.offset,
    });
  }
  createCall(tenantId: string, req: CreateCallRequest): Promise<UploadResult> {
    return callValueOs('valueos_api_create_call', { tenantId, request: req });
  }
  uploadTranscript(
    tenantId: string,
    activityType: ActivityType,
    targetId: string,
    req: UploadRequest,
  ): Promise<UploadResult> {
    return callValueOs('valueos_api_upload_transcript', {
      tenantId,
      activityType,
      targetId,
      request: req,
    });
  }
  checkUpdate(tenantId: string, platform: string, currentVersion: string): Promise<UpdateCheckResult> {
    return callValueOs('valueos_api_check_update', { tenantId, platform, currentVersion });
  }
  reportTelemetry(tenantId: string, event: TelemetryEvent): Promise<void> {
    return callValueOs('valueos_api_report_telemetry', { tenantId, event });
  }
}
