import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

/*
 * CDXC:RepoRestructure 2026-08-22: source trees moved under packages/ and apps/,
 * and cross-tree imports are written as repo-root `@/...` specifiers. Vitest needs
 * the same `@` -> repository-root alias every vite/esbuild/Storybook config already
 * declares, otherwise suites that pull in shared UI fail to resolve those modules.
 */
const repoRoot = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  resolve: {
    alias: {
      '@': repoRoot,
      'bun:test': 'vitest',
      'vite-plus/test': 'vitest',
    },
  },
  test: {
    /*
     * Root test discovery is limited to Ghostex-owned suites. Imported apps,
     * vendored sources, generated output, dependencies, and build/cache trees
     * have their own test runners and must not be collected by root Vitest.
     */
    exclude: [
      '**/node_modules/**',
      '**/bower_components/**',
      '**/vendor/**',
      '**/.git/**',
      '**/.hg/**',
      '**/.svn/**',
      '**/dist/**',
      '**/build/**',
      '**/out/**',
      '**/coverage/**',
      '**/.cache/**',
      '**/.turbo/**',
      '**/.vite/**',
      '**/.zig-cache/**',
      '**/zig-out/**',
      '**/DerivedData/**',
      '**/target/**',
      '.dependencies/**',
      'apps/mobile/app/android/.gradle/**',
      'apps/mobile/app/android/**/build/**',
      'storybook-static/**',
      'tmp/**',
    ],
  },
});
