import { checkClientStorage } from '../../tooling/client-storage/check.mjs';
checkClientStorage();
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';
import { defineConfig, type Plugin } from 'vite';
import { writeClassicModuleAssets } from '../../tooling/docs-classic-assets';

const gpuiRoot = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = path.resolve(gpuiRoot, '..', '..');
const sidebarOutDir = path.resolve(gpuiRoot, 'dist/sidebar');
const cefHtmlEntries = ['manage.html', 'work.html'] as const;
/*
 * CDXC:CefRuntime 2026-06-28-16:18:
 * GPUI CEF entry modules should describe the stable surface they mount, not the historical porting phase. Keep this explicit entry map as the source of truth for the CEF bundle inputs so HTML wrappers, Vite output, and packaged resources stay aligned.
 */
const cefHtmlEntryScripts = {
  'manage.html': path.resolve(gpuiRoot, 'sidebar/manage-main.tsx'),
  // CDXC:WorkMode 2026-10-09 SEE-ALSO: the Work view's page (app/work_view/ loads it into the view panel).
  'work.html': path.resolve(gpuiRoot, 'sidebar/work-main.tsx'),
} satisfies Record<(typeof cefHtmlEntries)[number], string>;

function inlineCefHtmlAssets(): Plugin {
  return {
    name: 'ghostex-gpui-inline-cef-html-assets',
    async writeBundle(options, bundle) {
      const outDir = options.dir ?? sidebarOutDir;
      /*
       * CDXC:CefRuntime 2026-06-14-14:37:
       * The packaged GPUI sidebar is loaded by CEF from a file:// app resource URL. Chromium blocks external module scripts and stylesheets from that opaque origin, so the app bundle must ship a self-contained HTML entry that mounts React without relaxing file-origin security switches.
       *
       * CDXC:CefRuntime 2026-06-24-11:03:
       * The Files view's embed page (manage.html) is a first-party CEF HTML entry beside the sidebar entry. Inline every emitted CEF entry so real runtime surfaces can navigate to bundled file URLs without a dev server, WKWebView/WebKit, temporary pages, or relaxed file-origin switches.
       *
       * CDXC:CefRuntime 2026-06-24-22:01:
       * Inlining only Vite's entry chunks leaves `import "./chunk.js"` specifiers inside the HTML-root module, even though emitted chunks live under assets/. CEF then loads a blank file:// sidebar before React can mount. Keep Vite as the CSS/HTML producer, but replace each CEF entry script with a single esbuild browser bundle so the Manage page does not depend on file-url module graph loading or relaxed Chromium switches.
       *
       * CDXC:CefRuntime 2026-06-24-22:07:
       * Rebuild the final file from the source HTML instead of regex-editing Vite's transformed inline JavaScript. Generated React code can contain script-tag-shaped strings, so final HTML assembly must extract only emitted style tags from Vite output, then inject the esbuild single-file module into the original CEF wrapper.
       *
       * CDXC:CefRuntime 2026-07-08:
       * Entries that load their page module through a dynamic import (Manage's embed page) get their CSS attached to the dynamic chunk instead of a <link> in the entry HTML, and Vite's runtime CSS loader is removed with the replaced module script while the esbuild bundle drops .css imports. Walk each entry's full chunk graph, including dynamic imports, and inline every reachable CSS asset so pages like Manage keep their styles without shipping unrelated entries' CSS.
       */
      const stagedImages = collectCefStagedImages(bundle, outDir);
      for (const htmlEntry of cefHtmlEntries) {
        const htmlPath = path.join(outDir, htmlEntry);
        if (!fs.existsSync(htmlPath)) {
          throw new Error(`Ghostex CEF build did not emit ${htmlPath}.`);
        }

        let html = fs.readFileSync(htmlPath, 'utf8');
        for (const cssFileName of collectCefEntryCssFileNames(bundle, [
          cefHtmlEntryScripts[htmlEntry],
          path.resolve(gpuiRoot, htmlEntry),
        ])) {
          const asset = bundle[cssFileName];
          if (!asset || asset.type !== 'asset') {
            throw new Error(`Ghostex CEF build did not emit CSS asset ${cssFileName}.`);
          }
          const styleTag = `<style>\n${inlineStyleContent(String(asset.source))}\n</style>`;
          const linkPattern = new RegExp(`<link([^>]*?)href="${escapeRegExp(`./${cssFileName}`)}"([^>]*?)>`);
          html = linkPattern.test(html)
            ? html.replace(linkPattern, () => styleTag)
            : html.replace('</head>', `${styleTag}\n</head>`);
        }
        const styleTags = collectInlineStyleTags(stripModulePreloadLinks(html));
        const finalHtml = injectInlineStyleTags(
          replaceCefEntryModuleScript(
            fs.readFileSync(path.join(gpuiRoot, htmlEntry), 'utf8'),
            await buildInlineCefEntryScript(
              cefHtmlEntryScripts[htmlEntry],
              stagedImages,
              cefClassicModuleTargets[htmlEntry]
            )
          ),
          styleTags
        );

        fs.writeFileSync(htmlPath, finalHtml);
      }
      removeUnloadableCefChunks(outDir);
    },
  };
}

