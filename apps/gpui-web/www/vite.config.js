import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

// CDXC:Collab 2026-11-08 DECISION:
// User (#495, Q1722): the guest page is pinned to each release. The share relay serves this build
// at `/s/<page id>/` (so its files are relative, `base: './'`), and the page checks its own code:
// index.html carries the script's integrity hash (one script: the WebAssembly loader is inlined
// into it), and the script carries the WebAssembly's SHA-256 (main.js `verifiedWasm`). The whole
// build's manifest is what gxserver pins (server/src/collab/page_pin.rs).
function pinnedPage() {
  let outDir = '';
  return {
    name: 'ghostex-pinned-page',
    apply: 'build',
    configResolved(config) {
      outDir = resolve(config.root, config.build.outDir);
    },
    // After every file is written, so nothing Vite does later can change the bytes hashed here.
    writeBundle(_options, bundle) {
      const files = Object.values(bundle);
      const wasm = files.filter((file) => file.fileName.endsWith('.wasm'));
      if (wasm.length !== 1) this.error(`expected one .wasm in the build, found ${wasm.length}`);
      const wasmHash = createHash('sha256').update(readFileSync(resolve(outDir, wasm[0].fileName))).digest('hex');
      const scripts = files.filter((file) => file.fileName.endsWith('.js'));
      const placeholder = '__GHOSTEX_WASM_SHA256__';
      let holders = 0;
      for (const script of scripts) {
        const path = resolve(outDir, script.fileName);
        const code = readFileSync(path, 'utf8');
        if (!code.includes(placeholder)) continue;
        holders += 1;
        writeFileSync(path, code.replaceAll(placeholder, wasmHash));
      }
      if (holders !== 1) this.error(`expected the WebAssembly hash placeholder in one script, found ${holders}`);
      const htmlPath = resolve(outDir, 'index.html');
      let html = readFileSync(htmlPath, 'utf8');
      for (const script of scripts) {
        const integrity = `sha384-${createHash('sha384').update(readFileSync(resolve(outDir, script.fileName))).digest('base64')}`;
        html = html.replace(`src="./${script.fileName}"`, `src="./${script.fileName}" integrity="${integrity}"`);
      }
      if (/<script(?![^>]*integrity)[^>]* src=[^>]*>/.test(html)) this.error('a script in index.html has no integrity hash');
      writeFileSync(htmlPath, html);
    },
  };
}

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
  base: './',
  plugins: [pinnedPage()],
  build: {
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
    // One script, so index.html's integrity hash covers the WebAssembly loader too.
    rollupOptions: { output: { inlineDynamicImports: true } },
  },
});
