import {
  expect,
  expectNoHorizontalOverflow,
  test,
} from './fixtures/tauri-fixtures';

for (const mode of ['light', 'dark'] as const) {
  test(`home shell renders semantic surfaces in ${mode} mode`, async ({
    page,
  }) => {
    await page.addInitScript((value) => {
      localStorage.setItem('themeMode', value);
    }, mode);

    await page.goto('/');
    const toggle = page.getByRole('button', { name: 'Expand sidebar' });
    await expect(toggle).toBeVisible();
    await toggle.click();

    await expect(page.getByText('Theme Test Meeting', { exact: true })).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Start Recording' }),
    ).toBeVisible();
    await expect(
      page.getByPlaceholder('Search meeting content...'),
    ).toBeVisible();

    const sidebar = page.getByTestId('app-sidebar');
    await expect(sidebar).toHaveCSS(
      'background-color',
      mode === 'dark' ? 'rgb(18, 18, 18)' : 'rgb(255, 255, 255)',
    );
    await expect(sidebar).toHaveCSS(
      'border-right-color',
      mode === 'dark' ? 'rgb(38, 38, 38)' : 'rgb(229, 229, 229)',
    );

    await expectNoHorizontalOverflow(page);
    await expect(sidebar).toHaveScreenshot(`home-${mode}.png`);
  });
}