type CefOutputBundleEntry =
  | { type: 'asset'; fileName: string; source: string | Uint8Array }
  | {
      dynamicImports: string[];
      facadeModuleId: string | null;
      fileName: string;
      imports: string[];
      isEntry: boolean;
      type: 'chunk';
      viteMetadata?: { importedCss: Set<string> };
    };

/*
 * CDXC:CefRuntime 2026-09-21 WHY:
 * Every page's script and styles are inlined (or staged as classic scripts) above, and a file:// page cannot load Vite's module chunks or stylesheets at all, so the emitted assets/*.js and assets/*.css were about 19 MB shipped in every install that nothing could read.
 * Images and fonts stay: pages reference them from beside the HTML.
 */
function removeUnloadableCefChunks(outDir: string): void {
  const assetsDir = path.join(outDir, 'assets');
  if (!fs.existsSync(assetsDir)) {
    return;
  }
  for (const fileName of fs.readdirSync(assetsDir)) {
    if (/\.(js|css)(\.map)?$/.test(fileName)) {
      fs.rmSync(path.join(assetsDir, fileName));
    }
  }
}

function collectCefEntryCssFileNames(
  bundle: Record<string, CefOutputBundleEntry>,
  entryFacadeModuleIds: readonly string[]
): string[] {
  const normalizedEntryFacadeModuleIds = entryFacadeModuleIds.map(normalizeFacadeModuleId);
  const entryChunk = Object.values(bundle).find(
    (entry) =>
      entry.type === 'chunk' &&
      entry.isEntry &&
      entry.facadeModuleId !== null &&
      normalizedEntryFacadeModuleIds.includes(normalizeFacadeModuleId(entry.facadeModuleId))
  );
  if (!entryChunk) {
    throw new Error(`Ghostex CEF build did not emit an entry chunk for ${entryFacadeModuleIds[0]}.`);
  }
  const cssFileNames: string[] = [];
  const visitedChunkFileNames = new Set<string>();
  const pendingChunkFileNames = [entryChunk.fileName];
  while (pendingChunkFileNames.length > 0) {
    const chunkFileName = pendingChunkFileNames.shift();
    if (chunkFileName === undefined || visitedChunkFileNames.has(chunkFileName)) {
      continue;
    }
    visitedChunkFileNames.add(chunkFileName);
    const chunk = bundle[chunkFileName];
    if (!chunk || chunk.type !== 'chunk') {
      continue;
    }
    for (const cssFileName of chunk.viteMetadata?.importedCss ?? []) {
      if (!cssFileNames.includes(cssFileName)) {
        cssFileNames.push(cssFileName);
      }
    }
    pendingChunkFileNames.push(...chunk.imports, ...chunk.dynamicImports);
  }
  return cssFileNames;
}

