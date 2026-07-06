import {
  expect,
  expectLastNativeTheme,
  lastNativeThemePayload,
  setRawStoredTheme,
  setStoredTheme,
  test,
  THEME_STORAGE_KEY,
} from './fixtures/tauri-fixtures';

test('applies stored dark mode before DOMContentLoaded', async ({ page }) => {
  await setStoredTheme(page, 'dark');
  await page.addInitScript(() => {
    const originalToggle = DOMTokenList.prototype.toggle;
    DOMTokenList.prototype.toggle = function toggle(token, force) {
      const result = originalToggle.call(this, token, force);
      if (
        token === 'dark' &&
        force === true &&
        this === document.documentElement?.classList
      ) {
        const state = window as typeof window & {
          __darkAppliedBeforeBody?: boolean;
        };
        // Latch on the FIRST dark application — the pre-hydration boot script.
        // ConfigContext re-applies the class after hydration (when <body>
        // exists); without this guard that later call overwrites the flag to
        // false and the assertion races (and loses) under parallel load.
        if (state.__darkAppliedBeforeBody === undefined) {
          state.__darkAppliedBeforeBody = document.body === null;
        }
      }
      return result;
    };
  });

  await page.goto('/settings');

  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __darkAppliedBeforeBody?: boolean;
            }
          ).__darkAppliedBeforeBody,
      ),
    )
    .toBe(true);
  await expect(page.locator('html')).toHaveClass(/dark/);
});

test('appearance highlight matches the stored theme without hydration errors', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));

  await setStoredTheme(page, 'dark');
  await page.goto('/settings');

  // The stored theme owns the highlight; System must not be stranded active.
  await expect(page.getByRole('button', { name: 'Dark' })).toHaveClass(
    /bg-background/,
  );
  await expect(page.getByRole('button', { name: 'System' })).not.toHaveClass(
    /bg-background/,
  );

  // Let the ~2s startup update check fire so a missing updater fixture would
  // surface as a console error.
  await page.waitForTimeout(2600);

  const relevant = errors.filter(
    (text) => /did not match/.test(text) || /E2E fixture missing/.test(text),
  );
  expect(relevant, `unexpected console errors:\n${relevant.join('\n')}`).toEqual(
    [],
  );
});

test('system mode follows the browser color scheme', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await setStoredTheme(page, 'system');
  await page.goto('/settings');
  await expect(page.locator('html')).toHaveClass(/dark/);

  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).not.toHaveClass(/dark/);
});

test('clicking Dark persists the theme across reloads', async ({ page }) => {
  await page.goto('/settings');

  await page.getByRole('button', { name: 'Dark', exact: true }).click();

  await expect(page.locator('html')).toHaveClass(/dark/);
  await expect
    .poll(() =>
      page.evaluate((key) => localStorage.getItem(key), THEME_STORAGE_KEY),
    )
    .toBe('dark');

  await page.reload();

  await expect(page.locator('html')).toHaveClass(/dark/);
  await expect
    .poll(() =>
      page.evaluate((key) => localStorage.getItem(key), THEME_STORAGE_KEY),
    )
    .toBe('dark');
});

test('syncs native Tauri theme payloads', async ({ page }) => {
  await page.addInitScript(
    (key) => localStorage.removeItem(key),
    THEME_STORAGE_KEY,
  );

  await page.goto('/settings');
  await expect(page.getByRole('heading', { name: 'Appearance' })).toBeVisible();

  // The default 'system' mode is Tauri's native default, so startup issues no
  // redundant set_app_theme call.
  expect(await lastNativeThemePayload(page)).toBeUndefined();

  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expectLastNativeTheme(page, 'light');

  await page.getByRole('button', { name: 'Dark', exact: true }).click();
  await expectLastNativeTheme(page, 'dark');

  await page.getByRole('button', { name: 'System', exact: true }).click();
  await expectLastNativeTheme(page, null);
});

test('invalid stored theme falls back to the dark system theme', async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await setRawStoredTheme(page, 'sepia');

  await page.goto('/settings');

  await expect(page.locator('html')).toHaveClass(/dark/);
});

test('unavailable theme storage falls back to system without blocking render', async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript((themeKey) => {
    const originalGetItem = Storage.prototype.getItem;
    Storage.prototype.getItem = function getItem(key) {
      if (key === themeKey) {
        throw new DOMException('Theme storage unavailable', 'SecurityError');
      }
      return originalGetItem.call(this, key);
    };
  }, THEME_STORAGE_KEY);

  await page.goto('/settings');

  await expect(page.getByRole('heading', { name: 'Appearance' })).toBeVisible();
  await expect(page.locator('html')).toHaveClass(/dark/);
  expect(pageErrors).toEqual([]);
});

test('selected theme still applies when theme storage writes fail', async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await page.emulateMedia({ colorScheme: 'light' });
  await page.addInitScript((themeKey) => {
    const originalSetItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function setItem(key, value) {
      if (key === themeKey) {
        throw new DOMException('Theme storage unavailable', 'SecurityError');
      }
      return originalSetItem.call(this, key, value);
    };
  }, THEME_STORAGE_KEY);

  await page.goto('/settings');
  await page.getByRole('button', { name: 'Dark', exact: true }).click();

  await expect(page.locator('html')).toHaveClass(/dark/);
  expect(pageErrors).toEqual([]);
});
