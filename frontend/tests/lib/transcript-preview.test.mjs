import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'src',
  'lib',
  'transcript-preview.ts'
);
const require = createRequire(import.meta.url);

function loadTsModule(filePath) {
  const source = fs.readFileSync(filePath, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2020,
    },
  }).outputText;

  const module = { exports: {} };
  vm.runInNewContext(compiled, {
    exports: module.exports,
    module,
    require,
  });
  return module.exports;
}

const { reduceTranscriptPreview } = loadTsModule(modulePath);

const first = reduceTranscriptPreview(null, {
  text: '  live hypothesis  ',
  audio_start_time: 1.2,
  audio_end_time: 2.4,
});
assert.deepEqual(
  JSON.parse(JSON.stringify(first)),
  {
    text: 'live hypothesis',
    audio_start_time: 1.2,
    audio_end_time: 2.4,
  },
  'a preview event should replace the current preview and trim display whitespace'
);

const replacement = reduceTranscriptPreview(first, {
  text: 'corrected hypothesis',
  audio_start_time: 1.2,
  audio_end_time: 2.8,
});
assert.equal(replacement.text, 'corrected hypothesis');
assert.equal(replacement.audio_end_time, 2.8);

assert.equal(
  reduceTranscriptPreview(replacement, {
    text: '   ',
    audio_start_time: 0,
    audio_end_time: 0,
  }),
  null,
  'an empty event should clear the preview'
);

const confirmedTranscripts = [{ id: 'confirmed-1', text: 'persisted' }];
reduceTranscriptPreview(replacement, {
  text: 'ephemeral',
  audio_start_time: 3,
  audio_end_time: 4,
});
assert.deepEqual(
  confirmedTranscripts,
  [{ id: 'confirmed-1', text: 'persisted' }],
  'preview reduction must not modify persisted transcript arrays'
);
