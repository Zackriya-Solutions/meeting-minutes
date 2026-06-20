# Full Application Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the frontend’s ad-hoc light-only styling with a complete semantic design system that supports light, dark, and system appearance modes across every user-visible route and state.

**Architecture:** Build and verify one shared foundation first: deterministic Playwright fixtures, pre-hydration theme resolution, semantic/status tokens, and themed UI primitives. Freeze those shared files, then migrate five disjoint feature groups in isolated worktrees with separate Playwright specs and integrate their commits into `codex-port-v04`. Finish with a full browser matrix, visual baselines, static token audit, TypeScript/build checks, and Rust regression tests.

**Tech Stack:** Next.js 14, React 18, TypeScript, Tailwind CSS 3, Radix UI, BlockNote, Tauri v2 frontend mocks, Playwright.

---

## File and Ownership Map

Shared foundation files are owned only by Tasks 1–3:

- `frontend/src/lib/theme.ts` — pure theme parsing and effective-theme resolution.
- `frontend/src/components/ThemeScript.tsx` — pre-hydration root class initialization.
- `frontend/src/testing/tauri-browser-mocks.ts` — deterministic browser-only Tauri IPC fixtures.
- `frontend/src/testing/E2EBootstrap.tsx` — installs browser fixtures before application effects.
- `frontend/src/app/layout.tsx` — mounts the theme script and E2E bootstrap.
- `frontend/src/app/globals.css` — semantic light/dark/status variables and global external-editor adjustments.
- `frontend/tailwind.config.js` — maps semantic variables to Tailwind utilities.
- `frontend/src/components/ui/*` — shared primitive styling.
- `frontend/scripts/check-theme-tokens.mjs` — static feature-component audit.
- `frontend/playwright.config.ts` and `frontend/tests/fixtures/*` — shared Playwright infrastructure.

Parallel migration groups own disjoint files:

- **Shell:** `frontend/src/app/page.tsx`, `frontend/src/components/Sidebar/**`, `frontend/src/components/MainContent.tsx`, `frontend/src/components/Logo.tsx`.
- **Settings:** `frontend/src/app/settings/page.tsx`, settings components, and model manager components.
- **Meetings:** meeting-detail pages/components, transcript views, notes, and BlockNote wrappers.
- **Onboarding:** onboarding, import, download-progress, device-selection, and recording setup surfaces.
- **Secondary:** dialogs, overlays, About/Info, analytics/update/compliance, and remaining secondary components.

Parallel agents must not edit shared foundation files. If a missing token or primitive is discovered, the agent reports the required change and uses no local workaround; the integration task adds the shared change centrally.

## Task 1: Establish the Playwright and Static-Audit Harness

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/pnpm-lock.yaml`
- Create: `frontend/playwright.config.ts`
- Create: `frontend/tests/fixtures/tauri-fixtures.ts`
- Create: `frontend/src/testing/tauri-browser-mocks.ts`
- Create: `frontend/src/testing/E2EBootstrap.tsx`
- Create: `frontend/tests/theme-foundation.spec.ts`
- Create: `frontend/scripts/check-theme-tokens.mjs`
- Modify: `frontend/.gitignore`

- [ ] **Step 1: Add Playwright as a direct development dependency and scripts**

Run:

```bash
cd frontend
pnpm add -D @playwright/test@^1.60.0
```

Update `package.json` scripts to include:

```json
{
  "test:e2e": "playwright test",
  "test:e2e:update": "playwright test --update-snapshots",
  "test:theme-audit": "node scripts/check-theme-tokens.mjs"
}
```

Add `/test-results` and `/playwright-report` to `frontend/.gitignore`.

- [ ] **Step 2: Create deterministic Tauri browser fixtures**

Create `frontend/src/testing/tauri-browser-mocks.ts`:

```ts
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';

const meeting = {
  id: 'theme-test-meeting',
  title: 'Theme Test Meeting',
  created_at: '2026-06-20T09:00:00Z',
  folder_path: '/tmp/theme-test-meeting',
  transcripts: [],
};

const commandFixtures: Record<string, unknown> = {
  get_onboarding_status: { completed: true },
  get_recording_state: { is_recording: false, is_paused: false },
  is_recording: false,
  api_get_meetings: [meeting],
  api_get_meeting: meeting,
  api_get_meeting_transcripts: [
    { id: 1, text: 'We approved the dark theme.', timestamp: 0, confidence: 0.98 },
  ],
  api_get_summary: {
    data: '# Theme Test Meeting\n## Decisions\nUse semantic tokens.',
  },
  api_get_model_config: {
    provider: 'builtin-ai',
    model: 'qwen3.5:2b',
    whisperModel: 'parakeet-tdt-0.6b-v3-int8',
    ollamaEndpoint: null,
  },
  api_get_transcript_config: {
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
    apiKey: null,
  },
  api_list_templates: [],
  get_notification_settings: null,
  get_default_recordings_folder_path: '/tmp/Meetily Recordings',
  is_analytics_enabled: false,
  builtin_ai_list_models: [],
  parakeet_get_available_models: [],
  whisper_get_available_models: [],
};

