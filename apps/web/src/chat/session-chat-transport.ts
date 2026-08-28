// ghostex-web SessionChatTransport implementation.
// Scoped to one (machineId, projectId, sessionId): RPC mutations go through the
// machine's gxserver connection, live frames ride the shared /api/events
// socket via the connection's session-chat subscription registry (which
// re-subscribes automatically after reconnects).

import type {
  GxserverQueueSessionChatPromptResult,
  GxserverReadSessionChatFilesResult,
  GxserverReadSessionChatImageResult,
  GxserverReadSessionChatResult,
  GxserverSaveSessionChatAttachmentResult,
  GxserverSaveSessionChatImageResult,
  GxserverSendSessionChatQueuedPromptResult,
  GxserverSessionChatQueueResult,
  GxserverSessionChatRemoveQueuedPromptResult,
} from '@/packages/shared/session-chat';
import type {
  GxserverReadSessionAgentNoteResult,
  GxserverReadSessionTerminalTailResult,
  GxserverSessionForkBranchesResult,
} from '@/packages/shared/gxserver-protocol';
import type { SessionChatTransport } from '@/packages/core-ui/chat/session-chat-transport';
import { listProjectMarkdownDocumentPaths, saveProjectMarkdownDocument } from '@/packages/shared/project-docs';
import { rpcForMachine, subscribeSessionChatForMachine } from '../connections/connection-registry';

