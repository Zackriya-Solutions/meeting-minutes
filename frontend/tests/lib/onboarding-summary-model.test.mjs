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
    selectedModel: 'qwen3.5:4b',
    recommendedModel: 'qwen3.5:4b',
    selectedModelReady: false,
  })),
  JSON.stringify({
    selectedSummaryModel: 'qwen3.5:4b',
    summaryModelDownloaded: false,
  }),
  'legacy Gemma availability must not make an undownloaded selected Qwen model ready'
);

assert.equal(
  JSON.stringify(resolveOnboardingSummaryModelStatus({
    selectedModel: 'gemma3:1b',
    recommendedModel: 'qwen3.5:4b',
    selectedModelReady: true,
  })),
  JSON.stringify({
    selectedSummaryModel: 'gemma3:1b',
    summaryModelDownloaded: true,
  }),
  'explicit selected model should win over a different recommendation'
);

assert.equal(
  JSON.stringify(resolveOnboardingSummaryModelStatus({
    selectedModel: '',
    recommendedModel: 'qwen3.5:2b',
    selectedModelReady: true,
  })),
  JSON.stringify({
    selectedSummaryModel: 'qwen3.5:2b',
    summaryModelDownloaded: true,
  }),
  'recommended Qwen should become the selected model when no model is selected yet'
);

assert.equal(getSummaryModelSizeMb('qwen3.5:2b'), 1221);
assert.equal(getSummaryModelSizeMb('qwen3.5:4b'), 2614);
assert.equal(getSummaryModelSizeMb('gemma3:1b'), 1019);
assert.equal(getSummaryModelSizeMb('unknown:model'), 0);

assert.equal(getSummaryModelSizeLabel('qwen3.5:2b'), '~1.2 GB');
assert.equal(getSummaryModelSizeLabel('qwen3.5:4b'), '~2.6 GB');
assert.equal(getSummaryModelSizeLabel('unknown:model'), '');

assert.equal(getDownloadTotalMb(0, 'qwen3.5:4b'), 2614);
assert.equal(getDownloadTotalMb(undefined, 'qwen3.5:2b'), 1221);
assert.equal(getDownloadTotalMb(512, 'qwen3.5:4b'), 512);
