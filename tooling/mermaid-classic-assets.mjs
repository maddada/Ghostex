import fs from 'node:fs/promises';
import path from 'node:path';
import * as esbuild from 'esbuild';

export const MERMAID_ASSET_DIR_NAME = 'mermaid';
const repoRoot = path.resolve(import.meta.dirname, '..');

/**
 * CDXC:CefRuntime 2026-09-06 WHY:
 * CEF file URLs cannot import module chunks. Stage Mermaid once as a classic script instead of inlining its renderer into every Markdown and Docs entry.
 */
export async function writeMermaidClassicAssets(outDir) {
  await fs.mkdir(outDir, { recursive: true });
  await esbuild.build({
    absWorkingDir: repoRoot,
    stdin: {
      contents: "import mermaid from 'mermaid'; globalThis.__ghostexMermaid = mermaid;",
      resolveDir: repoRoot,
      loader: 'js',
    },
    bundle: true,
    format: 'iife',
    platform: 'browser',
    // CDXC:CefRuntime 2026-09-06 WHY:
    // Monaco installs a global AMD define; Mermaid's bundled UMD dependencies otherwise register there and leave Mermaid's imports uninitialized. Resolve AMD detection at build time without changing Monaco's loader.
    define: { define: 'undefined' },
    minify: true,
    target: ['chrome120', 'safari16'],
    outfile: path.join(outDir, 'runtime.js'),
  });
}

const loader = `
let pending;
export function loadMermaid() {
  if (globalThis.__ghostexMermaid) return Promise.resolve(globalThis.__ghostexMermaid);
  if (!pending) {
    pending = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = new URL('./mermaid/runtime.js', document.baseURI).href;
      script.onload = () => globalThis.__ghostexMermaid
        ? resolve(globalThis.__ghostexMermaid)
        : reject(new Error('Mermaid runtime loaded without registering its renderer.'));
      script.onerror = () => reject(new Error('Could not load the bundled Mermaid renderer.'));
      document.head.appendChild(script);
    });
  }
  return pending;
}
`;

export function mermaidClassicScriptEsbuildPlugin() {
  return {
    name: 'ghostex-mermaid-classic-script',
    setup(build) {
      build.onResolve({ filter: /(^|\/)mermaid-loader(\.ts)?$/ }, () => ({
        path: 'loader',
        namespace: 'ghostex-mermaid-classic',
      }));
      build.onLoad({ filter: /.*/, namespace: 'ghostex-mermaid-classic' }, () => ({
        contents: loader,
        loader: 'js',
      }));
    },
  };
}