export function createSessionChatTransport(
  machineId: string,
  projectId: string,
  sessionId: string
): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await rpcForMachine(machineId, '/api/answerSessionChatPrompt', {
        ...params,
        projectId,
        sessionId,
      });
    },
    async interrupt() {
      await rpcForMachine(machineId, '/api/interruptSessionChat', {
        projectId,
        sessionId,
      });
    },
    read(params) {
      return rpcForMachine<GxserverReadSessionChatResult>(machineId, '/api/readSessionChat', {
        projectId,
        sessionId,
        ...(params.limit !== undefined ? { limit: params.limit } : {}),
        ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
      });
    },
    readFiles() {
      return rpcForMachine<GxserverReadSessionChatFilesResult>(machineId, '/api/readSessionChatFiles', {
        projectId,
        sessionId,
      });
    },
    async send(text, imagePaths) {
      await rpcForMachine(machineId, '/api/sendSessionChatMessage', {
        projectId,
        sessionId,
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    // Raw keystroke (Claude's Shift+Tab mode cycle): same endpoint, `key`
    // instead of a body, so the server writes the bytes verbatim.
    async sendKey(key) {
      await rpcForMachine(machineId, '/api/sendSessionChatMessage', {
        key,
        projectId,
        sessionId,
      });
    },
    // The RPC lands on the session's own machine, so a remote session's
    // pasted image is written on the remote host and the returned path is
    // valid for the agent running there.
    saveImage(params) {
      return rpcForMachine<GxserverSaveSessionChatImageResult>(machineId, '/api/saveSessionChatImage', {
        projectId,
        sessionId,
        base64Data: params.base64Data,
        ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
      });
    },
    // Non-image attachments land on the session's machine the same way and
    // come back as the "[File #N](path)" reference path.
    saveAttachment(params) {
      return rpcForMachine<GxserverSaveSessionChatAttachmentResult>(machineId, '/api/saveSessionChatAttachment', {
        projectId,
        sessionId,
        base64Data: params.base64Data,
        ...(params.directory !== undefined ? { directory: params.directory } : {}),
        ...(params.uploadId ? { uploadId: params.uploadId } : {}),
        ...(params.relativePath ? { relativePath: params.relativePath } : {}),
        ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
      });
    },
    loadImage(params) {
      return rpcForMachine<GxserverReadSessionChatImageResult>(machineId, '/api/readSessionChatImage', {
        path: params.path,
      });
    },
    listMessageMarkdownPaths() {
      return listProjectMarkdownDocumentPaths(projectId, (endpoint, request) =>
        rpcForMachine(machineId, endpoint, request)
      );
    },
    saveMessageMarkdown(params) {
      return saveProjectMarkdownDocument({ ...params, projectId }, (endpoint, request) =>
        rpcForMachine(machineId, endpoint, request)
      );
    },
    /*
    CDXC:SessionChatComposerReady 2026-08-26:
    The evidence read behind a `composerNotReady` refusal — the session's own
    machine answers with its composer verdict plus the bottom of the terminal
    screen, which the chat composer shows inside its refusal notice.
    */
    readTerminalTail() {
      return rpcForMachine<GxserverReadSessionTerminalTailResult>(machineId, '/api/readSessionTerminalTail', {
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:SessionForkFamilies 2026-08-28:
    The branch switcher's fork family. It lands on the session's own machine
    like every other call here, so the branches are the ones that machine's
    registry actually knows about.
    */
    forkBranches() {
      return rpcForMachine<GxserverSessionForkBranchesResult>(machineId, '/api/sessionForkBranches', {
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    The Ghostex prompt queue and the synced composer draft (plan 016). Every
    one of these is a plain RPC on the session's own machine, exactly like
    send/answerPrompt above, so a remote session's queue lives on the remote
    daemon that will actually deliver it. Implementing all six means the shared
    chat UI shows every queue control; the daemon-side gate (a `queue` array on
    the read result) still decides whether they render, so an older remote
    daemon hides them without this host doing anything.
    */
    queuePrompt(params) {
      return rpcForMachine<GxserverQueueSessionChatPromptResult>(machineId, '/api/queueSessionChatPrompt', {
        projectId,
        sessionId,
        text: params.text,
      });
    },
    updateQueuedPrompt(params) {
      return rpcForMachine<GxserverSessionChatQueueResult>(machineId, '/api/updateSessionChatQueuedPrompt', {
        projectId,
        sessionId,
        promptId: params.promptId,
        // `text` and `retry` are both optional and mean different things when
        // absent (leave the body alone / do not un-fail the row), so neither
        // may be sent as an undefined placeholder.
        ...(params.text !== undefined ? { text: params.text } : {}),
        ...(params.retry !== undefined ? { retry: params.retry } : {}),
      });
    },
    removeQueuedPrompt(params) {
      return rpcForMachine<GxserverSessionChatRemoveQueuedPromptResult>(
        machineId,
        '/api/removeSessionChatQueuedPrompt',
        { projectId, sessionId, promptId: params.promptId }
      );
    },
    reorderQueue(params) {
      return rpcForMachine<GxserverSessionChatQueueResult>(machineId, '/api/reorderSessionChatQueue', {
        projectId,
        sessionId,
        promptIds: params.promptIds,
      });
    },
    sendQueuedPrompt(params) {
      return rpcForMachine<GxserverSendSessionChatQueuedPromptResult>(machineId, '/api/sendSessionChatQueuedPrompt', {
        projectId,
        sessionId,
        promptId: params.promptId,
      });
    },
    /*
    CDXC:SessionAgentNotes 2026-08-24:
    The session note rides the session's own machine connection like every
    other call here, and gxserver resolves the provider conversation id it is
    filed under from (projectId, sessionId) — this host never sees that key.
    */
    readSessionNote() {
      return rpcForMachine<GxserverReadSessionAgentNoteResult>(machineId, '/api/readSessionAgentNote', {
        projectId,
        sessionId,
      });
    },
    async saveSessionNote(note) {
      await rpcForMachine(machineId, '/api/saveSessionAgentNote', {
        note,
        projectId,
        sessionId,
      });
    },
    /*
    CDXC:DraftSessions 2026-08-28:
    The draft's agent switch runs on the session's own machine, which is where
    the background CLI it kills and relaunches lives. Unconditional: the shared
    UI gates the composer's "Agents" section on the daemon's `availableAgents`
    field, so a daemon predating drafts never offers the switch.
    */
    async switchDraftAgent(params) {
      await rpcForMachine(machineId, '/api/switchDraftAgent', {
        agentId: params.agentId,
        projectId,
        sessionId,
      });
    },
    // `clientId` is minted and persisted by the shared hook, never here: a
    // fresh id per call would make this client's own draft echo look like
    // another device and pop the conflict bar for no reason.
    async setDraft(params) {
      await rpcForMachine(machineId, '/api/setSessionChatDraft', {
        projectId,
        sessionId,
        content: params.content,
        clientId: params.clientId,
      });
    },
    subscribe({ currentLimit, onEvent }) {
      // Registry-level subscription survives connection replacement (the
      // registry re-attaches entries when a machine's connection is rebuilt);
      // currentLimit is re-read on every re-attach so the follower's window
      // never comes back smaller than the displayed list.
      return subscribeSessionChatForMachine(machineId, projectId, sessionId, onEvent, currentLimit);
    },
  };
}
