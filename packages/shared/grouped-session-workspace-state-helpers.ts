import {
  DEFAULT_MAIN_GROUP_ID,
  DEFAULT_MAIN_GROUP_TITLE,
  MAX_SESSION_DISPLAY_ID_COUNT,
  formatSessionDisplayId,
  getOrderedSessions,
  type GroupedSessionWorkspaceSnapshot,
  type SessionGridSnapshot,
  type SessionGroupRecord,
  type SessionRecord,
} from "./session-grid-contract";
import { normalizeSessionGridSnapshot } from "./session-grid-state";

export type GroupLocation = {
  group: SessionGroupRecord;
  index: number;
};

export function insertSessionIntoGroup(
  snapshot: SessionGridSnapshot,
  sessionRecord: SessionRecord,
  targetIndex?: number,
): SessionGridSnapshot {
  const orderedSessions = getOrderedSessions(snapshot);
  const insertionIndex =
    typeof targetIndex === "number"
      ? Math.max(0, Math.min(targetIndex, orderedSessions.length))
      : orderedSessions.length;
  const nextSessions = [...orderedSessions];
  nextSessions.splice(insertionIndex, 0, sessionRecord);

  return normalizeSessionGridSnapshot({
    ...snapshot,
    sessions: nextSessions,
  });
}

export function updateActiveGroupSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot,
  getActiveGroup: (snapshot: GroupedSessionWorkspaceSnapshot) => SessionGroupRecord | undefined,
  updater: (groupSnapshot: SessionGridSnapshot) => SessionGridSnapshot,
): GroupedSessionWorkspaceSnapshot {
  const activeGroup = getActiveGroup(snapshot);
  if (!activeGroup) {
    return snapshot;
  }

  return {
    ...snapshot,
    groups: updateGroup(snapshot.groups, activeGroup.groupId, updater(activeGroup.snapshot)),
  };
}

export function updateOwningGroupSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  updater: (groupSnapshot: SessionGridSnapshot) => {
    changed: boolean;
    snapshot: SessionGridSnapshot;
  },
): { changed: boolean; snapshot: GroupedSessionWorkspaceSnapshot } {
  const location = findGroupContainingSession(snapshot, sessionId);
  if (!location) {
    return { changed: false, snapshot };
  }

  const result = updater(location.group.snapshot);
  return {
    changed: result.changed,
    snapshot: result.changed
      ? {
          ...snapshot,
          groups: updateGroup(snapshot.groups, location.group.groupId, result.snapshot),
        }
      : snapshot,
  };
}

export function updateGroup(
  groups: readonly SessionGroupRecord[],
  groupId: string,
  snapshot: SessionGridSnapshot,
): SessionGroupRecord[] {
  return groups.map((group) =>
    group.groupId === groupId
      ? {
          ...group,
          snapshot,
        }
      : group,
  );
}

export function findGroupContainingSession(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): GroupLocation | undefined {
  for (let index = 0; index < snapshot.groups.length; index += 1) {
    const group = snapshot.groups[index];
    if (group.snapshot.sessions.some((session) => session.sessionId === sessionId)) {
      return { group, index };
    }
  }

  return undefined;
}

export function getSessionRecord(
  snapshot: SessionGridSnapshot,
  sessionId: string,
): SessionRecord | undefined {
  return snapshot.sessions.find((session) => session.sessionId === sessionId);
}

export function isGroupedWorkspaceSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot | undefined,
): snapshot is GroupedSessionWorkspaceSnapshot {
  return Boolean(
    snapshot && Array.isArray(snapshot.groups) && typeof snapshot.activeGroupId === "string",
  );
}

export function isSessionGroupRecord(candidate: unknown): candidate is SessionGroupRecord {
  return Boolean(
    candidate &&
    typeof candidate === "object" &&
    typeof (candidate as SessionGroupRecord).groupId === "string" &&
    typeof (candidate as SessionGroupRecord).title === "string" &&
    typeof (candidate as SessionGroupRecord).snapshot === "object",
  );
}

