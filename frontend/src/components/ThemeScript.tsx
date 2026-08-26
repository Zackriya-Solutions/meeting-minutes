import {
  applyEffectiveTheme,
  parseThemeMode,
  resolveEffectiveTheme,
  THEME_STORAGE_KEY,
} from '@/lib/theme';

// The pre-hydration boot script is composed from the shared theme functions so
// there is a single source of truth for mode parsing/resolution/application.
// Only closure-free functions can be serialized here: anything referencing
// module-scope identifiers would break once the bundle is minified.
const themeScript = `
(() => {
  const parse = ${parseThemeMode.toString()};
  const resolve = ${resolveEffectiveTheme.toString()};
  const apply = ${applyEffectiveTheme.toString()};

  let storedMode = null;
  try {
    storedMode = window.localStorage.getItem(${JSON.stringify(THEME_STORAGE_KEY)});
  } catch {}

  apply(resolve(parse(storedMode), window.matchMedia('(prefers-color-scheme: dark)').matches));
})();
`;

export function ThemeScript() {
  return <script dangerouslySetInnerHTML={{ __html: themeScript }} />;
}
