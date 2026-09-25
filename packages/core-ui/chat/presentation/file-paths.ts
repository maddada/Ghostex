/*
 * Which strings in a chat message are file references, decided once for every
 * renderer.
 *
 * The rule, the reasoning behind each clause, and the deliberately high bar for
 * saying "yes" live in packages/core-ui/chat/session-chat-file-paths.ts, which
 * re-exports everything here and keeps the icon component mapping. The native
 * chat's port of these rules is packages/gx-chat-core/src/transcript/.
 *
 * CDXC:SessionChat 2026-09-18 SEE-ALSO:
 * The GPUI transcript consumes these through native-markdown.ts; React consumes
 * them through session-chat-file-paths.ts and session-chat-code-fence-meta.ts.
 * A path that becomes a chip in one renderer must become a pill in the other.
 */

import { splitSessionChatFilePosition, type SessionChatFilePosition } from './file-position';

export interface SessionChatFilePathRef {
  /**
   * Final path segment. Only the icon and the layout use it: the chip's label
   * is the whole path, and this is the part of it that may not be truncated.
   */
  basename: string;
  /** The path as the agent wrote it, minus its line, range, or column suffix. */
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
export const SESSION_CHAT_EXTENSIONLESS_FILE_NAMES = new Set([
  'AUTHORS',
  'BUILD',
  'Brewfile',
  'CHANGELOG',
  'CODEOWNERS',
  'COPYING',
  'Caddyfile',
  'Containerfile',
  'Dockerfile',
  'Fastfile',
  'GNUmakefile',
  'Gemfile',
  'Jenkinsfile',
  'Justfile',
  'LICENCE',
  'LICENSE',
  'Makefile',
  'NOTICE',
  'Podfile',
  'Procfile',
  'README',
  'Rakefile',
  'Vagrantfile',
  'WORKSPACE',
  'justfile',
  'makefile',
]);

/**
 * Enough of a generic-TLD list to catch a bare hostname written without a
 * scheme. Country codes are deliberately absent: `.pl`, `.pt`, `.es`, and
 * `.in` are all real file extensions, and refusing them would cost more real
 * paths than the fake hostnames it would save.
 */
const HOSTNAME_TLDS = new Set([
  'ai',
  'app',
  'biz',
  'cloud',
  'co',
  'com',
  'dev',
  'edu',
  'gov',
  'info',
  'io',
  'net',
  'org',
  'xyz',
]);

/** `example.com`, `localhost`, `127.0.0.1`, `1.2.3` — a host or a version. */
function looksLikeHostOrVersion(segment: string): boolean {
  if (segment === 'localhost') return true;
  if (DOTTED_NUMBER.test(segment)) return true;
  const labels = segment.toLowerCase().split('.');
  const lastLabel = labels[labels.length - 1];
  return labels.length > 1 && lastLabel !== undefined && HOSTNAME_TLDS.has(lastLabel);
}

/**
 * Decides whether one inline-code span is a file reference. Returns null for
 * everything the module doc refuses.
 */
export function resolveSessionChatInlineCodeFilePath(text: string): SessionChatFilePathRef | null {
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
export function resolveSessionChatFenceTitleFilePath(title: string): SessionChatFilePathRef | null {
  return resolveFilePathReference(title, { requirePathEvidence: false });
}

function resolveFilePathReference(
  text: string,
  { requirePathEvidence }: { requirePathEvidence: boolean }
): SessionChatFilePathRef | null {
  const trimmed = text.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_CANDIDATE_LENGTH) return null;
  if (DISQUALIFYING_CHARACTER.test(trimmed)) return null;

  const { path, position } = splitSessionChatFilePosition(trimmed);
  if (path.length === 0) return null;

  const isWindowsPath = WINDOWS_DRIVE_PREFIX.test(path) || WINDOWS_UNC_PREFIX.test(path);
  // Backslashes only separate directories on a path that announced itself as a
  // Windows path; anywhere else a backslash is an escape, and the segment check
  // below rejects it.
  const normalized = isWindowsPath ? path.replaceAll('\\', '/') : path;
  // The drive letter and the UNC leader carry a colon and a doubled slash that
  // no segment may contain, so they are peeled off before the segment check.
  const body = isWindowsPath ? normalized.replace(/^(?:[A-Za-z]:|\/\/)/, '') : normalized;

  const segments = body.split('/').filter((segment) => segment.length > 0);
  const basename = segments[segments.length - 1];
  if (basename === undefined) return null;
  if (segments.some((segment) => !PATH_SEGMENT.test(segment))) return null;

  const announcesItselfAsAPath = isWindowsPath || normalized.startsWith('/') || RELATIVE_PATH_PREFIX.test(normalized);
  const hasSeparator = announcesItselfAsAPath || segments.length > 1;
  if (requirePathEvidence && !hasSeparator && position === undefined) return null;

  const firstSegment = segments[0];
  if (!announcesItselfAsAPath && firstSegment !== undefined && looksLikeHostOrVersion(firstSegment)) {
    return null;
  }

  if (!LETTER_EXTENSION.test(basename) && !SESSION_CHAT_EXTENSIONLESS_FILE_NAMES.has(basename)) {
    return null;
  }

  return {
    basename,
    path,
    ...(position === undefined ? {} : { position }),
  };
}

/*
 * Ghostex has no file-type icon set of its own, so the chip picks from the
 * house icon set. Three glyphs is the whole vocabulary: prose, source, and
 * everything else. React maps these names onto @tabler/icons-react components
 * and the GPUI transcript onto the matching SVG assets.
 */
export type SessionChatFilePathIconName = 'markdown' | 'file-code' | 'file';

const MARKDOWN_EXTENSIONS = new Set(['markdown', 'md', 'mdown', 'mdx', 'mkdn', 'rst']);
const CODE_EXTENSIONS = new Set([
  'bash',
  'c',
  'cc',
  'cjs',
  'cpp',
  'cs',
  'css',
  'dart',
  'ex',
  'exs',
  'fish',
  'go',
  'gradle',
  'h',
  'hpp',
  'hs',
  'html',
  'java',
  'js',
  'json',
  'jsonc',
  'jsx',
  'kt',
  'kts',
  'lua',
  'm',
  'mjs',
  'mm',
  'php',
  'pl',
  'py',
  'rb',
  'rs',
  'scala',
  'scss',
  'sh',
  'sql',
  'svelte',
  'swift',
  'toml',
  'ts',
  'tsx',
  'vue',
  'xml',
  'yaml',
  'yml',
  'zig',
  'zsh',
]);

export function sessionChatFilePathIconName(basename: string): SessionChatFilePathIconName {
  const extension = basename.slice(basename.lastIndexOf('.') + 1).toLowerCase();
  if (MARKDOWN_EXTENSIONS.has(extension)) return 'markdown';
  if (CODE_EXTENSIONS.has(extension) || SESSION_CHAT_EXTENSIONLESS_FILE_NAMES.has(basename)) {
    return 'file-code';
  }
  return 'file';
}

/*
 * Fence meta: everything an agent writes after the language on a ``` line.
 *
 *     ```ts title="src/main.ts"
 *     ```ts file=src/main.ts
 *     ```ts filename=src/main.ts
 *     ```ts src/main.ts
 *
 * All four name the file the block came from, and agents use all four. The
 * header shows that name instead of the bare language when it is there, which
 * is the difference between "ts" and "the file you are about to edit".
 */
const FENCE_TITLE_ATTRIBUTE = /(?:^|\s)(?:title|file(?:name)?)=(?:"([^"]+)"|'([^']+)'|(\S+))/i;
/*
 * A bare token only counts as a filename when it reads like one: something
 * before a dot, an extension after it, and nothing but path characters in
 * between. Prose meta such as `showLineNumbers` or `{1,3-5}` must stay out of
 * the header, and so must a version number sitting on its own.
 */
