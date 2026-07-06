'use client';

import { installTauriBrowserMocks } from './tauri-browser-mocks';

if (process.env.NEXT_PUBLIC_E2E_TESTING === '1') {
  installTauriBrowserMocks();
}

export function E2EBootstrap() {
  return null;
}
