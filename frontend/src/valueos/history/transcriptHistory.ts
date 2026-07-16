// VALUEOS: local record of captured transcripts, for the home screen's list. ValueOS is
// write-only (we never read transcripts back from the server), so this list is purely the
// LOCAL history of meetings captured on this device. Mock = in-memory; real = localStorage.
import type { ActivityType } from '../api/types';

export interface TranscriptRecord {
  id: string; // == idempotency key
  targetLabel: string;
  tenantId: string;
  activityType: ActivityType;
  targetId: string;
  createdAt: number;
  path: string; // local transcript file path
  uploadStatus: 'uploaded' | 'pending' | 'failed';
}

export interface TranscriptHistory {
  list(): Promise<TranscriptRecord[]>;
  add(record: TranscriptRecord): Promise<void>;
}

/** ⚠️ MOCK — in-memory (per session). */
export class InMemoryTranscriptHistory implements TranscriptHistory {
  private records: TranscriptRecord[] = [];
  async list() {
    return [...this.records].sort((a, b) => b.createdAt - a.createdAt);
  }
  async add(record: TranscriptRecord) {
    this.records = [record, ...this.records.filter((r) => r.id !== record.id)];
  }
}

/** REAL — persisted in the webview's localStorage (non-sensitive metadata only). */
const KEY = 'valueos.transcriptHistory';
export class LocalStorageTranscriptHistory implements TranscriptHistory {
  private read(): TranscriptRecord[] {
    try {
      const raw = localStorage.getItem(KEY);
      return raw ? (JSON.parse(raw) as TranscriptRecord[]) : [];
    } catch {
      return [];
    }
  }
  async list() {
    return this.read().sort((a, b) => b.createdAt - a.createdAt);
  }
  async add(record: TranscriptRecord) {
    const next = [record, ...this.read().filter((r) => r.id !== record.id)];
    localStorage.setItem(KEY, JSON.stringify(next));
  }
}