let installed = false;

function scenarioFixture(command: string) {
  const scenario = new URLSearchParams(window.location.search).get('__e2e');
  if (scenario === 'onboarding' && command === 'get_onboarding_status') {
    return { completed: false };
  }
  if (scenario === 'empty-home' && command === 'api_get_meetings') {
    return [];
  }
  return undefined;
}

export function installTauriBrowserMocks() {
  if (installed || process.env.NEXT_PUBLIC_E2E_TESTING !== '1') return;
  installed = true;
  mockWindows('main');
  mockIPC((command) => {
    const scenario = scenarioFixture(command);
    if (scenario !== undefined) return scenario;
    if (command in commandFixtures) return commandFixtures[command];
    if (
      command.startsWith('track_') ||
      command.startsWith('plugin:') ||
      command.startsWith('set_') ||
      command.startsWith('start_') ||
      command.startsWith('stop_')
    ) {
      return null;
    }
    throw new Error(`[E2E fixture missing] ${command}`);
  }, { shouldMockEvents: true });
}
```

Create `frontend/src/testing/E2EBootstrap.tsx`:

```tsx
'use client';

import { installTauriBrowserMocks } from './tauri-browser-mocks';

installTauriBrowserMocks();

export function E2EBootstrap() {
  return null;
}
```

Mount `<E2EBootstrap />` as the first child of `<body>` only when `NEXT_PUBLIC_E2E_TESTING === '1'`.

- [ ] **Step 3: Create Playwright configuration and common fixture**

Create `frontend/playwright.config.ts`:

```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  expect: { timeout: 5_000 },
  use: {
    baseURL: 'http://127.0.0.1:3118',
    viewport: { width: 1440, height: 1000 },
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },
  projects: [{
    name: 'chromium',
    use: { ...devices['Desktop Chrome'] },
  }],
  webServer: {
    command: 'NEXT_PUBLIC_E2E_TESTING=1 pnpm dev',
    url: 'http://127.0.0.1:3118',
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
```

Create `frontend/tests/fixtures/tauri-fixtures.ts`:

```ts
import { expect, test as base, type Page } from '@playwright/test';

export const test = base;
export { expect };

export async function setStoredTheme(
  page: Page,
  mode: 'light' | 'dark' | 'system',
) {
  await page.addInitScript((value) => {
    localStorage.setItem('themeMode', value);
  }, mode);
}

export async function expectNoHorizontalOverflow(page: Page) {
  const dimensions = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
  expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth);
}
```

- [ ] **Step 4: Write the first failing theme acceptance tests**

Create `frontend/tests/theme-foundation.spec.ts`:

```ts
import { expect, test } from './fixtures/tauri-fixtures';

test('applies stored dark mode before DOMContentLoaded', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('themeMode', 'dark');
    window.addEventListener('DOMContentLoaded', () => {
      (window as typeof window & { __darkAtDOMContentLoaded?: boolean })
        .__darkAtDOMContentLoaded = document.documentElement.classList.contains('dark');
    }, { once: true });
  });

  await page.goto('/settings');

  await expect.poll(() => page.evaluate(
    () => (window as typeof window & { __darkAtDOMContentLoaded?: boolean })
      .__darkAtDOMContentLoaded,
  )).toBe(true);
  await expect(page.locator('html')).toHaveClass(/dark/);
});

test('system mode follows the browser color scheme', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => localStorage.setItem('themeMode', 'system'));
  await page.goto('/settings');
  await expect(page.locator('html')).toHaveClass(/dark/);

  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).not.toHaveClass(/dark/);
});
```

- [ ] **Step 5: Run the tests and verify RED**

Run:

```bash
cd frontend
pnpm test:e2e tests/theme-foundation.spec.ts
```

Expected: the first test fails because dark mode is currently applied in a React effect after initial document parsing. If the second test passes, retain it as regression coverage and continue; RED is established by the first test.

- [ ] **Step 6: Create the static theme audit and verify RED**

Create `frontend/scripts/check-theme-tokens.mjs`:

```js
import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';

const roots = ['src/app', 'src/components'];
const forbidden = /\b(?:bg-white|bg-gray-\d+|text-gray-\d+|border-gray-\d+)\b/g;
const allowlist = new Set([]);
const failures = [];

async function walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) await walk(file);
    if (!entry.isFile() || !/\.(ts|tsx)$/.test(entry.name)) continue;
    const source = await readFile(file, 'utf8');
    for (const match of source.matchAll(forbidden)) {
      const key = `${file}:${match[0]}`;
      if (!allowlist.has(key)) failures.push(key);
    }
  }
}

for (const root of roots) await walk(root);

