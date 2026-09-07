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
  'builtin-ai-models.ts'
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

const {
  getFirstSelectableBuiltInModelName,
  isSelectableBuiltInModelStatus,
} = loadTsModule(modulePath);

assert.equal(isSelectableBuiltInModelStatus('available'), true);
assert.equal(isSelectableBuiltInModelStatus('not_downloaded'), true);
assert.equal(isSelectableBuiltInModelStatus('not_downloaded', true), false);
assert.equal(isSelectableBuiltInModelStatus('downloading'), false);
assert.equal(isSelectableBuiltInModelStatus('corrupted'), false);
assert.equal(isSelectableBuiltInModelStatus('error'), false);

assert.equal(
  getFirstSelectableBuiltInModelName([
    { name: 'gemma4:e2b', status: { type: 'not_downloaded' } },
    { name: 'qwen3.5:2b', status: { type: 'available' } },
  ]),
  'qwen3.5:2b',
  'available model should remain the default when one exists'
);

assert.equal(
  getFirstSelectableBuiltInModelName([
    { name: 'gemma4:e2b', status: { type: 'not_downloaded' } },
    { name: 'gemma4:e4b', status: { type: 'not_downloaded' } },
  ]),
  'gemma4:e2b',
  'not-downloaded models should still be selectable for download/save flows'
);

assert.equal(
  getFirstSelectableBuiltInModelName([
    { name: 'gemma4:e2b', status: { type: 'corrupted' } },
    { name: 'gemma4:e4b', status: { type: 'error' } },
  ]),
  '',
  'error states should not be auto-selected'
);
