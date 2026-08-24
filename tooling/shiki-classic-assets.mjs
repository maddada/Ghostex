/*
CDXC:SessionChatCodeHighlighting 2026-08-21:
Shiki assets for the two Ghostex chat surfaces that cannot load ES modules.

Session Chat highlights fenced code with Shiki and loads the engine plus one
grammar per language on demand (packages/core-ui/chat/session-chat-shiki-engine.ts and
packages/core-ui/chat/session-chat-code-grammars.ts). On the web that is plain dynamic
`import()` and the bundler splits it. Two hosts cannot do that:

  * gpui packaged CEF surfaces are file:// documents.
  * The React Native chat webview loads from file:///android_asset (Android) or
    the app bundle (iOS).

Measured in Chromium on a real file:// document: `import()` fails ("Failed to
fetch dynamically imported module") and `fetch()` fails, but a classic
`<script src>` loads fine. Both hosts therefore get the same treatment — the
grammars ship as classic scripts staged beside the page, and the shared modules
are swapped at build time for a `<script src>` loader.

Grammars are stored SPLIT, not flattened. `@shikijs/langs/html` re-exports
`[...javascript, ...css, html]`, so staging each language's full export
duplicates shared grammars over and over: 11.08 MB across our language set
versus 2.69 MB when each file carries only its own registration plus the names
of the grammars it embeds. The loader below walks that dependency list, so the
page still hands Shiki exactly the array the flattened export would have given
it. `embeddedLangs` was verified to match each module's static imports exactly
for every language we stage.
*/

import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import * as esbuild from 'esbuild';

import { SESSION_CHAT_CODE_LANGUAGES } from '../packages/core-ui/chat/session-chat-code-languages.ts';

/** Directory name the staged scripts live in, relative to the page. */
export const SHIKI_ASSET_DIR_NAME = 'shiki';
export const SHIKI_CORE_FILE_NAME = 'core.js';
const SHIKI_LANGS_GLOBAL = '__ghostexShikiLangs';
const SHIKI_CORE_GLOBAL = '__ghostexShikiCore';

const repoRoot = path.resolve(import.meta.dirname, '..');

/** U+2028/2029 are valid in JS string literals but break some inline hosts. */
function jsStringLiteral(value) {
  // JSON.stringify leaves U+2028/U+2029 raw. They are legal in modern JS
  // string literals but still break some inline <script> hosts, so escape
  // them explicitly the way the rest of this repo does.
  return JSON.stringify(value).replace(/[\u2028\u2029]/g, (character) =>
    character === '\u2028' ? '\\u2028' : '\\u2029'
  );
}

async function buildClassicScript(contents) {
  const result = await esbuild.build({
    absWorkingDir: repoRoot,
    bundle: true,
    format: 'iife',
    logLevel: 'silent',
    minify: true,
    platform: 'browser',
    stdin: { contents, loader: 'ts', resolveDir: repoRoot },
    target: ['chrome120', 'safari16'],
    write: false,
  });
  const script = result.outputFiles.find((file) => file.path === '<stdout>');
  if (!script) {
    throw new Error('esbuild emitted no Shiki classic script.');
  }
  return script.text;
}

/**
 * Reads each grammar module for its own registration and the grammars it
 * embeds, following embeds until the set is closed. Our language list names 59
 * grammars; the closure adds 10 more that only exist as embeds (postcss,
 * markdown-nix, the vue-* helpers, …).
 */
async function collectGrammarClosure() {
  const grammars = new Map();
  const pending = [...SESSION_CHAT_CODE_LANGUAGES];
  while (pending.length > 0) {
    const language = pending.shift();
    if (grammars.has(language)) {
      continue;
    }
    const modulePath = path.join(repoRoot, 'node_modules/@shikijs/langs/dist', `${language}.mjs`);
    const module = await import(pathToFileURL(modulePath).href);
    const registrations = module.default;
    // Every @shikijs/langs module ends with its own registration:
    //   export default [...<embedded>, lang]
    const own = registrations[registrations.length - 1];
    if (!own || typeof own !== 'object') {
      throw new Error(`@shikijs/langs/${language} exported no registration.`);
    }
    const deps = [...(own.embeddedLangs ?? [])];
    grammars.set(language, { deps, own });
    pending.push(...deps);
  }
  return grammars;
}

/**
 * Writes `<outDir>/core.js` (the highlighter factory) plus one script per
 * grammar. Returns a summary the caller can log.
 */
