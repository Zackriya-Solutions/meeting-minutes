// sortModels drives the model picker's sort control. The ranks are hand-written
// maps over the catalog's tier strings, so a renamed tier would silently sort to
// NaN and scramble the list — these assert the ordering and the stable tiebreak.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const source = fs.readFileSync(path.join(root, 'src', 'lib', 'transcribe.ts'), 'utf8');

const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
}).outputText;

const module = { exports: {} };
vm.runInNewContext(compiled, {
  exports: module.exports,
  module,
  // The file imports @tauri-apps/api/core for the command wrappers; the sort
  // helpers do not touch it, so a stub keeps this a pure unit test.
  require: () => ({ invoke: () => {} }),
});
const { sortModels, MODEL_SORT_LABELS } = module.exports;

const m = (name, accuracy, speed, size_mb) => ({ name, accuracy, speed, size_mb });

// Catalog order is deliberately not quality/speed/size order, so each sort has
// something to actually do.
const models = [
  m('a', 'Decent', 'Slow', 900),
  m('b', 'High', 'Very Fast', 700),
  m('c', 'Good', 'Medium', 200),
  m('d', 'High', 'Fast', 400),
];

// Joined rather than deep-equalled: the module runs in a vm realm, so its arrays
// fail deepStrictEqual's prototype check even when the contents match.
const names = (sort) => sortModels(models, sort).map((x) => x.name).join(',');

assert.equal(names('catalog'), 'a,b,c,d', 'catalog order is untouched');
assert.equal(names('quality'), 'b,d,c,a', 'High > Good > Decent');
assert.equal(names('speed'), 'b,d,c,a', 'Very Fast > Fast > Medium > Slow');
assert.equal(names('size'), 'c,d,b,a', 'smallest download first');

// Ties fall back to catalog order rather than shuffling between renders.
const tied = [m('x', 'High', 'Fast', 100), m('y', 'High', 'Fast', 100)];
assert.equal(sortModels(tied, 'quality').map((x) => x.name).join(','), 'x,y');

// Sorting must not mutate the caller's array — the component sorts a filtered
// list derived from state on every render.
sortModels(models, 'quality');
assert.equal(models.map((x) => x.name).join(','), 'a,b,c,d', 'input is not mutated');

// Every sort the UI offers is one sortModels understands.
for (const key of Object.keys(MODEL_SORT_LABELS)) {
  assert.equal(sortModels(models, key).length, models.length, `${key} is a real sort`);
}

console.log('ok - sortModels:', Object.keys(MODEL_SORT_LABELS).join(', '));
