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
