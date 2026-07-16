// VALUEOS: pending-upload retry queue — resilience so we NEVER lose data. The transcript
// file is already stored locally before an item is enqueued; the queue owns getting the
// {transcript + digest} up to ValueOS. On network/503 failure the item is RETAINED (with
// attempt count) for later retry; on 401 the flush stops and signals re-auth; items are
// removed ONLY on a confirmed successful (or idempotent-replay) upload.
import type { ValueOsClient } from '../api/client';
import { ActivityType, UploadRequest, ValueOsApiError } from '../api/types';

export interface PendingUpload {
  id: string; // == request.idempotency_key
  tenantId: string;
  activityType: ActivityType;
  targetId: string;
  request: UploadRequest;
  transcriptPath: string; // local file already written at the configured folder
  createdAt: number;
  attempts: number;
  lastError?: string;
}

export interface PendingUploadStore {
  list(): Promise<PendingUpload[]>;
  put(item: PendingUpload): Promise<void>;
  remove(id: string): Promise<void>;
}

/** ⚠️ MOCK store (in-memory). Phase 3 persists via tauri-plugin-store/fs so pending
 *  uploads survive an app restart. */
export class InMemoryPendingUploadStore implements PendingUploadStore {
  private items = new Map<string, PendingUpload>();
  async list(): Promise<PendingUpload[]> {
    return [...this.items.values()].sort((a, b) => a.createdAt - b.createdAt);
  }
  async put(item: PendingUpload): Promise<void> {
    this.items.set(item.id, item);
  }
  async remove(id: string): Promise<void> {
    this.items.delete(id);
  }
}

export interface FailedUpload {
  id: string;
  tenantId: string;
  status: number;
  message: string;
}

export interface FlushOutcome {
  uploaded: string[]; // ids successfully uploaded (removed from the queue)
  retained: string[]; // ids kept for a later retry (retryable failures only)
  /** ids that failed TERMINALLY (404/413/422/403-not-member/403-feat_agent) — removed from
   *  the retry queue so they never become poison pills. The local transcript file + history
   *  entry still preserve the data. */
  failed: FailedUpload[];
  /** tenantIds hit with 403 feat_agent — the workspace lost the add-on; the caller MUST
   *  re-run the gate (GET /me/agent-tenants) and drop these tenants (contract §2.7). */
  deEntitled: string[];
  needsReauth: boolean; // a 401 was hit — caller must re-auth before retrying
}

export class PendingUploadQueue {
  constructor(
    private client: ValueOsClient,
    private store: PendingUploadStore,
    private now: () => number = () => Date.now(),
  ) {}

  async count(): Promise<number> {
    return (await this.store.list()).length;
  }

  /** Store a transcript+digest as a pending upload. Idempotency key doubles as the id. */
  async enqueue(
    item: Omit<PendingUpload, 'createdAt' | 'attempts'>,
  ): Promise<PendingUpload> {
    const full: PendingUpload = { ...item, createdAt: this.now(), attempts: 0 };
    await this.store.put(full);
    return full;
  }

  /** Try to upload every pending item. Safe to call repeatedly. Errors are classified so
   *  only genuinely retryable failures are retained — terminal failures are quarantined
   *  (removed from the retry loop) instead of retried forever. */
  async flush(): Promise<FlushOutcome> {
    const outcome: FlushOutcome = {
      uploaded: [],
      retained: [],
      failed: [],
      deEntitled: [],
      needsReauth: false,
    };
    for (const item of await this.store.list()) {
      try {
        await this.client.uploadTranscript(
          item.tenantId,
          item.activityType,
          item.targetId,
          item.request,
        );
        await this.store.remove(item.id); // success (incl. idempotent replay) → done
        outcome.uploaded.push(item.id);
      } catch (e) {
        const err = e instanceof ValueOsApiError ? e : null;
        const message = (e as Error)?.message ?? String(e);

        if (err?.isAuth) {
          // 401 → stop the flush; re-auth is required before any further attempt. Keep it.
          outcome.needsReauth = true;
          outcome.retained.push(item.id);
          break;
        }

        if (err?.isNotEntitled) {
          // 403 feat_agent → the workspace lost the add-on. It can NEVER succeed until
          // re-entitled, so quarantine it (don't retry forever) and tell the caller to
          // re-gate. Data is safe: the local file + history entry remain.
          await this.store.remove(item.id);
          outcome.failed.push({ id: item.id, tenantId: item.tenantId, status: 403, message });
          if (!outcome.deEntitled.includes(item.tenantId)) outcome.deEntitled.push(item.tenantId);
          continue;
        }

        // Retain ONLY genuinely retryable failures: 503, other 5xx, or a transport error
        // (status 0 / a non-API error). Everything else (404/413/422/403-not-member) is
        // terminal — quarantine it rather than retry a poison pill.
        const retryable = err === null || err.status === 0 || err.status >= 500;
        if (retryable) {
          await this.store.put({ ...item, attempts: item.attempts + 1, lastError: message });
          outcome.retained.push(item.id);
        } else {
          await this.store.remove(item.id);
          outcome.failed.push({ id: item.id, tenantId: item.tenantId, status: err.status, message });
        }
      }
    }
    return outcome;
  }
}
