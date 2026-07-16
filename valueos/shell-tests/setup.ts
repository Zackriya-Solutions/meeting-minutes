import '@testing-library/jest-dom/vitest';

// Some jsdom setups don't expose localStorage as a global; provide an in-memory shim so
// our localStorage-backed code (config folder, pending-upload queue) is testable. The real
// app uses the Tauri webview's localStorage.
if (typeof globalThis.localStorage === 'undefined') {
  const store = new Map<string, string>();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).localStorage = {
    getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
    setItem: (k: string, v: string) => {
      store.set(k, String(v));
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => store.clear(),
    key: (i: number) => [...store.keys()][i] ?? null,
    get length() {
      return store.size;
    },
  };
}
