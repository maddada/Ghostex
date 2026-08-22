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
export function hasMoreSessionChatHistory(
  returnedCount: number,
  requestedLimit: number,
): boolean {
  return returnedCount >= requestedLimit;
}