if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
```

Run:

```bash
pnpm test:theme-audit
```

Expected: FAIL and list the existing hard-coded neutral utilities.

- [ ] **Step 7: Commit the test harness**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml frontend/.gitignore \
  frontend/playwright.config.ts frontend/tests frontend/src/testing \
  frontend/src/app/layout.tsx frontend/scripts/check-theme-tokens.mjs
git commit -m "test: add browser theme acceptance harness"
```

## Task 2: Implement First-Paint Theme Resolution

**Files:**
- Create: `frontend/src/lib/theme.ts`
- Create: `frontend/src/components/ThemeScript.tsx`
- Modify: `frontend/src/app/layout.tsx`
- Modify: `frontend/src/contexts/ConfigContext.tsx`
- Test: `frontend/tests/theme-foundation.spec.ts`

- [ ] **Step 1: Extend the failing test with persistence and invalid-value behavior**

Add:

```ts
test('persists an explicit mode and restores it after reload', async ({ page }) => {
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Dark' }).click();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.reload();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await expect(page.evaluate(() => localStorage.getItem('themeMode'))).resolves.toBe('dark');
});

test('invalid stored mode falls back to system', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => localStorage.setItem('themeMode', 'sepia'));
  await page.goto('/settings');
  await expect(page.locator('html')).toHaveClass(/dark/);
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
pnpm test:e2e tests/theme-foundation.spec.ts
```

Expected: first-paint and invalid-value tests fail.

- [ ] **Step 3: Implement the pure theme resolver**

Create `frontend/src/lib/theme.ts`:

```ts
export type ThemeMode = 'light' | 'dark' | 'system';
export type EffectiveTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'themeMode';

export function parseThemeMode(value: string | null): ThemeMode {
  return value === 'light' || value === 'dark' || value === 'system'
    ? value
    : 'system';
}

export function resolveEffectiveTheme(
  mode: ThemeMode,
  prefersDark: boolean,
): EffectiveTheme {
  if (mode === 'system') return prefersDark ? 'dark' : 'light';
  return mode;
}

export function applyEffectiveTheme(theme: EffectiveTheme) {
  document.documentElement.classList.toggle('dark', theme === 'dark');
  document.documentElement.style.colorScheme = theme;
}
```

- [ ] **Step 4: Implement the pre-hydration script**

Create `frontend/src/components/ThemeScript.tsx`:

```tsx
const source = `
(() => {
  const value = localStorage.getItem('themeMode');
  const mode = value === 'light' || value === 'dark' || value === 'system'
    ? value
    : 'system';
  const dark = mode === 'dark' ||
    (mode === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', dark);
  document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
})();
`;

export function ThemeScript() {
  return <script dangerouslySetInnerHTML={{ __html: source }} />;
}
```

Render `<ThemeScript />` inside `<head>` in `frontend/src/app/layout.tsx`.

- [ ] **Step 5: Make ConfigContext use the shared resolver**

Replace local parsing and application logic with imports from `@/lib/theme`. Initialize `isDarkTheme` from the current root class when the browser is available:

```ts
const [themeMode, setThemeModeState] = useState<ThemeMode>(() =>
  typeof window === 'undefined'
    ? 'system'
    : parseThemeMode(localStorage.getItem(THEME_STORAGE_KEY))
);
const [isDarkTheme, setIsDarkTheme] = useState(() =>
  typeof document !== 'undefined' && document.documentElement.classList.contains('dark')
);
```

The effect computes `resolveEffectiveTheme(themeMode, media.matches)`, calls `applyEffectiveTheme`, updates `isDarkTheme`, and subscribes only for `system`.

- [ ] **Step 6: Run the focused test and verify GREEN**

Run:

```bash
pnpm test:e2e tests/theme-foundation.spec.ts
pnpm exec tsc --noEmit
```

Expected: all theme-foundation tests pass and TypeScript exits 0.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/lib/theme.ts frontend/src/components/ThemeScript.tsx \
  frontend/src/app/layout.tsx frontend/src/contexts/ConfigContext.tsx \
  frontend/tests/theme-foundation.spec.ts
git commit -m "feat: apply theme before first paint"
```

## Task 3: Define Semantic Status Tokens and Freeze Shared Primitives

**Files:**
- Modify: `frontend/src/app/globals.css`
- Modify: `frontend/tailwind.config.js`
- Modify: `frontend/src/components/ui/alert.tsx`
- Modify: `frontend/src/components/ui/button.tsx`
- Modify: `frontend/src/components/ui/command.tsx`
- Modify: `frontend/src/components/ui/dialog.tsx`
- Modify: `frontend/src/components/ui/dropdown-menu.tsx`
- Modify: `frontend/src/components/ui/input-group.tsx`
- Modify: `frontend/src/components/ui/input.tsx`
- Modify: `frontend/src/components/ui/popover.tsx`
- Modify: `frontend/src/components/ui/progress.tsx`
- Modify: `frontend/src/components/ui/scroll-area.tsx`
- Modify: `frontend/src/components/ui/select.tsx`
- Modify: `frontend/src/components/ui/sheet.tsx`
- Modify: `frontend/src/components/ui/switch.tsx`
- Modify: `frontend/src/components/ui/tabs.tsx`
- Modify: `frontend/src/components/ui/textarea.tsx`
- Modify: `frontend/src/components/ui/tooltip.tsx`
- Create: `frontend/tests/theme-primitives.spec.ts`

- [ ] **Step 1: Write failing primitive contrast tests**

Create `frontend/tests/theme-primitives.spec.ts`:

```ts
import { expect, test } from './fixtures/tauri-fixtures';

test.use({ colorScheme: 'dark' });

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('themeMode', 'dark'));
  await page.goto('/settings');
});

