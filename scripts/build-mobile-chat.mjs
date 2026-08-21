#!/usr/bin/env node
/*
CDXC:SessionChatMobileWebview 2026-07-31:
Builds mobile-chat/session-chat-main.tsx (the shared Session Chat page the
React Native app hosts in a webview) into a real asset directory that ships
inside the app bundle.

CDXC:SessionChatMobileOrigin 2026-08-21:
This used to emit ONE self-contained HTML string as a generated TypeScript
module, handed to react-native-webview as `source={{ html }}`. That sidestepped
platform differences around bundled assets, but it also left the page with no
base URL, so it could not load anything at runtime — which meant no lazily
loaded Shiki grammars and therefore no syntax highlighting on the phone.

The page now ships as a directory the app loads by URL, so relative
subresources resolve:

  * Android: android/build.gradle in modules/ghostex-native merges
    mobile/assets/webview into the APK assets root, so the page is at
    file:///android_asset/session-chat/index.html.
  * iOS: the GhostexNative podspec copies the same directory into the app
    bundle, so the page is at <Bundle.main>/session-chat/index.html.

The page ITSELF is still one self-contained document — its JS and CSS stay
inline, exactly as before — so the thing the old comment protected (no
multi-file module graph to load before React can mount) is unchanged. Only the
Shiki grammars are separate files, and they load as classic <script src>, which
is the one runtime-loading mechanism a file:// document actually has (measured:
`import()` and `fetch()` both fail there, `<script src>` works).

Run after changing sidebar/chat UI, sidebar styles, or the mobile-chat entry:

  bun run build:mobile-chat
*/

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

import {
  SHIKI_ASSET_DIR_NAME,
  shikiClassicScriptEsbuildPlugin,
  writeShikiClassicAssets,
} from "./shiki-classic-assets.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const entry = path.join(repoRoot, "mobile-chat", "session-chat-main.tsx");
/**
 * Both native projects reference this directory in place (Android through a
 * gradle assets.srcDir, iOS through the podspec's resources), so its name is
 * load-bearing on both platforms and in src/chat/SessionChatWebView.tsx.
 */
export const SESSION_CHAT_ASSET_DIR_NAME = "session-chat";
const webviewAssetsDir = path.join(repoRoot, "mobile", "assets", "webview");
const outDir = path.join(webviewAssetsDir, SESSION_CHAT_ASSET_DIR_NAME);
const legacyHtmlModule = path.join(
  repoRoot,
  "mobile",
  "src",
  "chat",
  "session-chat-html.generated.ts",
);
const agentsOutFile = path.join(
  repoRoot,
  "mobile",
  "src",
  "chat",
  "session-chat-agents.generated.ts",
);