export function normalizeGroup(group: SessionGroupRecord, index: number): SessionGroupRecord {
  return {
    groupId:
      group.groupId.trim().length > 0
        ? group.groupId
        : index === 0
          ? DEFAULT_MAIN_GROUP_ID
          : `group-${index + 1}`,
    snapshot: normalizeSessionGridSnapshot(group.snapshot),
    title:
      group.title.trim().length > 0
        ? group.title.trim()
        : index === 0
          ? DEFAULT_MAIN_GROUP_TITLE
          : `Group ${index + 1}`,
  };
}

export function normalizeWorkspaceSessionDisplayIds(groups: readonly SessionGroupRecord[]): {
  groups: SessionGroupRecord[];
  nextSessionDisplayId: number;
} {
  const usedDisplayIds = new Set<string>();
  let nextAvailableDisplayId = 0;
  let changed = false;

  const nextGroups = groups.map((group) => {
    const sessions = group.snapshot.sessions.map((session) => {
      const normalizedDisplayId = formatSessionDisplayId(session.displayId ?? "");
      if (
        !usedDisplayIds.has(normalizedDisplayId) &&
        isValidSessionDisplayId(normalizedDisplayId)
      ) {
        usedDisplayIds.add(normalizedDisplayId);
        return {
          ...session,
          displayId: normalizedDisplayId,
        };
      }

      changed = true;
      const claimedDisplayId = claimDisplayId(usedDisplayIds, nextAvailableDisplayId);
      nextAvailableDisplayId = claimedDisplayId.nextSessionDisplayId;
      const normalizedAlias = session.alias.trim();
      return {
        ...session,
        alias: normalizedAlias === normalizedDisplayId ? claimedDisplayId.displayId : session.alias,
        displayId: claimedDisplayId.displayId,
      };
    });

    return sessions === group.snapshot.sessions && !changed
      ? group
      : {
          ...group,
          snapshot: {
            ...group.snapshot,
            sessions,
          },
        };
  });

  const nextSessionDisplayId = findNextAvailableDisplayId(usedDisplayIds, nextAvailableDisplayId);
  return {
    groups: changed ? nextGroups : [...nextGroups],
    nextSessionDisplayId,
  };
}

export function claimNextSessionDisplayId(snapshot: GroupedSessionWorkspaceSnapshot): {
  displayId: string;
  nextSessionDisplayId: number;
} {
  const usedDisplayIds = new Set(
    snapshot.groups.flatMap((group) => group.snapshot.sessions.map((session) => session.displayId)),
  );
  return claimDisplayId(usedDisplayIds, snapshot.nextSessionDisplayId);
}

export function claimDisplayId(
  usedDisplayIds: Set<string>,
  startDisplayId: number,
): { displayId: string; nextSessionDisplayId: number } {
  const normalizedStartDisplayId =
    ((Math.floor(startDisplayId) % MAX_SESSION_DISPLAY_ID_COUNT) + MAX_SESSION_DISPLAY_ID_COUNT) %
    MAX_SESSION_DISPLAY_ID_COUNT;
  const nextDisplayId = findNextAvailableDisplayId(usedDisplayIds, normalizedStartDisplayId);
  const displayId = formatSessionDisplayId(nextDisplayId);
  usedDisplayIds.add(displayId);
  return {
    displayId,
    nextSessionDisplayId: (nextDisplayId + 1) % MAX_SESSION_DISPLAY_ID_COUNT,
  };
}

export function findNextAvailableDisplayId(
  usedDisplayIds: Set<string>,
  startDisplayId: number,
): number {
  for (let offset = 0; offset < MAX_SESSION_DISPLAY_ID_COUNT; offset += 1) {
    const candidate = (startDisplayId + offset) % MAX_SESSION_DISPLAY_ID_COUNT;
    if (!usedDisplayIds.has(formatSessionDisplayId(candidate))) {
      return candidate;
    }
  }

  return startDisplayId % MAX_SESSION_DISPLAY_ID_COUNT;
}

function isValidSessionDisplayId(displayId: string): boolean {
  return /^\d{2}$/.test(displayId) || /^(?:g|s)-[a-z0-9-]+$/i.test(displayId);
}
