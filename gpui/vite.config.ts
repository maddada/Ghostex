import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";
import { defineConfig, type Plugin } from "vite";
import {
  SHIKI_ASSET_DIR_NAME,
  shikiClassicScriptEsbuildPlugin,
  writeShikiClassicAssets,
} from "../scripts/shiki-classic-assets.mjs";

const gpuiRoot = fileURLToPath(new URL(".", import.meta.url));
const repoRoot = path.resolve(gpuiRoot, "..");
const sidebarOutDir = path.resolve(gpuiRoot, "dist/sidebar");
const cefHtmlEntries = [
  "index.html",
  "chat.html",
  "find.html",
  "kanban.html",
  "manage.html",
  "modal-host.html",
  "titlebar-host.html",
] as const;
/*
 * CDXC:GPUISidebarEntrypoints 2026-06-28-16:18:
 * GPUI CEF entry modules should describe the stable surface they mount, not the historical porting phase. Keep this explicit entry map as the source of truth for the sidebar, Kanban, and Manage bundle inputs so HTML wrappers, Vite output, and packaged resources stay aligned.
 */
const cefHtmlEntryScripts = {
  "index.html": path.resolve(gpuiRoot, "sidebar/main.tsx"),
  "chat.html": path.resolve(gpuiRoot, "sidebar/chat-main.tsx"),
  "find.html": path.resolve(gpuiRoot, "sidebar/find-main.tsx"),
  "kanban.html": path.resolve(gpuiRoot, "sidebar/kanban-main.tsx"),
  "manage.html": path.resolve(gpuiRoot, "sidebar/manage-main.tsx"),
  "modal-host.html": path.resolve(repoRoot, "native/sidebar/modal-host.tsx"),
  "titlebar-host.html": path.resolve(repoRoot, "native/sidebar/titlebar-host.tsx"),
} satisfies Record<(typeof cefHtmlEntries)[number], string>;

