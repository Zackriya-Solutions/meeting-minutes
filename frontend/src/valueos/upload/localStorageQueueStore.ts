// VALUEOS: REAL pending-upload store — persists the queue in the webview's localStorage so
// pending uploads survive an app restart (the transcript file itself is already on disk).
// Non-sensitive metadata only (no tokens).
import { PendingUpload, PendingUploadStore } from './pendingQueue';

const KEY = 'valueos.pendingUploads';

export class LocalStoragePendingUploadStore implements PendingUploadStore {
  private read(): PendingUpload[] {
    try {
      const raw = localStorage.getItem(KEY);
      return raw ? (JSON.parse(raw) as PendingUpload[]) : [];
    } catch {
      return [];
    }
  }
  private write(items: PendingUpload[]) {
    localStorage.setItem(KEY, JSON.stringify(items));
  }
  async list(): Promise<PendingUpload[]> {
    return this.read().sort((a, b) => a.createdAt - b.createdAt);
  }
  async put(item: PendingUpload): Promise<void> {
    const items = this.read().filter((i) => i.id !== item.id);
    items.push(item);
    this.write(items);
  }
  async remove(id: string): Promise<void> {
    this.write(this.read().filter((i) => i.id !== id));
  }
}