function normalizeFacadeModuleId(moduleId: string): string {
  const normalized = path.normalize(moduleId);
  return process.platform === 'win32' ? normalized.toLowerCase() : normalized;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function inlineScriptContent(value: string): string {
  return value.replace(/<\/script/gi, '<\\/script').replace(/<!--/g, '<\\!--');
}

function inlineStyleContent(value: string): string {
  return value.replace(/<\/style/gi, '<\\/style');
}

function stripModulePreloadLinks(html: string): string {
  return html.replace(/^\s*<link\b(?=[^>]*\brel=["']modulepreload["'])[^>]*>\s*$/gim, '');
}

function collectInlineStyleTags(html: string): string[] {
  return [...html.matchAll(/<style\b[^>]*>[\s\S]*?<\/style>/gi)].map((match) => match[0]);
}

function injectInlineStyleTags(html: string, styleTags: readonly string[]): string {
  if (styleTags.length === 0) {
    return html;
  }
  return html.replace('</head>', `${styleTags.join('\n')}\n  </head>`);
}

function replaceCefEntryModuleScript(html: string, bundledScript: string): string {
  const inlineModuleScript = `<script type="module">\n${inlineScriptContent(bundledScript)}\n</script>`;
  const moduleScriptWithSrc =
    /<script\b(?=[^>]*\btype=["']module["'])(?=[^>]*\bsrc=["'][^"']+["'])[^>]*>\s*<\/script>/i;
  if (moduleScriptWithSrc.test(html)) {
    return html.replace(moduleScriptWithSrc, () => inlineModuleScript);
  }

  const existingInlineModuleScript =
    /<script\b(?=[^>]*\btype=["']module["'])(?![^>]*\bsrc=["'][^"']+["'])[^>]*>[\s\S]*?<\/script>/i;
  if (existingInlineModuleScript.test(html)) {
    return html.replace(existingInlineModuleScript, () => inlineModuleScript);
  }

  throw new Error('Ghostex CEF build did not emit a module script to inline.');
}

/*
 * CDXC:CefRuntime 2026-09-21 WHY:
 * Inlining every image as a base64 data URL put about 18 MB of pet spritesheets and Discover screenshots inside the module script of modal-host.html (the React modal page, deleted 2026-10-01), so every Settings, Hotkeys, or Command Palette open showed a blank window while CEF parsed a 21 MB script; the image-free Find page (1.2 MB) painted at once.
 * Scripts and stylesheets must stay inlined because a file:// page cannot load them, but images load fine from beside the page, so images above the threshold are referenced from the copies Vite already emits under assets/ and resolved against the document URL.
 * Small images stay inlined so icons and textures paint with the first frame.
 */
const CEF_INLINE_IMAGE_BYTE_LIMIT = 32 * 1024;
const CEF_STAGED_IMAGE_FILTER = /\.(gif|jpe?g|png|webp)$/i;

interface CefStagedImages {
  fileNameByContentHash: Map<string, string>;
  outDir: string;
}

function cefImageContentHash(source: string | Uint8Array): string {
  return crypto.createHash('sha256').update(source).digest('hex');
}

function collectCefStagedImages(bundle: Record<string, CefOutputBundleEntry>, outDir: string): CefStagedImages {
  const fileNameByContentHash = new Map<string, string>();
  for (const entry of Object.values(bundle)) {
    if (entry.type === 'asset' && CEF_STAGED_IMAGE_FILTER.test(entry.fileName)) {
      fileNameByContentHash.set(cefImageContentHash(entry.source), entry.fileName);
    }
  }
  return { fileNameByContentHash, outDir };
}

function stagedCefImageFileName(stagedImages: CefStagedImages, imagePath: string, contents: Buffer): string {
  const contentHash = cefImageContentHash(contents);
  const emitted = stagedImages.fileNameByContentHash.get(contentHash);
  if (emitted) {
    return emitted;
  }
  // Vite only emits images its own module graph reached; an image only the esbuild bundle imports is staged here.
  const parsed = path.parse(imagePath);
  const fileName = `assets/${parsed.name}-${contentHash.slice(0, 8)}${parsed.ext}`;
  fs.mkdirSync(path.join(stagedImages.outDir, 'assets'), { recursive: true });
  fs.writeFileSync(path.join(stagedImages.outDir, fileName), contents);
  stagedImages.fileNameByContentHash.set(contentHash, fileName);
  return fileName;
}

/** Pages whose code loads on demand as classic scripts (see tooling/docs-classic-assets.ts); every other entry stays one inline module. */
const cefClassicModuleTargets: Partial<
  Record<(typeof cefHtmlEntries)[number], { label: string; runtimeDirName: string }>
> = {
  'manage.html': { label: 'Docs', runtimeDirName: 'docs-runtime' },
};

async function buildInlineCefEntryScript(
  entryPoint: string,
  stagedImages: CefStagedImages,
  classicModuleTarget?: { label: string; runtimeDirName: string }
): Promise<string> {
  const options: esbuild.BuildOptions = {
    absWorkingDir: repoRoot,
    alias: {
      '@': repoRoot,
    },
    bundle: true,
    conditions: ['production'],
    define: {
      'process.env.NODE_ENV': '"production"',
      // The Work page's sample data, left out unless asked for (`mount` in views/work.tsx).
      'process.env.GHOSTEX_WORK_FIXTURES': JSON.stringify(process.env.GHOSTEX_WORK_FIXTURES === '1' ? '1' : ''),
      // React Native's dev flag, read by react-native-enriched-markdown's web build (the Work page's Markdown).
      __DEV__: 'false',
    },
    entryPoints: [entryPoint],
    format: 'esm',
    jsx: 'automatic',
    loader: {
      '.gif': 'dataurl',
      '.jpeg': 'dataurl',
      '.jpg': 'dataurl',
      '.mp3': 'dataurl',
      '.png': 'dataurl',
      '.svg': 'text',
      '.ttf': 'dataurl',
      '.wav': 'dataurl',
      '.webp': 'dataurl',
      '.woff': 'dataurl',
      '.woff2': 'dataurl',
    },
    logLevel: 'silent',
    minify: true,
    platform: 'browser',
    plugins: [createCefSingleFileEsbuildPlugin(stagedImages)],
    target: ['chrome120'],
    write: false,
  };
  if (classicModuleTarget) return writeClassicModuleAssets(stagedImages.outDir, options, classicModuleTarget);
  const result = await esbuild.build(options);
  const script = result.outputFiles.find((file) => file.path === '<stdout>');
  if (!script) {
    throw new Error(`Ghostex CEF esbuild bundle did not emit ${entryPoint}.`);
  }
  return script.text;
}

/** react-native-enriched-markdown's web build imports `react-native`; the Work page answers that one import with a tiny shim (see the file's comment). */
const enrichedMarkdownImporter = /[\\/]node_modules[\\/]react-native-enriched-markdown[\\/]/;
const enrichedMarkdownReactNativeShim = path.resolve(gpuiRoot, 'views/work/react-native-shim.ts');

function resolveEnrichedMarkdownImport(source: string, importer: string | undefined): string | undefined {
  return source === 'react-native' && importer && enrichedMarkdownImporter.test(importer)
    ? enrichedMarkdownReactNativeShim
    : undefined;
}

function enrichedMarkdownShims(): Plugin {
  return {
    name: 'ghostex-enriched-markdown-shims',
    enforce: 'pre',
    resolveId: {
      filter: { id: /^react-native$/ },
      handler(source, importer) {
        return resolveEnrichedMarkdownImport(source, importer);
      },
    },
  };
}

const excalidrawDistFile = /[\\/]node_modules[\\/]@excalidraw[\\/]excalidraw[\\/]dist[\\/]/;
const excalidrawProdIndex = /[\\/]node_modules[\\/]@excalidraw[\\/]excalidraw[\\/]dist[\\/]prod[\\/]index\.js$/;
/** The "More tools" menu's "Generate" heading and its "Mermaid to Excalidraw" item in @excalidraw/excalidraw 0.18.1's minified build. */
const excalidrawMermaidMenuEntries = [
  /[\w$]+\("div",\{style:\{margin:"6px 0",fontSize:14,fontWeight:600\},children:"Generate"\}\),/g,
  /[\w$]+\([\w$]+\.Item,\{onSelect:\(\)=>[\w$]+\.setOpenDialog\(\{name:"ttd",tab:"mermaid"\}\),icon:[\w$]+,"data-testid":"toolbar-embeddable",children:[\w$]+\("toolBar\.mermaidToExcalidraw"\)\}\),/g,
];

/**
 * CDXC:Docs 2026-10-21 DECISION: The user chose to drop Excalidraw's unused locales and its Mermaid-to-Excalidraw tool to shrink the app (about 4 MB of docs-runtime chunks). The Files view never passes `langCode`, so every non-English locale resolves to one empty module; `@excalidraw/mermaid-to-excalidraw` (Mermaid, langium, cytoscape, dagre, KaTeX) resolves to a stub whose parse rejects, which Excalidraw's paste handler already catches and pastes the text as text. The menu item is cut from the minified source and `aiEnabled={false}` in excalidraw-editor.tsx removes the command-palette entries. The build fails if an Excalidraw upgrade changes the menu code, so the entry can never come back as a broken button.
 */
function trimExcalidrawBundle(build: esbuild.PluginBuild): void {
  build.onResolve({ filter: /^\.\/locales\// }, (args) =>
    excalidrawDistFile.test(args.importer)
      ? { path: 'unused-locale', namespace: 'excalidraw-unused-locale' }
      : undefined
  );
  build.onLoad({ filter: /.*/, namespace: 'excalidraw-unused-locale' }, () => ({
    contents: 'export default {};',
    loader: 'js',
  }));
  build.onResolve({ filter: /^@excalidraw\/mermaid-to-excalidraw$/ }, (args) => ({
    path: args.path,
    namespace: 'excalidraw-no-mermaid',
  }));
  build.onLoad({ filter: /.*/, namespace: 'excalidraw-no-mermaid' }, () => ({
    contents:
      'export async function parseMermaidToExcalidraw() { throw new Error("Mermaid to Excalidraw is not included in Ghostex."); }',
    loader: 'js',
  }));
  build.onLoad({ filter: excalidrawProdIndex }, (args) => {
    let contents = fs.readFileSync(args.path, 'utf8');
    for (const entry of excalidrawMermaidMenuEntries) {
      const matches = contents.match(entry)?.length ?? 0;
      if (matches !== 1) {
        throw new Error(
          `Excalidraw's Mermaid menu code changed (${matches} matches for ${entry}); update trimExcalidrawBundle in apps/desktop/vite.config.ts.`
        );
      }
      contents = contents.replace(entry, '');
    }
    return { contents, loader: 'js', resolveDir: path.dirname(args.path) };
  });
}

/**
 * CDXC:Build 2026-09-26 WHY:
 * esbuild can load thousands of icon modules at once. Synchronous reads close each file before the next loader runs, avoiding Windows EMFILE errors from unbounded asynchronous opens.
 */
function createCefSingleFileEsbuildPlugin(stagedImages: CefStagedImages): esbuild.Plugin {
  return {
    name: 'ghostex-gpui-cef-single-file',
    setup(build) {
      build.onResolve({ filter: /^react-native$/ }, (args) => {
        const shim = resolveEnrichedMarkdownImport(args.path, args.importer);
        return shim ? { path: shim } : undefined;
      });
      // The Work page turns LaTeX off, so the package's optional `require('katex')` (KaTeX is installed for mermaid) stays out of the bundle.
      build.onResolve({ filter: /^katex$/ }, (args) =>
        enrichedMarkdownImporter.test(args.importer) ? { path: 'katex', namespace: 'enriched-markdown-empty' } : undefined
      );
      build.onLoad({ filter: /.*/, namespace: 'enriched-markdown-empty' }, () => ({ contents: '', loader: 'js' }));
      trimExcalidrawBundle(build);
      // The package's md4c parser loader is Emscripten CommonJS (`module.exports`) inside a `"type": "module"` folder, so esbuild loads it as ESM and the export never runs; give it the ES export its importer asks for.
      build.onLoad({ filter: /[\\/]react-native-enriched-markdown[\\/]lib[\\/]module[\\/]web[\\/]wasm[\\/]md4c\.js$/ }, (args) => ({
        contents: `${fs.readFileSync(args.path, 'utf8')}\nexport default createMd4cModule;\n`,
        loader: 'js',
      }));
      /*
       * CDXC:CefRuntime 2026-09-05 WHY:
       * Vite owns the inlined CSS, so skip CSS when loading resolved files in the JavaScript-only bundle; bare package imports such as @fontsource-variable/inter resolve to CSS without a .css import suffix.
       */
      build.onLoad({ filter: /\.css$/ }, () => ({
        contents: '',
        loader: 'js',
      }));
      /*
       * CDXC:CefRuntime 2026-11-07 FIX:
       * Vite reads `?raw` imports as text, but this single-file esbuild pass loaded them as JS modules, so the Files view's `agentation.generated.js?raw` (Annotate's bundled fork) had no default export and the React pages build failed. Load any `?raw` import as a text string here too, before the general JS loader below.
       */
      build.onLoad({ filter: /.*/ }, (args) =>
        args.suffix === '?raw' ? { contents: fs.readFileSync(args.path, 'utf8'), loader: 'text' } : undefined
      );
      build.onLoad({ filter: CEF_STAGED_IMAGE_FILTER }, (args) => {
        const contents = fs.readFileSync(args.path);
        if (contents.byteLength <= CEF_INLINE_IMAGE_BYTE_LIMIT) {
          return undefined;
        }
        const fileName = stagedCefImageFileName(stagedImages, args.path, contents);
        return {
          contents: `export default new URL(${JSON.stringify(`./${fileName}`)}, document.baseURI).href;`,
          loader: 'js',
        };
      });
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, (args) => {
        const contents = fs.readFileSync(args.path, 'utf8');
        return {
          contents: contents.replace(/\s+with\s*\{\s*type\s*:\s*["']text["']\s*\}/g, ''),
          loader: args.path.endsWith('.tsx') || args.path.endsWith('.jsx') ? 'tsx' : 'ts',
          resolveDir: path.dirname(args.path),
        };
      });
    },
  };
}

export default defineConfig({
  base: './',
  root: gpuiRoot,
  plugins: [enrichedMarkdownShims(), inlineCefHtmlAssets()],
  build: {
    emptyOutDir: true,
    outDir: sidebarOutDir,
    // The gzip size report compresses every chunk only to print sizes nobody reads here.
    reportCompressedSize: false,
    rolldownOptions: {
      /*
       * CDXC:CefRuntime 2026-06-14-12:50:
       * The GPUI shell resolves the bundled pages through Contents/Resources/sidebar/<entry>.html. Keep the Vite HTML entries at the package root so production-style packaging and local development share those URLs.
       */
      input: {
        manage: path.resolve(gpuiRoot, 'manage.html'),
        work: path.resolve(gpuiRoot, 'work.html'),
      },
    },
  },
  resolve: {
    dedupe: ['react', 'react-dom'],
    alias: {
      /*
       * CDXC:Build 2026-06-14-12:06:
       * The GPUI CEF sidebar bundle imports app-owned sidebar and shadcn modules from the repository root. Keep the same @ alias as Storybook and Electron so this app exercises the production React component graph.
       */
      '@': repoRoot,
    },
  },
  server: {
    fs: {
      allow: [repoRoot],
    },
  },
});
