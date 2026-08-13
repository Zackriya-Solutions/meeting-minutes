const path = require('path');
const { PHASE_DEVELOPMENT_SERVER } = require('next/constants');
const tiptapPmResolveBase = path.dirname(require.resolve('@tiptap/pm/model'));
const resolveFromTiptapPm = (pkg) =>
  require.resolve(pkg, { paths: [tiptapPmResolveBase] });

/** @param {string} phase */
module.exports = (phase) => ({
  distDir: phase === PHASE_DEVELOPMENT_SERVER ? '.next-dev' : '.next',

  // The component showcase is a dev-only surface, and its catalog imports every
  // production component by namespace — so a `notFound()` guard inside the route hides
  // the page but still drags the whole component graph into the export (it was the
  // heaviest route in the bundle at 684 kB). Registering the extra extension only for
  // `next dev` means `page.showcase.tsx` is a route while developing and just a
  // colocated file in a production build.
  pageExtensions:
    phase === PHASE_DEVELOPMENT_SERVER
      ? ['showcase.tsx', 'tsx', 'ts', 'jsx', 'js']
      : ['tsx', 'ts', 'jsx', 'js'],
  reactStrictMode: false, // Disabled for BlockNote compatibility
  output: 'export',
  images: {
    unoptimized: true,
  },
  // Add basePath configuration
  basePath: '',
  assetPrefix: '/',

  experimental: {
    // These are barrel packages: one named import pulls the whole index into the
    // module graph. lucide-react alone is ~2000 files and date-fns ~2200, and in dev
    // webpack holds every one of those modules plus its source map in memory for the
    // life of the server. Rewriting the imports to direct paths keeps the dev
    // compiler's heap proportional to what the app actually uses.
    optimizePackageImports: [
      'lucide-react',
      'date-fns',
      'framer-motion',
      '@tanstack/react-virtual',
    ],
  },

  // Add webpack configuration for Tauri
  webpack: (config, { isServer }) => {
    if (!isServer) {
      config.resolve.fallback = {
        ...config.resolve.fallback,
        fs: false,
        path: false,
        os: false,
      };

      // Keep ProseMirror single-instanced for BlockNote/Tiptap.
      config.resolve.alias = {
        ...config.resolve.alias,
        '@blocknote/core$': require.resolve('@blocknote/core'),
        '@blocknote/react$': require.resolve('@blocknote/react'),
        '@blocknote/shadcn$': require.resolve('@blocknote/shadcn'),
        'prosemirror-model': resolveFromTiptapPm('prosemirror-model'),
        'prosemirror-state': resolveFromTiptapPm('prosemirror-state'),
        'prosemirror-view': resolveFromTiptapPm('prosemirror-view'),
        'prosemirror-transform': resolveFromTiptapPm('prosemirror-transform'),
        'prosemirror-tables': resolveFromTiptapPm('prosemirror-tables'),
        'prosemirror-schema-list': resolveFromTiptapPm('prosemirror-schema-list'),
        'prosemirror-keymap': resolveFromTiptapPm('prosemirror-keymap'),
        'prosemirror-commands': resolveFromTiptapPm('prosemirror-commands'),
        'prosemirror-history': resolveFromTiptapPm('prosemirror-history'),
        'prosemirror-inputrules': resolveFromTiptapPm('prosemirror-inputrules'),
        'prosemirror-gapcursor': resolveFromTiptapPm('prosemirror-gapcursor'),
        'prosemirror-dropcursor': resolveFromTiptapPm('prosemirror-dropcursor'),
      };
    }
    return config;
  },
});
