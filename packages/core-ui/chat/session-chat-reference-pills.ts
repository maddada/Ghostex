export type SessionChatReferenceKind = 'file' | 'folder' | 'image' | 'skill';

export interface SessionChatComposerReference {
  end: number;
  identity: string;
  kind: SessionChatReferenceKind;
  label: string;
  path: string;
  start: number;
}

export const SESSION_CHAT_REFERENCE_REVEAL_MARKER = '·';

const REFERENCE_LABEL_PATTERN = /\[((?:Image|File|Folder) #\d+|\$(?:\\.|[^\]\\\r\n])+)]\(/g;
const IMAGE_PATH_PATTERN = /\.(?:avif|bmp|gif|heic|heif|ico|jpe?g|png|svg|tiff?|webp)(?:[?#].*)?$/i;
const FILE_EXTENSION_PATTERN = /\.[A-Za-z][A-Za-z0-9_+-]*$/;
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
  if (/^File #\d+$/.test(label)) return 'file';
  if (/^Folder #\d+$/.test(label)) return 'folder';
  if (label.startsWith('$')) return 'skill';
  return null;
}

/** Classifies any rendered machine-path link for the shared pill styling. */
export function sessionChatReferenceKind(label: string, path: string): SessionChatReferenceKind {
  const explicit = explicitReferenceKind(label.trim());
  if (explicit && (explicit !== 'skill' || /(?:^|[\\/])SKILL\.md$/i.test(path))) {
    return explicit;
  }
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

/** Finds the attachment and skill links Monaco replaces with compact pills. */
export function sessionChatComposerReferences(text: string): SessionChatComposerReference[] {
  const references: SessionChatComposerReference[] = [];
  for (const match of text.matchAll(REFERENCE_LABEL_PATTERN)) {
    const sourceLabel = match[1];
    const start = match.index;
    if (sourceLabel === undefined || start === undefined) {
      continue;
    }
    const label = unescapeMarkdown(sourceLabel);
    const kind = explicitReferenceKind(label);
    const destinationStart = start + match[0].length;
    const destination = linkedDestination(text, destinationStart);
    if (!kind || !destination || destination.path === '') {
      continue;
    }
    if (kind === 'skill' && !/(?:^|[\\/])SKILL\.md$/i.test(destination.path)) {
      continue;
    }
    references.push({
      end: destination.end,
      identity: `${kind}:${label}`,
      kind,
      label,
      path: destination.path,
      start,
    });
  }
  return references;
}
