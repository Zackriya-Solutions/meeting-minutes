import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.ts',
  timeout: 30_000,
  fullyParallel: true,
  retries: process.env.CI ? 1 : 0,
  expect: {
    timeout: 5_000,
  },
  use: {
    baseURL: 'http://127.0.0.1:3118',
    viewport: { width: 1440, height: 1000 },
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1440, height: 1000 },
      },
    },
  ],
  webServer: {
    command: 'pnpm dev',
    env: { NEXT_PUBLIC_E2E_TESTING: '1' },
    url: 'http://127.0.0.1:3118',
    // Never reuse a running server: the Tauri mocks are compiled in at build
    // time, so a normal `pnpm dev` server on this port would run the whole
    // suite without mocks and fail confusingly.
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
