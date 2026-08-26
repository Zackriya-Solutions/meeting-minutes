import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';

const roots = ['src'];
const forbiddenPatterns = [
  /\bbg-white\b/g,
  /\bbg-black(?:\/\d+)?\b/g,
  /\bbg-opacity-\d+\b/g,
  /\btext-white\b/g,
  /\b(?:bg|border|border-[trblxy]|divide|placeholder|ring|text)-(?:slate|gray|zinc|neutral|stone)-\d+\b/g,
  /\b(?:hover:)?(?:bg|border|ring|text)-[a-z-]+\/\d+\/\d+\b/g,
  /#(?:fff(?:fff)?|f9fafb|f3f4f6|e5e7eb|d1d5db|9ca3af|6b7280|4b5563|374151|1f2937|111827|3b82f6)\b/gi,
];
const allowlist = new Set();
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
        const key = `${file}:${match[0]}`;
        if (!allowlist.has(key)) failures.push(key);
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
