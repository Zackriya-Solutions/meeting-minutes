'use client';

import { installTauriBrowserMocks } from './tauri-browser-mocks';

// Build-time exclusion happens in next.config.js (this module is replaced by
// E2ENoop.tsx outside E2E builds); installTauriBrowserMocks additionally
// guards on NEXT_PUBLIC_E2E_TESTING at runtime so the mocks can never
// activate against a real Tauri shell.
installTauriBrowserMocks();

export function E2EBootstrap() {
  return null;
}
