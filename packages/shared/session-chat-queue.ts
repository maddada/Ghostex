// Session Chat prompt queue + cross-client composer draft.
// Canonical wire types shared by gxserver (Rust mirror in
// server/src/session_chat_queue.rs), the shared React chat components
// (packages/core-ui/chat/), and every client host. Re-exported from ./session-chat and
// ./gxserver-protocol so consumers keep a single import surface.
// All values must stay plain JSON: they cross the /api/events websocket, the
// CEF bridge, the mobile bridge, and the gpui remote-machine proxy.

/*
CDXC:SessionChatPromptQueue 2026-08-21:
gxserver owns a per-session queue of prompts the user wrote but does not want
delivered yet. A server-side scheduler releases ONE row each time the agent
stops, so the queue drains with every client closed, the phone locked, or the
desktop app quit. Rows are plain text: the composer already interpolates
attachments into the text as "[Image #N](path)" / "[File #N](path)" before it
queues anything, so the queue stores no attachment state.

NAMING COLLISION, READ THIS TWICE: `SessionChatMessage.queued` is a DIFFERENT
thing. That flag means the agent CLI's OWN internal queue (Claude Code's
`queue-operation` rows) is holding a prompt the user already sent with Enter,
and it renders inside the transcript. THIS file is Ghostex's queue, which lives
above the composer and which the agent has never seen. Never reuse
`SessionChatMessage.queued`, and never render these rows in the transcript.
*/

/**
 * `queued`  — waiting for the next idle window (the normal state).
 * `sending` — the scheduler claimed this row and is delivering it right now.
 *             Clients render it busy and refuse edit/reorder/delete on it.
 * `failed`  — delivery was attempted and did not succeed, or the session is in
 *             a blocking terminal state. Draining STOPS at a failed head row
 *             until the user retries or deletes it; the text is never
 *             discarded.
 */
export type SessionChatQueuedPromptState = 'queued' | 'sending' | 'failed';

export interface SessionChatQueuedPrompt {
  /** Server-generated and stable across edits, so a row keeps its identity. */
  id: string;
  /** The exact text that will be sent, attachments already interpolated. */
  text: string;
  state: SessionChatQueuedPromptState;
  /** Set only when `state === "failed"`: why the delivery attempt failed. */
  errorMessage?: string;
  /** ISO-8601 millis. */
  createdAt: string;
  /** ISO-8601 millis. */
  updatedAt: string;
}

/*
CDXC:SessionChatDraft 2026-08-21:
The unsent composer text, synced through gxserver so the same session picked up
on another device shows what was already typed. Pushed on blur / leaving the
session / backgrounding — NOT per keystroke. It is a sync channel, not a
replacement for a host's own local draft cache.

Conflict rule: never clobber. A client that receives a draft with a newer
`updatedAt`, different `content` and a different `originClientId` shows a
one-line "Newer draft from another device — Use / Dismiss" bar above the
composer instead of overwriting a non-empty local composer.
*/
export interface SessionChatDraft {
  content: string;
  /** ISO-8601 millis. The only ordering signal; do not trust local clocks. */
  updatedAt: string;
  /**
   * Opaque per-client id of the writer, so a client can ignore its own echo
   * and so the conflict bar never fires against the device that typed it.
   */
  originClientId: string;
}

/*
CDXC:SessionChatQueueCarriage 2026-08-21:
`queue` and `draft` ride on GxserverReadSessionChatResult and on the
snapshot / replaced / state frames. They are NEVER on `appended` frames, which
only add transcript rows and must stay cheap.

Their omission semantics DIFFER from each other, and `draft` is the odd one out
relative to everything else in session-chat.ts:

  queue — PRESENT (even as an empty array) is the daemon capability probe. A
    daemon that predates this feature omits it, and a client that sees it
    omitted hides every queue control instead of calling endpoints that will
    404. When present it is AUTHORITATIVE, head first, and replaces the
    client's list wholesale.

  draft — an OMITTED draft means "unchanged / nothing on the server", NOT
    cleared. This is the opposite of the `prompt` / `terminalNotice` rule,
    where an omitted field on a frame that can carry it means cleared. The
    difference is deliberate: those fields describe server-observed state that
    genuinely goes away, whereas the draft is text the USER TYPED. Clearing a
    local draft just because an old daemon (or a frame built before the draft
    was written) never sends the field would silently destroy their words. To
    clear a draft, write an empty `content` through /api/setSessionChatDraft —
    an explicit empty string, never an absent field.
*/

