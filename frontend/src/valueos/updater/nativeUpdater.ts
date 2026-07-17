// VALUEOS WS4: the REAL bindings behind the updater — thin wrappers over native valueos_*
// commands (which own the token + reqwest + checksum verify + install), plus a localStorage
// key/value store. The webview never sees the token or writes the installer itself.
import { callValueOs } from '../transport/invoke';
import type { UpdaterNative, UpdaterStore } from './updater';

export const realUpdaterNative: UpdaterNative = {
  // OS + app version straight from the native side (std::env::consts::OS + package version).
  appInfo() {
    return callValueOs<{ platform: string; version: string }>('valueos_app_info');
  },
  installId() {
    return callValueOs<string>('valueos_install_id');
  },
  download(url: string, sha256?: string | null) {
    return callValueOs<string>('valueos_download_update', { downloadUrl: url, expectedSha256: sha256 ?? null });
  },
  apply(path: string) {
    return callValueOs<void>('valueos_apply_update', { path });
  },
};

/** localStorage-backed store for the updater's tiny bookkeeping (registered flag, last
 *  version). Non-sensitive; survives updates because localStorage is per-install data. */
export const localStorageUpdaterStore: UpdaterStore = {
  get(key) {
    try {
      return typeof localStorage !== 'undefined' ? localStorage.getItem(key) : null;
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      if (typeof localStorage !== 'undefined') localStorage.setItem(key, value);
    } catch {
      /* ignore */
    }
  },
};
