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
  'sidebar-tree.ts'
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

const { buildMeetingTree } = loadTsModule(modulePath);

const UNFILED = 'unfiled';

const childrenOf = (item) => [...item.children];

const folderNamed = (tree, id) =>
  childrenOf(tree[0]).find(child => child.id === id);

const meetingIdsIn = (folder) => childrenOf(folder).map(child => child.id);

const meetings = [
  { id: 'm-1', title: 'Weekly sync', project_folder_id: null, tags: ['roadmap'] },
  { id: 'm-2', title: 'Standup', project_folder_id: 'project-1', tags: [] },
  { id: 'm-3', title: 'Design review', project_folder_id: 'project-1', tags: [] },
];
const folders = [{ id: 'project-1', name: 'Mobile app' }];

const tree = buildMeetingTree(meetings, folders, UNFILED);

assert.deepEqual(
  meetingIdsIn(folderNamed(tree, UNFILED)),
  ['m-1'],
  'meetings with no folder belong to Unfiled'
);

assert.deepEqual(
  meetingIdsIn(folderNamed(tree, 'project-1')),
  ['m-2', 'm-3'],
  'meetings group under their own project folder in list order'
);

assert.deepEqual(
  childrenOf(folderNamed(tree, 'project-1')).map(child => child.project_folder_id),
  ['project-1', 'project-1'],
  'each meeting item carries its folder id so the move control needs no lookup'
);

assert.deepEqual(
  [...childrenOf(folderNamed(tree, UNFILED))[0].tags],
  ['roadmap'],
  'tag chips survive the tree build'
);

const orphanTree = buildMeetingTree(meetings, [], UNFILED);

assert.deepEqual(
  meetingIdsIn(folderNamed(orphanTree, UNFILED)),
  ['m-1', 'm-2', 'm-3'],
  'meetings whose folder is unknown fall back to Unfiled instead of vanishing'
);

assert.deepEqual(
  childrenOf(buildMeetingTree([], folders, UNFILED)[0]).map(child => child.id),
  [UNFILED, 'project-1'],
  'empty folders still render so the user can move meetings into them'
);

console.log('sidebar-tree tests passed');