function inlineCefHtmlAssets(): Plugin {
  return {
    name: "ghostex-gpui-inline-cef-html-assets",
    async writeBundle(options, bundle) {
      const outDir = options.dir ?? sidebarOutDir;
      /*
       * CDXC:GPUICefFileUrlBundle 2026-06-14-14:37:
       * The packaged GPUI sidebar is loaded by CEF from a file:// app resource URL. Chromium blocks external module scripts and stylesheets from that opaque origin, so the app bundle must ship a self-contained HTML entry that mounts React without relaxing file-origin security switches.
       *
       * CDXC:GPUIProjectWorkareaCefBundles 2026-06-24-11:03:
       * Source-independent Kanban and Manage GPUI workarea pages are first-party CEF HTML entries beside the sidebar entry. Inline every emitted CEF entry so real runtime surfaces can navigate to bundled file URLs without a dev server, WKWebView/WebKit, temporary pages, or relaxed file-origin switches.
       *
       * CDXC:GPUITitlebarAppModalHost 2026-06-24-10:42:
       * The GPUI app-modal window loads the same React modal host entry as macOS through a first-party CEF HTML file. Keep modal-host.html in the inlined CEF entry set so Settings, Hotkeys, and Command Palette can open without WebKit, duplicated modal UI, temporary pages, or dev-server-only assets.
       *
       * CDXC:GPUITitlebarTips 2026-06-24-23:17:
       * The GPUI info dropdown loads the shared React titlebar-host Tips panel through a first-party CEF HTML entry. Keep titlebar-host.html in the same single-file inlining path as modal-host.html so the gpui-component Popover can show production Tips content without AppKit/Swift dropdowns, duplicate GPUI tips data, or dev-server-only module loading.
       *
       * CDXC:GPUICefBundleInlining 2026-06-24-22:01:
       * Inlining only Vite's entry chunks leaves `import "./chunk.js"` specifiers inside the HTML-root module, even though emitted chunks live under assets/. CEF then loads a blank file:// sidebar before React can mount. Keep Vite as the CSS/HTML producer, but replace each CEF entry script with a single esbuild browser bundle so sidebar, Kanban, Manage, app-modal, and titlebar-panel hosts do not depend on file-url module graph loading or relaxed Chromium switches.
       *
       * CDXC:GPUICefBundleInlining 2026-06-24-22:07:
       * Rebuild the final file from the source HTML instead of regex-editing Vite's transformed inline JavaScript. Generated React code can contain script-tag-shaped strings, so final HTML assembly must extract only emitted style tags from Vite output, then inject the esbuild single-file module into the original CEF wrapper.
       *
       * CDXC:GPUICefDynamicChunkCss 2026-07-08:
       * Entries that load their page module through a dynamic import (Manage's shared Docs app, Kanban's tasks placeholder) get their CSS attached to the dynamic chunk instead of a <link> in the entry HTML, and Vite's runtime CSS loader is removed with the replaced module script while the esbuild bundle drops .css imports. Walk each entry's full chunk graph, including dynamic imports, and inline every reachable CSS asset so pages like Docs keep their editor styles without shipping unrelated entries' CSS.
       */
      for (const htmlEntry of cefHtmlEntries) {
        const htmlPath = path.join(outDir, htmlEntry);
        if (!fs.existsSync(htmlPath)) {
          throw new Error(`Ghostex CEF build did not emit ${htmlPath}.`);
        }

        let html = fs.readFileSync(htmlPath, "utf8");
        for (const cssFileName of collectCefEntryCssFileNames(bundle, [
          cefHtmlEntryScripts[htmlEntry],
          path.resolve(gpuiRoot, htmlEntry),
        ])) {
          const asset = bundle[cssFileName];
          if (!asset || asset.type !== "asset") {
            throw new Error(`Ghostex CEF build did not emit CSS asset ${cssFileName}.`);
          }
          const styleTag = `<style>\n${inlineStyleContent(String(asset.source))}\n</style>`;
          const linkPattern = new RegExp(
            `<link([^>]*?)href="${escapeRegExp(`./${cssFileName}`)}"([^>]*?)>`,
          );
          html = linkPattern.test(html)
            ? html.replace(linkPattern, () => styleTag)
            : html.replace("</head>", `${styleTag}\n</head>`);
        }
        const styleTags = collectInlineStyleTags(stripModulePreloadLinks(html));
        const finalHtml = injectInlineStyleTags(
          replaceCefEntryModuleScript(
            fs.readFileSync(path.join(gpuiRoot, htmlEntry), "utf8"),
            await buildInlineCefEntryScript(cefHtmlEntryScripts[htmlEntry]),
          ),
          styleTags,
        );

        fs.writeFileSync(htmlPath, finalHtml);
      }
    },
  };
}

/*
 * CDXC:SessionChatMonaco 2026-08-01:
 * chat.html's composer (and the Agents Hub modal in modal-host.html) load
 * Monaco at runtime through its AMD loader from ./monaco/vs — the only
 * Monaco route that works from CEF's file:// origin, since the ESM build
 * spawns module workers the single-file bundle cannot ship. Stage the
 * min/vs runtime beside the inlined CEF entries; build-macos-app.sh mirrors
 * dist/sidebar wholesale into Contents/Resources/sidebar.
 */
function stageMonacoVs(): Plugin {
  return {
    name: "ghostex-gpui-stage-monaco-vs",
    closeBundle() {
      const monacoSource = path.join(repoRoot, "node_modules", "monaco-editor", "min", "vs");
      if (!fs.existsSync(path.join(monacoSource, "loader.js"))) {
        throw new Error(`monaco-editor min/vs runtime is missing at ${monacoSource}.`);
      }
      const monacoDest = path.join(sidebarOutDir, "monaco", "vs");
      fs.rmSync(monacoDest, { force: true, recursive: true });
      fs.cpSync(monacoSource, monacoDest, { recursive: true });
    },
  };
}