test('dialog and popover surfaces use elevated semantic colors', async ({ page }) => {
  await page.getByRole('tab', { name: 'Summary' }).click();
  await page.getByRole('button', { name: /Summarization Model/ }).click();
  const popover = page.getByRole('listbox');
  await expect(popover).toBeVisible();
  await expect(popover).toHaveCSS('color', 'rgb(250, 250, 250)');
});

test('theme controls and cards have visible boundaries in dark mode', async ({ page }) => {
  const appearance = page.getByRole('heading', { name: 'Appearance' }).locator('..').locator('..');
  await expect(appearance).toHaveCSS('border-top-color', 'rgb(38, 38, 38)');
});
```

Use actual computed values produced by the chosen HSL variables. If browser rounding differs, assert contrast with a helper rather than weakening the test.

- [ ] **Step 2: Run the primitive test and verify RED**

Run:

```bash
pnpm test:e2e tests/theme-primitives.spec.ts
```

Expected: at least one assertion fails because fixed gray variants and light feature containers remain.

- [ ] **Step 3: Add semantic status variables**

Add light and dark values to `globals.css`:

```css
:root {
  --info: 217 91% 60%;
  --info-foreground: 0 0% 100%;
  --success: 142 71% 45%;
  --success-foreground: 0 0% 100%;
  --warning: 45 93% 47%;
  --warning-foreground: 26 83% 14%;
  --recording: 0 84% 60%;
  --recording-foreground: 0 0% 100%;
}

.dark {
  --info: 217 91% 65%;
  --info-foreground: 222 47% 11%;
  --success: 142 69% 58%;
  --success-foreground: 144 61% 8%;
  --warning: 48 96% 65%;
  --warning-foreground: 26 83% 14%;
  --recording: 0 91% 71%;
  --recording-foreground: 0 63% 12%;
}
```

Map each pair in `tailwind.config.js` using the same nested structure as `primary` and `destructive`.

- [ ] **Step 4: Remove color-named Button variants**

Replace `green`, `blue`, `red`, and `gray` with semantic variants:

```ts
success: 'bg-success text-success-foreground hover:bg-success/90',
info: 'bg-info text-info-foreground hover:bg-info/90',
recording: 'bg-recording text-recording-foreground hover:bg-recording/90',
muted: 'border border-input bg-muted text-foreground hover:bg-accent',
```

Update call sites later in the owning migration groups; temporary TypeScript or class-string audit failures are expected until the parallel wave is integrated.

- [ ] **Step 5: Normalize shared primitive surfaces**

Apply these contracts:

- Inputs and textareas: `border-input bg-background text-foreground placeholder:text-muted-foreground`.
- Dialog and sheet content: `border-border bg-card text-card-foreground`.
- Select, dropdown, command, popover, tooltip: `bg-popover text-popover-foreground border-border`.
- Tabs and switches: semantic background, selected state, and visible focus ring.
- Progress track: `bg-muted`; indicator defaults to `bg-primary`.
- Alerts: variants `default`, `info`, `success`, `warning`, `destructive`.

- [ ] **Step 6: Run primitive tests and TypeScript**

Run:

```bash
pnpm test:e2e tests/theme-primitives.spec.ts
pnpm exec tsc --noEmit
```

Expected: primitive tests pass. TypeScript may report only color-variant call sites owned by Tasks 4–8; update those call sites to the new names in their owning tasks rather than reintroducing old variants.

- [ ] **Step 7: Commit the shared foundation**

```bash
git add frontend/src/app/globals.css frontend/tailwind.config.js \
  frontend/src/components/ui frontend/tests/theme-primitives.spec.ts
git commit -m "feat: define semantic theme primitives"
```

## Parallel Wave Setup

After Task 3 is committed, create five isolated worktrees from the current `codex-port-v04` HEAD:

```bash
git worktree add ../theme-shell -b theme/shell codex-port-v04
git worktree add ../theme-settings -b theme/settings codex-port-v04
git worktree add ../theme-meetings -b theme/meetings codex-port-v04
git worktree add ../theme-onboarding -b theme/onboarding codex-port-v04
git worktree add ../theme-secondary -b theme/secondary codex-port-v04
```

Dispatch Tasks 4–8 concurrently. Each agent follows TDD, edits only its listed production files and test file, runs its focused Playwright spec plus TypeScript, and commits one branch-local commit.

## Task 4: Migrate the Application Shell

**Files:**
- Modify: `frontend/src/app/page.tsx`
- Modify: `frontend/src/components/MainContent.tsx`
- Modify: `frontend/src/components/Sidebar/index.tsx`
- Modify: `frontend/src/components/Sidebar/SidebarProvider.tsx`
- Modify: `frontend/src/components/Logo.tsx`
- Create: `frontend/tests/theme-shell.spec.ts`

- [ ] **Step 1: Write the failing shell test**

```ts
import { expect, test, expectNoHorizontalOverflow } from './fixtures/tauri-fixtures';