/*
The app's Vite/Bun builds deliberately import SVG source text with an import
attribute. esbuild only accepts the standardized JSON value for a `type`
attribute, so remove that source-only hint in this single-file build; the SVG
dataurl loader below remains compatible with the logo helpers.
*/
const mobileChatTextImportPlugin = {
  name: "mobile-chat-text-imports",
  setup(build) {
    build.onLoad(
      { filter: /sidebar\/(?:agent-logos\.ts|brand-icons\.tsx)$/ },
      async (args) => ({
        contents: (await fs.promises.readFile(args.path, "utf8")).replace(
          /\s+with\s+\{\s*type:\s*["']text["']\s*\}/g,
          "",
        ),
        loader: args.path.endsWith(".tsx") ? "tsx" : "ts",
      }),
    );
  },
};

/*
The mobile submodule cannot import shared/session-chat.ts, so the set of
chat-capable agent ids used to be hand-copied into the RN bridge and drifted
from the shared source. Emit it from the real export instead: the bundle is
evaluated here so the generated list is the runtime value gxserver and every
other client agree on, not a parsed approximation.
*/
const agentsBundle = await esbuild.build({
  bundle: true,
  format: "esm",
  platform: "node",
  stdin: {
    contents: 'export { SESSION_CHAT_SUPPORTED_AGENTS } from "./shared/session-chat";\n',
    loader: "ts",
    resolveDir: repoRoot,
    sourcefile: "session-chat-agents-entry.ts",
  },
  write: false,
});
const agentsModuleSource = agentsBundle.outputFiles[0]?.text ?? "";
const { SESSION_CHAT_SUPPORTED_AGENTS } = await import(
  `data:text/javascript;base64,${Buffer.from(agentsModuleSource).toString("base64")}`
);
const supportedAgents = [...SESSION_CHAT_SUPPORTED_AGENTS];
if (supportedAgents.length === 0) {
  throw new Error("shared/session-chat.ts exported no supported Session Chat agents.");
}

const result = await esbuild.build({
  bundle: true,
  define: { "process.env.NODE_ENV": '"production"' },
  entryPoints: [entry],
  format: "iife",
  jsx: "automatic",
  // Inline every url() asset so the single HTML document has no external
  // references (webview html sources have no usable base URL).
  loader: {
    ".png": "dataurl",
    ".svg": "dataurl",
    ".ttf": "dataurl",
    ".woff": "dataurl",
    ".woff2": "dataurl",
  },
  minify: true,
  outdir: "out",
  plugins: [mobileChatTextImportPlugin, shikiClassicScriptEsbuildPlugin()],
  target: ["safari16", "chrome110"],
  write: false,
});

let js = "";
let css = "";
for (const file of result.outputFiles) {
  if (file.path.endsWith(".js")) js += file.text;
  else if (file.path.endsWith(".css")) css += file.text;
}
if (!js) {
  throw new Error("esbuild produced no JS output for the mobile chat page.");
}

// </script>-shaped strings inside generated JS would end the inline script tag.
const inlineJs = js.replace(/<\/script>/gi, "<\\/script>");
const inlineCss = css.replace(/<\/style>/gi, "<\\/style>");

const html = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover" />
    <title>Ghostex Session Chat</title>
    <style>
      html,
      body {
        background: #111111;
        height: 100%;
        margin: 0;
        overscroll-behavior: none;
        -webkit-text-size-adjust: 100%;
      }
    </style>
    <style>${inlineCss}</style>
  </head>
  <body>
    <div id="root"></div>
    <script>${inlineJs}</script>
  </body>
</html>
`;

const agentsModule = `// GENERATED by scripts/build-mobile-chat.mjs in the Ghostex main repo — do not
// edit by hand. Source: SESSION_CHAT_SUPPORTED_AGENTS in shared/session-chat.ts.
// Regenerate with \`bun run build:mobile-chat\` there.

export const SESSION_CHAT_SUPPORTED_AGENT_IDS: readonly string[] = ${JSON.stringify(
  supportedAgents,
)};
`;

fs.rmSync(outDir, { force: true, recursive: true });
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(path.join(outDir, "index.html"), html);
// Inside the page directory, not beside it: every file directly under
// assets/webview is merged into the APK assets ROOT by the gradle srcDir, and
// that root is shared with the bundled font.
fs.writeFileSync(
  path.join(outDir, "README.md"),
  [
    "# Bundled webview assets",
    "",
    "GENERATED by `scripts/build-mobile-chat.mjs` in the Ghostex main repo — do",
    "not edit by hand. Regenerate with `bun run build:mobile-chat` there.",
    "",
    "This whole directory is merged into the app bundle by BOTH native projects:",
    "",
    "- Android: `modules/ghostex-native/android/build.gradle` adds it as an",
    "  `assets.srcDir`, so `session-chat/` lands at the APK assets root and the",
    "  page is reachable at `file:///android_asset/session-chat/index.html`.",
    "- iOS: `modules/ghostex-native/ios/GhostexNative.podspec` copies it through",
    "  the `WebAssets` symlink, so the page is at",
    "  `<Bundle.main>/session-chat/index.html`.",
    "",
    "`src/chat/SessionChatWebView.tsx` resolves those two URLs.",
    "",
  ].join("\n"),
);

const shiki = await writeShikiClassicAssets(path.join(outDir, SHIKI_ASSET_DIR_NAME));

// The page used to ship as one generated TypeScript string module. Remove it
// so a stale 946 KiB copy cannot linger in the app bundle or be imported by
// mistake after the switch to a real asset directory.
fs.rmSync(legacyHtmlModule, { force: true });

fs.mkdirSync(path.dirname(agentsOutFile), { recursive: true });
fs.writeFileSync(agentsOutFile, agentsModule);

const pageKib = Math.round(Buffer.byteLength(html) / 1024);
console.log(
  `Wrote ${path.relative(repoRoot, path.join(outDir, "index.html"))} (${pageKib} KiB; js ${Math.round(js.length / 1024)} KiB, css ${Math.round(css.length / 1024)} KiB).`,
);
console.log(
  `Wrote ${path.relative(repoRoot, path.join(outDir, SHIKI_ASSET_DIR_NAME))} (${shiki.grammarCount} Shiki grammars + core, ${Math.round(shiki.bytes / 1024)} KiB, loaded on demand).`,
);
console.log(
  `Wrote ${path.relative(repoRoot, agentsOutFile)} (${supportedAgents.length} chat-capable agent ids).`,
);
