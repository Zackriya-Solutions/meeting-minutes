import { expect, test as base, type Page } from '@playwright/test';

import { THEME_STORAGE_KEY } from '../../src/lib/theme';

export const test = base;
export { expect, THEME_STORAGE_KEY };

export async function setStoredTheme(
  page: Page,
  mode: 'light' | 'dark' | 'system',
) {
  await setRawStoredTheme(page, mode);
}

// Untyped variant for seeding invalid values in fallback tests.
export async function setRawStoredTheme(page: Page, value: string) {
  await page.addInitScript(
    ([key, mode]) => {
      localStorage.setItem(key, mode);
    },
    [THEME_STORAGE_KEY, value] as const,
  );
}

export function lastNativeThemePayload(page: Page) {
  return page.evaluate(
    () =>
      (
        window as typeof window & {
          __e2eTauriThemeCalls?: Array<{ theme?: string | null }>;
        }
      ).__e2eTauriThemeCalls?.at(-1)?.theme,
  );
}

export async function expectLastNativeTheme(
  page: Page,
  theme: string | null | undefined,
) {
  await expect.poll(() => lastNativeThemePayload(page)).toBe(theme);
}

export async function expectNoHorizontalOverflow(page: Page) {
  const dimensions = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));

  expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth);
}
