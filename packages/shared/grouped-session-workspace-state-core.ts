import {
  DEFAULT_MAIN_GROUP_ID,
  DEFAULT_MAIN_GROUP_TITLE,
  MAX_GROUP_COUNT,
  MAX_SESSION_DISPLAY_ID_COUNT,
  createDefaultGroupedSessionWorkspaceSnapshot,
  createDefaultSessionGridSnapshot,
  type GroupedSessionWorkspaceSnapshot,
  type SessionGroupRecord,
} from "./session-grid-contract";
import {
  isGroupedWorkspaceSnapshot,
  isSessionGroupRecord,
  normalizeGroup,
  normalizeWorkspaceSessionDisplayIds,
} from "./grouped-session-workspace-state-helpers";

export function normalizeGroupedSessionWorkspaceSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot | undefined,
): GroupedSessionWorkspaceSnapshot {
  if (!isGroupedWorkspaceSnapshot(snapshot)) {
    return createDefaultGroupedSessionWorkspaceSnapshot();
  }

  const groups = snapshot.groups
    .filter((group) => isSessionGroupRecord(group))
    .slice(0, MAX_GROUP_COUNT)
    .map((group, index) => normalizeGroup(group, index));
  const normalizedGroups =
    groups.length > 0
      ? groups
      : [
          {
            groupId: DEFAULT_MAIN_GROUP_ID,
            snapshot: createDefaultSessionGridSnapshot(),
            title: DEFAULT_MAIN_GROUP_TITLE,
          },
        ];
  const displayIdNormalization = normalizeWorkspaceSessionDisplayIds(normalizedGroups);

  return {
    activeGroupId: normalizedGroups.some((group) => group.groupId === snapshot.activeGroupId)
      ? snapshot.activeGroupId
      : normalizedGroups[0].groupId,
    groups: displayIdNormalization.groups,
    nextGroupNumber:
      typeof snapshot.nextGroupNumber === "number" && snapshot.nextGroupNumber > 1
        ? snapshot.nextGroupNumber
        : normalizedGroups.length + 1,
    nextSessionDisplayId:
      typeof snapshot.nextSessionDisplayId === "number" &&
      Number.isInteger(snapshot.nextSessionDisplayId) &&
      snapshot.nextSessionDisplayId >= 0
        ? snapshot.nextSessionDisplayId % MAX_SESSION_DISPLAY_ID_COUNT
        : displayIdNormalization.nextSessionDisplayId,
    nextSessionNumber:
      typeof snapshot.nextSessionNumber === "number" && snapshot.nextSessionNumber > 0
        ? snapshot.nextSessionNumber
        : 1,
  };
}

export function getActiveGroup(
  snapshot: GroupedSessionWorkspaceSnapshot,
): SessionGroupRecord | undefined {
  return snapshot.groups.find((group) => group.groupId === snapshot.activeGroupId);
}

export function getGroupById(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): SessionGroupRecord | undefined {
  return snapshot.groups.find((group) => group.groupId === groupId);
}

export function getGroupForSession(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): SessionGroupRecord | undefined {
  return snapshot.groups.find((group) =>
    group.snapshot.sessions.some((session) => session.sessionId === sessionId),
  );
}
