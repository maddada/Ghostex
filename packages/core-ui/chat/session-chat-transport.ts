// Session Chat transport contract.
// Hosts (ghostex-web, gpui CEF, mobile web views) inject an implementation so
// the shared chat components never talk to gxserver directly. The transport is
// scoped to one (projectId, sessionId): subscribe frames are pre-filtered by
// the host and the mutation calls omit the identity params.

import type {
  GxserverAnswerSessionChatPromptParams,
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatFilesResult,
  GxserverReadSessionChatImageResult,
  GxserverReadSessionChatResult,
  GxserverReadSessionChatSkillsResult,
  GxserverSaveSessionChatAttachmentResult,
  GxserverSaveSessionChatImageResult,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatEvent,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
  SessionChatSendKey,
} from '../../shared/session-chat';

export interface SessionChatTransport {
  read(params: { limit?: number; beforeOffset?: number }): Promise<GxserverReadSessionChatResult>;
  /** Lists skills gxserver resolved for this session's stored agent identity. */
  readSkills?(): Promise<GxserverReadSessionChatSkillsResult>;
  /**
   * Lists the session project's files for the composer's "@" mentions, walked
   * on the session's machine. Hosts without it leave "@" as plain text.
   */
  readFiles?(): Promise<GxserverReadSessionChatFilesResult>;
  /** Returns an unsubscribe function. Events must already be filtered to this session. */
  subscribe(handlers: {
    onEvent: (e: GxserverSessionChatEvent) => void;
    /**
     * Read at every (re)subscribe, never captured: snapshot/replaced frames
     * carry the follower's window, so a reconnect after a long live session
     * would otherwise answer with fewer rows than are already on screen.
     * Hosts that cannot pass a window ignore it.
     */
    currentLimit?: () => number;
  }): () => void;
  send(text: string, imagePaths?: string[]): Promise<void>;
  /**
   * Injects a raw keystroke sequence (no text, no Enter) for controls owned by
   * the agent TUI. Hosts without a path for it omit this, which hides those
   * controls instead of pretending they work.
   */
  sendKey?(key: SessionChatSendKey): Promise<void>;
  /**
   * Saves composer-pasted image bytes onto the session's machine and returns
   * the absolute path there (the shared terminal-paste path contract). Hosts
   * without an upload path (e.g. the mobile WebView) omit this, which
   * disables the composer's image paste.
   */
  saveImage?(params: { base64Data: string; suggestedName?: string }): Promise<GxserverSaveSessionChatImageResult>;
  /**
   * Saves any attached file's bytes into Ghostex storage on the session's machine
   * and returns the absolute path for the "[File #N](path)" reference. Hosts
   * without an upload path omit it, which limits the attach button to images.
   */
  saveAttachment?(params: {
    base64Data: string;
    suggestedName?: string;
  }): Promise<GxserverSaveSessionChatAttachmentResult>;
  /**
   * Reads an image file from the session's machine for inline display (chat
   * log thumbnails and image links open through it). Hosts without it fall
   * back to non-clickable chips.
   */
  loadImage?(params: { path: string }): Promise<GxserverReadSessionChatImageResult>;
  /**
   * Opens the host's native file/folder picker and resolves with absolute
   * paths on the session's machine (gpui). Hosts without one omit it and the
   * attach button uses a browser file input + upload instead.
   */
  pickAttachmentPaths?(): Promise<string[]>;
  /**
   * Writes an image from the conversation wherever the user chooses, through
   * the host's own save panel (gpui — a CEF page has no download handler to
   * write through). Hosts without one omit it and the image viewer's "Save
   * image" uses a browser download instead.
   */
  saveImageAs?(params: { base64Data: string; suggestedName: string }): Promise<void>;
  answerPrompt(params: Omit<GxserverAnswerSessionChatPromptParams, 'projectId' | 'sessionId'>): Promise<void>;
  interrupt(): Promise<void>;
  /*
  Ghostex prompt queue + synced composer draft (plan 016). Every method here is
  optional because a host may have no route to the endpoint yet. Two separate
  gates decide whether a queue control is shown, and BOTH must pass:
    1. the read result / frame carries a `queue` array (the daemon supports it);
    2. the transport implements the method (this host can reach it).
  When either is missing the shared UI hides that control entirely rather than
  offering a button that 404s or silently does nothing.
  */
  /**
   * Appends `text` at the end of the queue (Tab in the composer, or a
   * long-press on Send). Hosts without it lose queueing altogether: Tab falls
   * back to its normal behaviour and long-press just sends.
   */
  queuePrompt?(params: { text: string }): Promise<GxserverQueueSessionChatPromptResult>;
  /**
   * Edits a row's text and/or retries it. `retry: true` moves a `failed` row
   * back to `queued` and clears its error so draining can resume. Hosts
   * without it hide Retry and make rows read-only.
   */
  updateQueuedPrompt?(params: {
    promptId: string;
    text?: string;
    retry?: boolean;
  }): Promise<GxserverSessionChatQueueResult>;
  /**
   * Deletes a row and returns it, so Edit can pull the removed text into the
   * composer in the same round trip. Hosts without it hide Delete and Edit.
   */
  removeQueuedPrompt?(params: { promptId: string }): Promise<GxserverSessionChatRemoveQueuedPromptResult>;
  /**
   * Commits a drag-to-reorder with the full id list, head first. Hosts without
   * it render the rows without drag handles instead of animating a reorder
   * that the server would never persist.
   */
  reorderQueue?(params: { promptIds: string[] }): Promise<GxserverSessionChatQueueResult>;
  /**
   * "Send now": delivers one row immediately regardless of agent state, exactly
   * like pressing Enter. Hosts without it hide the per-row Send now control;
   * the row still drains on its own at the next idle window.
   */
  sendQueuedPrompt?(params: { promptId: string }): Promise<GxserverSendSessionChatQueuedPromptResult>;
  /**
   * Pushes the unsent composer text to gxserver so other devices see it.
   * Called on blur / session switch / unmount / backgrounding, never per
   * keystroke, and an empty `content` is how a draft is cleared. `clientId` is
   * this client's opaque id, echoed back as the draft's `originClientId` so it
   * can ignore its own broadcast. Hosts without it keep their local draft
   * cache and simply never sync — nothing in the UI is hidden.
   */
  setDraft?(params: { content: string; clientId: string }): Promise<void>;
  /*
  CDXC:SessionAgentNotes 2026-08-24:
  The session's "what to do next here" note. gxserver files it under the
  PROVIDER conversation id, so the transport passes only the note body and the
  host's own (projectId, sessionId) resolve the rest. Both methods are optional
  on the established gate of this interface: a host without a route to the two
  endpoints omits them and the composer's note control is not rendered at all,
  rather than opening a panel whose save would 404.
  */
  /** Reads this session's stored note; `note` is absent when none is stored. */
  readSessionNote?(): Promise<{ agentSessionId?: string; note?: string }>;
  /** Stores the note; an empty string clears it. */
  saveSessionNote?(note: string): Promise<void>;
}
