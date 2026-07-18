// VALUEOS WS3: bug-report transport. The real ValueOS endpoint now exists
// (VALUEOS_AGENT_API — "Filing bugs from the ValueOS Agent"):
//   POST {BASE}/api/agent/v1/bug-reports   scope: valueos/write:bug-reports
// ValueOS creates the GitHub issue server-side (private repo, labelled bug + source:agent); the
// agent NEVER holds or sees the GitHub token. The native `valueos_api_report_bug` command attaches
// the Bearer access token and returns { issue_number, issue_url }. All contract assumptions stay
// isolated behind the BugReportService interface. logs/context/title are secret-redacted server-side.
import { callValueOs } from '../transport/invoke';
import { ValueOsApiError } from '../api/types';
import type { BugReportBundle, BugReportResult, BugReportService } from './types';

/** Map our scrubbed bundle to the ValueOS bug-report body (§ Request body). Diagnostic metadata
 *  that has no first-class field rides along in `context` (redacted server-side with logs). */
function bundleToApiBody(bundle: BugReportBundle): Record<string, unknown> {
  const m = bundle.metadata;
  const platform = ['windows', 'macos', 'linux'].includes(m.platform) ? m.platform : undefined;
  const firstLine = bundle.description.split('\n')[0]?.trim() ?? '';
  return {
    description: bundle.description,
    title: firstLine ? firstLine.slice(0, 80) : undefined, // else the server defaults from description
    version: m.app_version,
    platform,
    userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : undefined,
    logs: bundle.scrubbed_logs,
    context: {
      build: m.build,
      arch: m.arch,
      os_version: m.os_version,
      engine: m.engine,
      install_id: m.install_id,
      tenant_id: m.tenant_id,
      timezone: m.timezone,
      timestamp: m.timestamp,
      idempotency_key: bundle.idempotency_key,
    },
  };
}

/**
 * The REAL transport (default in packaged builds). POSTs the mapped body via the native command.
 * 401 (reauth) / 403 (missing scope or not an agent token) / 422 (empty description) are actionable
 * and surface to the user. Transport (status 0) and 5xx (502/503) are retryable → the scrubbed
 * bundle is saved locally so a real user's report is never lost.
 */
export class ApiBugReportService implements BugReportService {
  async submit(bundle: BugReportBundle): Promise<BugReportResult> {
    try {
      const result = await callValueOs<{ issue_number?: number; issue_url?: string }>(
        'valueos_api_report_bug',
        { report: bundleToApiBody(bundle) },
      );
      return {
        reportId: result?.issue_number != null ? `#${result.issue_number}` : bundle.idempotency_key,
        issueNumber: result?.issue_number,
        issueUrl: result?.issue_url,
      };
    } catch (e) {
      const status = e instanceof ValueOsApiError ? e.status : 0;
      if (status === 0 || status >= 500) {
        const localPath = await saveBugReportLocally(bundle);
        return { reportId: bundle.idempotency_key, savedLocally: true, localPath: localPath ?? undefined };
      }
      throw e; // 401 / 403 / 422 — actionable, let the dialog show it
    }
  }
}

/** Legacy fallback service: always saves the scrubbed bundle to a local file. Kept for reference
 *  and as the manual save-locally path; the default is now {@link ApiBugReportService}. */
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
