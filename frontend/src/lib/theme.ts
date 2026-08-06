'use client';

import { useCallback, useSyncExternalStore } from 'react';

export type Theme = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'conversationaly.theme';

/**
 * Runs in <head> before first paint so the app never flashes the wrong theme.
 * Kept as a string (not a module) because it has to execute synchronously
 * ahead of hydration. Mirrors `resolve()` below — change both together.
 */
export const THEME_INIT_SCRIPT = `(function(){try{
var t=localStorage.getItem('${STORAGE_KEY}');
var d=t==='dark'||((t===null||t==='system')&&matchMedia('(prefers-color-scheme: dark)').matches);
document.documentElement.classList.toggle('dark',d);
}catch(e){}})();`;

const query = () =>
  typeof window === 'undefined' ? null : window.matchMedia('(prefers-color-scheme: dark)');

function read(): Theme {
  if (typeof window === 'undefined') return 'system';
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return stored === 'light' || stored === 'dark' ? stored : 'system';
}

function resolve(theme: Theme): ResolvedTheme {
  if (theme !== 'system') return theme;
  return query()?.matches ? 'dark' : 'light';
}

const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

function apply(theme: Theme) {
  document.documentElement.classList.toggle('dark', resolve(theme) === 'dark');
}

export function setTheme(theme: Theme) {
  if (theme === 'system') window.localStorage.removeItem(STORAGE_KEY);
  else window.localStorage.setItem(STORAGE_KEY, theme);
  apply(theme);
  emit();
}

function subscribe(onChange: () => void) {
  listeners.add(onChange);

  // Follow the OS while on 'system', and follow other windows of the app.
  const media = query();
  const onMedia = () => {
    if (read() === 'system') {
      apply('system');
      emit();
    }
  };
  const onStorage = (e: StorageEvent) => {
    if (e.key === STORAGE_KEY) {
      apply(read());
      emit();
    }
  };

  media?.addEventListener('change', onMedia);
  window.addEventListener('storage', onStorage);

  return () => {
    listeners.delete(onChange);
    media?.removeEventListener('change', onMedia);
    window.removeEventListener('storage', onStorage);
  };
}

export function useTheme() {
  const theme = useSyncExternalStore(subscribe, read, () => 'system' as Theme);
  const resolved = useSyncExternalStore(
    subscribe,
    () => resolve(read()),
    () => 'light' as ResolvedTheme
  );

  return {
    theme,
    resolved,
    setTheme: useCallback(setTheme, []),
  };
}
