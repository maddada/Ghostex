import { splitSessionChatFilePosition, type SessionChatFilePosition } from './file-position';

export type SessionChatLinkTarget = { kind: 'url'; url: string } | { kind: 'file'; path: string } | { kind: 'inert' };

/** Schemes a chat link may open as a web page. */
const WEB_SCHEME_PATTERN = /^https?:\/\//i;
/** Any URI scheme at all: "mailto:", "vscode:", "data:", … */
const URI_SCHEME_PATTERN = /^[a-z][a-z0-9+.-]*:/i;
/** Windows drive path ("C:\repo\app.ts"), which also matches a one-letter scheme. */
const WINDOWS_DRIVE_PATH_PATTERN = /^[a-z]:[\\/]/i;
/**
 * Classifies a markdown href into what the chat can do with it. Image hrefs
 * are handled before this by the image viewer, so they arrive here only when
 * no viewer can show them (in which case they behave like any other file).
 */
export function classifySessionChatLinkHref(href: string): SessionChatLinkTarget {
  const trimmed = href.trim();
  if (trimmed === '' || trimmed.startsWith('#')) {
    return { kind: 'inert' };
  }
  if (WEB_SCHEME_PATTERN.test(trimmed)) {
    return { kind: 'url', url: trimmed };
  }
  if (/^file:\/\//i.test(trimmed)) {
    return { kind: 'file', path: filePathFromHref(trimmed.slice('file://'.length)) };
  }
  /*
  CDXC:SessionChat 2026-09-19 WHY:
  The scheme test runs on the destination with its editor coordinates removed, because `Makefile:12`
  and `README:5` are a filename and a line, not a URI. Inline code judges those two by the file rule
  (resolveSessionChatInlineCodeFilePath) and the composer already strips the suffix the same way in
  sessionChatComposerReferences, so leaving it on here made the same reference clickable as inline
  code and inert as a link.
  */
  if (
    !WINDOWS_DRIVE_PATH_PATTERN.test(trimmed) &&
    URI_SCHEME_PATTERN.test(splitSessionChatFilePosition(trimmed).path)
  ) {
    // mailto:, vscode:, data:, … — nothing the chat's own surfaces can show.
    return { kind: 'inert' };
  }
  return { kind: 'file', path: filePathFromHref(trimmed) };
}

/**
 * Markdown destinations arrive percent-encoded and often carry the editor
 * coordinates an agent quoted them with; the host needs the literal path.
 */
function filePathFromHref(href: string): string {
  return splitSessionChatFilePosition(decodedFileHref(href)).path;
}

function decodedFileHref(href: string): string {
  try {
    return decodeURI(href);
  } catch {
    // Malformed escapes: use the raw href.
    return href;
  }
}

/** Preserves editor coordinates from a Markdown destination after path decoding. */
export function sessionChatFilePositionFromHref(href: string): SessionChatFilePosition | undefined {
  return splitSessionChatFilePosition(decodedFileHref(href)).position;
}
