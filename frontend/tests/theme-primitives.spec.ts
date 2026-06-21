import { expect, test } from './fixtures/tauri-fixtures';

type RGB = [number, number, number];

function luminance([red, green, blue]: RGB) {
  const channels = [red, green, blue].map((channel) => {
    const value = channel / 255;
    return value <= 0.03928
      ? value / 12.92
      : Math.pow((value + 0.055) / 1.055, 2.4);
  });

  return (
    0.2126 * channels[0] +
    0.7152 * channels[1] +
    0.0722 * channels[2]
  );
}

function contrastRatio(first: RGB, second: RGB) {
  const lighter = Math.max(luminance(first), luminance(second));
  const darker = Math.min(luminance(first), luminance(second));
  return (lighter + 0.05) / (darker + 0.05);
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('themeMode', 'dark'));
  await page.goto('/settings');
});

test('status tokens provide readable dark-mode foregrounds', async ({
  page,
}) => {
  const pairs = await page.evaluate(() => {
    const probe = document.createElement('div');
    document.body.appendChild(probe);

    const readToken = (token: string) => {
      probe.style.color = `hsl(var(--${token}))`;
      const color = getComputedStyle(probe).color.match(/\d+/g);
      return color?.slice(0, 3).map(Number) ?? [];
    };

    const result = ['info', 'success', 'warning', 'recording'].map(
      (status) => ({
        status,
        background: readToken(status),
        foreground: readToken(`${status}-foreground`),
      }),
    );

    probe.remove();
    return result;
  });

  for (const pair of pairs) {
    expect(pair.background, `${pair.status} background token`).toHaveLength(3);
    expect(pair.foreground, `${pair.status} foreground token`).toHaveLength(3);
    expect(
      contrastRatio(pair.background as RGB, pair.foreground as RGB),
      `${pair.status} foreground contrast`,
    ).toBeGreaterThanOrEqual(4.5);
  }
});

test('cards and dialogs are elevated above the dark page background', async ({
  page,
}) => {
  const pageBackground = await page.locator('body').evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  const appearanceCard = page
    .getByRole('heading', { name: 'Appearance' })
    .locator('..')
    .locator('..')
    .locator('..');

  await expect(appearanceCard).not.toHaveCSS('background-color', pageBackground);
  await expect(appearanceCard).toHaveCSS(
    'border-top-color',
    'rgb(38, 38, 38)',
  );

  await page.locator('button[title="About Meetily"]').click();
  const dialog = page.getByRole('dialog').filter({
    hasText: 'What makes Meetily different',
  });

  await expect(dialog).toBeVisible();
  await expect(dialog).not.toHaveCSS('background-color', pageBackground);
  await expect(dialog).toHaveCSS('color', 'rgb(250, 250, 250)');
});
