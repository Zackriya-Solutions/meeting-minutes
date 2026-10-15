import { describe, expect, test } from 'bun:test';
import { createDebouncedSaver } from '../../src/lib/debouncedSaver';

const DELAY = 20;
const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

function harness(save?: (key: string, value: string) => Promise<void>) {
  const calls: Array<[string, string]> = [];
  const errors: unknown[] = [];
  const saver = createDebouncedSaver<string>(
    async (key, value) => {
      calls.push([key, value]);
      await save?.(key, value);
    },
    error => errors.push(error),
    DELAY,
  );
  return { saver, calls, errors };
}

describe('debounced saver', () => {
  test('saves once, with the last value, after a quiet period', async () => {
    const { saver, calls } = harness();
    saver.schedule('a', 'h');
    saver.schedule('a', 'he');
    saver.schedule('a', 'hey');
    expect(calls).toEqual([]);
    await wait(DELAY * 4);
    expect(calls).toEqual([['a', 'hey']]);
  });

  test('flush saves at once and resolves only after the save has finished', async () => {
    let finished = false;
    const { saver, calls } = harness(async () => {
      await wait(DELAY);
      finished = true;
    });
    saver.schedule('a', 'now');
    await saver.flush();
    expect(calls).toEqual([['a', 'now']]);
    expect(finished).toBe(true);
    await wait(DELAY * 4);
    expect(calls).toEqual([['a', 'now']]);
  });

  test('a pending save keeps the key it was scheduled with', async () => {
    const { saver, calls } = harness();
    saver.schedule('meeting-a', 'typed in A');
    saver.schedule('meeting-b', 'typed in B');
    await saver.flush();
    expect(calls).toEqual([['meeting-a', 'typed in A'], ['meeting-b', 'typed in B']]);
  });

  test('saves run one at a time, in order', async () => {
    const order: string[] = [];
    const { saver } = harness(async (_key, value) => {
      order.push(`start ${value}`);
      await wait(value === 'first' ? DELAY * 2 : 0);
      order.push(`end ${value}`);
    });
    saver.schedule('a', 'first');
    void saver.flush();
    saver.schedule('a', 'second');
    await saver.flush();
    expect(order).toEqual(['start first', 'end first', 'start second', 'end second']);
  });

  test('a failed save is reported, flush still resolves, and later saves still run', async () => {
    let fail = true;
    const { saver, calls, errors } = harness(async () => {
      if (fail) throw new Error('disk full');
    });
    saver.schedule('a', 'lost');
    await saver.flush();
    expect(errors.map(String)).toEqual(['Error: disk full']);
    fail = false;
    saver.schedule('a', 'kept');
    await saver.flush();
    expect(calls).toEqual([['a', 'lost'], ['a', 'kept']]);
    expect(errors).toHaveLength(1);
  });

  test('an error handler that throws does not stop the saver', async () => {
    let fail = true;
    const calls: Array<[string, string]> = [];
    const saver = createDebouncedSaver<string>(
      async (key, value) => {
        calls.push([key, value]);
        if (fail) throw new Error('disk full');
      },
      () => { throw new Error('handler broke'); },
      DELAY,
    );
    saver.schedule('a', 'lost');
    await saver.flush();
    fail = false;
    saver.schedule('a', 'kept');
    await saver.flush();
    expect(calls).toEqual([['a', 'lost'], ['a', 'kept']]);
  });

  test('flush with nothing pending resolves', async () => {
    const { saver, calls } = harness();
    await saver.flush();
    expect(calls).toEqual([]);
  });
});
