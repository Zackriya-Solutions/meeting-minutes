// VALUEOS: shared flow types passed between capture and finalize.
import type { ActivityType } from '../api/types';

export interface CaptureResult {
  tenantId: string;
  activityType: ActivityType;
  targetId: string;
  targetLabel: string;
  transcriptText: string;
}
