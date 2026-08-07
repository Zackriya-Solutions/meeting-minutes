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
  getDownloadTotalMb,
  getSummaryModelSizeLabel,
  getSummaryModelSizeMb,
  resolveOnboardingSummaryModelStatus,
} = loadTsModule(modulePath);

assert.equal(
  JSON.stringify(resolveOnboardingSummaryModelStatus({
    selectedModel: 'gemma4:e4b',
    recommendedModel: 'gemma4:e4b',
    selectedModelReady: false,
  })),
  JSON.stringify({
    selectedSummaryModel: 'gemma4:e4b',
    summaryModelDownloaded: false,
  }),
  'another downloaded model must not make an undownloaded selected model ready'
);

assert.equal(
  JSON.stringify(resolveOnboardingSummaryModelStatus({
    selectedModel: 'gemma4:e2b',
    recommendedModel: 'gemma4:e4b',
    selectedModelReady: true,
  })),
  JSON.stringify({
    selectedSummaryModel: 'gemma4:e2b',
    summaryModelDownloaded: true,
  }),
  'explicit selected model should win over a different recommendation'
);

assert.equal(
  JSON.stringify(resolveOnboardingSummaryModelStatus({
    selectedModel: '',
    recommendedModel: 'gemma4:e2b',
    selectedModelReady: true,
  })),
  JSON.stringify({
    selectedSummaryModel: 'gemma4:e2b',
    summaryModelDownloaded: true,
  }),
  'recommended model should become the selected model when no model is selected yet'
);

// Sizes cover weights + audio projector, so the progress bar does not stall at 100%.
assert.equal(getSummaryModelSizeMb('gemma4:e2b'), 3651);
assert.equal(getSummaryModelSizeMb('gemma4:e4b'), 5324);
// Retired families must not linger in the size table.
assert.equal(getSummaryModelSizeMb('qwen3.5:4b'), 0);
assert.equal(getSummaryModelSizeMb('gemma3:1b'), 0);
assert.equal(getSummaryModelSizeMb('unknown:model'), 0);

assert.equal(getSummaryModelSizeLabel('gemma4:e2b'), '~3.6 GiB');
assert.equal(getSummaryModelSizeLabel('gemma4:e4b'), '~5.2 GiB');
assert.equal(getSummaryModelSizeLabel('unknown:model'), '');

assert.equal(getDownloadTotalMb(0, 'gemma4:e4b'), 5324);
assert.equal(getDownloadTotalMb(undefined, 'gemma4:e2b'), 3651);
assert.equal(getDownloadTotalMb(512, 'gemma4:e4b'), 512);
