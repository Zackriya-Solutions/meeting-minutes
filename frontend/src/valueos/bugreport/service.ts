// VALUEOS WS3: bug-report transport. ⚠️ VALUEOS-CONTRACT-TBD — the ValueOS bug-report endpoint
// does not exist yet. All assumptions about the real contract are isolated HERE, behind the
// BugReportService interface, so swapping it in is a one-file change. Until then the mock is
// authoritative for tests, and the app's default service SAVES the scrubbed bundle locally so
// nothing is lost.
import { callValueOs } from '../transport/invoke';
import type { BugReportBundle, BugReportResult, BugReportService } from './types';

/**
 * Assumed real contract (isolated — replace when ValueOS provides it):
 *   POST /api/agent/v1/tenants/{tenantId}/bug-reports
 *   Auth: Bearer <access_token>; JSON body { description, metadata, logs, idempotency_key }
 *   -> { reportId }
 * Interim behavior: write the scrubbed bundle to a local file (valueos_save_bug_report) and
 * return a local id, so a real user's report is captured until the endpoint exists.
 */
export class LocalFileBugReportService implements BugReportService {
  async submit(bundle: BugReportBundle): Promise<BugReportResult> {
    const path = await callValueOs<string>('valueos_save_bug_report', {
      content: JSON.stringify(bundle, null, 2),
    });
    return { reportId: bundle.idempotency_key, savedLocally: true, localPath: path };
  }
}

/** Save a bundle locally (the failure fallback + the interim default). Best-effort. */
export async function saveBugReportLocally(bundle: BugReportBundle): Promise<string | null> {
  try {
    return await callValueOs<string>('valueos_save_bug_report', {
      content: JSON.stringify(bundle, null, 2),
    });
  } catch {
    return null;
  }
}

/** ⚠️ MOCK — in-memory; authoritative for the flow tests until the real contract lands. */
export class MockBugReportService implements BugReportService {
  submissions: BugReportBundle[] = [];
  failNext = false;
  async submit(bundle: BugReportBundle): Promise<BugReportResult> {
    if (this.failNext) {
      this.failNext = false;
      throw new Error('Bug report submission failed');
    }
    this.submissions.push(bundle);
    return { reportId: `mock-${bundle.idempotency_key.slice(0, 8)}` };
  }
}
