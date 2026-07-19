import { describe, it, expect } from 'vitest';
import { InMemoryTranscriptHistory } from '@/valueos/history/transcriptHistory';
import type { TranscriptRecord } from '@/valueos/history/transcriptHistory';

// VALUEOS: local transcript history — supports removing an entry (local-only; the ValueOS
// cloud copy is never touched by delete).
function rec(id: string): TranscriptRecord {
  return {
    id,
    targetLabel: 'X',
    tenantId: 't',
    activityType: 'lead',
    targetId: 'l',
    createdAt: 1,
    path: `/p/${id}.txt`,
    uploadStatus: 'uploaded',
  };
}

describe('TranscriptHistory (local)', () => {
  it('adds, lists, and removes records', async () => {
    const h = new InMemoryTranscriptHistory();
    await h.add(rec('a'));
    await h.add(rec('b'));
    expect((await h.list()).map((r) => r.id).sort()).toEqual(['a', 'b']);

    await h.remove('a');
    expect((await h.list()).map((r) => r.id)).toEqual(['b']);

    // removing a non-existent id is a harmless no-op
    await h.remove('nope');
    expect((await h.list()).map((r) => r.id)).toEqual(['b']);
  });
});
