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

  const readyModel = page
    .getByText('Qwen 3.5 2B (Balanced)', { exact: true })
    .locator('xpath=ancestor::div[contains(@class, "rounded-lg")][1]');
  await expect(readyModel.getByText('Ready', { exact: true })).toBeVisible();

  const downloadableModel = page
    .getByText('Qwen 3.5 4B (High Quality)', { exact: true })
    .locator('xpath=ancestor::div[contains(@class, "rounded-lg")][1]');
  await expect(
    downloadableModel.getByRole('button', { name: 'Download' }),
  ).toBeVisible();

  const readiness = await page.evaluate(async () => {
    const invoke = (
      window as typeof window & {
        __TAURI_INTERNALS__: {
          invoke: (command: string, payload?: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__.invoke;

    return {
      available: await invoke('builtin_ai_is_model_ready', {
        modelName: 'qwen3.5:2b',
      }),
      unavailable: await invoke('builtin_ai_is_model_ready', {
        modelName: 'qwen3.5:4b',
      }),
    };
  });

  expect(readiness).toEqual({
    available: true,
    unavailable: false,
  });
  expect(missingFixtures).toEqual([]);
});

test('unknown commands fail loudly instead of matching a prefix', async ({
  page,
}) => {
  await page.goto('/');

  const error = await page.evaluate(async () => {
    const invoke = (
      window as typeof window & {
        __TAURI_INTERNALS__: {
          invoke: (command: string) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__.invoke;

    try {
      await invoke('track_unexpected_theme_event');
      return null;
    } catch (caught) {
      return String(caught);
    }
  });

  expect(error).toContain('[E2E fixture missing] track_unexpected_theme_event');
});
