// Chat-log link routing.
//
// A conversation's links are not web links by default: agents write web URLs,
// absolute machine paths ("[File #1](/Users/me/repo/src/app.ts)"), repo-relative
// paths, and image references side by side. This module classifies a markdown
// href and hands the click to whatever the host can actually do with it —
// gpui opens web URLs in its own Browser view and files in Code/Docs, while the
// web app and the phone just follow web URLs and copy machine paths when a
// reader clicks them (navigating a browser to /Users/... would only break the
// page).

import { createContext, useContext, type ReactNode } from 'react';
import type { SessionChatFilePosition } from './session-chat-file-paths';

export interface SessionChatHostLinks {
  /**
   * Opens a web URL. `external` is true when the reader asked for the OS
   * browser explicitly (Shift+click). Hosts that omit this get plain
   * target="_blank" anchors, which is already the right thing in a browser.
   */
  openUrl?: (url: string, options: { external: boolean }) => void;
  /**
   * Opens a file that lives on the session's machine, in whichever editor
   * surface the host has. Hosts without one omit it and file pills copy their
   * paths instead of trying to navigate the page away.
   *
   * The path arrives exactly as the agent wrote it, which means a relative one
   * arrives relative: the chat surface never learns the session's working
   * directory, and the host that owns an editor is also the one that knows the
   * project root (gpui joins it in open_session_chat_file). `position` carries
   * the `:line:column` an agent quoted, for hosts whose editor can land on it;
   * hosts that only know how to open a file ignore it.
   */
  openFile?: (path: string, position?: SessionChatFilePosition) => void;
}

export type SessionChatLinkTarget = { kind: 'url'; url: string } | { kind: 'file'; path: string } | { kind: 'inert' };

/** Schemes a chat link may open as a web page. */
const WEB_SCHEME_PATTERN = /^https?:\/\//i;
/** Any URI scheme at all: "mailto:", "vscode:", "data:", … */
const URI_SCHEME_PATTERN = /^[a-z][a-z0-9+.-]*:/i;
/** Windows drive path ("C:\repo\app.ts"), which also matches a one-letter scheme. */
const WINDOWS_DRIVE_PATH_PATTERN = /^[a-z]:[\\/]/i;
/** Trailing "path.ts:42" / "path.ts:42:7" editor coordinates agents like to write. */
const LINE_COORDINATE_SUFFIX = /:(\d+)(?::(\d+))?$/;

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
  if (!WINDOWS_DRIVE_PATH_PATTERN.test(trimmed) && URI_SCHEME_PATTERN.test(trimmed)) {
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
  return decodedFileHref(href).replace(LINE_COORDINATE_SUFFIX, '');
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
  const match = LINE_COORDINATE_SUFFIX.exec(decodedFileHref(href));
  const line = Number(match?.[1]);
  const column = Number(match?.[2]);
  if (!Number.isSafeInteger(line) || line < 1) {
    return undefined;
  }
  return {
    line,
    ...(Number.isSafeInteger(column) && column >= 1 ? { column } : {}),
  };
}

const SessionChatHostLinksContext = createContext<SessionChatHostLinks | null>(null);

export function useSessionChatHostLinks(): SessionChatHostLinks | null {
  return useContext(SessionChatHostLinksContext);
}

export function SessionChatHostLinksProvider({
  children,
  links,
}: {
  children: ReactNode;
  links?: SessionChatHostLinks;
}) {
  return <SessionChatHostLinksContext.Provider value={links ?? null}>{children}</SessionChatHostLinksContext.Provider>;
}
