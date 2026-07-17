// VALUEOS: shared flow types passed between capture and finalize.
import type { ActivityType } from '../api/types';

export interface CaptureResult {
  tenantId: string;
  tenantName: string;
  activityType: ActivityType;
  targetId: string;
  targetLabel: string;
  callName: string; // user-chosen name for the call (the /calls `name`)
  transcriptText: string;
}
