import { transformAsync } from '@babel/core';
import react from '@vitejs/plugin-react';
import reactCompiler from 'babel-plugin-react-compiler';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { tanstackRouter } from '@tanstack/router-plugin/vite';
import { defineConfig, type Plugin } from 'vite';

const webRoot = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = path.resolve(webRoot, '..', '..');
const monacoVsSource = path.join(repoRoot, 'node_modules', 'monaco-editor', 'min', 'vs');

/*
 * CDXC:PromptEditor 2026-08-01:
 * The session-chat composer loads Monaco at runtime via the AMD loader from
 * /monaco/vs (no ESM import anywhere in the repo). Serve min/vs straight
 * from node_modules in dev and copy it into dist for production, instead of
 * checking a 15 MB runtime into apps/web/public.
 */
function ghostexMonacoVs(): Plugin {
  const contentTypeFor = (filePath: string): string => {
    if (filePath.endsWith('.js')) return 'text/javascript';
    if (filePath.endsWith('.css')) return 'text/css';
    if (filePath.endsWith('.json')) return 'application/json';
    if (filePath.endsWith('.ttf')) return 'font/ttf';
    if (filePath.endsWith('.svg')) return 'image/svg+xml';
    return 'application/octet-stream';
  };
  return {
    name: 'ghostex-web-monaco-vs',
    closeBundle() {
      if (!fs.existsSync(path.join(monacoVsSource, 'loader.js'))) {
        throw new Error(`monaco-editor min/vs runtime is missing at ${monacoVsSource}.`);
      }
      const destination = path.join(webRoot, 'dist', 'monaco', 'vs');
      fs.rmSync(destination, { force: true, recursive: true });
      fs.cpSync(monacoVsSource, destination, { recursive: true });
    },
    configureServer(server) {
      server.middlewares.use('/monaco/vs', (request, response, next) => {
        const requestPath = (request.url ?? '').split('?', 1)[0];
        const filePath = path.join(monacoVsSource, requestPath);
        // path.join normalizes ../ segments; keep resolution inside min/vs.
        if (!filePath.startsWith(monacoVsSource) || !fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
          next();
          return;
        }
        response.setHeader('content-type', contentTypeFor(filePath));
        fs.createReadStream(filePath).pipe(response);
      });
    },
  };
}

function ghostexReactCompiler(): Plugin {
  return {
    enforce: 'pre',
    name: 'ghostex-web-react-compiler',
    async transform(code, id) {
      const filename = id.split('?', 1)[0];
      if (!filename.startsWith(webRoot) || !/\.[jt]sx$/.test(filename)) {
        return null;
      }
      const result = await transformAsync(code, {
        babelrc: false,
        configFile: false,
        filename,
        parserOpts: { plugins: ['jsx', 'typescript'] },
        plugins: [[reactCompiler, {}]],
        sourceMaps: true,
      });
      return result?.code ? { code: result.code, map: result.map } : null;
    },
  };
}

export default defineConfig({
  root: webRoot,
  plugins: [
    tanstackRouter({
      autoCodeSplitting: true,
      generatedRouteTree: './src/routeTree.gen.ts',
      routesDirectory: './src/routes',
      target: 'react',
    }),
    ghostexReactCompiler(),
    react(),
    ghostexMonacoVs(),
  ],
  resolve: {
    alias: {
      '@': repoRoot,
    },
  },
  build: {
    emptyOutDir: true,
    outDir: path.resolve(webRoot, 'dist'),
  },
  server: {
    proxy: {
      '/api': {
        changeOrigin: true,
        configure(proxy) {
          const stripBootstrapOrigin = (
            proxyRequest: { removeHeader(name: string): void },
            request: { url?: string }
          ) => {
            if (request.url?.startsWith('/api/webBootstrap')) {
              proxyRequest.removeHeader('origin');
            }
          };
          proxy.on('proxyReq', stripBootstrapOrigin);
          proxy.on('proxyReqWs', stripBootstrapOrigin);
        },
        target: 'http://127.0.0.1:58744',
        ws: true,
      },
    },
  },
});
