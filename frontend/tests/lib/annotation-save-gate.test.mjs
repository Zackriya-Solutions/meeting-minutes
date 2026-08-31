import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const modulePath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..', '..', 'src', 'lib', 'annotation-save-gate.ts'
);
const source = fs.readFileSync(modulePath, 'utf8');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;
const cjsModule = { exports: {} };
vm.runInNewContext(compiled, { exports: cjsModule.exports, module: cjsModule });
const { AnnotationSaveGate } = cjsModule.exports;

let releaseWrite;
const pendingWrite = new Promise(resolve => { releaseWrite = resolve; });
const gate = new AnnotationSaveGate();
gate.track(pendingWrite);
gate.begin();
assert.equal(gate.canWrite(), false, 'annotations must be locked at the save boundary');

let waited = false;
const wait = gate.wait().then(() => { waited = true; });
await Promise.resolve();
assert.equal(waited, false, 'save must await an in-flight IndexedDB annotation write');
releaseWrite();
await wait;
gate.finish();
assert.equal(gate.canWrite(), true, 'a failed/retried save can unlock annotation input');

console.log('annotation-save-gate tests passed');
