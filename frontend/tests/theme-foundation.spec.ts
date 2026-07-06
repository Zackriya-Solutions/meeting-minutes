import { expect, test } from './fixtures/tauri-fixtures';

test('applies stored dark mode before DOMContentLoaded', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('themeMode', 'dark');

    const originalToggle = DOMTokenList.prototype.toggle;
    DOMTokenList.prototype.toggle = function toggle(token, force) {
      const result = originalToggle.call(this, token, force);
      if (
        token === 'dark' &&
        force === true &&
        this === document.documentElement?.classList
      ) {
        (
          window as typeof window & {
            __darkAppliedBeforeBody?: boolean;
          }
        ).__darkAppliedBeforeBody = document.body === null;
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

test('system mode follows the browser color scheme', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => localStorage.setItem('themeMode', 'system'));
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
    .poll(() => page.evaluate(() => localStorage.getItem('themeMode')))
    .toBe('dark');

  await page.reload();

  await expect(page.locator('html')).toHaveClass(/dark/);
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem('themeMode')))
    .toBe('dark');
});

test('syncs native Tauri theme payloads', async ({ page }) => {
  await page.addInitScript(() => localStorage.removeItem('themeMode'));

  await page.goto('/settings');

  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __e2eTauriThemeCalls?: Array<{ theme?: string | null }>;
            }
          ).__e2eTauriThemeCalls?.at(-1)?.theme,
      ),
    )
    .toBe(null);

  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __e2eTauriThemeCalls?: Array<{ theme?: string | null }>;
            }
          ).__e2eTauriThemeCalls?.at(-1)?.theme,
      ),
    )
    .toBe('light');

  await page.getByRole('button', { name: 'Dark', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __e2eTauriThemeCalls?: Array<{ theme?: string | null }>;
            }
          ).__e2eTauriThemeCalls?.at(-1)?.theme,
      ),
    )
    .toBe('dark');

  await page.getByRole('button', { name: 'System', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (
            window as typeof window & {
              __e2eTauriThemeCalls?: Array<{ theme?: string | null }>;
            }
          ).__e2eTauriThemeCalls?.at(-1)?.theme,
      ),
    )
    .toBe(null);
});

test('invalid stored theme falls back to the dark system theme', async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => localStorage.setItem('themeMode', 'sepia'));

  await page.goto('/settings');

  await expect(page.locator('html')).toHaveClass(/dark/);
});

test('unavailable theme storage falls back to system without blocking render', async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await page.emulateMedia({ colorScheme: 'dark' });
  await page.addInitScript(() => {
    const originalGetItem = Storage.prototype.getItem;
    Storage.prototype.getItem = function getItem(key) {
      if (key === 'themeMode') {
        throw new DOMException('Theme storage unavailable', 'SecurityError');
      }
      return originalGetItem.call(this, key);
    };
  });

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
  await page.addInitScript(() => {
    const originalSetItem = Storage.prototype.setItem;
    Storage.prototype.setItem = function setItem(key, value) {
      if (key === 'themeMode') {
        throw new DOMException('Theme storage unavailable', 'SecurityError');
      }
      return originalSetItem.call(this, key, value);
    };
  });

  await page.goto('/settings');
  await page.getByRole('button', { name: 'Dark', exact: true }).click();

  await expect(page.locator('html')).toHaveClass(/dark/);
  expect(pageErrors).toEqual([]);
});
