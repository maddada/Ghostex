import {
  getghostexHotkeyActionIdForKey,
  ghostexHotkeyTextFromKeyboardEvent,
  normalizeghostexHotkeySettings,
  type ghostexHotkeySettings,
} from "../../shared/ghostex-hotkeys";
import { sessionMatchesSidebarTagFilters } from "../../shared/session-tags";
import {
  getAwakeTerminalAndBrowserCount,
  getGroupSessionSummary,
} from "../group-session-summary";
import { filterSidebarSessionItems } from "../previous-session-search";
import { useSidebarStore } from "../sidebar-store";
import type { SidebarSessionTagFilter } from "../session-tag-ui";
import type { WebviewApi } from "../webview-api";
import type {
  SessionIdsByGroup,
  SidebarSectionSessionSummary,
  SidebarSessionsById,
} from "./types";

export function getSidebarSectionSessionSummary(
  groupIds: readonly string[],
  sessionIdsByGroup: Readonly<Record<string, readonly string[] | undefined>>,
  sessionsById: SidebarSessionsById,
): SidebarSectionSessionSummary {
  const sessionIds = new Set(
    groupIds.flatMap((groupId) => sessionIdsByGroup[groupId] ?? []),
  );
  const sessions = [...sessionIds].flatMap((sessionId) => {
    const session = sessionsById[sessionId];
    return session ? [session] : [];
  });

  return {
    ...getGroupSessionSummary(sessions),
    awakeCount: getAwakeTerminalAndBrowserCount(sessions),
  };
}
export function createWorkspaceSessionIdsByGroup(
  workspaceGroupIds: readonly string[],
  sessionIdsByGroup: SessionIdsByGroup,
): SessionIdsByGroup {
  return Object.fromEntries(
    workspaceGroupIds.map((groupId) => [ groupId, sessionIdsByGroup[ groupId ] ?? [] ]),
  );
}

export function findSessionGroupId(
  sessionIdsByGroup: SessionIdsByGroup,
  sessionId: string,
): string | undefined {
  return Object.entries(sessionIdsByGroup).find(([ , sessionIds ]) =>
    sessionIds.includes(sessionId),
  )?.[ 0 ];
}
export function haveSameSessionOrder(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((sessionId, index) => sessionId === right[ index ]);
}

export function haveSameSessionSet(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  const rightIds = new Set(right);
  return left.every((sessionId) => rightIds.has(sessionId));
}

export function createPinnedFirstSessionOrder(
  previousSessionIds: readonly string[],
  pinnedSessionIds: readonly string[],
  sessionsById: Record<string, { isPinned?: boolean; } | undefined>,
): string[] {
  const pinnedSessionIdSet = new Set(pinnedSessionIds);
  const unpinnedSessionIds = previousSessionIds.filter(
    (sessionId) => sessionsById[ sessionId ]?.isPinned !== true,
  );

  return [
    ...pinnedSessionIds.filter((sessionId) => pinnedSessionIdSet.has(sessionId)),
    ...unpinnedSessionIds,
  ];
}
export function getSidebarStartupNow(): number {
  if (typeof performance !== "undefined") {
    return performance.now();
  }

  return Date.now();
}

export function getSidebarStartupElapsedMs(startedAt: number): number {
  return Math.round(getSidebarStartupNow() - startedAt);
}

export function countSidebarSessions(groups: readonly { sessions: readonly unknown[]; }[]): number {
  return groups.reduce((total, group) => total + group.sessions.length, 0);
}

export function postSidebarAgentIconBoundaryLog(
  vscode: WebviewApi,
  event: string,
  details: Record<string, unknown>,
): void {
  vscode.postMessage({
    details,
    event,
    scenarioId: "native.agent.detection",
    type: "sidebarDebugLog",
  });
}

export function summarizeSidebarAgentIconsFromGroups(
  groups: readonly {
    groupId: string;
    sessions: readonly {
      agentIcon?: string;
      sessionId: string;
      sessionKind?: string;
    }[];
  }[],
) {
  const sessions = groups.flatMap((group) =>
    group.sessions.map((session) => ({
      agentIcon: session.agentIcon,
      groupId: group.groupId,
      sessionId: session.sessionId,
      sessionKind: session.sessionKind,
    })),
  );

  return summarizeSidebarAgentIconSessions(sessions);
}

