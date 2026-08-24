// Pagination helpers (upstream chat spec §4 client-side constants).

export const SESSION_CHAT_INITIAL_LIMIT = 300;
export const SESSION_CHAT_PAGE = 200;
/** Mirrors gxserver's SESSION_CHAT_MAX_LIMIT clamp (session_chat.rs). */
export const SESSION_CHAT_MAX_LIMIT = 10_000;

export function nextSessionChatLimit(current: number): number {
  return current + SESSION_CHAT_PAGE;
}

/**
 * Client heuristic after a read: a full window implies more history. The
 * server's exact hasMore (on frames/read results) is preferred when present.
 */
export function hasMoreSessionChatHistory(returnedCount: number, requestedLimit: number): boolean {
  return returnedCount >= requestedLimit;
}

interface SessionChatPageBoundary {
  messages: readonly unknown[];
  hasMore: boolean;
  /** Capability probe added when gxserver began filtering before counting. */
  hasMoreExact?: boolean;
  beforeOffset: number;
}

/**
 * Trust a current daemon's exact boundary. For an older daemon, keep one
 * pagination probe available while its byte cursor is still moving backwards;
 * the probe disappears as soon as a read makes no progress, so a bad legacy
 * `hasMore: false` cannot strand history or cause an endless request loop.
 */
export function sessionChatPageHasMore(page: SessionChatPageBoundary, requestedBeforeOffset?: number): boolean {
  if (page.hasMore || page.hasMoreExact === true) {
    return page.hasMore;
  }
  if (page.messages.length === 0 || page.beforeOffset <= 0) {
    return false;
  }
  return requestedBeforeOffset === undefined || page.beforeOffset < requestedBeforeOffset;
}
