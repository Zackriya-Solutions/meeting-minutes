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
  'onboarding-summary-model.ts'
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
  DEFAULT_SUMMARY_MODEL,
  OFFERED_SUMMARY_MODELS,
  getDownloadTotalMb,
  getSummaryModelSizeLabel,
  getSummaryModelSizeMb,
  isOfferedSummaryModel,
  normalizeSummaryModel,
} = loadTsModule(modulePath);

// Summarization offers exactly two tiers. Anything else — a retired DeepSeek alias, or a
// local model saved by an older build — must resolve to the default rather than reaching
// the gateway, which would reject it.
assert.equal(
  JSON.stringify(OFFERED_SUMMARY_MODELS),
  JSON.stringify(['deepseek-v4-pro', 'deepseek-v4-flash'])
);
assert.equal(DEFAULT_SUMMARY_MODEL, 'deepseek-v4-pro');

assert.equal(isOfferedSummaryModel('deepseek-v4-flash'), true);
assert.equal(isOfferedSummaryModel('deepseek-v4-pro'), true);
assert.equal(isOfferedSummaryModel('deepseek-chat'), false);
assert.equal(isOfferedSummaryModel('qwen3.5:4b'), false);
assert.equal(isOfferedSummaryModel(''), false);
assert.equal(isOfferedSummaryModel(null), false);

assert.equal(normalizeSummaryModel('deepseek-v4-flash'), 'deepseek-v4-flash');
assert.equal(normalizeSummaryModel(' deepseek-v4-flash '), 'deepseek-v4-flash');
assert.equal(normalizeSummaryModel('deepseek-reasoner'), 'deepseek-v4-pro');
assert.equal(normalizeSummaryModel('gemma3:1b'), 'deepseek-v4-pro');
assert.equal(normalizeSummaryModel(undefined), 'deepseek-v4-pro');

assert.equal(getSummaryModelSizeMb('qwen3.5:2b'), 1221);
assert.equal(getSummaryModelSizeMb('qwen3.5:4b'), 2614);
assert.equal(getSummaryModelSizeMb('gemma3:1b'), 1019);
assert.equal(getSummaryModelSizeMb('unknown:model'), 0);

assert.equal(getSummaryModelSizeLabel('qwen3.5:2b'), '~1.2 GiB');
assert.equal(getSummaryModelSizeLabel('qwen3.5:4b'), '~2.6 GiB');
assert.equal(getSummaryModelSizeLabel('unknown:model'), '');

assert.equal(getDownloadTotalMb(0, 'qwen3.5:4b'), 2614);
assert.equal(getDownloadTotalMb(undefined, 'qwen3.5:2b'), 1221);
assert.equal(getDownloadTotalMb(512, 'qwen3.5:4b'), 512);
