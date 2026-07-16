import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// This vitest project lives OUTSIDE frontend/ so upstream's package.json stays untouched.
// It imports OUR shell from frontend/src/valueos via the same "@" alias the app uses.
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '../..');
const frontendSrc = path.resolve(repoRoot, 'frontend/src');

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': frontendSrc },
    // Ensure a single React instance even though components live outside this root.
    dedupe: ['react', 'react-dom'],
  },
  server: {
    // Allow importing our shell files from frontend/src (outside this project root).
    fs: { allow: [repoRoot] },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./setup.ts'],
    include: ['**/*.test.ts', '**/*.test.tsx'],
  },
});