/*
 * CDXC:SessionChatCodeHighlighting 2026-08-21:
 * Session Chat highlights fenced code with Shiki, loading the engine and one
 * grammar per language on demand. Those loaders are dynamic imports, which the
 * single-file CEF bundler above cannot code-split: esbuild inlines every
 * dynamic import when splitting is off, and a file:// CEF page cannot fetch
 * module chunks anyway. Left alone that turned chat.html from 1.3 MB into
 * 4.9 MB of highlighter parsed by EVERY chat pane.
 *
 * So the highlighter ships the way Monaco does: prebuilt classic scripts staged
 * beside the bundle, pulled in by <script src> only when a fence needs them.
 * scripts/shiki-classic-assets.mjs owns both the staged files and the loader
 * shim, and the mobile chat build uses the same module so the two hosts cannot
 * drift apart.
 */
function stageShikiChatRuntime(): Plugin {
  return {
    name: "ghostex-gpui-stage-shiki-chat-runtime",
    async closeBundle() {
      await writeShikiClassicAssets(path.join(sidebarOutDir, SHIKI_ASSET_DIR_NAME));
    },
  };
}

type CefOutputBundleEntry =
  | { type: "asset"; fileName: string; source: string | Uint8Array }
  | {
      dynamicImports: string[];
      facadeModuleId: string | null;
      fileName: string;
      imports: string[];
      isEntry: boolean;
      type: "chunk";
      viteMetadata?: { importedCss: Set<string> };
    };

