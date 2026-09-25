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
import { type SessionChatFilePosition } from './session-chat-file-paths';

export interface SessionChatHostLinks {
  /** Session-machine working directory used to shorten visible diff paths. */
  workingDirectory?: string;
  /**
   * Opens a web URL. `external` is true when the reader asked for the OS
   * browser explicitly (Shift+click). `forceEmbedded` is reserved for the
   * transcript context menu's explicit embedded-browser row. Hosts that omit
   * this get plain target="_blank" anchors, which is right in a browser.
   */
  openUrl?: (url: string, options: { external: boolean; forceEmbedded?: boolean }) => void;
  /**
   * Opens a file that lives on the session's machine, in whichever editor
   * surface the host has. Hosts without one omit it and file pills copy their
   * paths instead of trying to navigate the page away.
   *
   * The path arrives exactly as the agent wrote it, which means a relative one
   * arrives relative: the host that owns an editor resolves the project root
   * (gpui joins it in open_session_chat_file). `position` carries
   * the line, line range, or line and column an agent quoted. Editors land on
   * the first line of a range; hosts that only know how to open a file ignore it.
   */
  openFile?: (path: string, position?: SessionChatFilePosition) => void;
  /** Explicit destinations, supplied only while the host view is available. */
  openFileInCode?: (path: string, position?: SessionChatFilePosition) => void;
  openFileInDocs?: (path: string, position?: SessionChatFilePosition) => void;
  /** Reveals a file reference in the machine's file manager. */
  locateFile?: (path: string) => void;
}

/** Exposes an HTTP(S) target to the transcript context menu. */
export const SESSION_CHAT_WEB_URL_ATTRIBUTE = 'data-session-chat-web-url';

export {
  classifySessionChatLinkHref,
  sessionChatFilePositionFromHref,
  type SessionChatLinkTarget,
} from '@/packages/core-ui/chat/presentation/links';

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
