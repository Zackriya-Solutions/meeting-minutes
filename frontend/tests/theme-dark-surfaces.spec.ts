import {
  expect,
  expectNoHorizontalOverflow,
  setStoredTheme,
  test,
} from './fixtures/tauri-fixtures';

const routes = [
  { path: '/', readyText: 'Welcome to meetily!' },
  { path: '/settings', readyText: 'Settings' },
  {
    path: '/meeting-details?id=theme-test-meeting',
    readyText: 'Theme Test Meeting',
  },
] as const;

for (const route of routes) {
  test(`dark theme avoids light app surfaces on ${route.path}`, async ({
    page,
  }) => {
    await setStoredTheme(page, 'dark');

    await page.goto(route.path);

    await expect(page.locator('html')).toHaveClass(/dark/);
    await expect(page.getByText(route.readyText).first()).toBeVisible();
    await expectNoHorizontalOverflow(page);

    const lightSurfaces = await page.evaluate(() => {
      const appRoot = document.querySelector('main') ?? document.body;
      const candidates = Array.from(
        appRoot.querySelectorAll<HTMLElement>(
          [
            'aside',
            'button',
            'div',
            'header',
            'input',
            'main',
            'section',
            'textarea',
          ].join(','),
        ),
      );

      return candidates
        .filter((element) => {
          const rect = element.getBoundingClientRect();
          if (rect.width < 8 || rect.height < 8) return false;

          const style = window.getComputedStyle(element);
          if (style.visibility === 'hidden' || style.display === 'none') {
            return false;
          }

          const match = style.backgroundColor.match(
            /rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([0-9.]+))?\)/,
          );
          if (!match) return false;

          const alpha = match[4] === undefined ? 1 : Number(match[4]);
          if (alpha < 0.95) return false;

          const [, red, green, blue] = match.map(Number);
          return red >= 245 && green >= 245 && blue >= 245;
        })
        .slice(0, 10)
        .map((element) => ({
          className: element.className.toString(),
          tagName: element.tagName,
          text: element.textContent?.trim().slice(0, 80) ?? '',
        }));
    });

    expect(lightSurfaces).toEqual([]);
  });
}
