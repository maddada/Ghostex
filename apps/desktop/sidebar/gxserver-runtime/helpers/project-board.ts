/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_PROJECT_BOARD_CONVERSATION_ACTIONS,
  GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE,
  GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION,
} from "../constants";
import type { GpuiProjectBoardConversationRequest } from "../types-and-protocol";
import type { AppToastLevel } from "@/packages/shared/app-toast-contract";
import type { GxserverPresentationSearchResult } from "@/packages/shared/gxserver-protocol";
import { DEFAULT_TERMINAL_SESSION_TITLE } from "@/packages/shared/session-grid-contract";

export function boundedGpuiProjectBoardRequestString(
  value: unknown,
  maxChars: number,
): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > maxChars) {
    return undefined;
  }
  return trimmed;
}

export function normalizeGpuiProjectBoardConversationRequest(
  payload: unknown,
): GpuiProjectBoardConversationRequest | undefined {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  if (
    record.type !== GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_TYPE ||
    record.version !== GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_REQUEST_MESSAGE_VERSION ||
    !record.request ||
    typeof record.request !== "object" ||
    Array.isArray(record.request)
  ) {
    return undefined;
  }
  const request = record.request as Record<string, unknown>;
  const requestId = boundedGpuiProjectBoardRequestString(request.requestId, 256);
  const action = typeof request.action === "string" ? request.action : "";
  if (!requestId || !GPUI_PROJECT_BOARD_CONVERSATION_ACTIONS.has(action)) {
    return undefined;
  }
  return {
    action: action as GpuiProjectBoardConversationRequest["action"],
    agentId: boundedGpuiProjectBoardRequestString(request.agentId, 256),
    beadDisplayId: boundedGpuiProjectBoardRequestString(request.beadDisplayId, 256),
    beadId: boundedGpuiProjectBoardRequestString(request.beadId, 512),
    projectId: boundedGpuiProjectBoardRequestString(request.projectId, 512),
    projectPath: boundedGpuiProjectBoardRequestString(request.projectPath, 4096),
    prompt: boundedGpuiProjectBoardRequestString(request.prompt, 60_000),
    requestId,
    sessionId: boundedGpuiProjectBoardRequestString(request.sessionId, 512),
    startLocation: boundedGpuiProjectBoardRequestString(request.startLocation, 32),
    toastDescription: boundedGpuiProjectBoardRequestString(request.toastDescription, 2_000),
    toastLevel: boundedGpuiProjectBoardRequestString(request.toastLevel, 16),
    toastTitle: boundedGpuiProjectBoardRequestString(request.toastTitle, 300),
  };
}

export function normalizeGpuiProjectBoardToastLevel(level: string | undefined): AppToastLevel {
  switch (level) {
    case "error":
    case "info":
    case "success":
    case "warning":
      return level;
    default:
      return "error";
  }
}

export function gpuiProjectBoardPreviousSessionRowTitle(row: GxserverPresentationSearchResult): string {
  return row.displayTitle || row.primaryTitle || row.title || DEFAULT_TERMINAL_SESSION_TITLE;
}