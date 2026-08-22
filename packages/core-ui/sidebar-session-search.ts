import type { SidebarPreviousSessionItem } from "../shared/session-grid-contract";

type SessionIdsByGroup = Record<string, string[]>;

export type SidebarSessionSearchResult =
  | {
      groupId: string;
      kind: "session";
      sessionId: string;
    }
  | {
      historyId: string;
      kind: "previous";
    };

export type SidebarSessionSearchSelection =
  | {
      kind: "session";
      sessionId: string;
    }
  | {
      historyId: string;
      kind: "previous";
    };

export function createSidebarSessionSearchResults({
  displayedWorkspaceGroupIds,
  displayedWorkspaceSessionIdsByGroup,
  filteredPreviousSessions,
}: {
  displayedWorkspaceGroupIds: readonly string[];
  displayedWorkspaceSessionIdsByGroup: SessionIdsByGroup;
  filteredPreviousSessions: readonly SidebarPreviousSessionItem[];
}): SidebarSessionSearchResult[] {
  const results: SidebarSessionSearchResult[] = [];

  /**
   * CDXC:ProjectBrowserTabs 2026-05-16-12:59:
   * The standalone Browsers group is no longer part of the sidebar. Browser
   * pane sessions participate in search through their owning project group,
   * so keyboard navigation should follow the project session list only.
   */
  appendSessionResults(results, displayedWorkspaceGroupIds, displayedWorkspaceSessionIdsByGroup);

  for (const session of filteredPreviousSessions) {
    results.push({
      historyId: session.historyId,
      kind: "previous",
    });
  }

  return results;
}

export function getNextSidebarSessionSearchSelection({
  currentSelection,
  direction,
  results,
}: {
  currentSelection?: SidebarSessionSearchSelection;
  direction: -1 | 1;
  results: readonly SidebarSessionSearchResult[];
}): SidebarSessionSearchSelection | undefined {
  if (results.length === 0) {
    return undefined;
  }

  const currentIndex = currentSelection
    ? results.findIndex((result) => isSidebarSessionSearchSelectionMatch(result, currentSelection))
    : -1;
  if (currentIndex < 0) {
    return createSidebarSessionSearchSelection(
      direction === 1 ? results[0] : results[results.length - 1],
    );
  }

  const nextIndex = (currentIndex + direction + results.length) % results.length;
  return createSidebarSessionSearchSelection(results[nextIndex]);
}

export function isSidebarSessionSearchSelectionMatch(
  result: SidebarSessionSearchResult,
  selection: SidebarSessionSearchSelection,
): boolean {
  return result.kind === "session"
    ? selection.kind === "session" && result.sessionId === selection.sessionId
    : selection.kind === "previous" && result.historyId === selection.historyId;
}

export function createSidebarSessionSearchSelection(
  result: SidebarSessionSearchResult | undefined,
): SidebarSessionSearchSelection | undefined {
  if (!result) {
    return undefined;
  }

  return result.kind === "session"
    ? {
        kind: "session",
        sessionId: result.sessionId,
      }
    : {
        historyId: result.historyId,
        kind: "previous",
      };
}

function appendSessionResults(
  results: SidebarSessionSearchResult[],
  groupIds: readonly string[],
  sessionIdsByGroup: SessionIdsByGroup,
): void {
  for (const groupId of groupIds) {
    for (const sessionId of sessionIdsByGroup[groupId] ?? []) {
      results.push({
        groupId,
        kind: "session",
        sessionId,
      });
    }
  }
}
