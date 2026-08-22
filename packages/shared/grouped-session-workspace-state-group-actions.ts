import {
  MAX_GROUP_COUNT,
  createDefaultSessionGridSnapshot,
  type GroupedSessionWorkspaceSnapshot,
} from "./session-grid-contract";
import { focusSessionInSnapshot, removeSessionInSnapshot } from "./session-grid-state";
import {
  findGroupContainingSession,
  getSessionRecord,
  insertSessionIntoGroup,
} from "./grouped-session-workspace-state-helpers";
import {
  getGroupById,
  normalizeGroupedSessionWorkspaceSnapshot,
} from "./grouped-session-workspace-state-core";

export function focusGroupInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  if (!normalizedSnapshot.groups.some((group) => group.groupId === groupId)) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  return {
    changed: normalizedSnapshot.activeGroupId !== groupId,
    snapshot:
      normalizedSnapshot.activeGroupId === groupId
        ? normalizedSnapshot
        : {
            ...normalizedSnapshot,
            activeGroupId: groupId,
          },
  };
}

export function focusGroupByIndexInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupIndex: number,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const targetGroup = normalizedSnapshot.groups[groupIndex - 1];
  return targetGroup
    ? focusGroupInWorkspace(normalizedSnapshot, targetGroup.groupId)
    : { changed: false, snapshot: normalizedSnapshot };
}

export function renameGroupInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  title: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const normalizedTitle = title.trim();
  if (!normalizedTitle) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  let changed = false;
  const groups = normalizedSnapshot.groups.map((group) => {
    if (group.groupId !== groupId || group.title === normalizedTitle) {
      return group;
    }

    changed = true;
    return { ...group, title: normalizedTitle };
  });

  return { changed, snapshot: changed ? { ...normalizedSnapshot, groups } : normalizedSnapshot };
}

export function removeGroupInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  if (
    normalizedSnapshot.groups.length <= 1 ||
    !normalizedSnapshot.groups.some((group) => group.groupId === groupId)
  ) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const removedGroupIndex = normalizedSnapshot.groups.findIndex(
    (group) => group.groupId === groupId,
  );
  const groups = normalizedSnapshot.groups.filter((group) => group.groupId !== groupId);

  return {
    changed: true,
    snapshot: {
      ...normalizedSnapshot,
      activeGroupId:
        normalizedSnapshot.activeGroupId !== groupId
          ? normalizedSnapshot.activeGroupId
          : (groups[Math.max(0, removedGroupIndex - 1)] ?? groups[0]).groupId,
      groups,
    },
  };
}

export function syncGroupOrderInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupIds: readonly string[],
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const currentGroupIds = normalizedSnapshot.groups.map((group) => group.groupId);
  if (groupIds.length !== currentGroupIds.length) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  if (currentGroupIds.every((groupId, index) => groupId === groupIds[index])) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentGroupIdSet = new Set(currentGroupIds);
  const incomingGroupIdSet = new Set(groupIds);
  if (
    incomingGroupIdSet.size !== groupIds.length ||
    currentGroupIds.some((groupId) => !incomingGroupIdSet.has(groupId)) ||
    groupIds.some((groupId) => !currentGroupIdSet.has(groupId))
  ) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const groupsById = new Map(normalizedSnapshot.groups.map((group) => [group.groupId, group]));
  return {
    changed: true,
    snapshot: {
      ...normalizedSnapshot,
      groups: groupIds.map((groupId) => {
        const group = groupsById.get(groupId);
        if (!group) {
          throw new Error(`Missing group for reorder: ${groupId}`);
        }
        return group;
      }),
    },
  };
}

export function createGroupInWorkspace(snapshot: GroupedSessionWorkspaceSnapshot): {
  changed: boolean;
  groupId?: string;
  snapshot: GroupedSessionWorkspaceSnapshot;
} {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  if (normalizedSnapshot.groups.length >= MAX_GROUP_COUNT) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const nextGroupNumber = normalizedSnapshot.nextGroupNumber;
  const groupId = `group-${nextGroupNumber}`;

  return {
    changed: true,
    groupId,
    snapshot: {
      ...normalizedSnapshot,
      activeGroupId: groupId,
      groups: [
        ...normalizedSnapshot.groups,
        {
          groupId,
          snapshot: createDefaultSessionGridSnapshot(),
          title: `Group ${nextGroupNumber}`,
        },
      ],
      nextGroupNumber: nextGroupNumber + 1,
    },
  };
}

export function moveSessionToGroupInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  targetGroupId: string,
  targetIndex?: number,
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  const sourceLocation = findGroupContainingSession(normalizedSnapshot, sessionId);
  const targetGroup = getGroupById(normalizedSnapshot, targetGroupId);
  if (!sourceLocation || !targetGroup || sourceLocation.group.groupId === targetGroupId) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const sessionRecord = getSessionRecord(sourceLocation.group.snapshot, sessionId);
  if (!sessionRecord) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const removedSourceSnapshot = removeSessionInSnapshot(
    sourceLocation.group.snapshot,
    sessionId,
  ).snapshot;
  const focusedTargetSnapshot = focusSessionInSnapshot(
    insertSessionIntoGroup(targetGroup.snapshot, sessionRecord, targetIndex),
    sessionId,
  ).snapshot;

  return {
    changed: true,
    snapshot: {
      ...normalizedSnapshot,
      activeGroupId: targetGroupId,
      groups: normalizedSnapshot.groups.map((group) => {
        if (group.groupId === sourceLocation.group.groupId) {
          return { ...group, snapshot: removedSourceSnapshot };
        }
        if (group.groupId === targetGroupId) {
          return { ...group, snapshot: focusedTargetSnapshot };
        }
        return group;
      }),
    },
  };
}

export function createGroupFromSessionInWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): { changed: boolean; groupId?: string; snapshot: GroupedSessionWorkspaceSnapshot } {
  const normalizedSnapshot = normalizeGroupedSessionWorkspaceSnapshot(snapshot);
  if (normalizedSnapshot.groups.length >= MAX_GROUP_COUNT) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const sourceLocation = findGroupContainingSession(normalizedSnapshot, sessionId);
  if (!sourceLocation) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const sessionRecord = getSessionRecord(sourceLocation.group.snapshot, sessionId);
  if (!sessionRecord) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const groupId = `group-${normalizedSnapshot.nextGroupNumber}`;
  const nextGroupSnapshot = focusSessionInSnapshot(
    insertSessionIntoGroup(createDefaultSessionGridSnapshot(), sessionRecord),
    sessionId,
  ).snapshot;

  return {
    changed: true,
    groupId,
    snapshot: {
      ...normalizedSnapshot,
      activeGroupId: groupId,
      groups: [
        ...normalizedSnapshot.groups.map((group) =>
          group.groupId === sourceLocation.group.groupId
            ? {
                ...group,
                snapshot: removeSessionInSnapshot(group.snapshot, sessionId).snapshot,
              }
            : group,
        ),
        {
          groupId,
          snapshot: nextGroupSnapshot,
          title: `Group ${normalizedSnapshot.nextGroupNumber}`,
        },
      ],
      nextGroupNumber: normalizedSnapshot.nextGroupNumber + 1,
    },
  };
}
