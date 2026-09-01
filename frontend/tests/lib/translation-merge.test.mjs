import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'translation.ts'
);
const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, { exports: cjsModule.exports, module: cjsModule });
const { mergeTranslationUpdate } = cjsModule.exports;

const transcripts = [
  { id: 'seg-1', sequence_id: 1, text: 'Hello', translation: undefined },
  { id: 'seg-2', sequence_id: 2, text: 'Goodbye', translation: undefined },
];

const merged = mergeTranslationUpdate(transcripts, {
    sequence_id: 2,
    translation: 'Tchau',
    target_language: 'pt-BR',
    status: 'ready',
  });
assert.equal(merged.length, 2, 'translation merge must preserve segment count');
assert.equal(merged[0], transcripts[0], 'translation merge must preserve unrelated segments');
assert.equal(merged[1].sequence_id, 2);
assert.equal(merged[1].translation, 'Tchau');
assert.equal(merged[1].translation_target_language, 'pt-BR');
assert.equal(merged[1].translation_status, 'ready');

const pending = mergeTranslationUpdate(transcripts, {
  sequence_id: 1,
  translation: null,
  target_language: 'pt-BR',
  status: 'pending',
});
assert.equal(pending[0].text, 'Hello', 'pending translation must leave the original intact');
assert.equal(pending[0].translation_status, 'pending');

const errored = mergeTranslationUpdate(pending, {
  sequence_id: 1,
  translation: null,
  target_language: 'fr',
  status: 'error',
  error: 'mock engine unavailable',
});
assert.equal(errored[0].text, 'Hello', 'engine errors must leave the original intact');
assert.equal(errored[0].translation_error, 'mock engine unavailable');
assert.equal(errored[0].translation_target_language, 'fr', 'a changed target applies only to this new update');

assert.equal(
  mergeTranslationUpdate(transcripts, {
    sequence_id: 99,
    translation: null,
    target_language: 'pt-BR',
    status: 'error',
    error: 'translator unavailable',
  }), transcripts,
  'unknown sequence updates must not create transcript rows'
);

console.log('translation merge tests passed');
