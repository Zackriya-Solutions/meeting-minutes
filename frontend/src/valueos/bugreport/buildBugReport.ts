// VALUEOS WS3: assemble the bug-report bundle — metadata + recent logs + description — and run
// the fail-closed scrub BEFORE it can be sent or saved. Metadata sources are injected so the
// whole thing is testable offline.
import { getRecentLogs } from './logBuffer';
import { scrubBundle } from './scrub';
import type { BugReportBundle, BugReportMetadata } from './types';

export interface MetadataSource {
  appInfo(): Promise<{ platform: string; version: string }>;
  installId(): Promise<string>;
  build: string; // BUILD_INFO.label
  engine?: string; // active transcription engine/model, best-effort
  osVersion?: string;
  arch?: string;
}

export function newIdempotencyKey(): string {
  return globalThis.crypto?.randomUUID?.() ?? `bug-${Math.random().toString(36).slice(2)}`;
}

function detectArch(): string | undefined {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  if (/arm64|aarch64/i.test(ua)) return 'aarch64';
  if (/x86_64|x64|win64|intel/i.test(ua)) return 'x86_64';
  return undefined;
}

export async function buildBugReport(opts: {
  description: string;
  tenantId?: string;
  meta: MetadataSource;
  logs?: string;
  idempotencyKey?: string;
  now?: () => number;
}): Promise<BugReportBundle> {
  const now = opts.now ?? (() => Date.now());
  const [{ platform, version }, install_id] = await Promise.all([
    opts.meta.appInfo(),
    opts.meta.installId(),
  ]);
  const metadata: BugReportMetadata = {
    app_version: version,
    build: opts.meta.build,
    platform,
    os_version: opts.meta.osVersion,
    arch: opts.meta.arch ?? detectArch(),
    install_id,
    tenant_id: opts.tenantId,
    timestamp: new Date(now()).toISOString(),
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    engine: opts.meta.engine,
  };
  const raw: BugReportBundle = {
    description: opts.description,
    metadata,
    scrubbed_logs: opts.logs ?? getRecentLogs(),
    idempotency_key: opts.idempotencyKey ?? newIdempotencyKey(),
  };
  // Fail-closed scrub BEFORE the bundle can be submitted or saved.
  return scrubBundle(raw);
}
