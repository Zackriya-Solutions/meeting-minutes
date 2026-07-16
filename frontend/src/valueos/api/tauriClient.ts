// VALUEOS: REAL ValueOsClient — thin wrappers over native valueos_api_* commands. The Rust
// module (Phase 3) makes the authenticated HTTP calls with the keychain-held token, so the
// webview never sees the token and no CSP change is needed. Same interface as the mock.
import type { ValueOsClient } from './client';
import {
  ActivityType,
  Entitlement,
  Lead,
  ListParams,
  Opportunity,
  Paginated,
  Tenant,
  UploadRequest,
  UploadResult,
} from './types';
import { callValueOs } from '../transport/invoke';

export class TauriValueOsClient implements ValueOsClient {
  getTenants(): Promise<Paginated<Tenant>> {
    return callValueOs('valueos_api_get_tenants');
  }
  getEntitlement(tenantId: string): Promise<Entitlement> {
    return callValueOs('valueos_api_get_entitlement', { tenant_id: tenantId });
  }
  listLeads(tenantId: string, params?: ListParams): Promise<Paginated<Lead>> {
    return callValueOs('valueos_api_list_leads', {
      tenant_id: tenantId,
      q: params?.q,
      limit: params?.limit,
      offset: params?.offset,
    });
  }
  listOpportunities(tenantId: string, params?: ListParams): Promise<Paginated<Opportunity>> {
    return callValueOs('valueos_api_list_opportunities', {
      tenant_id: tenantId,
      q: params?.q,
      limit: params?.limit,
      offset: params?.offset,
    });
  }
  uploadTranscript(
    tenantId: string,
    activityType: ActivityType,
    targetId: string,
    req: UploadRequest,
  ): Promise<UploadResult> {
    return callValueOs('valueos_api_upload_transcript', {
      tenant_id: tenantId,
      activity_type: activityType,
      target_id: targetId,
      request: req,
    });
  }
}
