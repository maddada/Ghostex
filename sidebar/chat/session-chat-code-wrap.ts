/*
 * The wrap-lines default for fenced code blocks.
 *
 * Wrapping is per block: a transcript mixes prose-width logs with structured
 * code, and reflowing every block in the scroller because one of them was
 * toggled would move the reader's place on the page. What is remembered is the
 * last choice, so the blocks that mount after it — the rest of the transcript
 * as it scrolls in, the next session, the next launch — start the way the
 * reader last asked for.
 *
 * Same per-client localStorage shape the other chat preferences on this surface
 * use (session-chat-verbose-override.ts, session-chat-queue.ts); nothing here
 * reaches gxserver or the Ghostex settings.
 */

const STORAGE_KEY = "ghostex.sessionChat.codeWrap";

function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    // Storage disabled by the embedder: the toggle still works, per-block only.
    return null;
  }
}

export function readSessionChatCodeWrapDefault(): boolean {
  try {
    return storage()?.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeSessionChatCodeWrapDefault(wrapped: boolean): void {
  try {
    storage()?.setItem(STORAGE_KEY, wrapped ? "1" : "0");
  } catch {
    // Quota/private-mode failures must not break the toggle.
  }
}
