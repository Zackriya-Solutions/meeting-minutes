// VALUEOS test stub for @tauri-apps/api/* — the flow's screens import Tauri APIs (invoke,
// path helpers) that don't exist in this Node test project. Native calls are either
// unused or mocked in tests; this just satisfies module resolution.
export const invoke = async (): Promise<unknown> => undefined;
export const appDataDir = async (): Promise<string> => '/mock/appdata';
export const join = async (...parts: string[]): Promise<string> => parts.join('/');
