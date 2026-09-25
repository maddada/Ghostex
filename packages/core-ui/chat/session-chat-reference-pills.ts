export type SessionChatReferenceKind = 'file' | 'folder' | 'image' | 'skill' | 'url';

export interface SessionChatComposerReference {
  end: number;
  identity: string;
  kind: SessionChatReferenceKind;
  label: string;
  path: string;
  start: number;
  revealed?: boolean;
}

export const SESSION_CHAT_REFERENCE_REVEAL_MARKER = '·';
const REFERENCE_PILL_MAX_LABEL_CHARACTERS = 18;
const REFERENCE_PILL_ICON_SPACE = '\u00a0\u00a0\u00a0\u00a0\u2009';
const REFERENCE_PILL_TRAILING_SPACE = '\u2009';

const REFERENCE_LABEL_PATTERN = /\[((?:\\.|[^\]\\\r\n])+)]\(/g;
const IMAGE_PATH_PATTERN = /\.(?:avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)(?:[?#].*)?$/i;
const FILE_EXTENSION_PATTERN = /\.[A-Za-z][A-Za-z0-9_+-]*$/;
const VIDEO_PATH_PATTERN = /\.(?:3gp|avi|flv|m2ts|m4v|mkv|mov|mp4|mpe?g|mts|ogv|webm|wmv)(?::\d+(?::\d+)?)?$/i;
const AUDIO_PATH_PATTERN = /\.(?:aac|aiff?|flac|m4a|mp3|oga|ogg|opus|wav|wma)(?::\d+(?::\d+)?)?$/i;
const PDF_PATH_PATTERN = /\.pdf(?::\d+(?::\d+)?)?$/i;

/** Files that only a system app can play or show; opening them in Code is never right. */
export type SessionChatMediaKind = 'audio' | 'pdf' | 'video';

/**
 * CDXC:SessionChat 2026-09-24 DECISION:
 * User: a pasted video reads "Video #1" instead of "File #1", its menu names what it is, and clicking it opens it normally with the OS default app on macOS, Windows, and Linux instead of the code editor. Audio and PDFs follow the same rule.
 * SEE-ALSO: `open_session_chat_file_for_session` in `apps/desktop/src/app/session_chat.rs` keeps the same extension list for the click.
 */
export function sessionChatMediaKind(path: string): SessionChatMediaKind | null {
  if (VIDEO_PATH_PATTERN.test(path)) return 'video';
  if (AUDIO_PATH_PATTERN.test(path)) return 'audio';
  if (PDF_PATH_PATTERN.test(path)) return 'pdf';
  return null;
}

const MEDIA_NOUNS: Record<SessionChatMediaKind, string> = { audio: 'Audio', pdf: 'PDF', video: 'Video' };

/** The noun a reference to `path` is named with: `Video` in "[Video #1](clip.mp4)", "Open Video Location". */
export function sessionChatPathNoun(path: string): string {
  const media = sessionChatMediaKind(path);
  if (media) return MEDIA_NOUNS[media];
  const kind = sessionChatReferenceKind('', path);
  return kind === 'image' ? 'Image' : kind === 'folder' ? 'Folder' : 'File';
}
const EXTENSIONLESS_FILE_NAMES = new Set([
  'AGENTS',
  'AUTHORS',
  'BUILD',
  'Brewfile',
  'CHANGELOG',
  'CODEOWNERS',
  'COPYING',
  'Caddyfile',
  'Containerfile',
  'Dockerfile',
  'Gemfile',
  'LICENSE',
  'Makefile',
  'NOTICE',
  'Podfile',
  'Procfile',
  'README',
  'SKILL',
  'WORKSPACE',
]);

function unescapeMarkdown(value: string): string {
  return value.replace(/\\(.)/g, '$1');
}

function explicitReferenceKind(label: string): SessionChatReferenceKind | null {
  if (label.endsWith(SESSION_CHAT_REFERENCE_REVEAL_MARKER)) return null;
  if (/^Image #\d+$/.test(label)) return 'image';
  if (/^(?:Audio|File|PDF|Video) #\d+$/.test(label)) return 'file';
  if (/^Folder #\d+$/.test(label)) return 'folder';
  if (label.startsWith('$')) return 'skill';
  return null;
}

/** The compact label shared by every editable reference-pill backend. */
export function sessionChatReferenceDisplayLabel(label: string, kind: SessionChatReferenceKind): string {
  if (kind === 'skill') {
    return label;
  }
  const characters = [...label];
  if (characters.length <= REFERENCE_PILL_MAX_LABEL_CHARACTERS) {
    return label;
  }
  return `${characters.slice(0, REFERENCE_PILL_MAX_LABEL_CHARACTERS - 1).join('')}\u2026`;
}

/** Visible text whose measured width owns the pill icon and label. */
export function sessionChatReferencePillText(label: string, kind: SessionChatReferenceKind): string {
  return `${REFERENCE_PILL_ICON_SPACE}${sessionChatReferenceDisplayLabel(label, kind).replaceAll(' ', '\u00a0')}${REFERENCE_PILL_TRAILING_SPACE}`;
}

/** Classifies any rendered machine-path link for the shared pill styling. */
export function sessionChatReferenceKind(label: string, path: string): SessionChatReferenceKind {
  const explicit = explicitReferenceKind(label.trim());
  if (explicit && (explicit !== 'skill' || /(?:^|[\\/])SKILL\.md$/i.test(path))) {
    return explicit;
  }
  if (/^https?:\/\//i.test(path)) return 'url';
  if (IMAGE_PATH_PATTERN.test(path)) {
    return 'image';
  }
  if (/\b(?:folder|directory)\b/i.test(label) || /[\\/]$/.test(path)) {
    return 'folder';
  }
  const withoutPosition = path.replace(/:\d+(?::\d+)?$/, '');
  const separator = Math.max(withoutPosition.lastIndexOf('/'), withoutPosition.lastIndexOf('\\'));
  const basename = withoutPosition.slice(separator + 1);
  if (
    basename !== '' &&
    !FILE_EXTENSION_PATTERN.test(basename) &&
    !EXTENSIONLESS_FILE_NAMES.has(basename) &&
    !/^\.[^.]+$/.test(basename)
  ) {
    return 'folder';
  }
  return 'file';
}

function linkedDestination(text: string, destinationStart: number): { end: number; path: string } | null {
  if (text[destinationStart] === '<') {
    for (let index = destinationStart + 1; index < text.length; index += 1) {
      const character = text[index];
      if (character === '\n' || character === '\r') {
        return null;
      }
      if (character === '\\') {
        index += 1;
        continue;
      }
      if (character === '>' && text[index + 1] === ')') {
        return {
          end: index + 2,
          path: unescapeMarkdown(text.slice(destinationStart + 1, index)),
        };
      }
    }
    return null;
  }

  let depth = 1;
  for (let index = destinationStart; index < text.length; index += 1) {
    const character = text[index];
    if (character === '\n' || character === '\r') {
      return null;
    }
    if (character === '\\') {
      index += 1;
      continue;
    }
    if (character === '(') {
      depth += 1;
      continue;
    }
    if (character !== ')') {
      continue;
    }
    depth -= 1;
    if (depth === 0) {
      return {
        end: index + 1,
        path: unescapeMarkdown(text.slice(destinationStart, index)),
      };
    }
  }
  return null;
}

/** Finds file, skill, image, and HTTP(S) links for every composer backend. */
export function sessionChatComposerReferences(text: string, includeRevealed = false): SessionChatComposerReference[] {
  const references: SessionChatComposerReference[] = [];
  for (const match of text.matchAll(REFERENCE_LABEL_PATTERN)) {
    const sourceLabel = match[1];
    const start = match.index;
    if (sourceLabel === undefined || start === undefined) {
      continue;
    }
    const source = unescapeMarkdown(sourceLabel);
    const revealed = source.endsWith(SESSION_CHAT_REFERENCE_REVEAL_MARKER);
    if ((revealed && !includeRevealed) || text[start - 1] === '!') continue;
    const label = revealed ? source.slice(0, -SESSION_CHAT_REFERENCE_REVEAL_MARKER.length) : source;
    const destinationStart = start + match[0].length;
    const destination = linkedDestination(text, destinationStart);
    if (!destination || destination.path === '') {
      continue;
    }
    const pathWithoutPosition = destination.path.replace(/:\d+(?:-\d+|:\d+)?$/, '');
    if (
      destination.path.startsWith('#') ||
      (/^[a-z][a-z0-9+.-]*:/i.test(pathWithoutPosition) &&
        !/^(?:[a-z]:[\\/]|file:\/\/|https?:\/\/)/i.test(pathWithoutPosition))
    ) {
      continue;
    }
    const kind = sessionChatReferenceKind(label, destination.path);
    if (label.startsWith('$') && !/(?:^|[\\/])SKILL\.md$/i.test(destination.path)) {
      continue;
    }
    references.push({
      end: destination.end,
      identity: `${kind}:${label}`,
      kind,
      label,
      path: destination.path,
      start,
      ...(revealed ? { revealed: true } : {}),
    });
  }
  return references;
}
