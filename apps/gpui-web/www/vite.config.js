import { defineConfig } from 'vite';
import { resolve } from 'node:path';

const repoRoot = resolve(import.meta.dirname, '../../..');

// The page and gxserver are different origins; gxserver already allows loopback origins through CORS, so nothing is proxied except the token bootstrap that `ghostex web` owns.
export default defineConfig({
  root: import.meta.dirname,
  // Same alias as the rest of the repo: `@/` is the repo root.
  resolve: { alias: { '@': repoRoot } },
  server: {
    port: 4174,
    strictPort: true,
    fs: { allow: [repoRoot] },
    proxy: {
      // `ghostex web` only answers its own origin, so the proxied request carries that origin. The page's
      // x-ghostex-web-bootstrap header (the run's key from the `#bootstrap=` link `ghostex web` prints)
      // passes through unchanged; without it the bootstrap is refused.
      '/api/webBootstrap': {
        target: 'http://127.0.0.1:4173',
        changeOrigin: true,
        headers: { origin: 'http://127.0.0.1:4173' },
      },
    },
  },
  build: { target: 'esnext', outDir: 'dist', emptyOutDir: true },
});
