import {
  type CreateSessionRecordOptions,
  type GroupedSessionWorkspaceSnapshot,
  type SessionRecord,
  type TerminalViewMode,
  type VisibleSessionCount,
} from "./session-grid-contract";
import {
  createSessionInSnapshot,
  focusSessionInSnapshot,
  removeSessionInSnapshot,
  renameSessionAliasInSnapshot,
  setBrowserSessionMetadataInSnapshot,
  setSessionTitleInSnapshot,
  setViewModeInSnapshot,
  setVisibleCountInSnapshot,
  toggleFullscreenSessionInSnapshot,
} from "./session-grid-state";
import { reorderGroupSessions } from "./session-order-reorder";
import {
  claimNextSessionDisplayId,
  findGroupContainingSession,
  updateActiveGroupSnapshot,
  updateGroup,
  updateOwningGroupSnapshot,
} from "./grouped-session-workspace-state-helpers";
import {
  getActiveGroup,
  getGroupById,
  normalizeGroupedSessionWorkspaceSnapshot,
} from "./grouped-session-workspace-state-core";

export function createSessionInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  options?: CreateSessionRecordOptions,
): {
  session?: SessionRecord;
  snapshot: GroupedSessionWorkspaceSnapshot;
} {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const activeGroup = getActiveGroup(normalizedSnapshot);
  if (!activeGroup) {
    return { snapshot: normalizedSnapshot };
  }

  const nextDisplayId = claimNextSessionDisplayId(normalizedSnapshot);
  const result = createSessionInSnapshot(
    activeGroup.snapshot,
    normalizedSnapshot.nextSessionNumber,
    {
      ...options,
      displayId: nextDisplayId.displayId,
    } as CreateSessionRecordOptions & { displayId: string },
  );
  if (!result.session) {
    return { snapshot: normalizedSnapshot };
  }

  return {
    session: result.session,
    snapshot: {
      ...normalizedSnapshot,
      groups: updateGroup(normalizedSnapshot.groups, activeGroup.groupId, result.snapshot),
      nextSessionDisplayId: nextDisplayId.nextSessionDisplayId,
      nextSessionNumber: normalizedSnapshot.nextSessionNumber + 1,
    },
  };
}

export function focusSessionInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const location = findGroupContainingSession(normalizedSnapshot, sessionId);
  if (!location) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const result = focusSessionInSnapshot(location.group.snapshot, sessionId);
  return {
    changed: result.changed || normalizedSnapshot.activeGroupId !== location.group.groupId,
    snapshot: {
      ...normalizedSnapshot,
      activeGroupId: location.group.groupId,
      groups: updateGroup(normalizedSnapshot.groups, location.group.groupId, result.snapshot),
    },
  };
}

export function renameSessionAliasInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  alias: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  return updateOwningGroupSnapshot(snapshot, sessionId, (groupSnapshot) =>
    renameSessionAliasInSnapshot(groupSnapshot, sessionId, alias),
  );
}

export function setSessionTitleInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  title: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  return updateOwningGroupSnapshot(snapshot, sessionId, (groupSnapshot) =>
    setSessionTitleInSnapshot(groupSnapshot, sessionId, title),
  );
}

export function setBrowserSessionMetadataInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  title: string,
  url: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  return updateOwningGroupSnapshot(snapshot, sessionId, (groupSnapshot) =>
    setBrowserSessionMetadataInSnapshot(groupSnapshot, sessionId, title, url),
  );
}

export function removeSessionInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  return updateOwningGroupSnapshot(snapshot, sessionId, (groupSnapshot) =>
    removeSessionInSnapshot(groupSnapshot, sessionId),
  );
}

export function setVisibleCountInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  visibleCount: VisibleSessionCount,
): GroupedSessionWorkspaceSnapshot {
  return updateActiveGroupSnapshot(snapshot, getActiveGroup, (groupSnapshot) =>
    setVisibleCountInSnapshot(groupSnapshot, visibleCount),
  );
}

export function toggleFullscreenSessionInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
): GroupedSessionWorkspaceSnapshot {
  return updateActiveGroupSnapshot(snapshot, getActiveGroup, (groupSnapshot) =>
    toggleFullscreenSessionInSnapshot(groupSnapshot),
  );
}

export function setViewModeInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  viewMode: TerminalViewMode,
): GroupedSessionWorkspaceSnapshot {
  return updateActiveGroupSnapshot(snapshot, getActiveGroup, (groupSnapshot) =>
    setViewModeInSnapshot(groupSnapshot, viewMode),
  );
}

export function syncSessionOrderInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sessionIds: readonly string[],
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const group = getGroupById(normalizedSnapshot, groupId);
  if (!group) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const result = reorderGroupSessions(group.snapshot, sessionIds);
  return {
    changed: result.changed,
    snapshot: result.changed
      ? {
          ...normalizedSnapshot,
          groups: updateGroup(normalizedSnapshot.groups, groupId, result.snapshot),
        }
      : normalizedSnapshot,
  };
}
