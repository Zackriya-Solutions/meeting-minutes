type TauriInternals = {
  invoke: (command: string, args?: unknown, options?: unknown) => Promise<unknown>;
  transformCallback: (callback: (payload: unknown) => void, once?: boolean) => number;
  unregisterCallback: (id: number) => void;
  convertFileSrc: (path: string) => string;
};

let callbackId = 0;

export function installShowcaseTauriBoundary() {
  if (typeof window === 'undefined') return;
  const target = window as Window & { __TAURI_INTERNALS__?: TauriInternals };
  if (target.__TAURI_INTERNALS__) return;

  target.__TAURI_INTERNALS__ = {
    async invoke(command) {
      if (command === 'plugin:event|listen') return ++callbackId;
      if (command === 'plugin:app|version') return '0.5.0';
      if (command === 'plugin:updater|check') return null;
      return null;
    },
    transformCallback() {
      callbackId += 1;
      return callbackId;
    },
    unregisterCallback() {},
    convertFileSrc(path) {
      return path;
    },
  };
}
