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
  'sidebar-search.ts'
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

const { filterSidebarItems, isFolderExpanded } = loadTsModule(modulePath);

const tree = () => [
  {
    id: 'meetings',
    title: 'Meeting Notes',
    type: 'folder',
    children: [
      {
        id: 'unfiled',
        title: 'Unfiled',
        type: 'folder',
        children: [{ id: 'm-1', title: 'Weekly sync', type: 'file', tags: ['roadmap'] }],
      },
      {
        id: 'project-1',
        title: 'Mobile app',
        type: 'folder',
        children: [
          { id: 'm-2', title: 'Standup', type: 'file', tags: [] },
          { id: 'm-3', title: 'Design review', type: 'file', tags: [] },
        ],
      },
    ],
  },
];

const meetingIdsIn = (items) =>
  items.flatMap(item => item.type === 'file' ? [item.id] : meetingIdsIn(item.children ?? []));

const foldersIn = (items) =>
  items.flatMap(item => item.type === 'folder' ? [item.id, ...foldersIn(item.children ?? [])] : []);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'meeting', new Set())),
  [],
  'the root container title must not short-circuit search and list every meeting'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'notes', new Set())),
  [],
  'a query matching only the root title must not bypass meeting-level filtering'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'mobile', new Set())),
  ['m-2', 'm-3'],
  'a project folder matched by its own name keeps its full child list'
);

assert.deepEqual(
  foldersIn(filterSidebarItems(tree(), 'mobile', new Set())),
  ['meetings', 'project-1'],
  'a folder-name search drops sibling folders that match nothing'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'standup', new Set())),
  ['m-2'],
  'a meeting title match keeps only that meeting'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'roadmap', new Set())),
  ['m-1'],
  'a tag match keeps the tagged meeting'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), 'budget', new Set(['m-3']))),
  ['m-3'],
  'a transcript search hit keeps its meeting even when the title does not match'
);

assert.deepEqual(
  meetingIdsIn(filterSidebarItems(tree(), '   ', new Set())),
  ['m-1', 'm-2', 'm-3'],
  'a blank query leaves the tree untouched'
);

assert.deepEqual(
  foldersIn(filterSidebarItems(tree(), 'nothing-matches', new Set())),
  ['meetings'],
  'the root container survives an empty result so the sidebar keeps its header'
);

assert.equal(
  isFolderExpanded('project-1', new Set(), 'standup'),
  true,
  'a collapsed folder must open while a search is active so its matches are visible'
);

assert.equal(
  isFolderExpanded('project-1', new Set(), '   '),
  false,
  'a whitespace-only query is not an active search'
);

assert.equal(
  isFolderExpanded('project-1', new Set(), ''),
  false,
  'a collapsed folder stays collapsed with no search'
);

assert.equal(
  isFolderExpanded('project-1', new Set(['project-1']), ''),
  true,
  'an explicitly expanded folder stays open with no search'
);

console.log('sidebar-search tests passed');
