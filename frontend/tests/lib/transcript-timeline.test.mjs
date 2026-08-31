import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'transcript-timeline.ts'
);

const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, { exports: cjsModule.exports, module: cjsModule });
const { mergeTranscriptTimeline } = cjsModule.exports;

const segments = [
  { id: 's-2', timestamp: 20, text: 'later' },
  { id: 's-1', timestamp: 5, text: 'first' },
];
const annotations = [
  { id: 'a-1', type: 'note', anchorTime: 10, createdAt: 1, text: 'remember' },
  { id: 'a-2', type: 'bookmark', anchorTime: 5, createdAt: 2 },
];

assert.deepEqual(
  Array.from(mergeTranscriptTimeline(segments, annotations), item => `${item.kind}:${item.time}`),
  ['segment:5', 'annotation:5', 'annotation:10', 'segment:20'],
  'segments and annotations are merged in chronological order with stable ties'
);

assert.equal(
  Array.from(mergeTranscriptTimeline(segments, []), item => item.segment?.id).join(','),
  's-1,s-2',
  'segment order is chronological even when input is not sorted'
);

console.log('transcript-timeline tests passed');
