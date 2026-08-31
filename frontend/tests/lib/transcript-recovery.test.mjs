import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'transcript-recovery.ts'
);
const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, { exports: cjsModule.exports, module: cjsModule });
const { formatStoredAnnotations } = cjsModule.exports;

assert.deepEqual(
  JSON.parse(JSON.stringify(formatStoredAnnotations([{
    id: 'annotation-image',
    meetingId: 'meeting-crashed',
    type: 'image',
    anchorTime: 12.5,
    createdAt: '2026-08-31T10:00:00Z',
    imageData: [1, 2, 3],
    imageMime: 'image/png',
  }]))),
  [{
    id: 'annotation-image',
    type: 'image',
    anchorTime: 12.5,
    createdAt: '2026-08-31T10:00:00Z',
    imageData: [1, 2, 3],
    imageMime: 'image/png',
  }],
  'crash recovery must carry image annotations into the SQLite save payload'
);

console.log('transcript-recovery tests passed');
