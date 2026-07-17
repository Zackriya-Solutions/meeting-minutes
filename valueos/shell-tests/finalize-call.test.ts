import { describe, it, expect, vi } from 'vitest';
import { finalizeCall } from '@/valueos/upload/finalizeCall';
import { MockValueOsClient, defaultMockSeed } from '@/valueos/api/mockClient';
import { MockDigestGenerator } from '@/valueos/digest/digest';
import { PendingUploadQueue, InMemoryPendingUploadStore } from '@/valueos/upload/pendingQueue';
import { InMemoryTranscriptHistory } from '@/valueos/history/transcriptHistory';
import { createMockConfigService } from '@/valueos/config/configService';
import type { CaptureResult } from '@/valueos/shell/flowTypes';

// VALUEOS WS2: finalizeCall must never lose the transcript when the configured folder is
// missing/unwritable — the upload (which carries the full text) and the local history record
// still happen; only the on-disk .txt is skipped, and fileSaved reports that.

const capture: CaptureResult = {
  tenantId: 'tenant-acme',
  tenantName: 'Acme GmbH',
  activityType: 'lead',
  targetId: 'lead-1',
  targetLabel: 'Ada Lovelace',
  callName: 'Discovery Call — Ada Lovelace',
  transcriptText: 'We discussed pricing and agreed next steps.',
};

function makeServices(configWritable = true) {
  const client = new MockValueOsClient(defaultMockSeed());
  const config = createMockConfigService({ initialFolder: '/tmp/tx', writable: configWritable });
  const history = new InMemoryTranscriptHistory();
  const uploadQueue = new PendingUploadQueue(client, new InMemoryPendingUploadStore());
  return { client, config, history, uploadQueue, digest: new MockDigestGenerator() };
}

describe('finalizeCall', () => {
  it('writes the .txt to the configured folder and uploads BOTH artifacts', async () => {
    const s = makeServices();
    const callSpy = vi.spyOn(s.client, 'createCall');
    const out = await finalizeCall(
      { digest: s.digest, config: s.config, uploadQueue: s.uploadQueue, history: s.history },
      capture,
      'key-1',
    );
    expect(out.fileSaved).toBe(true);
    expect(out.status).toBe('done');
    expect(out.record.path).toContain('/tmp/tx/');
    expect(callSpy).toHaveBeenCalledTimes(1);
    expect(callSpy.mock.calls[0][1].transcript.raw_content).toBe(capture.transcriptText);
    expect(callSpy.mock.calls[0][1].transcript.digest.length).toBeGreaterThan(0);
    expect((await s.history.list())[0].id).toBe('key-1');
  });

  it('never loses the transcript when the folder is unwritable at save time', async () => {
    const s = makeServices();
    const callSpy = vi.spyOn(s.client, 'createCall');
    // Folder deleted / permissions changed since it was selected.
    s.config.writeTranscriptFile = vi.fn(async () => {
      throw new Error('EACCES: permission denied');
    });

    const out = await finalizeCall(
      { digest: s.digest, config: s.config, uploadQueue: s.uploadQueue, history: s.history },
      capture,
      'key-2',
    );

    // reported, not thrown
    expect(out.fileSaved).toBe(false);
    expect(out.fileError).toMatch(/permission denied/i);
    // upload STILL happened → cloud copy is safe
    expect(callSpy).toHaveBeenCalledTimes(1);
    expect(callSpy.mock.calls[0][1].transcript.raw_content).toBe(capture.transcriptText);
    expect(out.status).toBe('done');
    // history STILL recorded with the full text retained, path empty (no local file)
    const recs = await s.history.list();
    expect(recs[0].id).toBe('key-2');
    expect(recs[0].transcript).toBe(capture.transcriptText);
    expect(recs[0].path).toBe('');
  });

  it('records the server failure reason (status + message) on a terminal reject', async () => {
    const s = makeServices();
    // Link to a record that does not exist → the API rejects with 404 (terminal).
    const out = await finalizeCall(
      { digest: s.digest, config: s.config, uploadQueue: s.uploadQueue, history: s.history },
      { ...capture, targetId: 'does-not-exist' },
      'key-3',
    );
    expect(out.status).toBe('error');
    expect(out.record.uploadStatus).toBe('failed');
    expect(out.record.error).toMatch(/404/);
    expect(out.record.error).toMatch(/does not exist/i);
  });
});
