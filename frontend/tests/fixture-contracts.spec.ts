import { expect, test } from './fixtures/tauri-fixtures';
import type { Page } from '@playwright/test';

function collectMissingFixtures(page: Page) {
  const missingFixtures: string[] = [];

  page.on('console', (message) => {
    const text = message.text();
    if (text.includes('[E2E fixture missing]')) {
      missingFixtures.push(text);
    }
  });

  page.on('pageerror', (error) => {
    if (error.message.includes('[E2E fixture missing]')) {
      missingFixtures.push(error.message);
    }
  });

  return missingFixtures;
}

test('meeting fixtures render paginated transcripts and markdown summary', async ({
  page,
}) => {
  const missingFixtures = collectMissingFixtures(page);

  await page.goto('/meeting-details?id=theme-test-meeting');

  await expect(
    page.getByText('We approved the dark theme.', { exact: true }),
  ).toBeVisible();
  await expect(page.getByText('Use semantic tokens.', { exact: true })).toBeVisible();
  expect(missingFixtures).toEqual([]);
});

test('onboarding fixtures keep the initial and download steps stable', async ({
  page,
}) => {
  const missingFixtures = collectMissingFixtures(page);

  await page.goto('/?__e2e=onboarding');

  await expect(
    page.getByRole('heading', { name: 'Welcome to Meetily' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Get Started' }).click();
  await expect(
    page.getByRole('heading', { name: 'Setup Overview' }),
  ).toBeVisible();
  await page.getByRole('button', { name: "Let's Go" }).click();

  await expect(
    page.getByRole('heading', { name: 'Getting things ready' }),
  ).toBeVisible();
  await expect(page.getByText('Transcription Engine', { exact: true })).toBeVisible();
  await expect(page.getByText('Summary Engine', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Continue' })).toBeEnabled();
  expect(missingFixtures).toEqual([]);
});

test('built-in model fixtures expose ready and downloadable states', async ({
  page,
}) => {
  const missingFixtures = collectMissingFixtures(page);

  await page.goto('/settings');
  await page.getByRole('tab', { name: 'Summary' }).click();

  const readyModel = page.locator('div').filter({
    has: page.getByText('Qwen 3.5 2B (Balanced)', { exact: true }),
  }).filter({
    has: page.getByText('Ready', { exact: true }),
  });
  await expect(readyModel.first()).toBeVisible();

  const downloadableModel = page.locator('div').filter({
    has: page.getByText('Qwen 3.5 4B (High Quality)', { exact: true }),
  }).filter({
    has: page.getByRole('button', { name: 'Download' }),
  });
  await expect(downloadableModel.first()).toBeVisible();
  expect(missingFixtures).toEqual([]);
});