function collectCefEntryCssFileNames(
  bundle: Record<string, CefOutputBundleEntry>,
  entryFacadeModuleIds: readonly string[],
): string[] {
  const normalizedEntryFacadeModuleIds = entryFacadeModuleIds.map(normalizeFacadeModuleId);
  const entryChunk = Object.values(bundle).find(
    (entry) =>
      entry.type === "chunk" &&
      entry.isEntry &&
      entry.facadeModuleId !== null &&
      normalizedEntryFacadeModuleIds.includes(normalizeFacadeModuleId(entry.facadeModuleId)),
  );
  if (!entryChunk) {
    throw new Error(
      `Ghostex CEF build did not emit an entry chunk for ${entryFacadeModuleIds[0]}.`,
    );
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
    if (!chunk || chunk.type !== "chunk") {
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
  return process.platform === "win32" ? normalized.toLowerCase() : normalized;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function inlineScriptContent(value: string): string {
  return value.replace(/<\/script/gi, "<\\/script").replace(/<!--/g, "<\\!--");
}

function inlineStyleContent(value: string): string {
  return value.replace(/<\/style/gi, "<\\/style");
}

function stripModulePreloadLinks(html: string): string {
  return html.replace(/^\s*<link\b(?=[^>]*\brel=["']modulepreload["'])[^>]*>\s*$/gim, "");
}

function collectInlineStyleTags(html: string): string[] {
  return [...html.matchAll(/<style\b[^>]*>[\s\S]*?<\/style>/gi)].map((match) => match[0]);
}

function injectInlineStyleTags(html: string, styleTags: readonly string[]): string {
  if (styleTags.length === 0) {
    return html;
  }
  return html.replace("</head>", `${styleTags.join("\n")}\n  </head>`);
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

  throw new Error("Ghostex CEF build did not emit a module script to inline.");
}

async function buildInlineCefEntryScript(entryPoint: string): Promise<string> {
  const result = await esbuild.build({
    absWorkingDir: repoRoot,
    alias: {
      "@": repoRoot,
    },
    bundle: true,
    define: {
      "process.env.NODE_ENV": "\"production\"",
    },
    entryPoints: [entryPoint],
    format: "esm",
    jsx: "automatic",
    loader: {
      ".gif": "dataurl",
      ".jpeg": "dataurl",
      ".jpg": "dataurl",
      ".mp3": "dataurl",
      ".png": "dataurl",
      ".svg": "text",
      ".wav": "dataurl",
      ".webp": "dataurl",
      ".woff": "dataurl",
      ".woff2": "dataurl",
    },
    logLevel: "silent",
    minify: true,
    platform: "browser",
    plugins: [createCefSingleFileEsbuildPlugin()],
    target: ["chrome120"],
    write: false,
  });
  const script = result.outputFiles.find((file) => file.path === "<stdout>");
  if (!script) {
    throw new Error(`Ghostex CEF esbuild bundle did not emit ${entryPoint}.`);
  }
  return script.text;
}

function createCefSingleFileEsbuildPlugin(): esbuild.Plugin {
  return {
    name: "ghostex-gpui-cef-single-file",
    setup(build) {
      build.onResolve({ filter: /\.css$/ }, (args) => ({
        namespace: "ghostex-gpui-empty-css",
        path: args.path,
      }));
      build.onLoad({ filter: /.*/, namespace: "ghostex-gpui-empty-css" }, () => ({
        contents: "",
        loader: "js",
      }));
      // See stageShikiChatRuntime: CEF pages load the Shiki engine and its
      // grammars as classic scripts from ./shiki, never as ES module chunks.
      // The shared plugin swaps both dynamic-import modules for that loader
      // before esbuild can inline the highlighter into every chat pane.
      shikiClassicScriptEsbuildPlugin().setup(build);
      build.onLoad({ filter: /\.[cm]?[jt]sx?$/ }, async (args) => {
        const contents = await fs.promises.readFile(args.path, "utf8");
        return {
          contents: contents.replace(
            /\s+with\s*\{\s*type\s*:\s*["']text["']\s*\}/g,
            "",
          ),
          loader: args.path.endsWith(".tsx") || args.path.endsWith(".jsx") ? "tsx" : "ts",
          resolveDir: path.dirname(args.path),
        };
      });
    },
  };
}

export default defineConfig({
  base: "./",
  root: gpuiRoot,
  plugins: [inlineCefHtmlAssets(), stageMonacoVs(), stageShikiChatRuntime()],
  build: {
    emptyOutDir: true,
    outDir: sidebarOutDir,
    rolldownOptions: {
      /*
       * CDXC:GPUICefHtmlEntries 2026-06-14-12:50:
       * The GPUI shell resolves the bundled sidebar through Contents/Resources/sidebar/index.html. Keep the Vite HTML entry at the package root so production-style packaging and local development share that single entry URL.
       */
      input: {
        index: path.resolve(gpuiRoot, "index.html"),
        chat: path.resolve(gpuiRoot, "chat.html"),
        find: path.resolve(gpuiRoot, "find.html"),
        kanban: path.resolve(gpuiRoot, "kanban.html"),
        manage: path.resolve(gpuiRoot, "manage.html"),
        modalHost: path.resolve(gpuiRoot, "modal-host.html"),
        titlebarHost: path.resolve(gpuiRoot, "titlebar-host.html"),
      },
    },
  },
  resolve: {
    dedupe: ["react", "react-dom"],
    alias: {
      /*
       * CDXC:GPUISidebarReactImports 2026-06-14-12:06:
       * The GPUI CEF sidebar bundle imports app-owned sidebar and shadcn modules from the repository root. Keep the same @ alias as Storybook and Electron so this app exercises the production React component graph.
       */
      "@": repoRoot,
      /*
       * CDXC:GPUIDevNativeEntryUrls 2026-08-09:
       * modal-host.html and titlebar-host.html point at ../native/sidebar/*.tsx,
       * which the browser normalises to /native/sidebar/*.tsx against the dev
       * root (gpui/). That URL escapes the root, so the dev server answered the
       * SPA fallback HTML instead of the module and both windows rendered blank.
       * Map the escaped prefix back to the real directory so dev serves the same
       * entry the build resolves from disk. Build output is unaffected: Rollup
       * resolves the relative src itself and no module specifier starts with
       * /native.
       */
      "/native": path.resolve(repoRoot, "native"),
    },
  },
  server: {
    fs: {
      allow: [repoRoot],
    },
  },
});
