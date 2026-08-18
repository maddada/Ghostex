// ghostex-web SessionChatTransport implementation.
// Scoped to one (machineId, projectId, sessionId): RPC mutations go through the
// machine's gxserver connection, live frames ride the shared /api/events
// socket via the connection's session-chat subscription registry (which
// re-subscribes automatically after reconnects).

import type {
  GxserverReadSessionChatFilesResult,
  GxserverReadSessionChatImageResult,
  GxserverReadSessionChatResult,
  GxserverSaveSessionChatAttachmentResult,
  GxserverSaveSessionChatImageResult,
} from "@/shared/session-chat";
import type { SessionChatTransport } from "@/sidebar/chat/session-chat-transport";
import {
  rpcForMachine,
  subscribeSessionChatForMachine,
} from "../connections/connection-registry";

export function createSessionChatTransport(
  machineId: string,
  projectId: string,
  sessionId: string,
): SessionChatTransport {
  return {
    async answerPrompt(params) {
      await rpcForMachine(machineId, "/api/answerSessionChatPrompt", {
        ...params,
        projectId,
        sessionId,
      });
    },
    async interrupt() {
      await rpcForMachine(machineId, "/api/interruptSessionChat", {
        projectId,
        sessionId,
      });
    },
    read(params) {
      return rpcForMachine<GxserverReadSessionChatResult>(
        machineId,
        "/api/readSessionChat",
        {
          projectId,
          sessionId,
          ...(params.limit !== undefined ? { limit: params.limit } : {}),
          ...(params.beforeOffset !== undefined ? { beforeOffset: params.beforeOffset } : {}),
        },
      );
    },
    readFiles() {
      return rpcForMachine<GxserverReadSessionChatFilesResult>(
        machineId,
        "/api/readSessionChatFiles",
        { projectId, sessionId },
      );
    },
    async send(text, imagePaths) {
      await rpcForMachine(machineId, "/api/sendSessionChatMessage", {
        projectId,
        sessionId,
        text,
        ...(imagePaths && imagePaths.length > 0 ? { imagePaths } : {}),
      });
    },
    // Raw keystroke (Claude's Shift+Tab mode cycle): same endpoint, `key`
    // instead of a body, so the server writes the bytes verbatim.
    async sendKey(key) {
      await rpcForMachine(machineId, "/api/sendSessionChatMessage", {
        key,
        projectId,
        sessionId,
      });
    },
    // The RPC lands on the session's own machine, so a remote session's
    // pasted image is written on the remote host and the returned path is
    // valid for the agent running there.
    saveImage(params) {
      return rpcForMachine<GxserverSaveSessionChatImageResult>(
        machineId,
        "/api/saveSessionChatImage",
        {
          projectId,
          sessionId,
          base64Data: params.base64Data,
          ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
        },
      );
    },
    // Non-image attachments land on the session's machine the same way and
    // come back as the "[File #N](path)" reference path.
    saveAttachment(params) {
      return rpcForMachine<GxserverSaveSessionChatAttachmentResult>(
        machineId,
        "/api/saveSessionChatAttachment",
        {
          projectId,
          sessionId,
          base64Data: params.base64Data,
          ...(params.suggestedName ? { suggestedName: params.suggestedName } : {}),
        },
      );
    },
    loadImage(params) {
      return rpcForMachine<GxserverReadSessionChatImageResult>(
        machineId,
        "/api/readSessionChatImage",
        { path: params.path },
      );
    },
    subscribe({ currentLimit, onEvent }) {
      // Registry-level subscription survives connection replacement (the
      // registry re-attaches entries when a machine's connection is rebuilt);
      // currentLimit is re-read on every re-attach so the follower's window
      // never comes back smaller than the displayed list.
      return subscribeSessionChatForMachine(
        machineId,
        projectId,
        sessionId,
        onEvent,
        currentLimit,
      );
    },
  };
}