export function summarizeSidebarAgentIconsFromStore(
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
) {
  return summarizeSidebarAgentIconSessions(
    Object.values(sessionsById).map((session) => ({
      agentIcon: session.agentIcon,
      sessionId: session.sessionId,
      sessionKind: session.sessionKind,
    })),
  );
}

export function summarizeSidebarAgentIconSessions(
  sessions: readonly {
    agentIcon?: string;
    groupId?: string;
    sessionId: string;
    sessionKind?: string;
  }[],
) {
  const agentSessions = sessions.filter((session) => Boolean(session.agentIcon));
  return {
    agentIconSessionCount: agentSessions.length,
    agentSessions: agentSessions.slice(0, 10),
    sessionCount: sessions.length,
  };
}

export function createDisplayedSessionIdsByGroup({
  groupIds,
  query,
  selectedSessionTags,
  sessionIdsByGroup,
  sessionsById,
  shouldFilter,
}: {
  groupIds: readonly string[];
  query: string;
  selectedSessionTags: readonly SidebarSessionTagFilter[];
  sessionIdsByGroup: SessionIdsByGroup;
    sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ];
  shouldFilter: boolean;
}): SessionIdsByGroup {
  const displayedSessionIdsByGroup: SessionIdsByGroup = {};

  for (const groupId of groupIds) {
    const sessionIds = sessionIdsByGroup[ groupId ] ?? [];
    const queryFilteredSessionIds = !shouldFilter
      ? [ ...sessionIds ]
      : filterSessionIdsByQuery(sessionIds, sessionsById, query);
    displayedSessionIdsByGroup[ groupId ] = filterSessionIdsByTags(
      queryFilteredSessionIds,
      sessionsById,
      selectedSessionTags,
    );
  }

  return displayedSessionIdsByGroup;
}

export function filterSessionIdsByTags(
  sessionIds: readonly string[],
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
  selectedSessionTags: readonly SidebarSessionTagFilter[],
): string[] {
  if (selectedSessionTags.length === 0) {
    return [ ...sessionIds ];
  }

  return sessionIds.filter((sessionId) => {
    const session = sessionsById[ sessionId ];
    return session ? sessionMatchesSidebarTagFilters(session, selectedSessionTags) : false;
  });
}

export function filterSessionIdsByQuery(
  sessionIds: readonly string[],
  sessionsById: ReturnType<typeof useSidebarStore.getState>[ "sessionsById" ],
  query: string,
): string[] {
  const sessions = sessionIds.flatMap((sessionId) => {
    const session = sessionsById[ sessionId ];
    return session ? [ session ] : [];
  });
  const matchedSessionIds = new Set(
    filterSidebarSessionItems(sessions, query).map((session) => session.sessionId),
  );

  return sessionIds.filter((sessionId) => matchedSessionIds.has(sessionId));
}

export function createDisplayedGroupIds(
  groupIds: readonly string[],
  sessionIdsByGroup: SessionIdsByGroup,
  shouldFilter: boolean,
): string[] {
  if (!shouldFilter) {
    return [ ...groupIds ];
  }

  return groupIds.filter((groupId) => (sessionIdsByGroup[ groupId ] ?? []).length > 0);
}

export function getCommandPaletteHotkeyActionId(
  event: KeyboardEvent,
  hotkeys: ghostexHotkeySettings | undefined,
): "openCommandPalette" | "openSessionSearchPalette" | undefined {
  const hotkeyText = ghostexHotkeyTextFromKeyboardEvent(event);
  if (!hotkeyText) {
    return undefined;
  }
  const actionId = getghostexHotkeyActionIdForKey(
    normalizeghostexHotkeySettings(hotkeys),
    hotkeyText,
  );
  return actionId === "openCommandPalette" || actionId === "openSessionSearchPalette"
    ? actionId
    : undefined;
}

export function hasActiveSidebarHotkeyRecorder(): boolean {
  return Boolean(document.querySelector("[data-hotkey-recorder='true'][data-recording='true']"));
}

export function isSidebarSessionSearchNavigationKey(event: KeyboardEvent): boolean {
  return (
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Tab")
  );
}

export function getSidebarSessionSearchNavigationDirection(event: KeyboardEvent): -1 | 1 {
  return event.key === "ArrowUp" || (event.key === "Tab" && event.shiftKey) ? -1 : 1;
}

export function isEditableSidebarKeyboardTarget(target: Node): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }

  if (target.isContentEditable) {
    return true;
  }

  return Boolean(target.closest("input, textarea, select, [contenteditable]"));
}
