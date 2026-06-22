import { expect, test } from './fixtures/tauri-fixtures';
import type { Page } from '@playwright/test';

import { Alert } from '../src/components/ui/alert';

type RGB = [number, number, number];
type RGBA = [number, number, number, number];
type ThemeMode = 'light' | 'dark';
type AlertStatus = 'info' | 'success' | 'warning' | 'destructive';

const solidStatuses = ['info', 'success', 'warning', 'recording'] as const;
const alertStatuses: AlertStatus[] = [
  'info',
  'success',
  'warning',
  'destructive',
];

function getAlertClassName(variant: AlertStatus) {
  const alertRender = Alert as unknown as {
    render: (
      props: { variant: AlertStatus },
      ref: null,
    ) => { props: { className: string } };
  };

  return alertRender.render({ variant }, null).props.className;
}

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

function parseColor(value: string): RGBA {
  const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
  if (channels.length < 3) {
    throw new Error(`Unable to parse color: ${value}`);
  }

  return [
    channels[0],
    channels[1],
    channels[2],
    channels.length > 3 ? channels[3] : 1,
  ];
}

function compositeColor(foreground: RGBA, background: RGB): RGB {
  const [red, green, blue, alpha] = foreground;
  return [
    Math.round(red * alpha + background[0] * (1 - alpha)),
    Math.round(green * alpha + background[1] * (1 - alpha)),
    Math.round(blue * alpha + background[2] * (1 - alpha)),
  ];
}

async function openSettings(page: Page, mode: ThemeMode) {
  await page.addInitScript((value) => {
    localStorage.setItem('themeMode', value);
  }, mode);
  await page.goto('/settings');
}

for (const mode of ['light', 'dark'] as const) {
  test(`solid status tokens meet text contrast in ${mode} mode`, async ({
    page,
  }) => {
    await openSettings(page, mode);

    const pairs = await page.evaluate((statuses) => {
      const probe = document.createElement('div');
      document.body.appendChild(probe);

      const readToken = (token: string) => {
        probe.style.color = `hsl(var(--${token}))`;
        const color = getComputedStyle(probe).color.match(/\d+/g);
        return color?.slice(0, 3).map(Number) ?? [];
      };

      const result = statuses.map((status) => ({
        status,
        background: readToken(status),
        foreground: readToken(`${status}-foreground`),
      }));

      probe.remove();
      return result;
    }, solidStatuses);

    for (const pair of pairs) {
      expect(pair.background, `${pair.status} background token`).toHaveLength(3);
      expect(pair.foreground, `${pair.status} foreground token`).toHaveLength(3);
      expect(
        contrastRatio(pair.background as RGB, pair.foreground as RGB),
        `${pair.status} foreground contrast in ${mode} mode`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });

  test(`alert variants meet non-text contrast in ${mode} mode`, async ({
    page,
  }) => {
    await openSettings(page, mode);

    const variants = alertStatuses.map((status) => ({
      status,
      className: getAlertClassName(status),
    }));

    const measurements = await page.evaluate((alertVariants) => {
      const container = document.createElement('section');
      document.body.appendChild(container);

      const bodyBackground = getComputedStyle(document.body).backgroundColor;

      return alertVariants.map(({ status, className }) => {
        const alert = document.createElement('div');
        alert.dataset.alertStatus = status;
        alert.className = className;

        const icon = document.createElementNS(
          'http://www.w3.org/2000/svg',
          'svg',
        );
        icon.dataset.alertIcon = status;
        alert.appendChild(icon);
        container.appendChild(alert);

        const alertStyle = getComputedStyle(alert);
        return {
          status,
          bodyBackground,
          alertBackground: alertStyle.backgroundColor,
          boundaryColor: alertStyle.borderTopColor,
          stripeColor: alertStyle.borderLeftColor,
          stripeWidth: alertStyle.borderLeftWidth,
          iconColor: getComputedStyle(icon).color,
        };
      });
    }, variants);

    for (const measurement of measurements) {
      const bodyBackground = parseColor(measurement.bodyBackground);
      const bodyRgb = bodyBackground.slice(0, 3) as RGB;
      const alertBackground = compositeColor(
        parseColor(measurement.alertBackground),
        bodyRgb,
      );
      const iconColor = compositeColor(
        parseColor(measurement.iconColor),
        alertBackground,
      );
      const boundaryColor = compositeColor(
        parseColor(measurement.boundaryColor),
        bodyRgb,
      );
      const stripeColor = compositeColor(
        parseColor(measurement.stripeColor),
        bodyRgb,
      );

      expect(
        contrastRatio(iconColor, alertBackground),
        `${measurement.status} icon contrast in ${mode} mode`,
      ).toBeGreaterThanOrEqual(3);
      expect(
        contrastRatio(boundaryColor, bodyRgb),
        `${measurement.status} boundary contrast in ${mode} mode`,
      ).toBeGreaterThanOrEqual(3);
      expect(measurement.stripeWidth).toBe('4px');
      expect(
        contrastRatio(stripeColor, bodyRgb),
        `${measurement.status} stripe contrast in ${mode} mode`,
      ).toBeGreaterThanOrEqual(3);
    }
  });
}

test('cards, dialogs, and popovers are elevated above the dark page background', async ({
  page,
}) => {
  await openSettings(page, 'dark');

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

  await page.getByRole('tab', { name: 'Summary' }).click();
  await page.locator('button[role="combobox"]').first().click();
  const popover = page.getByRole('listbox');

  await expect(popover).toBeVisible();
  await expect(popover).not.toHaveCSS('background-color', pageBackground);
  await expect(popover).toHaveCSS('border-top-color', 'rgb(38, 38, 38)');

  await page.keyboard.press('Escape');
  await page.locator('button[title="About Meetily"]').click();
  const dialog = page.getByRole('dialog').filter({
    hasText: 'What makes Meetily different',
  });

  await expect(dialog).toBeVisible();
  await expect(dialog).not.toHaveCSS('background-color', pageBackground);
  await expect(dialog).toHaveCSS('color', 'rgb(250, 250, 250)');
});