for (const mode of ['light', 'dark'] as const) {
  test(`home shell renders in ${mode} mode`, async ({ page }) => {
    await page.addInitScript((value) => localStorage.setItem('themeMode', value), mode);
    await page.goto('/');
    await expect(page.getByText('Theme Test Meeting')).toBeVisible();
    await expect(page.getByRole('button', { name: /Start Recording/ })).toBeVisible();
    await expectNoHorizontalOverflow(page);
    await expect(page).toHaveScreenshot(`home-${mode}.png`, { fullPage: true });
  });
}
```

- [ ] **Step 2: Run and verify RED**

Run:

```bash
pnpm test:e2e tests/theme-shell.spec.ts
```

Expected: dark screenshot or semantic-surface assertions fail because the shell and sidebar use fixed white/gray classes.

- [ ] **Step 3: Migrate shell classes**

Use:

- Page: `bg-background text-foreground`.
- Sidebar: `bg-card text-card-foreground border-border`.
- Navigation hover/active: `hover:bg-accent`, active `bg-accent text-accent-foreground`.
- Search: shared input-group semantics.
- Recording action: recording token.
- Import action: info token.
- Search matches: warning token with dark-readable foreground.
- Version and metadata: `text-muted-foreground`.

- [ ] **Step 4: Verify GREEN**

```bash
pnpm test:e2e tests/theme-shell.spec.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app/page.tsx frontend/src/components/MainContent.tsx \
  frontend/src/components/Sidebar frontend/src/components/Logo.tsx \
  frontend/tests/theme-shell.spec.ts
git commit -m "feat: theme application shell"
```

## Task 5: Migrate Settings and Model Management

**Files:**
- Modify: `frontend/src/app/settings/page.tsx`
- Modify: `frontend/src/components/PreferenceSettings.tsx`
- Modify: `frontend/src/components/RecordingSettings.tsx`
- Modify: `frontend/src/components/TranscriptSettings.tsx`
- Modify: `frontend/src/components/SummaryModelSettings.tsx`
- Modify: `frontend/src/components/SummaryLanguageSettings.tsx`
- Modify: `frontend/src/components/BetaSettings.tsx`
- Modify: `frontend/src/components/ModelSettingsModal.tsx`
- Modify: `frontend/src/components/BuiltInModelManager.tsx`
- Modify: `frontend/src/components/ParakeetModelManager.tsx`
- Modify: `frontend/src/components/WhisperModelManager.tsx`
- Modify: `frontend/src/components/AudioBackendSelector.tsx`
- Modify: `frontend/src/components/AnalyticsConsentSwitch.tsx`
- Create: `frontend/tests/theme-settings.spec.ts`

- [ ] **Step 1: Write failing tests for every settings tab**

Create tests that open `/settings`, set dark mode, click each exact tab name, assert its heading is visible, assert no horizontal overflow, and capture:

```ts
await expect(page).toHaveScreenshot(`settings-${tab.value}-dark.png`, {
  fullPage: true,
});
```

Add a focused status test:

```ts
await page.getByRole('tab', { name: 'Summary' }).click();
await expect(page.getByText('Not Downloaded')).toBeVisible();
await expect(page.getByText('Ready')).toHaveCSS('color', 'rgb(74, 222, 128)');
```

Use the actual computed success value if token tuning changes the RGB result.

- [ ] **Step 2: Run and verify RED**

```bash
pnpm test:e2e tests/theme-settings.spec.ts
```

Expected: screenshots and/or status contrast fail because settings containers and managers retain fixed neutral classes.

- [ ] **Step 3: Migrate all settings surfaces**

Apply the approved design:

- Settings frame: background/foreground semantic tokens.
- Cards: `bg-card text-card-foreground border-border`.
- Secondary descriptions: `text-muted-foreground`.
- Selected tabs/models: primary or info treatment with visible border.
- Ready/success: success tokens.
- Download/error/corrupted: destructive tokens.
- Warnings: warning tokens.
- All controls use shared primitives rather than local white inputs.

- [ ] **Step 4: Verify GREEN**

```bash
pnpm test:e2e tests/theme-settings.spec.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app/settings/page.tsx frontend/src/components/PreferenceSettings.tsx \
  frontend/src/components/RecordingSettings.tsx frontend/src/components/TranscriptSettings.tsx \
  frontend/src/components/SummaryModelSettings.tsx frontend/src/components/SummaryLanguageSettings.tsx \
  frontend/src/components/BetaSettings.tsx frontend/src/components/ModelSettingsModal.tsx \
  frontend/src/components/BuiltInModelManager.tsx frontend/src/components/ParakeetModelManager.tsx \
  frontend/src/components/WhisperModelManager.tsx frontend/src/components/AudioBackendSelector.tsx \
  frontend/src/components/AnalyticsConsentSwitch.tsx frontend/tests/theme-settings.spec.ts
