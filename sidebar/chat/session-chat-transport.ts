// Session Chat transport contract.
// Hosts (ghostex-web, gpui CEF, mobile web views) inject an implementation so
// the shared chat components never talk to gxserver directly. The transport is
// scoped to one (projectId, sessionId): subscribe frames are pre-filtered by
// the host and the mutation calls omit the identity params.

import type {
  GxserverAnswerSessionChatPromptParams,
  GxserverReadSessionChatFilesResult,
  GxserverReadSessionChatImageResult,
  GxserverReadSessionChatResult,
  GxserverReadSessionChatSkillsResult,
  GxserverSaveSessionChatAttachmentResult,
  GxserverSaveSessionChatImageResult,
  GxserverSessionChatEvent,
  SessionChatSendKey,
} from "../../shared/session-chat";

export interface SessionChatTransport {
  read(params: {
    limit?: number;
    beforeOffset?: number;
  }): Promise<GxserverReadSessionChatResult>;
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
  saveImage?(params: {
    base64Data: string;
    suggestedName?: string;
  }): Promise<GxserverSaveSessionChatImageResult>;
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
  answerPrompt(
    params: Omit<GxserverAnswerSessionChatPromptParams, "projectId" | "sessionId">,
  ): Promise<void>;
  interrupt(): Promise<void>;
}