export async function writeShikiClassicAssets(outDir) {
  fs.rmSync(outDir, { force: true, recursive: true });
  fs.mkdirSync(outDir, { recursive: true });

  const coreScript = await buildClassicScript(
    [
      `import { createHighlighterCore } from "shiki/core";`,
      `import { createOnigurumaEngine } from "shiki/engine/oniguruma";`,
      `import wasm from "shiki/wasm";`,
      `import light from "@shikijs/themes/github-light-default";`,
      `import dark from "@shikijs/themes/github-dark-default";`,
      `globalThis.${SHIKI_CORE_GLOBAL} = () =>`,
      `  createHighlighterCore({ engine: createOnigurumaEngine(wasm), langs: [], themes: [light, dark] });`,
    ].join('\n')
  );
  fs.writeFileSync(path.join(outDir, SHIKI_CORE_FILE_NAME), coreScript);

  const grammars = await collectGrammarClosure();
  for (const [language, { deps, own }] of grammars) {
    // JSON.parse of a string literal parses faster than an object literal,
    // which is why @shikijs/langs itself stores grammars this way.
    const script =
      `(globalThis.${SHIKI_LANGS_GLOBAL} ??= {})[${jsStringLiteral(language)}] = ` +
      `{deps:${JSON.stringify(deps)},lang:JSON.parse(${jsStringLiteral(JSON.stringify(own))})};\n`;
    fs.writeFileSync(path.join(outDir, `${language}.js`), script);
  }

  let bytes = 0;
  for (const name of fs.readdirSync(outDir)) {
    bytes += fs.statSync(path.join(outDir, name)).size;
  }
  return { bytes, grammarCount: grammars.size };
}

/**
 * The loader both shims share. A `<script src>` is the only way a file://
 * document can pull in code at runtime, which is also why the chat page loads
 * Monaco through its classic AMD loader on gpui.
 */
const CLASSIC_LOADER_SOURCE = `
const scripts = new Map();

function loadShikiScript(fileName) {
  let pending = scripts.get(fileName);
  if (pending) {
    return pending;
  }
  pending = new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.async = true;
    script.src = "./${SHIKI_ASSET_DIR_NAME}/" + fileName;
    script.addEventListener("load", () => resolve());
    script.addEventListener("error", () => {
      reject(new Error("Shiki asset " + fileName + " could not be loaded"));
    });
    document.head.appendChild(script);
  });
  scripts.set(fileName, pending);
  return pending;
}
`;

const GRAMMARS_SHIM_SOURCE = `${CLASSIC_LOADER_SOURCE}
function loadRegistration(language) {
  return loadShikiScript(language + ".js").then(() => {
    const entry = globalThis.${SHIKI_LANGS_GLOBAL}?.[language];
    if (!entry) {
      throw new Error("Shiki grammar " + language + " loaded but registered nothing");
    }
    return entry;
  });
}

// Depth-first over embedded grammars so every dependency is registered before
// the grammar that embeds it, which is the order the flattened
// @shikijs/langs export uses. The visited set also breaks embed cycles.
async function collectRegistrations(language, visited, out) {
  if (visited.has(language)) {
    return;
  }
  visited.add(language);
  const entry = await loadRegistration(language);
  for (const dep of entry.deps) {
    await collectRegistrations(dep, visited, out);
  }
  out.push(entry.lang);
}

export function loadSessionChatGrammar(language) {
  const registrations = [];
  return collectRegistrations(language, new Set(), registrations).then(
    () => registrations,
  );
}
`;

const ENGINE_SHIM_SOURCE = `${CLASSIC_LOADER_SOURCE}
export const SESSION_CHAT_HIGHLIGHTING_AVAILABLE = true;

export function createSessionChatHighlighterCore() {
  return loadShikiScript("${SHIKI_CORE_FILE_NAME}").then(() => {
    const factory = globalThis.${SHIKI_CORE_GLOBAL};
    if (!factory) {
      throw new Error("Shiki core loaded but registered no factory");
    }
    return factory();
  });
}
`;

/**
 * esbuild plugin replacing the two dynamic-import modules with the classic
 * script loader. Both bundles that need it (gpui's single-file CEF entries and
 * the mobile chat page) use this one plugin so their loaders cannot drift.
 */
export function shikiClassicScriptEsbuildPlugin() {
  return {
    name: 'ghostex-shiki-classic-scripts',
    setup(build) {
      build.onResolve({ filter: /(^|\/)session-chat-code-grammars(\.ts)?$/ }, () => ({
        namespace: 'ghostex-shiki-classic',
        path: 'grammars',
      }));
      build.onResolve({ filter: /(^|\/)session-chat-shiki-engine(\.ts)?$/ }, () => ({
        namespace: 'ghostex-shiki-classic',
        path: 'engine',
      }));
      build.onLoad({ filter: /.*/, namespace: 'ghostex-shiki-classic' }, (args) => ({
        contents: args.path === 'grammars' ? GRAMMARS_SHIM_SOURCE : ENGINE_SHIM_SOURCE,
        loader: 'js',
      }));
    },
  };
}
