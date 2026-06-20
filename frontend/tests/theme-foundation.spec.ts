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