const FENCE_FILENAME_TOKEN = /^[\w@][\w@./-]*\.[A-Za-z0-9]+$/;

/** The filename this fence names, or null when it names none. */
export function sessionChatFenceTitle(meta: string | null): string | null {
  if (meta === null) {
    return null;
  }
  const attribute = FENCE_TITLE_ATTRIBUTE.exec(meta);
  const named = attribute?.[1] ?? attribute?.[2] ?? attribute?.[3];
  if (named) {
    return named;
  }
  return meta.split(/\s+/).find((token) => FENCE_FILENAME_TOKEN.test(token)) ?? null;
}

/** Composer mentions may quote paths containing spaces; ordinary prose is tokenized on whitespace. */
const BARE_PROSE_TOKEN = /[([{'"<]*@"[^\r\n]+?"(?=$|[\s)\]},.!?;'">])|\S+/g;
/** Sentence punctuation that cannot be part of a path under PATH_SEGMENT. */
const LEADING_PROSE_PUNCTUATION = /^[([{'"<]+/;
const TRAILING_PROSE_PUNCTUATION = /[)\]},.!?;'">]+$/;

export interface SessionChatBareFilePath {
  /** Offset of the candidate inside the scanned text. */
  start: number;
  end: number;
  /** The path the renderer opens: an `@mention` has already lost its marker. */
  path: string;
  /** True when the author asked for it with `@`, rather than it being spotted in prose. */
  mention: boolean;
}

/**
 * Finds the paths a person typed into a chat message without marking them up:
 * `packages/shared/x.ts:12`, `@src/main.rs`, `@"my notes/plan.md"`.
 *
 * Candidate discovery only splits on whitespace and sentence punctuation;
 * resolveSessionChatInlineCodeFilePath still makes every implicit path /
 * not-path decision. Callers are responsible for skipping text that is already
 * a link, code, or HTML.
 */
export function sessionChatBareFilePaths(text: string): SessionChatBareFilePath[] {
  const found: SessionChatBareFilePath[] = [];
  const pattern = new RegExp(BARE_PROSE_TOKEN.source, 'g');
  let match = pattern.exec(text);
  while (match !== null) {
    const rawToken = match[0];
    const leading = LEADING_PROSE_PUNCTUATION.exec(rawToken)?.[0].length ?? 0;
    const withoutLeading = rawToken.slice(leading);
    const quotedMention = withoutLeading.startsWith('@"') && withoutLeading.endsWith('"');
    let trailing = quotedMention ? 0 : (TRAILING_PROSE_PUNCTUATION.exec(withoutLeading)?.[0].length ?? 0);
    if (withoutLeading.startsWith('@') && !quotedMention) {
      // Keep closing delimiters owned by the filename, such as @report(final).pdf or @reports/(final).
      while (trailing > 0) {
        const closing = withoutLeading[withoutLeading.length - trailing];
        const opening = closing === ')' ? '(' : closing === ']' ? '[' : closing === '}' ? '{' : null;
        if (!opening) break;
        const kept = withoutLeading.slice(0, withoutLeading.length - trailing);
        if (kept.split(opening).length <= kept.split(closing!).length) break;
        trailing -= 1;
      }
    }
    const candidate = withoutLeading.slice(0, withoutLeading.length - trailing);
    const mentionPath = quotedMention
      ? candidate.slice(2, -1)
      : candidate.startsWith('@') && !candidate.startsWith('@"')
        ? candidate.slice(1)
        : '';
    const reference = mentionPath === '' ? resolveSessionChatInlineCodeFilePath(candidate) : null;
    if (mentionPath !== '' || reference) {
      const start = match.index + leading;
      found.push({
        start,
        end: start + candidate.length,
        path: mentionPath !== '' ? mentionPath : candidate,
        mention: mentionPath !== '',
      });
    }
    match = pattern.exec(text);
  }
  return found;
}
