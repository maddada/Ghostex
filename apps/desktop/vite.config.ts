import { checkClientStorage } from '../../tooling/client-storage/check.mjs';
checkClientStorage();
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';
import { defineConfig, type Plugin } from 'vite';
import { writeClassicModuleAssets } from '../../tooling/docs-classic-assets';
import {
  MERMAID_ASSET_DIR_NAME,
  mermaidClassicScriptEsbuildPlugin,
  writeMermaidClassicAssets,
} from '../../tooling/mermaid-classic-assets.mjs';
import {
  SHIKI_ASSET_DIR_NAME,
  shikiClassicScriptEsbuildPlugin,
  writeShikiClassicAssets,
} from '../../tooling/shiki-classic-assets.mjs';

const gpuiRoot = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = path.resolve(gpuiRoot, '..', '..');
const sidebarOutDir = path.resolve(gpuiRoot, 'dist/sidebar');
const cefHtmlEntries = [
  'find.html',
  'kanban.html',
  'manage.html',
  'modal-host.html',
  'titlebar-host.html',
] as const;
/*
 * CDXC:CefRuntime 2026-06-28-16:18:
 * GPUI CEF entry modules should describe the stable surface they mount, not the historical porting phase. Keep this explicit entry map as the source of truth for the sidebar, Kanban, and Manage bundle inputs so HTML wrappers, Vite output, and packaged resources stay aligned.
 */
