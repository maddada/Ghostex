import type { AccountsTransport } from '@/packages/shared/agent-accounts';
// Session Chat transport contract.
// Hosts (ghostex-web, gpui CEF, mobile web views) inject an implementation so
// the shared chat components never talk to gxserver directly. The transport is
// scoped to one (projectId, sessionId): subscribe frames are pre-filtered by
// the host and the mutation calls omit the identity params.

import type {
  GxserverReadSessionTerminalTailResult,
  GxserverRewindSessionChatResult,
  GxserverSelectSessionChatModelResult,
  GxserverSessionForkBranchesResult,
} from '../../shared/gxserver-protocol';
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
  accounts?: AccountsTransport;
  read(params: { limit?: number; beforeOffset?: number }): Promise<GxserverReadSessionChatResult>;
  /** Lists skills gxserver resolved for this session's stored agent identity. */
  readSkills?(): Promise<GxserverReadSessionChatSkillsResult>;
  /**
   * Lists the session project's files for the composer's "@" mentions, walked
   * on the session's machine. Hosts without it leave "@" as plain text.
   */
  readFiles?(): Promise<GxserverReadSessionChatFilesResult>;
  /*
  CDXC:SessionFork 2026-08-28:
  Every session that shares this conversation's earlier history, ancestors
  included (`/api/sessionForkBranches`). Optional on the same gate as
  everything else here: a host without a route to the endpoint omits it and the
  chat's branch switcher is not rendered at all, rather than offering a control
  whose list would 404.
  */
  forkBranches?(): Promise<GxserverSessionForkBranchesResult>;
  /*
  CDXC:SessionChat 2026-09-02:
  Drives the agent's own rewind flow in its terminal back to the point before
  `messageId` (`/api/rewindSessionChat`). Optional on the same gate as
  everything else here: a host without a route to the endpoint omits it and the
  transcript's "Rewind to here" action is not rendered at all, rather than
  offering a control whose call would 404. The daemon re-snapshots the chat
  stream itself, so nothing here prunes rows.
  */
  rewindSessionChat?(params: { messageId: string }): Promise<GxserverRewindSessionChatResult>;
  /**
   * Drives Codex's own `/model` picker to `model` + `effort`
   * (`/api/selectSessionChatModel`). Optional on the same gate as everything
   * else here: a host without a route to the endpoint omits it and the model
   * pill keeps its terminal handoff row instead of offering rows it cannot apply.
   */
  selectSessionChatModel?(params: {
    model: string;
    effort: string;
    defer?: boolean;
  }): Promise<GxserverSelectSessionChatModelResult>;
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
    /** Creates a dropped directory (or one of its empty descendants). */
    directory?: boolean;
    /** Stable per-drop id used to recreate every entry under one root. */
    uploadId?: string;
    /** Slash-separated path below a dropped directory's root. */
    relativePath?: string;
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
   * Absolute paths of the OS drag currently over this surface, captured by
   * the host shell at drag-enter (gpui — Chromium never exposes `File.path`
   * to a page). Only hosts whose session runs on this machine provide it;
   * drops elsewhere upload bytes through saveAttachment instead.
   */
  readDropPaths?(): readonly string[];
  /**
   * Writes an image from the conversation into Downloads (gpui — a CEF page
   * has no download handler to write through). Hosts without one omit it and
   * the image viewer's "Save image" uses a browser download instead.
   */
  saveImageAs?(params: { base64Data: string; suggestedName: string }): Promise<void>;
  /** Lists existing project Markdown paths used to choose a non-colliding save name. */
  listMessageMarkdownPaths?(): Promise<readonly string[]>;
  /** Saves a final assistant response inside the session project's Docs tree. */
  saveMessageMarkdown?(params: { content: string; path: string }): Promise<{ path: string }>;
  /*
  CDXC:SessionChat 2026-08-26:
  The evidence behind a `composerNotReady` send refusal: the daemon's composer
  verdict plus the bottom of the session's terminal screen, ANSI-stripped. The
  composer's refusal notice reads it on demand so the user can see the trust /
  auth / setup prompt that is holding the input line. Hosts without a route to
  the endpoint omit it and the notice renders without its excerpt rather than
  offering a disclosure that would 404.
  */
  readTerminalTail?(): Promise<GxserverReadSessionTerminalTailResult>;
  /*
  CDXC:Drafts 2026-08-28:
  Switches a DRAFT session's agent (`/api/switchDraftAgent`): gxserver kills the
  draft's background CLI, rewrites its agent identity and launch plan, and
  starts the new agent's CLI. Optional on the same gate as everything else here
  — a host without a route to the endpoint omits it and the composer's "Agents"
  section is not rendered at all, rather than offering a switch that 404s. The
  daemon refuses the call once the draft has been promoted, and that rejection
  is surfaced, never swallowed.
  */
  switchDraftAgent?(params: { agentId: string }): Promise<void>;
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
  CDXC:SessionNotes 2026-08-24:
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