git commit -m "feat: theme settings and model management"
```

## Task 6: Migrate Meetings, Transcripts, Notes, and Editors

**Files:**
- Modify: `frontend/src/app/meeting-details/page.tsx`
- Modify: `frontend/src/app/meeting-details/page-content.tsx`
- Modify: `frontend/src/app/notes/[id]/page.tsx`
- Modify: `frontend/src/app/_components/TranscriptPanel.tsx`
- Modify: `frontend/src/components/MeetingDetails/SummaryGeneratorButtonGroup.tsx`
- Modify: `frontend/src/components/MeetingDetails/SummaryPanel.tsx`
- Modify: `frontend/src/components/MeetingDetails/SummaryUpdaterButtonGroup.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx`
- Modify: `frontend/src/components/MeetingDetails/TranscriptPanel.tsx`
- Modify: `frontend/src/components/TranscriptView.tsx`
- Modify: `frontend/src/components/VirtualizedTranscriptView.tsx`
- Modify: `frontend/src/components/EmptyStateSummary.tsx`
- Modify: `frontend/src/components/AISummary/Block.tsx`
- Modify: `frontend/src/components/AISummary/Section.tsx`
- Modify: `frontend/src/components/AISummary/index.tsx`
- Modify: `frontend/src/components/AISummary/BlockNoteSummaryView.tsx`
- Modify: `frontend/src/components/BlockNoteEditor/Editor.tsx`
- Modify: `frontend/src/components/EditableTitle.tsx`
- Create: `frontend/tests/theme-meetings.spec.ts`

- [ ] **Step 1: Write failing meeting and notes tests**

Test `/meeting-details?id=theme-test-meeting` and `/notes/team-sync-dec-26` in light and dark modes. Assert transcript text, summary text, editor surface, toolbar actions, and no overflow. Capture stable screenshots.

Add:

```ts
const editor = page.locator('.bn-container');
await expect(editor).toBeVisible();
await expect(editor).toHaveAttribute('data-color-scheme', 'dark');
```

If BlockNote exposes theme through a different stable attribute, assert that actual attribute after inspecting the rendered DOM.

- [ ] **Step 2: Run and verify RED**

```bash
pnpm test:e2e tests/theme-meetings.spec.ts
```

Expected: fixed white panels and editor/toolbar colors fail dark-mode screenshots or computed-style assertions.

- [ ] **Step 3: Migrate content surfaces**

Use semantic page, panel, toolbar, transcript-row, timestamp, confidence, empty-state, and summary-card styles. Keep BlockNote wired to `isDarkTheme`; remove fixed light wrappers around it. Preserve confidence and action colors with readable dark variants.

- [ ] **Step 4: Verify GREEN**

```bash
pnpm test:e2e tests/theme-meetings.spec.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app/meeting-details frontend/src/app/notes \
  frontend/src/app/_components/TranscriptPanel.tsx frontend/src/components/MeetingDetails \
  frontend/src/components/TranscriptView.tsx frontend/src/components/VirtualizedTranscriptView.tsx \
  frontend/src/components/EmptyStateSummary.tsx frontend/src/components/AISummary \
  frontend/src/components/BlockNoteEditor frontend/src/components/EditableTitle.tsx \
  frontend/tests/theme-meetings.spec.ts
