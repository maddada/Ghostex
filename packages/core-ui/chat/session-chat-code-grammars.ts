// The default Shiki grammar loader: one lazy chunk per language.
//
// Every specifier is written out as a literal on purpose. Bundlers can only
// code-split a dynamic import whose specifier is static, so a computed
// `import(`@shikijs/langs/${language}`)` would either fail to resolve or pull
// all ~700 grammars into the chat bundle. As written, each grammar is its own
// chunk, fetched the first time a fence in that language is rendered.
//
// gpui's packaged CEF surfaces REPLACE this module at build time
// (`createCefSingleFileEsbuildPlugin` in apps/desktop/vite.config.ts). Those pages are
// file:// documents, where Chromium refuses module scripts from the opaque
// origin — the same reason chat.html loads Monaco through its classic AMD
// loader — so gpui swaps in a `<script src="./shiki-langs/<lang>.js">` loader
// over the grammars staged next to the bundle. Keep this module's public
// surface to exactly `loadSessionChatGrammar` so that swap stays a one-liner.

import type { SessionChatCodeLanguage } from "./session-chat-code-languages";

const GRAMMAR_LOADERS: Record<SessionChatCodeLanguage, () => Promise<unknown>> = {
  "astro": () => import("@shikijs/langs/astro"),
  "c": () => import("@shikijs/langs/c"),
  "clojure": () => import("@shikijs/langs/clojure"),
  "cpp": () => import("@shikijs/langs/cpp"),
  "csharp": () => import("@shikijs/langs/csharp"),
  "css": () => import("@shikijs/langs/css"),
  "dart": () => import("@shikijs/langs/dart"),
  "diff": () => import("@shikijs/langs/diff"),
  "dockerfile": () => import("@shikijs/langs/dockerfile"),
  "elixir": () => import("@shikijs/langs/elixir"),
  "erlang": () => import("@shikijs/langs/erlang"),
  "fsharp": () => import("@shikijs/langs/fsharp"),
  "go": () => import("@shikijs/langs/go"),
  "graphql": () => import("@shikijs/langs/graphql"),
  "groovy": () => import("@shikijs/langs/groovy"),
  "haskell": () => import("@shikijs/langs/haskell"),
  "hcl": () => import("@shikijs/langs/hcl"),
  "html": () => import("@shikijs/langs/html"),
  "http": () => import("@shikijs/langs/http"),
  "ini": () => import("@shikijs/langs/ini"),
  "java": () => import("@shikijs/langs/java"),
  "javascript": () => import("@shikijs/langs/javascript"),
  "json": () => import("@shikijs/langs/json"),
  "jsonc": () => import("@shikijs/langs/jsonc"),
  "jsx": () => import("@shikijs/langs/jsx"),
  "julia": () => import("@shikijs/langs/julia"),
  "kotlin": () => import("@shikijs/langs/kotlin"),
  "less": () => import("@shikijs/langs/less"),
  "lua": () => import("@shikijs/langs/lua"),
  "make": () => import("@shikijs/langs/make"),
  "markdown": () => import("@shikijs/langs/markdown"),
  "nix": () => import("@shikijs/langs/nix"),
  "objective-c": () => import("@shikijs/langs/objective-c"),
  "ocaml": () => import("@shikijs/langs/ocaml"),
  "perl": () => import("@shikijs/langs/perl"),
  "php": () => import("@shikijs/langs/php"),
  "powershell": () => import("@shikijs/langs/powershell"),
  "prisma": () => import("@shikijs/langs/prisma"),
  "proto": () => import("@shikijs/langs/proto"),
  "python": () => import("@shikijs/langs/python"),
  "r": () => import("@shikijs/langs/r"),
  "regexp": () => import("@shikijs/langs/regexp"),
  "ruby": () => import("@shikijs/langs/ruby"),
  "rust": () => import("@shikijs/langs/rust"),
  "scala": () => import("@shikijs/langs/scala"),
  "scss": () => import("@shikijs/langs/scss"),
  "shellscript": () => import("@shikijs/langs/shellscript"),
  "solidity": () => import("@shikijs/langs/solidity"),
  "sql": () => import("@shikijs/langs/sql"),
  "svelte": () => import("@shikijs/langs/svelte"),
  "swift": () => import("@shikijs/langs/swift"),
  "toml": () => import("@shikijs/langs/toml"),
  "tsx": () => import("@shikijs/langs/tsx"),
  "typescript": () => import("@shikijs/langs/typescript"),
  "vim": () => import("@shikijs/langs/vim"),
  "vue": () => import("@shikijs/langs/vue"),
  "xml": () => import("@shikijs/langs/xml"),
  "yaml": () => import("@shikijs/langs/yaml"),
  "zig": () => import("@shikijs/langs/zig"),
};

export function loadSessionChatGrammar(
  language: SessionChatCodeLanguage,
): Promise<unknown> {
  return GRAMMAR_LOADERS[language]();
}