const cefHtmlEntryScripts = {
  'find.html': path.resolve(gpuiRoot, 'sidebar/find-main.tsx'),
  'kanban.html': path.resolve(gpuiRoot, 'sidebar/kanban-main.tsx'),
  'manage.html': path.resolve(gpuiRoot, 'sidebar/manage-main.tsx'),
  'modal-host.html': path.resolve(gpuiRoot, 'views/modal-host.tsx'),
  'titlebar-host.html': path.resolve(gpuiRoot, 'views/titlebar-host.tsx'),
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
       * Source-independent Kanban and Manage GPUI workarea pages are first-party CEF HTML entries beside the sidebar entry. Inline every emitted CEF entry so real runtime surfaces can navigate to bundled file URLs without a dev server, WKWebView/WebKit, temporary pages, or relaxed file-origin switches.
       *
       * CDXC:AppModal 2026-06-24-10:42:
       * The GPUI app-modal window loads the same React modal host entry as macOS through a first-party CEF HTML file. Keep modal-host.html in the inlined CEF entry set so Settings, Hotkeys, and Command Palette can open without WebKit, duplicated modal UI, temporary pages, or dev-server-only assets.
       *
       * CDXC:Onboarding 2026-06-24-23:17:
       * The GPUI info dropdown loads the shared React titlebar-host Tips panel through a first-party CEF HTML entry. Keep titlebar-host.html in the same single-file inlining path as modal-host.html so the gpui-component Popover can show production Tips content without AppKit/Swift dropdowns, duplicate GPUI tips data, or dev-server-only module loading.
       *
       * CDXC:CefRuntime 2026-06-24-22:01:
       * Inlining only Vite's entry chunks leaves `import "./chunk.js"` specifiers inside the HTML-root module, even though emitted chunks live under assets/. CEF then loads a blank file:// sidebar before React can mount. Keep Vite as the CSS/HTML producer, but replace each CEF entry script with a single esbuild browser bundle so sidebar, Kanban, Manage, app-modal, and titlebar-panel hosts do not depend on file-url module graph loading or relaxed Chromium switches.
       *
       * CDXC:CefRuntime 2026-06-24-22:07:
       * Rebuild the final file from the source HTML instead of regex-editing Vite's transformed inline JavaScript. Generated React code can contain script-tag-shaped strings, so final HTML assembly must extract only emitted style tags from Vite output, then inject the esbuild single-file module into the original CEF wrapper.
       *
       * CDXC:CefRuntime 2026-07-08:
       * Entries that load their page module through a dynamic import (Manage's shared Docs app, Kanban's tasks placeholder) get their CSS attached to the dynamic chunk instead of a <link> in the entry HTML, and Vite's runtime CSS loader is removed with the replaced module script while the esbuild bundle drops .css imports. Walk each entry's full chunk graph, including dynamic imports, and inline every reachable CSS asset so pages like Docs keep their editor styles without shipping unrelated entries' CSS.
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

/*
 * CDXC:PromptEditor 2026-08-01:
 * The Agents Hub modal in modal-host.html loads
 * Monaco at runtime through its AMD loader from ./monaco/vs — the only
 * Monaco route that works from CEF's file:// origin, since the ESM build
 * spawns module workers the single-file bundle cannot ship. Stage the
 * min/vs runtime beside the inlined CEF entries; build-macos-app.sh mirrors
 * dist/sidebar wholesale into Contents/Resources/sidebar.
 */
function stageMonacoVs(): Plugin {
  return {
    name: 'ghostex-gpui-stage-monaco-vs',
    closeBundle() {
      const monacoSource = path.join(repoRoot, 'node_modules', 'monaco-editor', 'min', 'vs');
      if (!fs.existsSync(path.join(monacoSource, 'loader.js'))) {
        throw new Error(`monaco-editor min/vs runtime is missing at ${monacoSource}.`);
      }
      const monacoDest = path.join(sidebarOutDir, 'monaco', 'vs');
      fs.rmSync(monacoDest, { force: true, recursive: true });
      fs.cpSync(monacoSource, monacoDest, { recursive: true });
    },
  };
}

/*
 * CDXC:SessionChat 2026-08-21:
 * Session Chat highlights fenced code with Shiki, loading the engine and one
 * grammar per language on demand. Those loaders are dynamic imports, which the
 * single-file CEF bundler above cannot code-split: esbuild inlines every
 * dynamic import when splitting is off, and a file:// CEF page cannot fetch
 * module chunks anyway. Left alone that turned chat.html from 1.3 MB into
 * 4.9 MB of highlighter parsed by EVERY chat pane.
 *
 * So the highlighter ships the way Monaco does: prebuilt classic scripts staged
 * beside the bundle, pulled in by <script src> only when a fence needs them.
 * tooling/shiki-classic-assets.mjs owns both the staged files and the loader
 * shim.
 */
function stageShikiChatRuntime(): Plugin {
  return {
    name: 'ghostex-gpui-stage-shiki-chat-runtime',
    async closeBundle() {
      await writeShikiClassicAssets(path.join(sidebarOutDir, SHIKI_ASSET_DIR_NAME));
      await writeMermaidClassicAssets(path.join(sidebarOutDir, MERMAID_ASSET_DIR_NAME));
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
 * Inlining every image as a base64 data URL put about 18 MB of pet spritesheets and Discover screenshots inside modal-host.html's module script, so every Settings, Hotkeys, or Command Palette open showed a blank window while CEF parsed a 21 MB script; find.html (1.2 MB, no images) painted at once.
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
  'modal-host.html': { label: 'Ghostex', runtimeDirName: 'modal-runtime' },
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

function createCefSingleFileEsbuildPlugin(stagedImages: CefStagedImages): esbuild.Plugin {
  return {
    name: 'ghostex-gpui-cef-single-file',
    setup(build) {
      /*
       * CDXC:CefRuntime 2026-09-05 WHY:
       * Vite owns the inlined CSS, so skip CSS when loading resolved files in the JavaScript-only bundle; bare package imports such as @fontsource-variable/inter resolve to CSS without a .css import suffix.
       */
      build.onLoad({ filter: /\.css$/ }, () => ({
        contents: '',
        loader: 'js',
      }));
      build.onLoad({ filter: CEF_STAGED_IMAGE_FILTER }, async (args) => {
        const contents = await fs.promises.readFile(args.path);
        if (contents.byteLength <= CEF_INLINE_IMAGE_BYTE_LIMIT) {
          return undefined;
        }
        const fileName = stagedCefImageFileName(stagedImages, args.path, contents);
        return {
          contents: `export default new URL(${JSON.stringify(`./${fileName}`)}, document.baseURI).href;`,
          loader: 'js',
        };
      });
      // See stageShikiChatRuntime: CEF pages load the Shiki engine and its
      // grammars as classic scripts from ./shiki, never as ES module chunks.
      // The shared plugin swaps both dynamic-import modules for that loader
      // before esbuild can inline the highlighter into every chat pane.
      shikiClassicScriptEsbuildPlugin().setup(build);
      mermaidClassicScriptEsbuildPlugin().setup(build);
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, async (args) => {
        const contents = await fs.promises.readFile(args.path, 'utf8');
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
  plugins: [inlineCefHtmlAssets(), stageMonacoVs(), stageShikiChatRuntime()],
  build: {
    emptyOutDir: true,
    outDir: sidebarOutDir,
    // The gzip size report compresses every chunk (Monaco, Mermaid) only to print sizes nobody reads here.
    reportCompressedSize: false,
    rolldownOptions: {
      /*
       * CDXC:CefRuntime 2026-06-14-12:50:
       * The GPUI shell resolves the bundled pages through Contents/Resources/sidebar/<entry>.html. Keep the Vite HTML entries at the package root so production-style packaging and local development share those URLs.
       */
      input: {
        find: path.resolve(gpuiRoot, 'find.html'),
        kanban: path.resolve(gpuiRoot, 'kanban.html'),
        manage: path.resolve(gpuiRoot, 'manage.html'),
        modalHost: path.resolve(gpuiRoot, 'modal-host.html'),
        titlebarHost: path.resolve(gpuiRoot, 'titlebar-host.html'),
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
