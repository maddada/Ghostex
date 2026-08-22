import {
  type BeadsBridgeRequest,
  type BeadsBridgeResponse,
} from "../project-board-shared";
import {
  type ProjectBoardBridgeRequest,
  type ProjectBoardBridgeResponse,
  type ProjectBoardConversationState,
} from "@/packages/shared/bead-conversation-links";
import {
  type ProjectBoardImageBridgeRequest,
  type ProjectBoardImageBridgeResponse,
  type ProjectBeadsWebKitWindow,
} from "./types";

export const BRIDGE_REQUEST_PREFIX = "__GHOSTEX_PROJECT_BEADS_REQUEST__";
export const BRIDGE_RESPONSE_EVENT = "ghostex-project-beads-response";
export const PROJECT_BOARD_RESPONSE_EVENT = "ghostex-project-board-response";
export const PROJECT_BOARD_IMAGE_RESPONSE_EVENT = "ghostex-project-board-image-response";

export function sendBeadsRequest(
  request: Omit<BeadsBridgeRequest, "requestId">,
): Promise<BeadsBridgeResponse> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
      reject(new Error("Beads command timed out."));
    }, 60_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<BeadsBridgeResponse>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(BRIDGE_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBeadsBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBeads;
    if (projectBeadsBridge) {
      projectBeadsBridge.postMessage(message);
      return;
    }
    if (request.action === "listIssues" && request.cwd) {
      void fetch(
        `file://${request.cwd}/.beads/issues.jsonl`,
      ).then(() => reject(new Error("Beads bridge is unavailable outside Ghostex."))).catch(() => {
        reject(new Error("Beads bridge is unavailable outside Ghostex."));
      });
      return;
    }
    console.info(`${BRIDGE_REQUEST_PREFIX}${JSON.stringify(message)}`);
    reject(new Error("Beads bridge is unavailable outside Ghostex."));
  });
}

export function sendProjectBoardRequest<TPayload = ProjectBoardConversationState>(
  request: Omit<ProjectBoardBridgeRequest, "requestId">,
): Promise<ProjectBoardBridgeResponse<TPayload>> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
      reject(new Error("Project board bridge timed out."));
    }, 60_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<ProjectBoardBridgeResponse<TPayload>>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBoardBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBoard;
    if (projectBoardBridge) {
      projectBoardBridge.postMessage(message);
      return;
    }
    window.clearTimeout(timeout);
    window.removeEventListener(PROJECT_BOARD_RESPONSE_EVENT, onResponse);
    reject(new Error("Project board bridge is unavailable outside Ghostex."));
  });
}

export function sendProjectBoardImageRequest(
  request: Omit<ProjectBoardImageBridgeRequest, "requestId">,
): Promise<ProjectBoardImageBridgeResponse> {
  return new Promise((resolve, reject) => {
    const requestId = crypto.randomUUID();
    const timeout = window.setTimeout(() => {
      window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
      reject(new Error("Project board image bridge timed out."));
    }, 30_000);
    const onResponse = (event: Event) => {
      const response = (event as CustomEvent<ProjectBoardImageBridgeResponse>).detail;
      if (response?.requestId !== requestId) {
        return;
      }
      window.clearTimeout(timeout);
      window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
      resolve(response);
    };
    window.addEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
    const message = { ...request, requestId };
    const projectBoardImagesBridge = (window as ProjectBeadsWebKitWindow).webkit?.messageHandlers
      ?.ghostexProjectBoardImages;
    if (projectBoardImagesBridge) {
      projectBoardImagesBridge.postMessage(message);
      return;
    }
    window.clearTimeout(timeout);
    window.removeEventListener(PROJECT_BOARD_IMAGE_RESPONSE_EVENT, onResponse);
    reject(new Error("Project board image bridge is unavailable outside Ghostex."));
  });
}