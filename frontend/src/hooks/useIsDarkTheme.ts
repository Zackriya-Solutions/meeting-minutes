'use client';

import { useSyncExternalStore } from 'react';

function subscribe(onStoreChange: () => void): () => void {
  const observer = new MutationObserver(onStoreChange);
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['class'],
  });
  return () => observer.disconnect();
}

function getSnapshot(): boolean {
  return document.documentElement.classList.contains('dark');
}

function getServerSnapshot(): boolean {
  return false;
}

/**
 * Effective theme derived from the root `dark` class — the single source of
 * truth written by ThemeScript (pre-hydration) and ConfigContext (runtime).
 * Scoped here instead of ConfigContext so OS-level theme flips only re-render
 * the components that actually branch on the effective theme.
 */
export function useIsDarkTheme(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}
