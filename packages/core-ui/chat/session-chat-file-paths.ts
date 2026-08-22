/*
 * Inline code that is a file reference, promoted to a clickable chip.
 *
 * Agents write `sidebar/styles/chat.css:913` and `gpui/src/cef/shell.rs:42:8`
 * constantly, and today every one of them is an inert grey span. This module
 * decides which inline-code spans are actually file references, and hands the
 * renderer the pieces it needs to draw one: the path to open, the line/column
 * to open it at, and where that path may be cut if the column is too narrow to
 * show all of it.
 *
 * The bar for saying "yes" is deliberately high. A false positive is worse than
 * a miss: it turns a piece of ordinary prose code — `npm install`, `--flag`,
 * `Array.map`, `origin/main` — into something that looks like a link and then
 * fails to open. So a span only becomes a chip when it carries real path
 * evidence, and everything below is written to say no first.
 *
 * The rule, in order:
 *
 *  1. One token only. Anything with whitespace or a backtick in it is a
 *     command line or a sentence, not a path.
 *  2. Every character has to be path-shaped. Each `/`-separated segment must be
 *     `[A-Za-z0-9._+@~-]+`, which rejects `foo.bar()`, `array[0]`, `a|b`,
 *     `src/**\/*.ts`, `key=value`, `mailto:x`, and `https://…` outright — a
 *     stray `:` that is not a trailing `:line[:column]` is disqualifying, and
 *     so is a backslash outside a Windows path.
 *  3. It has to look like more than a word: either it contains a directory
 *     separator (or announces itself with `/`, `./`, `../`, `~/`, `C:\`, `\\`)
 *     or it carries a trailing `:line`. A bare `README.md` therefore stays
 *     plain inline code — the same call the reference implementation makes,
 *     because agents name files in prose far more often than they mean "open
 *     this". (This is the one clause a fenced block's title is exempt from —
 *     see resolveSessionChatFenceTitleFilePath.)
 *  4. It must not be a host or a version. `example.com/x.html`, `localhost`,
 *     and `1.2.3` are not files.
 *  5. Its basename must name a file: a letter-initial extension (`.ts`, `.rs`,
 *     `.zshrc`) or a conventional extensionless filename (`Makefile`,
 *     `Dockerfile`, `README`). A digit-initial "extension" is a version
 *     number, not a file type, so `release/v1.2` and `p99.9` are refused.
 *
 * Relative paths are handed to the host exactly as the agent wrote them: the
 * chat surface has no cwd of its own, and the host that owns an editor is the
 * one that knows the project root (gpui resolves them against the active
 * project in open_session_chat_file). Resolving here would mean guessing a
 * root, which is precisely the wrong move.
 */

import { IconFile, IconFileCode, IconMarkdown } from "@tabler/icons-react";

export interface SessionChatFilePosition {
  line: number;
  column?: number;
}

export interface SessionChatFilePathRef {
  /**
   * Final path segment. Only the icon and the layout use it: the chip's label
   * is the whole path, and this is the part of it that may not be truncated.
   */
  basename: string;
  /** The path as the agent wrote it, minus the `:line[:column]` suffix. */
  path: string;
  /** Present only when the span carried editor coordinates. */
  position?: SessionChatFilePosition;
}

