// Which fence languages Session Chat can highlight, and what agents call them.
//
// Data only — deliberately no `import()` calls, because both grammar loaders
// depend on this table:
//
//   * session-chat-code-grammars.ts   (dynamic import, one chunk per grammar)
//   * gpui's CEF build shim           (classic <script>, see gpui/vite.config.ts)
//
// Every key is the real `@shikijs/langs/<name>` subpath so the gpui build can
// stage exactly this list without a second name mapping.

export const SESSION_CHAT_CODE_LANGUAGES = [
  "astro",
  "c",
  "clojure",
  "cpp",
  "csharp",
  "css",
  "dart",
  "diff",
  "dockerfile",
  "elixir",
  "erlang",
  "fsharp",
  "go",
  "graphql",
  "groovy",
  "haskell",
  "hcl",
  "html",
  "http",
  "ini",
  "java",
  "javascript",
  "json",
  "jsonc",
  "jsx",
  "julia",
  "kotlin",
  "less",
  "lua",
  "make",
  "markdown",
  "nix",
  "objective-c",
  "ocaml",
  "perl",
  "php",
  "powershell",
  "prisma",
  "proto",
  "python",
  "r",
  "regexp",
  "ruby",
  "rust",
  "scala",
  "scss",
  "shellscript",
  "solidity",
  "sql",
  "svelte",
  "swift",
  "toml",
  "tsx",
  "typescript",
  "vim",
  "vue",
  "xml",
  "yaml",
  "zig",
] as const;

export type SessionChatCodeLanguage =
  (typeof SESSION_CHAT_CODE_LANGUAGES)[number];

const LANGUAGE_KEYS: ReadonlySet<string> = new Set(SESSION_CHAT_CODE_LANGUAGES);

/**
 * Fence info strings agents actually write, mapped onto the grammar keys above.
 * A fence whose language is missing here renders as today's plain `<pre>` — an
 * unsupported grammar is a normal outcome, not an error to paper over.
 */
const LANGUAGE_ALIASES: Record<string, SessionChatCodeLanguage> = {
  "c#": "csharp",
  "c++": "cpp",
  "obj-c": "objective-c",
  astro: "astro",
  bash: "shellscript",
  c: "c",
  cc: "cpp",
  cfg: "ini",
  cjs: "javascript",
  clj: "clojure",
  clojure: "clojure",
  conf: "ini",
  console: "shellscript",
  cpp: "cpp",
  cs: "csharp",
  csharp: "csharp",
  css: "css",
  cts: "typescript",
  cxx: "cpp",
  dart: "dart",
  diff: "diff",
  docker: "dockerfile",
  dockerfile: "dockerfile",
  dotenv: "ini",
  editorconfig: "ini",
  elixir: "elixir",
  env: "ini",
  erl: "erlang",
  erlang: "erlang",
  ex: "elixir",
  exs: "elixir",
  fish: "shellscript",
  fs: "fsharp",
  fsharp: "fsharp",
  // Shiki ships no gitignore grammar; ini is the closest match.
  gitignore: "ini",
  go: "go",
  golang: "go",
  gql: "graphql",
  graphql: "graphql",
  groovy: "groovy",
  h: "c",
  haskell: "haskell",
  hcl: "hcl",
  hpp: "cpp",
  hs: "haskell",
  htm: "html",
  html: "html",
  http: "http",
  ini: "ini",
  java: "java",
  javascript: "javascript",
  jl: "julia",
  js: "javascript",
  json: "json",
  json5: "jsonc",
  jsonc: "jsonc",
  jsx: "jsx",
  julia: "julia",
  kotlin: "kotlin",
  kt: "kotlin",
  kts: "kotlin",
  less: "less",
  lua: "lua",
  make: "make",
  makefile: "make",
  markdown: "markdown",
  md: "markdown",
  mdx: "markdown",
  mjs: "javascript",
  ml: "ocaml",
  mts: "typescript",
  nix: "nix",
  node: "javascript",
  objc: "objective-c",
  "objective-c": "objective-c",
  ocaml: "ocaml",
  patch: "diff",
  perl: "perl",
  php: "php",
  pl: "perl",
  powershell: "powershell",
  prisma: "prisma",
  properties: "ini",
  proto: "proto",
  protobuf: "proto",
  ps: "powershell",
  ps1: "powershell",
  psql: "sql",
  pwsh: "powershell",
  py: "python",
  python: "python",
  python3: "python",
  r: "r",
  rb: "ruby",
  regex: "regexp",
  regexp: "regexp",
  rs: "rust",
  ruby: "ruby",
  rust: "rust",
  sass: "scss",
  scala: "scala",
  scss: "scss",
  sh: "shellscript",
  shell: "shellscript",
  shellscript: "shellscript",
  sol: "solidity",
  solidity: "solidity",
  sql: "sql",
  svelte: "svelte",
  svg: "xml",
  swift: "swift",
  terminal: "shellscript",
  terraform: "hcl",
  tf: "hcl",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  typescript: "typescript",
  udiff: "diff",
  vim: "vim",
  vimscript: "vim",
  vue: "vue",
  xml: "xml",
  yaml: "yaml",
  yml: "yaml",
  zig: "zig",
  zsh: "shellscript",
};

/**
 * Resolves a fence info string to a loadable grammar key, or `null` when the
 * fence has no language or names one we ship no grammar for.
 */
export function resolveSessionChatCodeLanguage(
  info: string | null | undefined,
): SessionChatCodeLanguage | null {
  if (typeof info !== "string") {
    return null;
  }
  const normalized = info.trim().toLowerCase();
  if (normalized === "") {
    return null;
  }
  const alias = LANGUAGE_ALIASES[normalized];
  if (alias) {
    return alias;
  }
  return LANGUAGE_KEYS.has(normalized)
    ? (normalized as SessionChatCodeLanguage)
    : null;
}