git commit -m "feat: theme meetings and editors"
```

## Task 7: Migrate Onboarding, Import, Downloads, and Recording Setup

**Files:**
- Modify: `frontend/src/components/onboarding/OnboardingContainer.tsx`
- Modify: `frontend/src/components/onboarding/OnboardingFlow.tsx`
- Modify: `frontend/src/components/onboarding/shared/PermissionRow.tsx`
- Modify: `frontend/src/components/onboarding/shared/ProgressIndicator.tsx`
- Modify: `frontend/src/components/onboarding/shared/StatusIndicator.tsx`
- Modify: `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx`
- Modify: `frontend/src/components/onboarding/steps/PermissionsStep.tsx`
- Modify: `frontend/src/components/onboarding/steps/SetupOverviewStep.tsx`
- Modify: `frontend/src/components/onboarding/steps/WelcomeStep.tsx`
- Modify: `frontend/src/components/ImportAudio/ImportAudioDialog.tsx`
- Modify: `frontend/src/components/ImportAudio/ImportDropOverlay.tsx`
- Modify: `frontend/src/components/DeviceSelection.tsx`
- Modify: `frontend/src/components/AudioLevelMeter.tsx`
- Modify: `frontend/src/components/RecordingControls.tsx`
- Modify: `frontend/src/components/RecordingStatusBar.tsx`
- Modify: `frontend/src/components/ModelDownloadProgress.tsx`
- Modify: `frontend/src/components/ChunkProgressDisplay.tsx`
- Modify: `frontend/src/components/shared/DownloadProgressToast.tsx`
- Create: `frontend/tests/theme-onboarding.spec.ts`

- [ ] **Step 1: Add fixture support for onboarding states and write RED tests**

Use the `__e2e=onboarding` scenario installed by Task 1:

```ts
await page.addInitScript(() => localStorage.setItem('themeMode', 'dark'));
await page.goto('/?__e2e=onboarding');
await expect(page.getByText('Welcome to Meetily')).toBeVisible();
await expect(page).toHaveScreenshot('onboarding-welcome-dark.png');
```

Advance with the visible onboarding buttons and capture Setup, Permissions, and Download states. Test the import dialog by completing the mocked onboarding flow, returning to `/`, and clicking the visible Import Audio action. Trigger recording and download states through their visible controls and deterministic IPC fixtures.

- [ ] **Step 2: Run and verify RED**

```bash
pnpm test:e2e tests/theme-onboarding.spec.ts
```

Expected: fixed white cards, gray text, and progress surfaces fail dark screenshots.

- [ ] **Step 3: Migrate onboarding and progress surfaces**

Use semantic cards, muted copy, status tokens, input/border tokens, and recording tokens. Keep download progress visually distinguishable and retain platform permission meaning.

- [ ] **Step 4: Verify GREEN**

```bash
pnpm test:e2e tests/theme-onboarding.spec.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/onboarding frontend/src/components/ImportAudio \
  frontend/src/components/DeviceSelection.tsx frontend/src/components/AudioLevelMeter.tsx \
  frontend/src/components/RecordingControls.tsx frontend/src/components/RecordingStatusBar.tsx \
  frontend/src/components/ModelDownloadProgress.tsx frontend/src/components/ChunkProgressDisplay.tsx \
  frontend/src/components/shared/DownloadProgressToast.tsx frontend/tests/theme-onboarding.spec.ts
git commit -m "feat: theme onboarding and recording flows"
```

## Task 8: Migrate Dialogs, Overlays, and Secondary Surfaces

**Files:**
- Modify: `frontend/src/app/_components/SettingsModal.tsx`
- Modify: `frontend/src/app/_components/StatusOverlays.tsx`
- Modify: `frontend/src/components/About.tsx`
- Modify: `frontend/src/components/AnalyticsDataModal.tsx`
- Modify: `frontend/src/components/ComplianceNotification.tsx`
- Modify: `frontend/src/components/ConfirmationModel/confirmation-modal.tsx`
- Modify: `frontend/src/components/Info.tsx`
- Modify: `frontend/src/components/MeetingDetails/RetranscribeDialog.tsx`
- Modify: `frontend/src/components/UpdateDialog.tsx`
- Modify: `frontend/src/components/DatabaseImport/HomebrewDatabaseDetector.tsx`
- Modify: `frontend/src/components/DatabaseImport/LegacyDatabaseImport.tsx`
- Modify: `frontend/src/components/LanguagePickerPopover.tsx`
- Modify: `frontend/src/components/LanguageSelection.tsx`
- Modify: `frontend/src/components/molecules/form-components/form-input-item.tsx`
- Create: `frontend/tests/theme-secondary.spec.ts`

- [ ] **Step 1: Write failing modal and overlay tests**

Open representative dialogs in dark mode and assert:

```ts
const dialog = page.getByRole('dialog');
await expect(dialog).toBeVisible();
await expect(dialog).toHaveCSS('background-color', 'rgb(10, 10, 10)');
await expect(dialog).toHaveCSS('color', 'rgb(250, 250, 250)');
```

Capture screenshots for confirmation, update, analytics, language picker, database import, and status overlay states.

- [ ] **Step 2: Run and verify RED**

```bash
pnpm test:e2e tests/theme-secondary.spec.ts
```

Expected: one or more dialogs retain light-only backgrounds or gray text.

- [ ] **Step 3: Migrate secondary surfaces**

Replace local dialog/panel/input/button neutral colors with shared primitives and semantic tokens. Use status tokens for compliance, update, and database-import state messages.

- [ ] **Step 4: Verify GREEN**

```bash
pnpm test:e2e tests/theme-secondary.spec.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src/app/_components/SettingsModal.tsx \
  frontend/src/app/_components/StatusOverlays.tsx frontend/src/components/About.tsx \
  frontend/src/components/AnalyticsDataModal.tsx frontend/src/components/ComplianceNotification.tsx \
  frontend/src/components/ConfirmationModel frontend/src/components/Info.tsx \
  frontend/src/components/MeetingDetails/RetranscribeDialog.tsx frontend/src/components/UpdateDialog.tsx \
  frontend/src/components/DatabaseImport frontend/src/components/LanguagePickerPopover.tsx \
  frontend/src/components/LanguageSelection.tsx frontend/src/components/molecules \
  frontend/tests/theme-secondary.spec.ts
