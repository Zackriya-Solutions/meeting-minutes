// The live transcription worker emitted `transcript-partial` for the whole
// streaming path and nothing in the frontend listened. Nothing threw — the
// uncommitted text was just dropped, so a streaming model looked exactly like a
// batch one. Same shape as download-event-names.test.mjs, other direction:
// every event stream_worker.rs emits must have a frontend listener.
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

const worker = fs.readFileSync(
  path.join(root, 'src-tauri', 'src', 'audio', 'transcription', 'stream_worker.rs'),
  'utf8'
);
const ts = readAll(path.join(root, 'src'), ['.ts', '.tsx']).join('\n');

// `\s*` because the payload often pushes the name onto its own line.
const emitted = new Set(
  [...worker.matchAll(/\.emit\(\s*"([a-z0-9-]+)"/g)].map((m) => m[1])
);

assert.ok(emitted.size > 0, 'expected stream_worker.rs to emit events');

for (const name of emitted) {
  assert.ok(
    new RegExp(`listen(<[^>]*>)?\\(\\s*['"\`]${name}['"\`]`).test(ts),
    `stream_worker.rs emits "${name}" but no frontend code listens for it`
  );
}

console.log(`ok - ${emitted.size} live transcription events wired:`, [...emitted].sort().join(', '));