// ---------------------------------------------------------------------------
// /api/readSessionChatQueue
// ---------------------------------------------------------------------------

export interface GxserverReadSessionChatQueueParams {
  projectId: string;
  sessionId: string;
}

export interface GxserverReadSessionChatQueueResult {
  /** Authoritative queue, head first. Empty array = supported but empty. */
  queue: SessionChatQueuedPrompt[];
  /** Omitted ⇒ unchanged / none on the server. NEVER "cleared". */
  draft?: SessionChatDraft;
}

// ---------------------------------------------------------------------------
// Queue mutations
// ---------------------------------------------------------------------------

/**
 * The shared shape of every queue mutation's answer: the full, authoritative
 * queue after the change. Clients replace their list with it rather than
 * patching, so an optimistic reorder that lost a race self-corrects.
 */
export interface GxserverSessionChatQueueResult {
  queue: SessionChatQueuedPrompt[];
}

export interface GxserverQueueSessionChatPromptParams {
  projectId: string;
  sessionId: string;
  /** Appended at the END of the queue. Callers trim before sending. */
  text: string;
}

export interface GxserverQueueSessionChatPromptResult extends GxserverSessionChatQueueResult {
  /** The row that was just created, so the caller knows its server id. */
  prompt: SessionChatQueuedPrompt;
}

export interface GxserverUpdateSessionChatQueuedPromptParams {
  projectId: string;
  sessionId: string;
  promptId: string;
  /** New body. Omitted leaves the text alone (e.g. a pure `retry` call). */
  text?: string;
  /**
   * `true` moves a `failed` row back to `queued` and clears its
   * `errorMessage`, which lets draining resume past it. Ignored for rows that
   * are not `failed`.
   */
  retry?: boolean;
}

export interface GxserverRemoveSessionChatQueuedPromptParams {
  projectId: string;
  sessionId: string;
  promptId: string;
}

export interface GxserverSessionChatRemoveQueuedPromptResult extends GxserverSessionChatQueueResult {
  /**
   * The row that was removed. Returned so Edit can pull its text back into the
   * composer without having cached it — the delete and the edit are one
   * round trip, and the text survives even if the client re-renders in between.
   */
  prompt: SessionChatQueuedPrompt;
}

export interface GxserverReorderSessionChatQueueParams {
  projectId: string;
  sessionId: string;
  /**
   * The complete set of row ids in their new order, head first. Ids the server
   * does not know are ignored; rows the caller omitted keep their relative
   * order after the listed ones, so a stale client cannot drop a row that was
   * queued from another device mid-drag.
   */
  promptIds: string[];
}

export interface GxserverSendSessionChatQueuedPromptParams {
  projectId: string;
  sessionId: string;
  promptId: string;
}

export interface GxserverSendSessionChatQueuedPromptResult extends GxserverSessionChatQueueResult {
  /**
   * True when the prompt was handed to the agent (and therefore removed from
   * `queue`). "Send now" delivers immediately regardless of agent state,
   * exactly like pressing Enter, so a false here means the send itself failed
   * and the row is now `failed` with an `errorMessage`.
   */
  sent: boolean;
}

// ---------------------------------------------------------------------------
// /api/setSessionChatDraft
// ---------------------------------------------------------------------------

export interface GxserverSetSessionChatDraftParams {
  projectId: string;
  sessionId: string;
  /** Verbatim composer text. An empty string is how a draft is CLEARED. */
  content: string;
  /**
   * The caller's opaque per-client id, echoed back as the draft's
   * `originClientId` so this client can ignore its own broadcast.
   */
  clientId: string;
}

export interface GxserverSetSessionChatDraftResult {
  draft: SessionChatDraft;
}
