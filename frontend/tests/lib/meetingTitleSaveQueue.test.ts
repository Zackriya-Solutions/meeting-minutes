import { describe, expect, test } from 'bun:test';
import {
  isLatestMeetingTitle,
  MeetingTitleSaveQueue,
  normalizeMeetingTitle,
} from '../../src/lib/meetingTitleSaveQueue';

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe('meeting title normalization', () => {
  test('trims a saved title and leaves whitespace-only input empty', () => {
    expect(normalizeMeetingTitle('  Daily sync  ')).toBe('Daily sync');
    expect(normalizeMeetingTitle('   ')).toBe('');
  });

  test('only the latest edited value may be published to the shared archive', () => {
    expect(isLatestMeetingTitle('Next title', 'Intermediate title')).toBe(false);
    expect(isLatestMeetingTitle('  Next title  ', 'Next title')).toBe(true);
  });
});

describe('MeetingTitleSaveQueue', () => {
  test('deduplicates Enter and blur saves for the same title', async () => {
    const queue = new MeetingTitleSaveQueue();
    const pending = deferred();
    let saves = 0;
    const save = () => {
      saves += 1;
      return pending.promise;
    };

    const enterSave = queue.enqueue('Renamed meeting', save);
    const blurSave = queue.enqueue('Renamed meeting', save);

    expect(blurSave).toBe(enterSave);
    pending.resolve();
    await Promise.all([enterSave, blurSave]);
    expect(saves).toBe(1);
  });

  test('serializes rapid distinct renames in edit order', async () => {
    const queue = new MeetingTitleSaveQueue();
    const first = deferred();
    const order: string[] = [];

    const firstSave = queue.enqueue('Intermediate', async () => {
      order.push('first:start');
      await first.promise;
      order.push('first:end');
    });
    const finalSave = queue.enqueue('Final', async () => {
      order.push('final');
    });

    await Promise.resolve();
    expect(order).toEqual(['first:start']);
    first.resolve();
    await Promise.all([firstSave, finalSave]);
    expect(order).toEqual(['first:start', 'first:end', 'final']);
  });

  test('a failed save does not block a newer title or a retry', async () => {
    const queue = new MeetingTitleSaveQueue();
    const failure = new Error('database unavailable');
    let attempts = 0;

    const failed = queue.enqueue('First', async () => {
      attempts += 1;
      throw failure;
    });
    const newer = queue.enqueue('Second', async () => {
      attempts += 1;
    });

    expect(await failed.catch((error) => error)).toBe(failure);
    await newer;
    await queue.enqueue('First', async () => {
      attempts += 1;
    });
    expect(attempts).toBe(3);
  });
});
