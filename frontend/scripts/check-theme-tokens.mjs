import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Resolve scan roots relative to this script so the audit works from any cwd
// (repo root, frontend/, CI checkout, …).
const frontendDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const roots = [path.join(frontendDir, 'src')];

// Hardcoded colors are forbidden in app code: every surface must go through
// the semantic tokens in globals.css/tailwind.config.js so light and dark
// modes stay in sync. Raw palette utilities, white/black shortcuts, and the
// legacy hex values all bypass that layer.
const forbiddenPatterns = [
  /\bbg-white\b/g,
  /\bbg-black(?:\/\d+)?\b/g,
  /\btext-white\b/g,
  /\btext-black\b/g,
  /\bborder-white\b/g,
  /\bbg-opacity-\d+\b/g,
  /\b(?:hover:|focus:|dark:)*(?:bg|border|border-[trblxy]|divide|placeholder|ring|text)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+(?:\/\d+)?\b/g,
  /\b(?:hover:)?(?:bg|border|ring|text)-[a-z-]+\/\d+\/\d+\b/g,
  /#(?:fff(?:fff)?|f9fafb|f3f4f6|e5e7eb|d1d5db|9ca3af|6b7280|4b5563|374151|1f2937|111827|3b82f6)\b/gi,
];
const failures = [];

async function walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);

    if (entry.isDirectory()) {
      await walk(file);
      continue;
    }

    if (!entry.isFile() || !/\.(css|js|jsx|ts|tsx)$/.test(entry.name)) continue;

    const source = await readFile(file, 'utf8');
    for (const pattern of forbiddenPatterns) {
      for (const match of source.matchAll(pattern)) {
        failures.push(`${path.relative(frontendDir, file)}:${match[0]}`);
      }
    }
  }
}

for (const root of roots) {
  await walk(root);
}

if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
