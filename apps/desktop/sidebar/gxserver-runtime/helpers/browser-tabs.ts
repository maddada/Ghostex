/*
CDXC:GxserverRuntimeSplit 2026-08-22:
Split out of the single 21,861-line `gxserver-runtime.ts`. Pure move: no logic
changed. See `core.ts` for how the runtime's methods are re-attached.
*/
import {
  GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT,
  GPUI_SIDEBAR_BROWSER_FAVICON_URL_MAX_CHARS,
  GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS,
} from "../constants";
import type { GpuiBrowserTabSummary } from "../types-and-protocol";
import { normalizeNonEmptyString } from "./records";
import type { SidebarSessionItem } from "@/packages/shared/session-grid-contract";
import { GRID_COLUMN_COUNT } from "@/packages/shared/session-grid-contract";

export function normalizeGpuiBrowserTabs(
  tabs: readonly GpuiBrowserTabSummary[] | unknown,
): GpuiBrowserTabSummary[] {
  if (!Array.isArray(tabs)) {
    return [];
  }
  return tabs.slice(0, 256).flatMap((tab) => {
    if (!tab || typeof tab !== "object") {
      return [];
    }
    const record = tab as Partial<Record<keyof GpuiBrowserTabSummary, unknown>>;
    const projectId =
      typeof record.projectId === "string" ? normalizeNonEmptyString(record.projectId) : undefined;
    const tabId =
      typeof record.tabId === "string" ? normalizeNonEmptyString(record.tabId) : undefined;
    const title =
      typeof record.title === "string"
        ? normalizeNonEmptyString(record.title)?.slice(0, 512)
        : undefined;
    const url =
      typeof record.url === "string"
        ? record.url.trim().slice(0, GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS)
        : "";
    const faviconUrl = normalizeGpuiBrowserFaviconUrl(record.faviconUrl);
    if (!projectId || !tabId || !title) {
      return [];
    }
    return [
      {
        ...(faviconUrl ? { faviconUrl } : {}),
        isActive: record.isActive === true,
        isSleeping: record.isSleeping === true,
        isVisible: record.isVisible === true,
        projectId,
        tabId,
        title,
        url,
      },
    ];
  });
}

export function normalizeGpuiBrowserFaviconUrl(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  if (!trimmed || trimmed.length > GPUI_SIDEBAR_BROWSER_FAVICON_URL_MAX_CHARS) {
    return undefined;
  }
  try {
    const parsed = new URL(trimmed);
    if (
      (parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
      parsed.username ||
      parsed.password ||
      !parsed.hostname
    ) {
      return undefined;
    }
    parsed.hash = "";
    const normalized = parsed.toString();
    return normalized.length <= GPUI_SIDEBAR_BROWSER_FAVICON_URL_MAX_CHARS
      ? normalized
      : undefined;
  } catch {
    return undefined;
  }
}

export function relayoutGpuiSidebarSessions(
  sessions: readonly SidebarSessionItem[],
): SidebarSessionItem[] {
  return sessions.map((session, index) => ({
    ...session,
    column: index % GRID_COLUMN_COUNT,
    row: Math.floor(index / GRID_COLUMN_COUNT),
  }));
}

export function normalizeGpuiBrowserTabRevealRequest(
  payload: unknown,
): { projectId: string; requestId: number; tabId: string } | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }
  const record = payload as Record<string, unknown>;
  const projectId =
    typeof record.projectId === "string" ? normalizeNonEmptyString(record.projectId) : undefined;
  const tabId = typeof record.tabId === "string" ? normalizeNonEmptyString(record.tabId) : undefined;
  const requestId = typeof record.requestId === "number" ? record.requestId : undefined;
  if (!projectId || !tabId || requestId === undefined || !Number.isFinite(requestId)) {
    return undefined;
  }
  return { projectId, requestId, tabId };
}

export function gpuiBrowserSidebarSessionId(tab: { projectId: string; tabId: string }): string {
  return `gpui-browser:${encodeURIComponent(tab.projectId)}:${tab.tabId}`;
}

export function normalizeGpuiDisplayedWorkspaceSessionIds(sessionIds: unknown): string[] {
  if (!Array.isArray(sessionIds)) {
    return [];
  }
  const normalized: string[] = [];
  for (const sessionId of sessionIds.slice(0, GPUI_COMMAND_PANE_SESSION_SUMMARY_LIMIT)) {
    const normalizedSessionId = normalizeNonEmptyString(sessionId)?.trim();
    if (normalizedSessionId && !normalized.includes(normalizedSessionId)) {
      normalized.push(normalizedSessionId);
    }
  }
  return normalized;
}