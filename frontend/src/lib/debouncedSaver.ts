export interface DebouncedSaver<V> {
  /** Save `value` for `key` once no newer value for that key arrives within the delay. */
  schedule(key: string, value: V): void;
  /** Start every pending save now. Resolves when all saves have finished; never rejects. */
  flush(): Promise<void>;
}

/**
 * Debounced saving keyed by id. A pending save keeps the key it was scheduled
 * with, saves run one at a time in order, and failures go to `onError`.
 */
export function createDebouncedSaver<V>(
  save: (key: string, value: V) => Promise<void>,
  onError: (error: unknown) => void,
  delayMs: number,
): DebouncedSaver<V> {
  const pending = new Map<string, { value: V; timer: ReturnType<typeof setTimeout> }>();
  let queue: Promise<void> = Promise.resolve();

  const run = (key: string, value: V) => {
    queue = queue.then(() => save(key, value)).catch(error => {
      try {
        onError(error);
      } catch {
        // The handler is the last place to report a failure; nothing is left to tell.
      }
    });
  };

  return {
    schedule(key, value) {
      const existing = pending.get(key);
      if (existing) clearTimeout(existing.timer);
      const timer = setTimeout(() => {
        pending.delete(key);
        run(key, value);
      }, delayMs);
      pending.set(key, { value, timer });
    },
    flush() {
      for (const [key, { value, timer }] of [...pending]) {
        clearTimeout(timer);
        pending.delete(key);
        run(key, value);
      }
      return queue;
    },
  };
}