/** Long enough for any real path; past this it is a blob, not a reference. */
const MAX_CANDIDATE_LENGTH = 240;
/** Whitespace or a backtick means this span holds more than one token. */
const DISQUALIFYING_CHARACTER = /[\s`]/;
const WINDOWS_DRIVE_PREFIX = /^[A-Za-z]:[\\/]/;
const WINDOWS_UNC_PREFIX = /^\\\\/;
const RELATIVE_PATH_PREFIX = /^(?:~\/|\.{1,2}\/)/;
/** Trailing editor coordinates: `:913` or `:42:8`. */
const POSITION_SUFFIX = /:(\d{1,7})(?::(\d{1,7}))?$/;
const PATH_SEGMENT = /^[A-Za-z0-9._+@~-]+$/;
/**
 * A file type starts with a letter. `.ts`, `.rs`, `.zshrc` are extensions;
 * `.2` in `v1.2` and `.9` in `p99.9` are version fragments.
 */
const LETTER_EXTENSION = /\.[A-Za-z][A-Za-z0-9_+-]*$/;
const DOTTED_NUMBER = /^\d+(?:\.\d+)+$/;

/**
 * Conventional filenames that carry no extension. Any other extensionless
 * basename stays plain: `src/utils`, `origin/main`, and `text/plain` are all
 * shaped like paths and none of them is one.
 */
const EXTENSIONLESS_FILE_NAMES = new Set([
  "AUTHORS",
  "BUILD",
  "Brewfile",
  "CHANGELOG",
  "CODEOWNERS",
  "COPYING",
  "Caddyfile",
  "Containerfile",
  "Dockerfile",
  "Fastfile",
  "GNUmakefile",
  "Gemfile",
  "Jenkinsfile",
  "Justfile",
  "LICENCE",
  "LICENSE",
  "Makefile",
  "NOTICE",
  "Podfile",
  "Procfile",
  "README",
  "Rakefile",
  "Vagrantfile",
  "WORKSPACE",
  "justfile",
  "makefile",
]);

/**
 * Enough of a generic-TLD list to catch a bare hostname written without a
 * scheme. Country codes are deliberately absent: `.pl`, `.pt`, `.es`, and
 * `.in` are all real file extensions, and refusing them would cost more real
 * paths than the fake hostnames it would save.
 */
const HOSTNAME_TLDS = new Set([
  "ai",
  "app",
  "biz",
  "cloud",
  "co",
  "com",
  "dev",
  "edu",
  "gov",
  "info",
  "io",
  "net",
  "org",
  "xyz",
]);

/** `example.com`, `localhost`, `127.0.0.1`, `1.2.3` — a host or a version. */
function looksLikeHostOrVersion(segment: string): boolean {
  if (segment === "localhost") return true;
  if (DOTTED_NUMBER.test(segment)) return true;
  const labels = segment.toLowerCase().split(".");
  const lastLabel = labels[labels.length - 1];
  return labels.length > 1 && lastLabel !== undefined && HOSTNAME_TLDS.has(lastLabel);
}

/**
 * Decides whether one inline-code span is a file reference. Returns null for
 * everything the module doc refuses.
 */
export function resolveSessionChatInlineCodeFilePath(
  text: string,
): SessionChatFilePathRef | null {
  return resolveFilePathReference(text, { requirePathEvidence: true });
}

/**
 * The same decision for the title a fenced code block names
 * (```ts src/main.ts, ```json file=package.json), so a path in a fence header
 * and the same path mid-sentence are judged by one rule.
 *
 * One clause is dropped, and only one: rule 3, the demand that a bare word
 * carry a separator or a `:line` before it counts as a path. That rule exists
 * because inline code is ambiguous — an agent naming `README.md` in a sentence
 * usually means the words, not the file. A fence title is not ambiguous: the
 * fence says "this block is that file", which is why the header already draws
 * a file glyph beside it. Everything else still applies, so `v1.2.3`,
 * `example.com`, `showLineNumbers`, and `{1,3-5}` are refused here too.
 */
export function resolveSessionChatFenceTitleFilePath(
  title: string,
): SessionChatFilePathRef | null {
  return resolveFilePathReference(title, { requirePathEvidence: false });
}

function resolveFilePathReference(
  text: string,
  { requirePathEvidence }: { requirePathEvidence: boolean },
): SessionChatFilePathRef | null {
  const trimmed = text.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_CANDIDATE_LENGTH) return null;
  if (DISQUALIFYING_CHARACTER.test(trimmed)) return null;

  const positionMatch = POSITION_SUFFIX.exec(trimmed);
  const path = positionMatch
    ? trimmed.slice(0, trimmed.length - positionMatch[0].length)
    : trimmed;
  if (path.length === 0) return null;

  const isWindowsPath =
    WINDOWS_DRIVE_PREFIX.test(path) || WINDOWS_UNC_PREFIX.test(path);
  // Backslashes only separate directories on a path that announced itself as a
  // Windows path; anywhere else a backslash is an escape, and the segment check
  // below rejects it.
  const normalized = isWindowsPath ? path.replaceAll("\\", "/") : path;
  // The drive letter and the UNC leader carry a colon and a doubled slash that
  // no segment may contain, so they are peeled off before the segment check.
  const body = isWindowsPath
    ? normalized.replace(/^(?:[A-Za-z]:|\/\/)/, "")
    : normalized;

  const segments = body.split("/").filter((segment) => segment.length > 0);
  const basename = segments[segments.length - 1];
  if (basename === undefined) return null;
  if (segments.some((segment) => !PATH_SEGMENT.test(segment))) return null;

  const announcesItselfAsAPath =
    isWindowsPath || normalized.startsWith("/") || RELATIVE_PATH_PREFIX.test(normalized);
  const hasSeparator = announcesItselfAsAPath || segments.length > 1;
  if (requirePathEvidence && !hasSeparator && positionMatch === null) return null;

  const firstSegment = segments[0];
  if (
    !announcesItselfAsAPath &&
    firstSegment !== undefined &&
    looksLikeHostOrVersion(firstSegment)
  ) {
    return null;
  }

  if (!LETTER_EXTENSION.test(basename) && !EXTENSIONLESS_FILE_NAMES.has(basename)) {
    return null;
  }

  const line = positionMatch?.[1];
  const column = positionMatch?.[2];
  return {
    basename,
    path,
    ...(line === undefined
      ? {}
      : {
          position: {
            line: Number(line),
            ...(column === undefined ? {} : { column: Number(column) }),
          },
        }),
  };
}

/**
 * The chip's visible text, split where it is allowed to be cut.
 *
 * `gpui/src/cef/shell.rs:42:8` becomes parent `gpui/src/cef`, name
 * `/shell.rs`, and detail `L42:C8`. The chip shows the whole path, so in a
 * narrow transcript column something has to give when it does not fit — and
 * the part that may go is the tail of the parent. The leading folder says
 * which of the repo's worlds this is, the filename says what it is, and the
 * coordinates say where; the directories in between are the only part a reader
 * can lose and still know what they are looking at.
 *
 * The separator goes with the name, not with the parent, so that a truncated
 * parent still ends in one: `gpui/src/c…/shell.rs` reads as a path with a hole
 * in it, while `gpui/src/c…shell.rs` reads as a typo.
 */
export function sessionChatFilePathChipLabel(ref: SessionChatFilePathRef): {
  detail: string;
  name: string;
  parent: string;
} {
  // Split the path as written rather than by its normalized segments, so a
  // Windows path keeps its backslashes both on screen and in the split.
  const separatorIndex = Math.max(ref.path.lastIndexOf("/"), ref.path.lastIndexOf("\\"));
  const { column, line } = ref.position ?? {};
  return {
    detail:
      line === undefined ? "" : `L${line}${column === undefined ? "" : `:C${column}`}`,
    name: separatorIndex < 0 ? ref.path : ref.path.slice(separatorIndex),
    parent: separatorIndex < 0 ? "" : ref.path.slice(0, separatorIndex),
  };
}

/** The path plus its coordinates, as the tooltip and the title attribute show it. */
export function sessionChatFilePathTitle(ref: SessionChatFilePathRef): string {
  if (!ref.position) return ref.path;
  const { column, line } = ref.position;
  return `${ref.path}:${line}${column === undefined ? "" : `:${column}`}`;
}

/*
 * Ghostex has no file-type icon set of its own — the Docs tree keeps a private
 * three-way switch and nothing else — so rather than pull in a new icon
 * dependency for this, the chip picks from @tabler/icons-react, which is
 * already the house set. Three glyphs is the whole vocabulary: prose, source,
 * and everything else.
 */
const MARKDOWN_EXTENSIONS = new Set(["markdown", "md", "mdown", "mdx", "mkdn", "rst"]);
const CODE_EXTENSIONS = new Set([
  "bash",
  "c",
  "cc",
  "cjs",
  "cpp",
  "cs",
  "css",
  "dart",
  "ex",
  "exs",
  "fish",
  "go",
  "gradle",
  "h",
  "hpp",
  "hs",
  "html",
  "java",
  "js",
  "json",
  "jsonc",
  "jsx",
  "kt",
  "kts",
  "lua",
  "m",
  "mjs",
  "mm",
  "php",
  "pl",
  "py",
  "rb",
  "rs",
  "scala",
  "scss",
  "sh",
  "sql",
  "svelte",
  "swift",
  "toml",
  "ts",
  "tsx",
  "vue",
  "xml",
  "yaml",
  "yml",
  "zig",
  "zsh",
]);

export function sessionChatFilePathIcon(basename: string): typeof IconFile {
  const extension = basename.slice(basename.lastIndexOf(".") + 1).toLowerCase();
  if (MARKDOWN_EXTENSIONS.has(extension)) return IconMarkdown;
  if (CODE_EXTENSIONS.has(extension) || EXTENSIONLESS_FILE_NAMES.has(basename)) {
    return IconFileCode;
  }
  return IconFile;
}

/*
 * Marks every inline-code node so the shared `code` renderer can tell an inline
 * span from a fence's body — react-markdown hands both to the same component,
 * and only the inline ones may become chips. Spans inside a link are skipped:
 * the link already decides where that text goes.
 */
interface MarkdownAstNode {
  children?: MarkdownAstNode[];
  data?: {
    hProperties?: Record<string, unknown>;
  };
  type?: string;
}

export function remarkSessionChatInlineCode() {
  return (tree: MarkdownAstNode) => {
    const visit = (node: MarkdownAstNode, insideLink: boolean) => {
      if (node.type === "inlineCode" && !insideLink) {
        node.data = {
          ...node.data,
          hProperties: { ...node.data?.hProperties, dataInlineCode: "" },
        };
      }
      const childInsideLink =
        insideLink || node.type === "link" || node.type === "linkReference";
      node.children?.forEach((child) => visit(child, childInsideLink));
    };
    visit(tree, false);
  };
}
