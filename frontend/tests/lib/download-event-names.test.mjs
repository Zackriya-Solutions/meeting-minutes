// The onboarding download UI once listened for `parakeet-model-download-*`
// while Rust emitted `model-download-*`. Nothing threw — the progress bar just
// sat at 0% forever and Continue never enabled. This asserts every download
// event the frontend waits on is actually emitted by the Rust side.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

function readAll(dir, exts) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const full = path.join(dir, e.name);
    if (e.isDirectory()) return readAll(full, exts);
    return exts.some((x) => e.name.endsWith(x)) ? [fs.readFileSync(full, 'utf8')] : [];
  });
}

const ts = readAll(path.join(root, 'src'), ['.ts', '.tsx']).join('\n');
const rs = readAll(path.join(root, 'src-tauri', 'src'), ['.rs']).join('\n');

const listened = new Set(
  [...ts.matchAll(/listen(?:<[^>]*>)?\(\s*'([a-z0-9-]*download[a-z0-9-]*)'/g)].map((m) => m[1])
);

assert.ok(listened.size > 0, 'expected to find download event listeners');

for (const name of listened) {
  assert.ok(rs.includes(`"${name}"`), `frontend listens for "${name}" but no Rust code emits it`);
}

console.log(`ok - ${listened.size} download events wired:`, [...listened].sort().join(', '));
