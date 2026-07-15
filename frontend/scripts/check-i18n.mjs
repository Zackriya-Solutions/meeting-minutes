import { readFile, readdir } from 'node:fs/promises';

const localeDirectory = new URL('../src/i18n/locales/', import.meta.url);

const readLocale = async (name) =>
  JSON.parse(await readFile(new URL(`${name}.json`, localeDirectory), 'utf8'));

const flatten = (value, prefix = '') =>
  Object.entries(value).flatMap(([key, child]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return child && typeof child === 'object'
      ? flatten(child, path)
      : [[path, child]];
  });

const placeholders = (value) =>
  [...String(value).matchAll(/{{\s*([^},\s]+).*?}}/g)]
    .map((match) => match[1])
    .sort();

const english = new Map(flatten(await readLocale('en')));
const russian = new Map(flatten(await readLocale('ru')));
const missing = [...english.keys()].filter((key) => !russian.has(key));
const extra = [...russian.keys()].filter((key) => !english.has(key));
const placeholderErrors = [...english.entries()].flatMap(([key, value]) => {
  if (!russian.has(key)) return [];
  const expected = placeholders(value);
  const actual = placeholders(russian.get(key));
  return expected.join('\0') === actual.join('\0')
    ? []
    : [`${key}: expected [${expected.join(', ')}], found [${actual.join(', ')}]`];
});

const sourceDirectory = new URL('../src/', import.meta.url);
const sourceFiles = async (directory) => {
  const entries = await readdir(directory, { withFileTypes: true });
  return (await Promise.all(entries.map(async (entry) => {
    const url = new URL(entry.name, directory.href.endsWith('/') ? directory : new URL(`${directory.href}/`));
    if (entry.isDirectory()) return sourceFiles(new URL(`${url.href}/`));
    return /\.(?:ts|tsx)$/.test(entry.name) ? [url] : [];
  }))).flat();
};

const missingUsedKeys = [];
for (const file of await sourceFiles(sourceDirectory)) {
  const source = await readFile(file, 'utf8');
  const keyPattern = /(?:\bt|\bi18n\.t)\(\s*(['"])([A-Za-z0-9_.-]+)\1/g;
  for (const match of source.matchAll(keyPattern)) {
    if (!english.has(match[2])) {
      missingUsedKeys.push(`${decodeURIComponent(file.pathname)}: ${match[2]}`);
    }
  }
}

if (missing.length || extra.length || placeholderErrors.length || missingUsedKeys.length) {
  if (missing.length) console.error(`Missing Russian keys:\n${missing.join('\n')}`);
  if (extra.length) console.error(`Unexpected Russian keys:\n${extra.join('\n')}`);
  if (placeholderErrors.length) {
    console.error(`Placeholder mismatches:\n${placeholderErrors.join('\n')}`);
  }
  if (missingUsedKeys.length) {
    console.error(`Unknown translation keys used in source:\n${missingUsedKeys.join('\n')}`);
  }
  process.exitCode = 1;
} else {
  console.log(`Russian locale matches all ${english.size} English keys and placeholders.`);
}
