import { THEME_STORAGE_KEY } from '@/lib/theme';

const themeScript = `
(() => {
  let storedMode = null;

  try {
    storedMode = window.localStorage.getItem(${JSON.stringify(THEME_STORAGE_KEY)});
  } catch {}

  const mode =
    storedMode === 'light' || storedMode === 'dark' || storedMode === 'system'
      ? storedMode
      : 'system';
  const isDark =
    mode === 'dark' ||
    (mode === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  const root = document.documentElement;

  root.classList.toggle('dark', isDark);
  root.style.colorScheme = isDark ? 'dark' : 'light';
})();
`;

export function ThemeScript() {
  return <script dangerouslySetInnerHTML={{ __html: themeScript }} />;
}
