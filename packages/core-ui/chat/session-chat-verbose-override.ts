// Per-session Verbose Mode override. The `sessionChatVerboseMode` Ghostex
// setting stays the default for every chat; the composer pill pins a value for
// one session only, so a chat keeps its mode across reloads and restarts
// without moving the global default. Same per-session localStorage shape the
// option pills use (session-chat-session-options.ts §Persistence).

const STORAGE_PREFIX = 'ghostex.sessionChat.verbose.';

function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    // Storage disabled by the embedder: the pill still works, just per-mount.
    return null;
  }
}

/** Stored override for this session, or null when it follows the setting. */
export function readStoredSessionChatVerbose(sessionKey: string | null | undefined): boolean | null {
  if (!sessionKey) {
    return null;
  }
  let raw: string | null;
  try {
    raw = storage()?.getItem(`${STORAGE_PREFIX}${sessionKey}`) ?? null;
  } catch {
    return null;
  }
  if (raw === '1') {
    return true;
  }
  if (raw === '0') {
    return false;
  }
  return null;
}

export function writeStoredSessionChatVerbose(sessionKey: string | null | undefined, verbose: boolean): void {
  if (!sessionKey) {
    return;
  }
  try {
    storage()?.setItem(`${STORAGE_PREFIX}${sessionKey}`, verbose ? '1' : '0');
  } catch {
    // Quota/private-mode failures must not break the toggle.
  }
}
