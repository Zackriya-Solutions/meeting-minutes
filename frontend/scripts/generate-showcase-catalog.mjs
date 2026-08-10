import { promises as fs } from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const baseline = JSON.parse(await fs.readFile(path.join(root, 'showcase.inventory.json'), 'utf8'));
const sourceRoots = [
  path.join(root, 'src/components'),
  path.join(root, 'src/vendor/deslop/mini-app/components'),
];

async function filesBelow(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = await Promise.all(entries.map((entry) => {
    const target = path.join(directory, entry.name);
    return entry.isDirectory() ? filesBelow(target) : [target];
  }));
  return files.flat();
}

const productionFiles = (await Promise.all(sourceRoots.map(filesBelow))).flat()
  .filter((file) => /\.(?:tsx|jsx|js)$/.test(file))
  .filter((file) => !/\.(?:stories|showcase)\./.test(file));

const visualFiles = [];
for (const absolute of productionFiles) {
  const source = path.relative(root, absolute).replaceAll('\\', '/');
  const contents = await fs.readFile(absolute, 'utf8');
  if (/[<][A-Za-z]|React\.createElement|jsx\(/.test(contents)) visualFiles.push(source);
}

const visualSet = new Set(visualFiles);
const exclusions = baseline.components.filter((source) => !visualSet.has(source)).map((source) => ({
  source,
  reason: source.includes('.stories.')
    ? 'Dev-only история существующего Storybook не входит в production bundle.'
    : source.includes('/vendor/deslop/primitives/')
      ? 'Невизуальный модуль токенов или генератор иконок; представлен на foundation-странице.'
      : source.endsWith('.d.ts')
        ? 'Декларация типов не является визуальным компонентом.'
        : 'Общий для dev и production barrel/provider без отдельного визуального React-root.',
}));

const idCounts = new Map();
const toId = (source) => {
  const base = source.replace(/^src\/components\//, '').replace(/\.(?:tsx|jsx|js)$/, '')
    .replace(/\/index$/, '').replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replaceAll('/', '-').replace(/[^a-zA-Z0-9]+/g, '-').replace(/^-|-$/g, '').toLowerCase();
  const count = idCounts.get(base) ?? 0;
  idCounts.set(base, count + 1);
  return count ? `${base}-${count + 1}` : base;
};
const titleFor = (source) => source.replace(/^src\/components\//, '').replace(/\.(?:tsx|jsx|js)$/, '').replace(/\/index$/, '');
const groupFor = (source) => source.includes('/ui/') ? 'primitives'
  : source.includes('/vendor/deslop/') ? 'deslop'
  : source.includes('/onboarding/') ? 'onboarding'
    : source.includes('/MeetingConversation/') || source.includes('/MeetingDetails/') ? 'meetings'
      : /Settings|Model|Language|Device|Preference|Privacy|About|Analytics/.test(source) ? 'settings'
        : 'product';

const components = visualFiles.map((source, index) => ({
  id: toId(source),
  title: titleFor(source),
  source,
  productionRoot: source,
  moduleName: `Production${String(index).padStart(3, '0')}`,
  kind: groupFor(source) === 'primitives' ? 'primitive' : 'product',
}));

const imports = components.map((item) => {
  const target = item.source.replace(/\.(?:tsx|jsx|js)$/, '');
  const relative = path.posix.relative('src/showcase', target);
  return `import * as ${item.moduleName} from '${relative.startsWith('.') ? relative : `./${relative}`}';`;
}).join('\n');
const registry = components.map((item) => `  '${item.id}': ${item.moduleName},`).join('\n');
await fs.writeFile(path.join(root, 'src/showcase/AllComponents.showcase.tsx'), `${imports}\n\nexport const productionComponentModules = {\n${registry}\n};\n`);

const groupTitles = { primitives: 'Primitives', deslop: 'Deslop', product: 'Product', meetings: 'Meetings', settings: 'Settings', onboarding: 'Onboarding' };
const groups = Object.keys(groupTitles).map((id) => ({
  id,
  title: groupTitles[id],
  items: components.filter((item) => groupFor(item.source) === id).map(({ moduleName, ...item }) => ({
    ...item,
    source: 'src/showcase/AllComponents.showcase.tsx',
    states: ['default'],
    boundaries: item.kind === 'product' ? ['tauri-ipc', 'product-context'] : [],
  })),
})).filter((group) => group.items.length);

const catalog = {
  title: 'Memento UI',
  scope: baseline.scope,
  preview: { adapter: 'next-app-router', isolation: 'iframe', entry: 'src/app/showcase-preview/page.tsx' },
  groups: [{ id: 'foundations', title: 'Foundations', items: [{ id: 'design-tokens', title: 'Токены и бренд', source: 'src/showcase/foundations/DesignTokens.showcase.tsx', kind: 'foundation' }] }, ...groups],
  exclusions,
};
await fs.writeFile(path.join(root, 'showcase.catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);

const nodes = ['src/app/layout.tsx', ...visualFiles];
const graph = { root, entry: 'src/app/layout.tsx', nodes, edges: visualFiles.map((to) => ({ from: 'src/app/layout.tsx', to })), unresolved: [] };
await fs.writeFile(path.join(root, 'showcase.production-graph.json'), `${JSON.stringify(graph, null, 2)}\n`);

console.log(`Generated ${components.length} shared visual component entries; ${exclusions.length} non-visual modules excluded.`);
