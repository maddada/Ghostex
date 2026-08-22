import type { SessionPaneLayoutNode } from "./session-grid-contract-core";

export function collectActivePaneOwnerSessionIds(
  layout: SessionPaneLayoutNode | undefined,
  options: { validSessionIds?: ReadonlySet<string> | readonly string[] } = {},
): string[] {
  if (!layout) {
    return [];
  }
  const validSessionIds = normalizeValidSessionIds(options.validSessionIds);
  return collectActivePaneOwnerSessionIdsFromNode(layout, validSessionIds);
}

function normalizeValidSessionIds(
  validSessionIds: ReadonlySet<string> | readonly string[] | undefined,
): ReadonlySet<string> | undefined {
  if (!validSessionIds) {
    return undefined;
  }
  return validSessionIds instanceof Set ? validSessionIds : new Set(validSessionIds);
}

function collectActivePaneOwnerSessionIdsFromNode(
  node: SessionPaneLayoutNode,
  validSessionIds: ReadonlySet<string> | undefined,
): string[] {
  switch (node.kind) {
    case "leaf":
      return isValidPaneOwnerSession(node.sessionId, validSessionIds) ? [node.sessionId] : [];
    case "tabs": {
      /*
       * CDXC:AutoSleep 2026-06-09-20:33:
       * Auto Sleep protects the selected owner of every persisted split pane,
       * even when Focus Mode or Source/Browser/Kanban/Manage hides the Agents workarea.
       * If a tab group's stored active id is stale, protect the first valid tab
       * member so pane ownership remains conservative instead of sleeping a
       * split's current conversation.
       */
      const activeSessionId =
        node.activeSessionId &&
        node.sessionIds.includes(node.activeSessionId) &&
        isValidPaneOwnerSession(node.activeSessionId, validSessionIds)
          ? node.activeSessionId
          : node.sessionIds.find((sessionId) => isValidPaneOwnerSession(sessionId, validSessionIds));
      return activeSessionId ? [activeSessionId] : [];
    }
    case "split":
      return node.children.flatMap((child) =>
        collectActivePaneOwnerSessionIdsFromNode(child, validSessionIds),
      );
  }
}

function isValidPaneOwnerSession(
  sessionId: string,
  validSessionIds: ReadonlySet<string> | undefined,
): boolean {
  return validSessionIds === undefined || validSessionIds.has(sessionId);
}
