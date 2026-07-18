// VALUEOS WS3: types for in-app bug reporting. The transport contract to ValueOS is NOT
// finalized — all such assumptions are isolated in service.ts and kept behind this interface,
// so swapping the real contract in is a one-file change.

export interface BugReportMetadata {
  app_version: string;
  build: string;
  platform: string; // macos | windows | linux
  os_version?: string;
  arch?: string; // aarch64 | x86_64 | …
  install_id: string;
  tenant_id?: string; // only when a workspace is selected
  timestamp: string; // ISO-8601
  timezone: string;
  engine?: string; // active transcription engine/model, best-effort
}

export interface BugReportBundle {
  description: string;
  metadata: BugReportMetadata;
  /** Recent local logs — ALREADY SCRUBBED (no tokens/PII/transcript content). */
  scrubbed_logs: string;
  /** Generated once per submission; reused on retry (idempotency convention). */
  idempotency_key: string;
}

export interface BugReportResult {
  reportId: string;
  /** GitHub issue number ValueOS created server-side (real API path). */
  issueNumber?: number;
  /** GitHub issue URL (private repo — for support reference, not necessarily user-openable). */
  issueUrl?: string;
  /** True when the bundle was only saved locally (transient send failure → not lost). */
  savedLocally?: boolean;
  localPath?: string;
}

export interface BugReportService {
  submit(bundle: BugReportBundle): Promise<BugReportResult>;
}