git commit -m "feat: theme dialogs and secondary surfaces"
```

## Task 9: Integrate Parallel Branches and Close Shared Gaps

**Files:**
- Modify only as required: shared foundation files from Tasks 1–3
- Modify: `frontend/src/testing/tauri-browser-mocks.ts`
- Modify: `frontend/scripts/check-theme-tokens.mjs`
- Test: all `frontend/tests/theme-*.spec.ts`

- [ ] **Step 1: Cherry-pick the five reviewed commits**

From `codex-port-v04`:

```bash
git cherry-pick theme/shell
git cherry-pick theme/settings
git cherry-pick theme/meetings
git cherry-pick theme/onboarding
git cherry-pick theme/secondary
```

Resolve conflicts by preserving the shared foundation from Tasks 1–3 and the feature-owned changes from each branch.

- [ ] **Step 2: Run the static audit and verify remaining failures**

```bash
cd frontend
pnpm test:theme-audit
```

Expected: FAIL only for files omitted from the five ownership lists or documented invariant artwork. Do not add broad patterns to the allowlist.

- [ ] **Step 3: Add missing shared fixtures or tokens centrally**

For every agent-reported fixture or token gap:

1. Add a failing assertion to the relevant existing `theme-*.spec.ts`.
2. Run that spec and confirm RED.
3. Add the smallest fixture/token/primitive change.
4. Re-run and confirm GREEN.

- [ ] **Step 4: Remove remaining unexplained fixed neutrals**

Run:

```bash
pnpm test:theme-audit
```

Expected: PASS with no failures. Any allowlist entry must include a comment naming the invariant asset or third-party compatibility reason.

- [ ] **Step 5: Run all focused specs**

```bash
pnpm test:e2e tests/theme-foundation.spec.ts \
  tests/theme-primitives.spec.ts \
  tests/theme-shell.spec.ts \
  tests/theme-settings.spec.ts \
  tests/theme-meetings.spec.ts \
  tests/theme-onboarding.spec.ts \
  tests/theme-secondary.spec.ts
```

Expected: all pass.

- [ ] **Step 6: Commit integration fixes**

```bash
git add frontend
git commit -m "fix: integrate complete application theme"
```

## Task 10: Add the Final Visual Matrix and Verify the Branch

**Files:**
- Create: `frontend/tests/theme-visual-matrix.spec.ts`
- Create/update: `frontend/tests/theme-visual-matrix.spec.ts-snapshots/*`
- Modify if required by a reproduced failure: feature-owned frontend files

- [ ] **Step 1: Write the complete visual matrix**

Create a table-driven Playwright spec covering:

```ts
const cases = [
  { name: 'home', path: '/', ready: 'Theme Test Meeting' },
  { name: 'settings-general', path: '/settings', ready: 'Appearance' },
  { name: 'meeting-details', path: '/meeting-details?id=theme-test-meeting', ready: 'Use semantic tokens.' },
  { name: 'notes', path: '/notes/team-sync-dec-26', ready: 'Team Sync' },
];

for (const theme of ['light', 'dark'] as const) {
  for (const entry of cases) {
    test(`${entry.name} ${theme}`, async ({ page }) => {
      await page.addInitScript((mode) => localStorage.setItem('themeMode', mode), theme);
      await page.goto(entry.path);
      await expect(page.getByText(entry.ready, { exact: false })).toBeVisible();
      await expect(page).toHaveScreenshot(`${entry.name}-${theme}.png`, { fullPage: true });
    });
  }
}
```

Extend the table with explicit tests for settings tabs, onboarding, import, and representative dialogs using the helpers created in Tasks 4–8.

- [ ] **Step 2: Generate baselines and inspect them**

Run:

```bash
pnpm test:e2e:update tests/theme-visual-matrix.spec.ts
```

Inspect every generated screenshot against the supplied dark-theme design. Do not accept baselines containing light islands, unreadable muted text, invisible status colors, clipped controls, or horizontal overflow.

- [ ] **Step 3: Re-run without snapshot updates**

```bash
pnpm test:e2e tests/theme-visual-matrix.spec.ts
```

Expected: PASS without writing snapshots.

- [ ] **Step 4: Run complete verification**

```bash
pnpm test:theme-audit
pnpm exec tsc --noEmit
pnpm build
pnpm test:e2e
cd src-tauri
cargo test summary:: --lib
```

Expected:

- Theme audit: exit 0.
- TypeScript: exit 0.
- Next production build: exit 0.
- Playwright: all tests pass with no unexpected screenshots.
- Rust summary tests: 0 failures.

- [ ] **Step 5: Review final diff and commit**

```bash
git diff --check origin/main...HEAD
git status --short
git add frontend
git commit -m "test: verify full application theme"
```

The only untracked path allowed outside the implementation is `.superpowers/` from the visual companion; it must not be committed.
