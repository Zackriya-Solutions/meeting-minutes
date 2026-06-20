export type ThemeMode = 'light' | 'dark' | 'system';
export type EffectiveTheme = 'light' | 'dark';

export const THEME_STORAGE_KEY = 'themeMode';

export function parseThemeMode(value: string | null | undefined): ThemeMode {
  return value === 'light' || value === 'dark' || value === 'system'
    ? value
    : 'system';
}

export function resolveEffectiveTheme(
  mode: ThemeMode,
  prefersDark: boolean,
): EffectiveTheme {
  if (mode === 'system') {
    return prefersDark ? 'dark' : 'light';
  }

  return mode;
}

export function applyEffectiveTheme(theme: EffectiveTheme) {
  if (typeof document === 'undefined') return;

  const root = document.documentElement;
  root.classList.toggle('dark', theme === 'dark');
  root.style.colorScheme = theme;
}
