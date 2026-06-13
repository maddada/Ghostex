import {
  clampVisibleSessionCount,
  DEFAULT_MAIN_GROUP_ID,
  DEFAULT_MAIN_GROUP_TITLE,
  MAX_GROUP_COUNT,
  createDefaultGroupedSessionWorkspaceSnapshot,
  createDefaultSessionGridSnapshot,
  createSessionRecord,
  createTimestampedSessionId,
  formatSessionDisplayId,
  getOrderedSessions,
  getSessionNumberFromSessionId,
  getSlotPosition,
  type GroupedSessionWorkspaceSnapshot,
  type SessionGroupRecord,
  type SessionGridDirection,
  type SessionGridSnapshot,
  type SessionPaneLayoutNode,
  type SessionPaneSplitDirection,
  type SessionRecord,
  type SessionTitleSource,
  type TerminalEngine,
  type TerminalSessionPersistenceProvider,
  type SidebarSessionTag,
  type T3SessionMetadata,
  type TerminalViewMode,
  type VisibleSessionCount,
  type CreateSessionRecordOptions,
} from "./session-grid-contract";
import { normalizeWorkspaceSessionDisplayIds } from "./grouped-session-workspace-state-helpers";
import { focusVisibleDirectionInSnapshot } from "./session-grid-state-create-focus";
import { normalizeSessionRecord, reindexSessionsInOrder } from "./session-grid-state-helpers";
import { reorderGroupSessions } from "./session-order-reorder";
import { normalizeT3SessionMetadata } from "./t3-session-metadata";

type WorkspaceMutationResult = {
  changed: boolean;
  snapshot: GroupedSessionWorkspaceSnapshot;
};

type CreateSessionResult = {
  session?: SessionRecord;
  snapshot: GroupedSessionWorkspaceSnapshot;
};

type SessionPaneTabInsertPosition = "after" | "append" | "before";

export type VisibleSessionPlacement =
  | {
      kind: "appendFullWidth";
    }
  | {
      kind: "appendToTabGroup";
      position?: SessionPaneTabInsertPosition;
      targetSessionId: string;
    }
  | {
      kind: "insertAfter";
      splitDirection?: SessionPaneSplitDirection;
      targetSessionId?: string;
    }
  | {
      kind: "replace";
      targetSessionId: string;
    }
  | {
      kind: "replaceNonFocused";
      preserveSessionId?: string;
    };

export type SessionPaneDropPlacement = "bottom" | "center" | "left" | "right" | "top";
export type SessionPaneTabReorderPosition = "after" | "before";

type CreateSessionInSimpleWorkspaceOptions = {
  usedSessionIds?: readonly string[];
  visiblePlacement?: VisibleSessionPlacement;
};

export type VirtualPaneTabMaterializationIntent = "explicitLayoutMutation" | "passiveSync";

type EnsureVirtualPaneTabsOptions = {
  intent?: VirtualPaneTabMaterializationIntent;
};

type MaterializeVirtualPaneTabsOptions = {
  preserveSplitTopology?: boolean;
};

type MoveSessionInPaneLayoutOptions = {
  wakeSourceSession?: boolean;
};

type CreateGroupResult = WorkspaceMutationResult & {
  groupId?: string;
};

export function normalizeSimpleGroupedSessionWorkspaceSnapshot(
  snapshot: GroupedSessionWorkspaceSnapshot | undefined,
): GroupedSessionWorkspaceSnapshot {
  const baseSnapshot = snapshot ?? createDefaultGroupedSessionWorkspaceSnapshot();
  const preparedGroups = baseSnapshot.groups.map((group, index) =>
    prepareGroupForDisplayIdNormalization(group, index),
  );
  const groups =
    preparedGroups.length > 0
      ? preparedGroups
      : [createEmptyGroup(DEFAULT_MAIN_GROUP_ID, DEFAULT_MAIN_GROUP_TITLE)];
  const displayIdNormalization = normalizeWorkspaceSessionDisplayIds(groups);
  const normalizedGroups = displayIdNormalization.groups.map((group, index) =>
    normalizeGroup(group, index),
  );
  const activeGroupId = normalizedGroups.some(
    (group) => group.groupId === baseSnapshot.activeGroupId,
  )
    ? baseSnapshot.activeGroupId
    : normalizedGroups[0]!.groupId;

  return {
    activeGroupId,
    groups: normalizedGroups,
    nextGroupNumber: Math.max(
      2,
      baseSnapshot.nextGroupNumber,
      getNextGroupNumber(normalizedGroups),
    ),
    nextSessionDisplayId: Math.max(0, displayIdNormalization.nextSessionDisplayId),
    nextSessionNumber: Math.max(
      1,
      baseSnapshot.nextSessionNumber,
      getNextSessionNumber(normalizedGroups),
    ),
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

export function createSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  options?: CreateSessionRecordOptions,
  createOptions?: CreateSessionInSimpleWorkspaceOptions,
): CreateSessionResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const activeGroup = getActiveGroup(normalizedSnapshot);
  if (!activeGroup) {
    return { snapshot: normalizedSnapshot };
  }

  const usedSessionIds = [
    ...getWorkspaceSessionIds(normalizedSnapshot),
    ...(createOptions?.usedSessionIds ?? []),
  ];
  /*
  CDXC:GxserverSessionIdentity 2026-05-30-18:20:
  gxserver-generated session IDs must be preserved when the macOS sidebar creates the local layout record after daemon allocation. The shared workspace helper may still mint timestamp IDs for non-daemon panes, but an explicit sessionId is authoritative and duplicate use is a caller bug rather than a recoverable layout decision.
  */
  const requestedSessionId = typeof options?.sessionId === "string" && options.sessionId.trim()
    ? options.sessionId.trim()
    : undefined;
  if (requestedSessionId && usedSessionIds.includes(requestedSessionId)) {
    throw new Error(`Session ${requestedSessionId} already exists in this workspace.`);
  }
  const sessionId = requestedSessionId ?? createTimestampedSessionId(usedSessionIds);
  const nextSession = createSessionRecord(
    normalizedSnapshot.nextSessionNumber,
    activeGroup.snapshot.sessions.length,
    {
      ...options,
      displayId: sessionId,
      sessionId,
    } as CreateSessionRecordOptions & { displayId: string },
  );
  const nextSessionRecord = normalizeSessionRecord(nextSession);
  const shouldCreateInBackground = options?.initialPresentation === "background";
  const nextSnapshot = updateGroup(normalizedSnapshot, activeGroup.groupId, (group) => {
    const nextSessions = [...group.snapshot.sessions, nextSessionRecord];
    const visiblePlacement = createOptions?.visiblePlacement;
    const currentVisibleSessionIds = group.snapshot.visibleSessionIds;
    const placementResult =
      shouldCreateInBackground || !visiblePlacement
        ? undefined
        : getVisibleSessionPlacementResult(
            getAwakeSessions(nextSessions),
            group.snapshot.visibleCount,
            group.snapshot.visibleSessionIds,
            nextSessionRecord.sessionId,
            visiblePlacement,
          );
    const nextVisibleSessionIds = shouldCreateInBackground
      ? currentVisibleSessionIds
      : (placementResult?.visibleSessionIds ??
        getNormalizedVisibleIds(
          nextSessions,
          group.snapshot.visibleCount,
          nextSessionRecord.sessionId,
          [...currentVisibleSessionIds, nextSessionRecord.sessionId],
        ));
    const nextGroup = {
      ...group,
      snapshot: normalizeGroupSnapshot({
        ...group.snapshot,
        focusedSessionId: shouldCreateInBackground
          ? group.snapshot.focusedSessionId
          : nextSessionRecord.sessionId,
        paneLayout: shouldCreateInBackground
          ? group.snapshot.paneLayout
          : getNextPaneLayoutForCreatedSession(
              group.snapshot.paneLayout,
              currentVisibleSessionIds,
              nextVisibleSessionIds,
              nextSessionRecord.sessionId,
              group.snapshot.focusedSessionId,
              visiblePlacement,
              placementResult?.replacedSessionId,
            ),
        sessions: nextSessions,
        visibleCount: placementResult?.visibleCount ?? group.snapshot.visibleCount,
        visibleSessionIds: nextVisibleSessionIds,
      }),
    };
    return nextGroup;
  });

  return {
    session: nextSessionRecord,
    snapshot: {
      ...nextSnapshot,
      nextSessionDisplayId: normalizedSnapshot.nextSessionDisplayId,
      nextSessionNumber: normalizedSnapshot.nextSessionNumber + 1,
    },
  };
}

export function focusGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): WorkspaceMutationResult {
  if (
    !snapshot.groups.some((group) => group.groupId === groupId) ||
    snapshot.activeGroupId === groupId
  ) {
    return { changed: false, snapshot };
  }

  return {
    changed: true,
    snapshot: normalizeSimpleGroupedSessionWorkspaceSnapshot({
      ...snapshot,
      activeGroupId: groupId,
    }),
  };
}

export function focusGroupByIndexInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupIndex: number,
): WorkspaceMutationResult {
  const targetGroup = snapshot.groups[groupIndex - 1];
  if (!targetGroup) {
    return { changed: false, snapshot };
  }

  return focusGroupInSimpleWorkspace(snapshot, targetGroup.groupId);
}

export function focusVisibleDirectionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  direction: SessionGridDirection,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const activeGroup = getActiveGroup(normalizedSnapshot);
  if (!activeGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const result = focusVisibleDirectionInSnapshot(activeGroup.snapshot, direction);
  if (!result.changed) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  /**
   * CDXC:PaneFocus 2026-05-28-14:29:
   * Native macOS Cmd+Alt+Arrow focus is constrained to the active group's visible pane set.
   * Preserve visibleSessionIds exactly so directional focus never replaces a visible native session tab with a hidden/background session.
   */
  const nextSnapshot = updateGroup(normalizedSnapshot, activeGroup.groupId, (group) => ({
    ...group,
    snapshot: result.snapshot,
  }));
  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function focusSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): WorkspaceMutationResult {
  const normalizedSnapshot = restoreFocusModeForExternalSession(
    normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot),
    sessionId,
  );
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  const isRestoringSleepingSession = currentSession?.isSleeping === true;
  const focusedTabSessionIds = owningGroup.snapshot.focusedSessionId
    ? findPaneTabGroupSessionIds(
        owningGroup.snapshot.paneLayout,
        owningGroup.snapshot.focusedSessionId,
      )
    : undefined;
  const isFocusingWithinFocusModeTabGroup =
    owningGroup.snapshot.visibleCount === 1 &&
    owningGroup.snapshot.fullscreenRestoreVisibleCount !== undefined &&
    focusedTabSessionIds?.includes(sessionId) === true;
  const nextSessions = owningGroup.snapshot.sessions.map((session) =>
    session.sessionId === sessionId ? { ...session, isSleeping: false } : session,
  );
  const nextVisibleSessionIds = isRestoringSleepingSession
    ? getNextVisibleIdsForRestoredSleepingSession(
        getAwakeSessions(nextSessions),
        owningGroup.snapshot.visibleCount,
        sessionId,
        owningGroup.snapshot.visibleSessionIds,
        owningGroup.snapshot.focusedSessionId,
      )
    : getNextVisibleIdsForFocusedSession(
        getAwakeSessions(nextSessions),
        owningGroup.snapshot.visibleCount,
        sessionId,
        owningGroup.snapshot.visibleSessionIds,
        owningGroup.snapshot.focusedSessionId,
      );
  const nextVisibleCount = isRestoringSleepingSession
    ? clampSupportedVisibleCount(Math.max(1, nextVisibleSessionIds.length))
    : owningGroup.snapshot.visibleCount;
  const nextPaneLayout = isRestoringSleepingSession
    ? getNextPaneLayoutForRestoredSleepingSession(
        owningGroup.snapshot.paneLayout,
        owningGroup.snapshot.visibleSessionIds,
        nextVisibleSessionIds,
        sessionId,
        owningGroup.snapshot.focusedSessionId,
      )
    : isFocusingWithinFocusModeTabGroup
      ? setActiveSessionInPaneLayout(owningGroup.snapshot.paneLayout, sessionId)
    : getNextPaneLayoutForFocusedSession(
        owningGroup.snapshot.paneLayout,
        owningGroup.snapshot.visibleSessionIds,
        nextVisibleSessionIds,
        sessionId,
        owningGroup.snapshot.focusedSessionId,
      );

  const nextSnapshot = updateGroup(
    {
      ...normalizedSnapshot,
      activeGroupId: owningGroup.groupId,
    },
    owningGroup.groupId,
    (group) => ({
      ...group,
      snapshot: normalizeGroupSnapshot({
        ...group.snapshot,
        focusedSessionId: sessionId,
        ...(nextPaneLayout ? { paneLayout: nextPaneLayout } : {}),
        sessions: nextSessions,
        visibleCount: nextVisibleCount,
        visibleSessionIds: nextVisibleSessionIds,
      }),
    }),
  );

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function focusSidebarSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): WorkspaceMutationResult {
  const normalizedSnapshot = restoreFocusModeForExternalSession(
    normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot),
    sessionId,
  );
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }
  const groupSnapshotWithVirtualTabs = materializeAllSessionsInFocusedPaneTabGroup(
    owningGroup.snapshot,
  );
  const currentSession = groupSnapshotWithVirtualTabs.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  const shouldSelectExistingPaneTab =
    currentSession?.isSleeping !== true &&
    paneLayoutContainsSession(groupSnapshotWithVirtualTabs.paneLayout, sessionId);
  if (!shouldSelectExistingPaneTab) {
    return focusSessionInSimpleWorkspace(normalizedSnapshot, sessionId);
  }
  const snapshotWithVirtualTabs = updateGroup(
    {
      ...normalizedSnapshot,
      activeGroupId: owningGroup.groupId,
    },
    owningGroup.groupId,
    (group) => ({
      ...group,
      /**
       * CDXC:SidebarSessionFocus 2026-05-29-09:47:
       * Sidebar clicks on unmounted-but-not-sleeping sessions should select the
       * session in its existing paneLayout tab group. The old generic focus
       * path reused the currently active pane and moved the tab before restore,
       * which changed split ownership and tab order even though paneLayout
       * already knew where the session belonged.
       */
      snapshot: groupSnapshotWithVirtualTabs,
    }),
  );
  return selectPaneTabInSimpleWorkspace(snapshotWithVirtualTabs, owningGroup.groupId, sessionId);
}

export function focusSessionExclusivelyInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const directOwningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  const directGroupSnapshot = directOwningGroup?.snapshot;
  const directPaneLayout = directGroupSnapshot
    ? setActiveSessionInPaneLayout(directGroupSnapshot.paneLayout, sessionId)
    : undefined;
  if (
    directOwningGroup &&
    directGroupSnapshot &&
    directPaneLayout &&
    paneLayoutContainsSession(directPaneLayout, sessionId)
  ) {
    const nextSessions = directGroupSnapshot.sessions.map((session) =>
      session.sessionId === sessionId ? { ...session, isSleeping: false } : session,
    );
    const selectedSnapshot = normalizeGroupSnapshot({
      ...directGroupSnapshot,
      focusedSessionId: sessionId,
      paneLayout: directPaneLayout,
      sessions: nextSessions,
    });
    if (!hasMultiplePaneOwners(selectedSnapshot)) {
      return {
        changed: !areGroupSnapshotsEqual(directGroupSnapshot, selectedSnapshot),
        snapshot: updateGroup(normalizedSnapshot, directOwningGroup.groupId, (group) => ({
          ...group,
          snapshot: selectedSnapshot,
        })),
      };
    }
    const restoreVisibleCount =
      selectedSnapshot.fullscreenRestoreVisibleCount ??
      (selectedSnapshot.visibleCount > 1
        ? clampSupportedVisibleCount(selectedSnapshot.visibleCount)
        : undefined);
    const nextSnapshot = updateGroup(normalizedSnapshot, directOwningGroup.groupId, (group) => ({
      ...group,
      snapshot: normalizeGroupSnapshot({
        ...selectedSnapshot,
        /*
         * CDXC:SessionFocusMode 2026-06-04-20:37:
         * Double-click Focus targets the paneLayout tab group that already owns the clicked native tab.
         * Preserve the full split tree and only mark the clicked tab active before zooming, because using generic hidden-session focus can move sibling pane tabs into one tab group and leave Exit focus with no split tree to restore.
         */
        fullscreenRestoreVisibleCount: restoreVisibleCount,
        visibleCount: 1,
        visibleSessionIds: [sessionId],
      }),
    }));

    return {
      changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
      snapshot: nextSnapshot,
    };
  }

  const focusedResult = focusSessionInSimpleWorkspace(snapshot, sessionId);
  const focusedSnapshot = focusedResult.snapshot;
  const owningGroup = getGroupForSession(focusedSnapshot, sessionId);
  if (!owningGroup) {
    return focusedResult;
  }
  if (!hasMultiplePaneOwners(owningGroup.snapshot)) {
    /**
     * CDXC:SessionFocusMode 2026-05-28-09:41:
     * Double-click Focus is only meaningful when a project has split panes to collapse.
     * A single pane may contain multiple top tabs, but focusing it should stay a normal tab selection without creating reversible focus mode or an Exit focus titlebar button.
     *
     * CDXC:SessionFocusMode 2026-05-28-15:35:
     * Sleeping split panes are persisted topology, not rendered panes. If every other pane owner is sleeping, Focus should stay a normal selection because the user visually has one pane and there is nothing visible to zoom.
     */
    return focusedResult;
  }
  const restoreVisibleCount =
    owningGroup.snapshot.fullscreenRestoreVisibleCount ??
    (owningGroup.snapshot.visibleCount > 1
      ? clampSupportedVisibleCount(owningGroup.snapshot.visibleCount)
      : undefined);
  const nextSnapshot = updateGroup(focusedSnapshot, owningGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      /**
       * CDXC:SessionFocusMode 2026-05-23-09:28:
       * Double-click Focus is a reversible pane/tab-group zoom, not a split
       * rewrite. Store the previous visible count on the active group so
       * unfocus restores the user's split density while native layout scopes
       * the visible workarea to the focused tab group.
       */
      fullscreenRestoreVisibleCount: restoreVisibleCount,
      visibleCount: 1,
    }),
  }));

  return {
    changed: focusedResult.changed || !areSnapshotsEqual(focusedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function renameGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  title: string,
): WorkspaceMutationResult {
  const nextTitle = title.trim();
  if (!nextTitle) {
    return { changed: false, snapshot };
  }

  const nextSnapshot = updateGroup(snapshot, groupId, (group) => ({
    ...group,
    title: nextTitle,
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function removeGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): WorkspaceMutationResult {
  if (!snapshot.groups.some((group) => group.groupId === groupId)) {
    return { changed: false, snapshot };
  }

  const remainingGroups = snapshot.groups.filter((group) => group.groupId !== groupId);
  const normalizedGroups =
    remainingGroups.length > 0
      ? remainingGroups
      : [createEmptyGroup(DEFAULT_MAIN_GROUP_ID, DEFAULT_MAIN_GROUP_TITLE)];
  const activeGroupId = normalizedGroups.some((group) => group.groupId === snapshot.activeGroupId)
    ? snapshot.activeGroupId
    : normalizedGroups[0]!.groupId;

  return {
    changed: true,
    snapshot: normalizeSimpleGroupedSessionWorkspaceSnapshot({
      ...snapshot,
      activeGroupId,
      groups: normalizedGroups,
    }),
  };
}

export function removeSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const replacementSessionId = getClosingPaneTabReplacementSessionId(
    owningGroup.snapshot.paneLayout,
    sessionId,
  );
  const shouldPromoteReplacementSession =
    replacementSessionId !== undefined &&
    owningGroup.snapshot.sessions.some((session) => session.sessionId === replacementSessionId) &&
    isActivePaneLayoutSession(owningGroup.snapshot.paneLayout, sessionId);
  const paneLayoutWithoutSession = owningGroup.snapshot.paneLayout
    ? removeSessionFromPaneLayout(
        owningGroup.snapshot.paneLayout,
        sessionId,
        shouldPromoteReplacementSession ? replacementSessionId : undefined,
      )
    : undefined;
  const visibleSessionIdsWithoutSession = getVisibleSessionIdsAfterSessionClose(
    owningGroup.snapshot.visibleSessionIds,
    sessionId,
    shouldPromoteReplacementSession ? replacementSessionId : undefined,
  );
  const visibleCountAfterClose = paneLayoutWithoutSession
    ? clampSupportedVisibleCount(Math.max(1, countPaneLayoutOwnerSlots(paneLayoutWithoutSession)))
    : owningGroup.snapshot.visibleCount;
  const snapshotWithoutSession = updateGroup(normalizedSnapshot, owningGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      /*
       * CDXC:PaneTabs 2026-06-06-04:32:
       * Closing the active tab in a split pane should select the tab immediately to its right in the same pane before publish-time virtual-tab materialization runs.
       * If that tab was parked, wake it so native sync still has a concrete pane owner and does not collapse the split into the other pane.
       * When the closed tab was the pane's only tab, there is no replacement; remove that split branch and reduce the visible pane count so the remaining pane can show the project's tabs as one stack.
       */
      focusedSessionId:
        group.snapshot.focusedSessionId === sessionId && shouldPromoteReplacementSession
          ? replacementSessionId
          : group.snapshot.focusedSessionId === sessionId
            ? undefined
            : group.snapshot.focusedSessionId,
      paneLayout: paneLayoutWithoutSession,
      sessions: group.snapshot.sessions
        .filter((session) => session.sessionId !== sessionId)
        .map((session) =>
          shouldPromoteReplacementSession && session.sessionId === replacementSessionId
            ? { ...session, isPoppedOut: undefined, isSleeping: false }
            : session,
        ),
      visibleCount: visibleCountAfterClose,
      visibleSessionIds: visibleSessionIdsWithoutSession,
    }),
  }));
  const shouldSwitchGroups =
    normalizedSnapshot.activeGroupId === owningGroup.groupId &&
    getActiveSessionCount(getGroupById(snapshotWithoutSession, owningGroup.groupId)) === 0;
  const fallbackActiveGroupId = shouldSwitchGroups
    ? getFallbackActiveGroupId(snapshotWithoutSession, owningGroup.groupId)
    : snapshotWithoutSession.activeGroupId;
  const nextSnapshot =
    fallbackActiveGroupId === snapshotWithoutSession.activeGroupId
      ? snapshotWithoutSession
      : normalizeSimpleGroupedSessionWorkspaceSnapshot({
          ...snapshotWithoutSession,
          activeGroupId: fallbackActiveGroupId,
        });

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function setSessionSleepingInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  sleeping: boolean,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!currentSession || currentSession.isSleeping === sleeping) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  if (!sleeping) {
    const snapshotWithWakeState = updateGroup(normalizedSnapshot, owningGroup.groupId, (group) => ({
      ...group,
      snapshot: wakeSessionsIntoFocusedPaneTabGroup(group.snapshot, [sessionId]),
    }));
    return {
      changed: !areSnapshotsEqual(normalizedSnapshot, snapshotWithWakeState),
      snapshot: snapshotWithWakeState,
    };
  }

  const snapshotWithSleepState = updateGroup(normalizedSnapshot, owningGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      sessions: group.snapshot.sessions.map((session) =>
        session.sessionId === sessionId
          ? {
              ...session,
              isPoppedOut: sleeping ? undefined : session.isPoppedOut,
              isSleeping: sleeping,
            }
          : session,
      ),
    }),
  }));
  const shouldSwitchGroups =
    sleeping &&
    normalizedSnapshot.activeGroupId === owningGroup.groupId &&
    getActiveSessionCount(getGroupById(snapshotWithSleepState, owningGroup.groupId)) === 0;
  const fallbackActiveGroupId = shouldSwitchGroups
    ? getFallbackActiveGroupId(snapshotWithSleepState, owningGroup.groupId)
    : snapshotWithSleepState.activeGroupId;
  const nextSnapshot =
    fallbackActiveGroupId === snapshotWithSleepState.activeGroupId
      ? snapshotWithSleepState
      : normalizeSimpleGroupedSessionWorkspaceSnapshot({
          ...snapshotWithSleepState,
          activeGroupId: fallbackActiveGroupId,
        });

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function wakePaneTabSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sessionId: string,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const group = getGroupById(normalizedSnapshot, groupId);
  if (!group) {
    return { changed: false, snapshot: normalizedSnapshot };
  }
  const groupSnapshotWithVirtualTabs = materializeAllSessionsInFocusedPaneTabGroup(group.snapshot);
  const currentSession = groupSnapshotWithVirtualTabs?.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!currentSession || currentSession.isSleeping !== true) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const nextVisibleSessionIds = groupSnapshotWithVirtualTabs.visibleSessionIds.includes(sessionId)
    ? groupSnapshotWithVirtualTabs.visibleSessionIds
    : [...groupSnapshotWithVirtualTabs.visibleSessionIds, sessionId];

  const nextSnapshot = updateGroup(normalizedSnapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...groupSnapshotWithVirtualTabs,
      focusedSessionId: sessionId,
      /**
       * CDXC:PaneTabs 2026-05-23-09:08:
       * Clicking a sleeping native pane tab is a restore intent for that tab's
       * existing split/tab group. Do not reuse the generic sidebar wake path,
       * because that intentionally moves sleeping cards into the focused pane
       * and can drain a right split into the left tab group.
       *
       * CDXC:PaneTabs 2026-05-29-09:04:
       * Virtual native tabs are persisted into the focused pane tab group before
       * wake. Selecting a sleeping/unmounted/missing-provider tab must activate
       * that tab in its current group, not synthesize a new split or ignore the
       * click because paneLayout lacked the displayed tab id.
       */
      sessions: groupSnapshotWithVirtualTabs.sessions.map((session) =>
        session.sessionId === sessionId
          ? { ...session, isPoppedOut: undefined, isSleeping: false }
          : session,
      ),
      visibleCount: clampSupportedVisibleCount(
        Math.max(groupSnapshotWithVirtualTabs.visibleCount, nextVisibleSessionIds.length),
      ),
      visibleSessionIds: nextVisibleSessionIds,
    }),
  }));

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function setSessionPoppedOutInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  poppedOut: boolean,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (
    !currentSession ||
    currentSession.isSleeping === true ||
    (currentSession.isPoppedOut === true) === poppedOut
  ) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  /**
   * CDXC:PanePopOut 2026-05-11-09:35
   * Pop-out is a live presentation toggle. It must update the session record
   * without changing tab groups, visible order, focus, or sleep lifecycle so
   * the original pane slot can render a reattach placeholder.
   */
  return updateSession(normalizedSnapshot, sessionId, (session) => ({
    ...session,
    isPoppedOut: poppedOut ? true : undefined,
  }));
}

export function setSessionFavoriteInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  favorite: boolean,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!currentSession || currentSession.isFavorite === favorite) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  return updateSession(normalizedSnapshot, sessionId, (session) => ({
    ...session,
    isFavorite: favorite,
  }));
}

export function setSessionTagInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  sessionTag: SidebarSessionTag | undefined,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!currentSession || currentSession.sessionTag === sessionTag) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  /**
   * CDXC:SessionTags 2026-06-05-12:30:
   * Local workspace snapshots store the expanded session tag while deriving
   * legacy `isFavorite` only from the Favorite tag. This lets old Favorite rows
   * keep their behavior without treating High Priority, Research, Todo, Low
   * Priority, On Hold, or Done as favorites.
   */
  return updateSession(normalizedSnapshot, sessionId, (session) => ({
    ...session,
    isFavorite: sessionTag === "favorite" ? true : undefined,
    sessionTag,
  }));
}

export function setSessionPinnedInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  pinned: boolean,
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const owningGroup = getGroupForSession(normalizedSnapshot, sessionId);
  if (!owningGroup) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const currentSession = owningGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!currentSession || currentSession.isPinned === pinned) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  /**
   * CDXC:PinnedSessions 2026-05-28-12:04:
   * Pinning belongs to the session record inside its project workspace. Keep
   * the mutation narrow so pin/unpin updates live sidebar ordering without
   * changing focus, group membership, or favorite state.
   */
  return updateSession(normalizedSnapshot, sessionId, (session) => ({
    ...session,
    isPinned: pinned,
  }));
}

export function setGroupSleepingInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sleeping: boolean,
  sessionIds?: readonly string[],
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const group = getGroupById(normalizedSnapshot, groupId);
  if (!group) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const targetSessionIdSet =
    sessionIds === undefined ? undefined : new Set(sessionIds.map((sessionId) => sessionId.trim()));
  const targetSessions =
    targetSessionIdSet === undefined
      ? group.snapshot.sessions
      : group.snapshot.sessions.filter((session) => targetSessionIdSet.has(session.sessionId));
  const hasChange = targetSessions.some((session) => session.isSleeping !== sleeping);
  if (!hasChange) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  if (!sleeping) {
    const snapshotWithWakeState = updateGroup(normalizedSnapshot, groupId, (targetGroup) => ({
      ...targetGroup,
      snapshot: wakeSessionsIntoFocusedPaneTabGroup(
        targetGroup.snapshot,
        targetSessions.map((session) => session.sessionId),
      ),
    }));
    return {
      changed: !areSnapshotsEqual(normalizedSnapshot, snapshotWithWakeState),
      snapshot: snapshotWithWakeState,
    };
  }

  const snapshotWithSleepState = updateGroup(normalizedSnapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...targetGroup.snapshot,
      sessions: targetGroup.snapshot.sessions.map((session) =>
        targetSessionIdSet === undefined || targetSessionIdSet.has(session.sessionId)
          ? {
              ...session,
              isPoppedOut: sleeping ? undefined : session.isPoppedOut,
              isSleeping: sleeping,
            }
          : session,
      ),
    }),
  }));
  const shouldSwitchGroups =
    sleeping &&
    normalizedSnapshot.activeGroupId === groupId &&
    getActiveSessionCount(getGroupById(snapshotWithSleepState, groupId)) === 0;
  const fallbackActiveGroupId = shouldSwitchGroups
    ? getFallbackActiveGroupId(snapshotWithSleepState, groupId)
    : snapshotWithSleepState.activeGroupId;
  const nextSnapshot =
    fallbackActiveGroupId === snapshotWithSleepState.activeGroupId
      ? snapshotWithSleepState
      : normalizeSimpleGroupedSessionWorkspaceSnapshot({
          ...snapshotWithSleepState,
          activeGroupId: fallbackActiveGroupId,
        });

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function renameSessionAliasInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  alias: string,
): WorkspaceMutationResult {
  const nextAlias = alias.trim();
  if (!nextAlias) {
    return { changed: false, snapshot };
  }

  return updateSession(snapshot, sessionId, (session) => ({
    ...session,
    alias: nextAlias,
  }));
}

export function setSessionTitleInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  title: string,
  options: { titleSource?: SessionTitleSource } = {},
): WorkspaceMutationResult {
  const nextTitle = title.trim();
  if (!nextTitle) {
    return { changed: false, snapshot };
  }

  return updateSession(snapshot, sessionId, (session) => ({
    ...session,
    title: nextTitle,
    titleSource: options.titleSource ?? "user",
  }));
}

export function setBrowserSessionUrlInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  url: string,
): WorkspaceMutationResult {
  const nextUrl = url.trim();
  if (!nextUrl) {
    return { changed: false, snapshot };
  }

  /**
   * CDXC:BrowserPanes 2026-05-03-03:41
   * Browser pane cards persist their current WKWebView location in the simple
   * workspace snapshot. Restoring from this field keeps app reopen on the page
   * the user last reached instead of the original create-pane URL.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "browser" || session.browser.url === nextUrl) {
      return session;
    }

    return {
      ...session,
      browser: {
        ...session.browser,
        url: nextUrl,
      },
    };
  });
}

export function setBrowserSessionFaviconDataUrlInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  faviconDataUrl: string | undefined,
): WorkspaceMutationResult {
  /**
   * CDXC:BrowserPanes 2026-05-03-11:28
   * Browser pane sidebar cards should use the loaded tab favicon when WebKit
   * discovers one. Persist the favicon data URL on the browser session so the
   * card keeps the tab identity across sidebar hydration and app reopen.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "browser") {
      return session;
    }
    const nextFaviconDataUrl = faviconDataUrl?.trim() || undefined;
    if (session.browser.faviconDataUrl === nextFaviconDataUrl) {
      return session;
    }

    return {
      ...session,
      browser: {
        ...session.browser,
        faviconDataUrl: nextFaviconDataUrl,
      },
    };
  });
}

export function setSessionLifecycleTimestampsInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  timestamps: {
    lastAccessedAt?: string;
    lastStartedAt?: string;
  },
): WorkspaceMutationResult {
  const nextLastAccessedAt = normalizeSessionLifecycleTimestamp(timestamps.lastAccessedAt);
  const nextLastStartedAt = normalizeSessionLifecycleTimestamp(timestamps.lastStartedAt);
  if (nextLastAccessedAt === undefined && nextLastStartedAt === undefined) {
    return { changed: false, snapshot: normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot) };
  }
  /**
   * CDXC:AutoSleep 2026-05-28-08:32:
   * Auto Sleep needs durable lifecycle timestamps for terminal and browser-class
   * sessions. Keep the mutation generic because browser panes sleep by access
   * time while agent terminals sleep by the newer of semantic activity and
   * start/wake time.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (
      session.lastAccessedAt === (nextLastAccessedAt ?? session.lastAccessedAt) &&
      session.lastStartedAt === (nextLastStartedAt ?? session.lastStartedAt)
    ) {
      return session;
    }

    return {
      ...session,
      lastAccessedAt: nextLastAccessedAt ?? session.lastAccessedAt,
      lastStartedAt: nextLastStartedAt ?? session.lastStartedAt,
    };
  });
}

export function setTerminalSessionAgentNameInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  agentName: string | undefined,
): WorkspaceMutationResult {
  const nextAgentName = agentName?.replace(/\s+/g, " ").trim() || undefined;
  /**
   * CDXC:AgentDetection 2026-04-26-21:31
   * ghostex native keeps project/session state in one persistence model:
   * localStorage-backed workspace snapshots. Agent identity belongs on the
   * terminal session record, not in a parallel per-session file, so restores
   * and sidebar cards read the same canonical session model.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "terminal" || session.agentName === nextAgentName) {
      return session;
    }

    return {
      ...session,
      agentName: nextAgentName,
    };
  });
}

export function setTerminalSessionAgentSessionMetadataInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  metadata: { agentSessionId?: string; agentSessionPath?: string },
): WorkspaceMutationResult {
  const nextAgentSessionId = metadata.agentSessionId?.trim() || undefined;
  const nextAgentSessionPath = metadata.agentSessionPath?.trim() || undefined;
  /**
   * CDXC:PiAgent 2026-05-08-09:42
   * Pi resumes and forks by its own jsonl session path/id, not by the visible
   * sidebar title. Persist those values on the terminal record whenever the Pi
   * extension reports them so sleeping, wake, app restart, and previous-session
   * restore all target the original Pi conversation.
   *
   * CDXC:CodexAgent 2026-05-11-07:35
   * Codex terminal-title UUIDs use the same metadata slot. The UUID is durable
   * restore identity and must not be promoted into the visible session title.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (
      session.kind !== "terminal" ||
      (session.agentSessionId === nextAgentSessionId &&
        session.agentSessionPath === nextAgentSessionPath)
    ) {
      return session;
    }

    return {
      ...session,
      agentSessionId: nextAgentSessionId,
      agentSessionPath: nextAgentSessionPath,
    };
  });
}

export function setTerminalSessionLastActivityAtInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  lastActivityAt: string | undefined,
): WorkspaceMutationResult {
  const nextLastActivityAt = normalizeTerminalSessionLastActivityAt(lastActivityAt);
  /**
   * CDXC:SessionLastActive 2026-05-17-02:45:
   * Last Active belongs on terminal session records because sleeping cards are
   * rendered without live terminal state after restart. Update only the durable
   * timestamp here; runtime activity still remains in terminalState.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "terminal" || session.lastActivityAt === nextLastActivityAt) {
      return session;
    }

    return {
      ...session,
      lastActivityAt: nextLastActivityAt,
    };
  });
}

export function setTerminalSessionPersistenceNameInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  sessionPersistenceName: string | undefined,
): WorkspaceMutationResult {
  const nextSessionPersistenceName = sessionPersistenceName?.trim() || undefined;
  /**
   * CDXC:SessionPersistence 2026-05-05-07:28
   * Provider reconnect identity must be stored separately from the sidebar
   * title. Titles can change as agent CLIs rename work, while restart restore
   * needs the last known persistence session name to attach before recreating.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (
      session.kind !== "terminal" ||
      session.sessionPersistenceName === nextSessionPersistenceName
    ) {
      return session;
    }

    return {
      ...session,
      sessionPersistenceName: nextSessionPersistenceName,
    };
  });
}

export function setTerminalSessionPersistenceProviderInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  sessionPersistenceProvider: TerminalSessionPersistenceProvider | undefined,
): WorkspaceMutationResult {
  /**
   * CDXC:SessionPersistence 2026-05-07-20:32
   * Provider-backed sessions reconnect by a provider/name pair. Store the
   * provider on the terminal record so changing Settings later cannot wake a
   * tmux session through zmx or a zmx session through zellij.
   */
  return updateSession(snapshot, sessionId, (session) => {
    if (
      session.kind !== "terminal" ||
      session.sessionPersistenceProvider === sessionPersistenceProvider
    ) {
      return session;
    }

    return {
      ...session,
      sessionPersistenceProvider,
    };
  });
}

export function setTerminalSessionEngineInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  terminalEngine: TerminalEngine,
): WorkspaceMutationResult {
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "terminal" || session.terminalEngine === terminalEngine) {
      return session;
    }

    return {
      ...session,
      terminalEngine,
    };
  });
}

export function setT3SessionMetadataInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  t3: T3SessionMetadata,
): WorkspaceMutationResult {
  return updateSession(snapshot, sessionId, (session) => {
    if (session.kind !== "t3") {
      return session;
    }

    return {
      ...session,
      t3: normalizeT3SessionMetadata(t3),
    };
  });
}

export function setVisibleCountInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  visibleCount: VisibleSessionCount,
): GroupedSessionWorkspaceSnapshot {
  const activeGroup = getActiveGroup(snapshot);
  if (!activeGroup) {
    return snapshot;
  }

  return updateGroup(snapshot, activeGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      fullscreenRestoreVisibleCount: undefined,
      visibleCount: clampSupportedVisibleCount(visibleCount),
    }),
  }));
}

export function toggleFullscreenSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
): GroupedSessionWorkspaceSnapshot {
  const activeGroup = getActiveGroup(snapshot);
  if (!activeGroup) {
    return snapshot;
  }

  const currentVisibleCount = clampSupportedVisibleCount(activeGroup.snapshot.visibleCount);
  const restoreVisibleCount =
    activeGroup.snapshot.fullscreenRestoreVisibleCount === undefined
      ? undefined
      : clampSupportedVisibleCount(activeGroup.snapshot.fullscreenRestoreVisibleCount);
  if (currentVisibleCount === 1 && restoreVisibleCount !== undefined) {
    return updateGroup(snapshot, activeGroup.groupId, (group) => ({
      ...group,
      snapshot: normalizeGroupSnapshot({
        ...group.snapshot,
        fullscreenRestoreVisibleCount: undefined,
        visibleCount: restoreVisibleCount,
      }),
    }));
  }

  return updateGroup(snapshot, activeGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      fullscreenRestoreVisibleCount:
        group.snapshot.visibleCount > 1
          ? clampSupportedVisibleCount(group.snapshot.visibleCount)
          : undefined,
      visibleCount: 1,
    }),
  }));
}

export function setViewModeInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  viewMode: TerminalViewMode,
): GroupedSessionWorkspaceSnapshot {
  const activeGroup = getActiveGroup(snapshot);
  if (!activeGroup) {
    return snapshot;
  }

  return updateGroup(snapshot, activeGroup.groupId, (group) => ({
    ...group,
    snapshot: {
      ...group.snapshot,
      viewMode,
    },
  }));
}

export function syncSessionOrderInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sessionIds: readonly string[],
): WorkspaceMutationResult {
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }

  const result = reorderGroupSessions(group.snapshot, sessionIds);
  if (!result.changed) {
    return { changed: false, snapshot };
  }

  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot(result.snapshot),
  }));

  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function syncSessionOrderAcrossSimpleWorkspaceGroups(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionIds: readonly string[],
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  let nextSnapshot = normalizedSnapshot;
  let changed = false;

  /*
   * CDXC:PinnedSessions 2026-05-28-20:27:
   * The combined project sidebar renders one project-level list over multiple
   * real workspace groups. Persist a combined pinned reorder by applying the
   * requested relative order inside each owning group while preserving group
   * membership and pane layout ownership.
   */
  for (const group of normalizedSnapshot.groups) {
    const groupSessionIds = group.snapshot.sessions.map((session) => session.sessionId);
    const groupSessionIdSet = new Set(groupSessionIds);
    const nextGroupSessionIds = sessionIds.filter((sessionId) => groupSessionIdSet.has(sessionId));
    if (
      nextGroupSessionIds.length !== groupSessionIds.length ||
      !haveSameStringSet(nextGroupSessionIds, groupSessionIds)
    ) {
      continue;
    }

    const result = syncSessionOrderInSimpleWorkspace(
      nextSnapshot,
      group.groupId,
      nextGroupSessionIds,
    );
    nextSnapshot = result.snapshot;
    changed = changed || result.changed;
  }

  return {
    changed,
    snapshot: nextSnapshot,
  };
}

export function swapVisibleSessionsInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sourceSessionId: string,
  targetSessionId: string,
): WorkspaceMutationResult {
  if (sourceSessionId === targetSessionId) {
    return { changed: false, snapshot };
  }

  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }

  const sourceVisibleIndex = group.snapshot.visibleSessionIds.indexOf(sourceSessionId);
  const targetVisibleIndex = group.snapshot.visibleSessionIds.indexOf(targetSessionId);
  if (sourceVisibleIndex < 0 || targetVisibleIndex < 0) {
    return { changed: false, snapshot };
  }

  const sourceSessionIndex = group.snapshot.sessions.findIndex(
    (session) => session.sessionId === sourceSessionId,
  );
  const targetSessionIndex = group.snapshot.sessions.findIndex(
    (session) => session.sessionId === targetSessionId,
  );
  if (sourceSessionIndex < 0 || targetSessionIndex < 0) {
    return { changed: false, snapshot };
  }

  const nextVisibleSessionIds = [...group.snapshot.visibleSessionIds];
  nextVisibleSessionIds[sourceVisibleIndex] = targetSessionId;
  nextVisibleSessionIds[targetVisibleIndex] = sourceSessionId;
  const nextFocusedSessionId = nextVisibleSessionIds.includes(group.snapshot.focusedSessionId ?? "")
    ? group.snapshot.focusedSessionId
    : nextVisibleSessionIds[0];

  const nextSessions = [...group.snapshot.sessions];
  nextSessions[sourceSessionIndex] = group.snapshot.sessions[targetSessionIndex]!;
  nextSessions[targetSessionIndex] = group.snapshot.sessions[sourceSessionIndex]!;

  /**
   * CDXC:NativePaneReorder 2026-05-03-06:38
   * Native pane drag-and-drop reorders the panes the user can currently see;
   * it must never change which hidden/background sessions are surfaced. Keep
   * visibleSessionIds as the source of truth for the displayed pane set, while
   * swapping the same two records in the stored session order so sidebar order
   * and native placement remain aligned without pulling hidden sessions into
   * the visible split.
   *
   * If focus points at a hidden/background session, pin focus to the reordered
   * visible set before normalization. Otherwise the normal focus-preservation
   * rule would add that hidden id back to visibleSessionIds and replace a pane.
   */
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...targetGroup.snapshot,
      focusedSessionId: nextFocusedSessionId,
      sessions: reindexSessionsInOrder(nextSessions),
      visibleSessionIds: nextVisibleSessionIds,
    }),
  }));

  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function selectPaneTabInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sessionId: string,
): WorkspaceMutationResult {
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }
  const groupSnapshotWithVirtualTabs = materializeAllSessionsInFocusedPaneTabGroup(group.snapshot);
  const isPaneTabSession = paneLayoutContainsSession(
    groupSnapshotWithVirtualTabs.paneLayout,
    sessionId,
  );
  const selectedSession = groupSnapshotWithVirtualTabs.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  /**
   * CDXC:SleepingPanePlaceholders 2026-06-13-01:44:
   * Selecting a sleeping native tab should only change the pane/tab owner in
   * paneLayout. Preserve the current awake runtime focus until the black
   * placeholder body sends the explicit wake request.
   */
  const nextFocusedSessionId =
    selectedSession?.isSleeping === true
      ? groupSnapshotWithVirtualTabs.focusedSessionId
      : sessionId;
  if (!groupSnapshotWithVirtualTabs.visibleSessionIds.includes(sessionId)) {
    const focusedTabSessionIds =
      groupSnapshotWithVirtualTabs.visibleCount === 1 &&
      groupSnapshotWithVirtualTabs.fullscreenRestoreVisibleCount !== undefined &&
      groupSnapshotWithVirtualTabs.focusedSessionId
        ? findPaneTabGroupSessionIds(
            groupSnapshotWithVirtualTabs.paneLayout,
            groupSnapshotWithVirtualTabs.focusedSessionId,
          )
        : undefined;
    if (focusedTabSessionIds?.includes(sessionId) === true) {
      /**
       * CDXC:SessionFocusMode 2026-05-26-22:47:
       * Focus mode stores only the focused tab in visibleSessionIds while native chrome still shows sibling tabs from the preserved pane tab group.
       * A native same-group tab click must therefore select and focus that sibling without exiting focus mode instead of being rejected as a hidden session.
       */
      return focusSessionInSimpleWorkspace(snapshot, sessionId);
    }
    if (!isPaneTabSession) {
      return { changed: false, snapshot };
    }
    const nextLayout = setActiveSessionInPaneLayout(
      groupSnapshotWithVirtualTabs.paneLayout,
      sessionId,
    );
    const nextVisibleSessionIds =
      selectedSession?.isSleeping === true
        ? groupSnapshotWithVirtualTabs.visibleSessionIds
        : [...groupSnapshotWithVirtualTabs.visibleSessionIds, sessionId];
    /**
     * CDXC:PaneTabs 2026-05-29-09:04:
     * Native tab chrome can expose virtual tab members that are not in legacy
     * visibleSessionIds. Selecting one must focus that paneLayout tab group and
     * promote the tab into visible ids so the subsequent native restore/focus
     * command has a real workspace target.
     */
    const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
      ...targetGroup,
      snapshot: normalizeGroupSnapshot({
        ...groupSnapshotWithVirtualTabs,
        focusedSessionId: nextFocusedSessionId,
        ...(nextLayout ? { paneLayout: nextLayout } : {}),
        visibleCount:
          selectedSession?.isSleeping === true
            ? groupSnapshotWithVirtualTabs.visibleCount
            : clampSupportedVisibleCount(
                Math.max(groupSnapshotWithVirtualTabs.visibleCount, nextVisibleSessionIds.length),
              ),
        visibleSessionIds: nextVisibleSessionIds,
      }),
    }));
    return {
      changed: !areSnapshotsEqual(snapshot, nextSnapshot),
      snapshot: nextSnapshot,
    };
  }
  const nextLayout = setActiveSessionInPaneLayout(
    groupSnapshotWithVirtualTabs.paneLayout,
    sessionId,
  );
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...groupSnapshotWithVirtualTabs,
      focusedSessionId: nextFocusedSessionId,
      ...(nextLayout ? { paneLayout: nextLayout } : {}),
    }),
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function moveSessionInPaneLayoutInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sourceSessionId: string,
  targetSessionId: string,
  placement: SessionPaneDropPlacement,
  options: MoveSessionInPaneLayoutOptions = {},
): WorkspaceMutationResult {
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }
  const groupSnapshotWithVirtualTabs = materializeAllSessionsInFocusedPaneTabGroup(group.snapshot);
  const paneSessionIds = getPaneSessionIds(groupSnapshotWithVirtualTabs);
  const paneSessionIdSet = new Set(paneSessionIds);
  if (!paneSessionIdSet.has(sourceSessionId) || !paneSessionIdSet.has(targetSessionId)) {
    return { changed: false, snapshot };
  }
  const currentLayout =
    groupSnapshotWithVirtualTabs.paneLayout ?? createPaneLayoutFromVisibleIds(paneSessionIds);
  if (!currentLayout) {
    return { changed: false, snapshot };
  }
  const isSameSessionSideDrop = sourceSessionId === targetSessionId && placement !== "center";
  const resolvedTargetSessionId = isSameSessionSideDrop
    ? getSamePaneSplitAnchorSessionId(
        currentLayout,
        sourceSessionId,
        placement === "right" || placement === "bottom",
      )
    : targetSessionId;
  if (!resolvedTargetSessionId || (sourceSessionId === targetSessionId && placement === "center")) {
    return { changed: false, snapshot };
  }
  const layoutWithoutSource = removeSessionFromPaneLayout(currentLayout, sourceSessionId);
  if (!layoutWithoutSource) {
    return { changed: false, snapshot };
  }

  const nextLayout =
    placement === "center"
      ? addSessionToPaneTabGroup(layoutWithoutSource, resolvedTargetSessionId, sourceSessionId)
      : insertExistingSessionBesidePane(
          layoutWithoutSource,
          resolvedTargetSessionId,
          sourceSessionId,
          placement === "left" || placement === "right" ? "horizontal" : "vertical",
          placement === "right" || placement === "bottom",
        );
  if (!nextLayout) {
    return { changed: false, snapshot };
  }
  const sourceSession = groupSnapshotWithVirtualTabs.sessions.find(
    (session) => session.sessionId === sourceSessionId,
  );
  const shouldWakeSourceSession = options.wakeSourceSession === true && sourceSession?.isSleeping === true;
  const nextSessions = shouldWakeSourceSession
    ? groupSnapshotWithVirtualTabs.sessions.map((session) =>
        session.sessionId === sourceSessionId
          ? { ...session, isPoppedOut: undefined, isSleeping: false }
          : session,
      )
    : groupSnapshotWithVirtualTabs.sessions;
  const nextVisibleSessionIds =
    placement === "center"
      ? paneSessionIds
      : moveVisibleSessionNearTarget(
          paneSessionIds,
          sourceSessionId,
          resolvedTargetSessionId,
          placement === "right" || placement === "bottom",
        );

  /**
   * CDXC:PaneTabs 2026-05-10-18:30
   * Native tab dragging mutates the persisted paneLayout directly: middle
   * drops create/update a tab group, while side drops move the dragged session
   * into a new split beside the target pane. visibleSessionIds keeps the set of
   * running sessions surfaced in the workspace; paneLayout owns grouping.
   *
   * CDXC:PaneTabs 2026-05-12-11:08
   * Dragging the active tab to an edge of its own multi-tab pane must split
   * that tab out. Resolve that same-session side drop to a remaining sibling
   * before removing the source tab; single-tab panes still have no valid
   * sibling anchor and remain a no-op.
   *
   * CDXC:PaneTabs 2026-05-16-09:43:
   * Dragging a sleeping tab into a center or edge pane drop is an explicit
   * restore action. Wake the dragged source in the same paneLayout mutation so
   * normalization keeps it focused, includes it in active visible ids, and lets
   * native sync show the restored split instead of parking the moved tab.
   *
   * CDXC:PaneTabs 2026-05-29-09:04:
   * Dragging a virtual sleeping/unmounted/missing-provider tab must first
   * materialize that displayed tab in paneLayout, then move/focus/wake it using
   * the same mutation as an already mounted tab. Native chrome may show tabs
   * that have no renderer yet, so drag handling cannot assume paneLayout already
   * contains every visible title-bar tab.
   */
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...groupSnapshotWithVirtualTabs,
      focusedSessionId: sourceSessionId,
      paneLayout: nextLayout,
      sessions: nextSessions,
      visibleCount: clampSupportedVisibleCount(Math.max(1, nextVisibleSessionIds.length)),
      visibleSessionIds: nextVisibleSessionIds,
    }),
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function reorderSessionInPaneTabGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  sourceSessionId: string,
  targetSessionId: string,
  position: SessionPaneTabReorderPosition,
): WorkspaceMutationResult {
  if (sourceSessionId === targetSessionId) {
    return { changed: false, snapshot };
  }
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }
  const groupSnapshotWithVirtualTabs = materializeAllSessionsInFocusedPaneTabGroup(group.snapshot);
  const currentLayout = groupSnapshotWithVirtualTabs.paneLayout;
  if (!currentLayout) {
    return { changed: false, snapshot };
  }
  const result = reorderSessionInPaneTabGroupNode(
    currentLayout,
    sourceSessionId,
    targetSessionId,
    position,
  );
  if (!result.didReorder) {
    return { changed: false, snapshot };
  }
  /**
   * CDXC:PaneTabs 2026-05-11-01:43
   * Native tab-bar drags reorder only the tab list inside the containing
   * paneLayout tab group. They must not split panes, swap visibleSessionIds, or
   * move the session into a different group.
   */
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...groupSnapshotWithVirtualTabs,
      paneLayout: result.node,
    }),
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function mergeAllTabsInPaneLayoutInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  activeSessionId: string,
): WorkspaceMutationResult {
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }
  const sessionIds = group.snapshot.sessions.map((session) => session.sessionId);
  const paneSessionIds = dedupeVisibleSessionIds([
    ...getPaneLayoutSessionIds(group.snapshot.paneLayout),
    ...group.snapshot.visibleSessionIds,
    ...sessionIds,
  ]).filter((sessionId) => sessionIds.includes(sessionId));
  if (paneSessionIds.length <= 1 || !paneSessionIds.includes(activeSessionId)) {
    return { changed: false, snapshot };
  }
  /**
   * CDXC:PaneTabs 2026-05-15-13:35
   * Merge All Tabs is scoped to a normal workspace group. Flatten only that
   * group's paneLayout into one tabs node, preserve tab order from the split
   * tree, and keep the clicked tab active. Command Terminal tabs are stored in
   * the project Commands panel state, so this workspace mutation cannot absorb
   * them.
   */
  const nextPaneLayout: SessionPaneLayoutNode = {
    activeSessionId,
    kind: "tabs",
    sessionIds: paneSessionIds,
  };
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...targetGroup.snapshot,
      focusedSessionId: activeSessionId,
      paneLayout: nextPaneLayout,
      visibleCount: clampSupportedVisibleCount(Math.max(1, paneSessionIds.length)),
      visibleSessionIds: paneSessionIds,
    }),
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function rotatePaneLayoutClockwiseInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
): WorkspaceMutationResult {
  const group = getGroupById(snapshot, groupId);
  if (!group) {
    return { changed: false, snapshot };
  }
  const sessionIds = group.snapshot.sessions.map((session) => session.sessionId);
  const paneLayoutSessionIds = dedupeVisibleSessionIds([
    ...group.snapshot.visibleSessionIds,
    ...getPaneLayoutSessionIds(group.snapshot.paneLayout),
  ]).filter((sessionId) => sessionIds.includes(sessionId));
  const currentLayout = normalizePaneLayout(
    group.snapshot.paneLayout ?? createPaneLayoutFromVisibleIds(group.snapshot.visibleSessionIds),
    sessionIds,
    paneLayoutSessionIds,
    group.snapshot.focusedSessionId,
  );
  if (!currentLayout || currentLayout.kind !== "split") {
    return { changed: false, snapshot };
  }
  const nextSnapshot = updateGroup(snapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...targetGroup.snapshot,
      paneLayout: rotatePaneLayoutNodeClockwise(currentLayout),
    }),
  }));
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function syncGroupOrderInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupIds: readonly string[],
): WorkspaceMutationResult {
  const groupById = new Map(snapshot.groups.map((group) => [group.groupId, group]));
  const orderedGroups = groupIds
    .map((groupId) => groupById.get(groupId))
    .filter((group): group is SessionGroupRecord => group !== undefined);
  for (const group of snapshot.groups) {
    if (!orderedGroups.some((candidate) => candidate.groupId === group.groupId)) {
      orderedGroups.push(group);
    }
  }

  const nextSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
    ...snapshot,
    groups: orderedGroups,
  });
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function moveSessionToGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  groupId: string,
  targetIndex?: number,
): WorkspaceMutationResult {
  const sourceGroup = getGroupForSession(snapshot, sessionId);
  const targetGroup = getGroupById(snapshot, groupId);
  if (!sourceGroup || !targetGroup) {
    return { changed: false, snapshot };
  }

  const sessionToMove = sourceGroup.snapshot.sessions.find(
    (session) => session.sessionId === sessionId,
  );
  if (!sessionToMove) {
    return { changed: false, snapshot };
  }

  if (sourceGroup.groupId === groupId) {
    const nextSessions = sourceGroup.snapshot.sessions.filter(
      (session) => session.sessionId !== sessionId,
    );
    const insertIndex =
      typeof targetIndex === "number"
        ? Math.max(0, Math.min(targetIndex, nextSessions.length))
        : nextSessions.length;
    nextSessions.splice(insertIndex, 0, sessionToMove);
    const reorderedSessions = nextSessions.map((session, index) => ({
      ...session,
      slotIndex: index,
    }));

    const nextSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
      ...updateGroup(snapshot, groupId, (group) => ({
        ...group,
        snapshot: normalizeGroupSnapshot({
          ...group.snapshot,
          focusedSessionId: sessionId,
          sessions: reorderedSessions,
          visibleSessionIds: getNextVisibleIdsForFocusedSession(
            reorderedSessions,
            group.snapshot.visibleCount,
            sessionId,
            group.snapshot.visibleSessionIds,
            group.snapshot.focusedSessionId,
          ),
        }),
      })),
      activeGroupId: groupId,
    });

    return {
      changed: !areSnapshotsEqual(snapshot, nextSnapshot),
      snapshot: nextSnapshot,
    };
  }

  const strippedSnapshot = updateGroup(
    updateGroup(snapshot, sourceGroup.groupId, (group) => ({
      ...group,
      snapshot: normalizeGroupSnapshot({
        ...group.snapshot,
        sessions: group.snapshot.sessions.filter((session) => session.sessionId !== sessionId),
        visibleSessionIds: group.snapshot.visibleSessionIds.filter((id) => id !== sessionId),
        focusedSessionId:
          group.snapshot.focusedSessionId === sessionId
            ? undefined
            : group.snapshot.focusedSessionId,
      }),
    })),
    groupId,
    (group) => {
      const nextSessions = [...group.snapshot.sessions];
      const insertIndex =
        typeof targetIndex === "number"
          ? Math.max(0, Math.min(targetIndex, nextSessions.length))
          : nextSessions.length;
      nextSessions.splice(insertIndex, 0, sessionToMove);
      const reorderedSessions = nextSessions.map((session, index) => ({
        ...session,
        slotIndex: index,
      }));
      return {
        ...group,
        snapshot: normalizeGroupSnapshot({
          ...group.snapshot,
          focusedSessionId: sessionId,
          sessions: reorderedSessions,
          visibleSessionIds: getNextVisibleIdsForFocusedSession(
            reorderedSessions,
            group.snapshot.visibleCount,
            sessionId,
            group.snapshot.visibleSessionIds,
            group.snapshot.focusedSessionId,
          ),
        }),
      };
    },
  );

  const nextSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
    ...strippedSnapshot,
    activeGroupId: groupId,
  });
  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

export function createGroupFromSessionInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): CreateGroupResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const resolvedSession = resolveSessionReference(normalizedSnapshot, sessionId);
  const sourceGroup = resolvedSession?.group;
  if (!sourceGroup || normalizedSnapshot.groups.length >= MAX_GROUP_COUNT) {
    return { changed: false, snapshot: normalizedSnapshot };
  }
  const resolvedSessionId = resolvedSession.session.sessionId;

  const session = sourceGroup.snapshot.sessions.find(
    (candidate) => candidate.sessionId === resolvedSessionId,
  );
  if (!session) {
    return { changed: false, snapshot: normalizedSnapshot };
  }
  const canonicalSessionId = session.sessionId === sessionId ? session.sessionId : sessionId;
  const sessionForNewGroup =
    session.sessionId === canonicalSessionId ? session : { ...session, sessionId: canonicalSessionId };

  const nextGroupNumber = Math.max(
    normalizedSnapshot.nextGroupNumber,
    getNextGroupNumber(normalizedSnapshot.groups),
  );
  const nextGroupId = `group-${nextGroupNumber}`;
  const nextGroup: SessionGroupRecord = {
    groupId: nextGroupId,
    snapshot: normalizeGroupSnapshot({
      ...createDefaultSessionGridSnapshot(),
      focusedSessionId: canonicalSessionId,
      sessions: [sessionForNewGroup],
      visibleCount: 1,
      visibleSessionIds: [canonicalSessionId],
    }),
    title: `Group ${nextGroupNumber}`,
  };
  const snapshotWithoutSession = updateGroup(normalizedSnapshot, sourceGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      sessions: group.snapshot.sessions.filter(
        (candidate) => candidate.sessionId !== resolvedSessionId,
      ),
      visibleSessionIds: group.snapshot.visibleSessionIds.filter((id) => id !== resolvedSessionId),
      focusedSessionId:
        group.snapshot.focusedSessionId === resolvedSessionId
          ? undefined
          : group.snapshot.focusedSessionId,
    }),
  }));
  const nextSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
    ...snapshotWithoutSession,
    activeGroupId: nextGroupId,
    groups: [...snapshotWithoutSession.groups, nextGroup],
    nextGroupNumber: nextGroupNumber + 1,
  });

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    groupId: nextGroupId,
    snapshot: nextSnapshot,
  };
}

export function createGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
): CreateGroupResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  if (normalizedSnapshot.groups.length >= MAX_GROUP_COUNT) {
    return { changed: false, snapshot: normalizedSnapshot };
  }

  const nextGroupNumber = Math.max(
    normalizedSnapshot.nextGroupNumber,
    getNextGroupNumber(normalizedSnapshot.groups),
  );
  const nextGroupId = `group-${nextGroupNumber}`;
  const nextSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot({
    ...normalizedSnapshot,
    activeGroupId: nextGroupId,
    groups: [
      ...normalizedSnapshot.groups,
      createEmptyGroup(nextGroupId, `Group ${nextGroupNumber}`),
    ],
    nextGroupNumber: nextGroupNumber + 1,
  });

  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    groupId: nextGroupId,
    snapshot: nextSnapshot,
  };
}

function normalizeGroup(group: SessionGroupRecord, index: number): SessionGroupRecord {
  return {
    groupId: group.groupId?.trim() || `group-${index + 1}`,
    snapshot: normalizeGroupSnapshot(group.snapshot),
    title: group.title?.trim() || (index === 0 ? DEFAULT_MAIN_GROUP_TITLE : `Group ${index + 1}`),
  };
}

function prepareGroupForDisplayIdNormalization(
  group: SessionGroupRecord,
  index: number,
): SessionGroupRecord {
  return {
    groupId: group.groupId?.trim() || `group-${index + 1}`,
    snapshot: {
      ...createDefaultSessionGridSnapshot(),
      ...group.snapshot,
      /**
       * CDXC:BrowserPanes 2026-05-02-12:04
       * Browser panes are embedded WKWebView workspace sessions, not transient
       * external browser overlays. Keep browser records during workspace
       * normalization so sidebar cards, visible ids, and native split layout
       * all use the same persistent session list as terminal and T3 panes.
       */
      sessions: group.snapshot.sessions,
    },
    title: group.title?.trim() || (index === 0 ? DEFAULT_MAIN_GROUP_TITLE : `Group ${index + 1}`),
  };
}

function normalizeGroupSnapshot(
  snapshot: SessionGroupRecord["snapshot"],
): SessionGroupRecord["snapshot"] {
  const sessionIdByLegacyId = new Map<string, string>();
  const sessions = getOrderedSessions({
    ...createDefaultSessionGridSnapshot(),
    ...snapshot,
    sessions: snapshot.sessions,
  }).map((session, index) => {
    const position = getSlotPosition(index);
    const nextSession = normalizeSessionRecord(session);
    sessionIdByLegacyId.set(session.sessionId, nextSession.sessionId);
    return {
      ...nextSession,
      column: position.column,
      row: position.row,
      slotIndex: index,
    };
  });
  const awakeSessions = getAwakeSessions(sessions);
  const focusedSessionId = awakeSessions.some(
    (session) => session.sessionId === sessionIdByLegacyId.get(snapshot.focusedSessionId ?? ""),
  )
    ? sessionIdByLegacyId.get(snapshot.focusedSessionId ?? "")
    : awakeSessions[0]?.sessionId;
  const visibleCount =
    awakeSessions.length === 0 ? 1 : clampSupportedVisibleCount(snapshot.visibleCount);
  const normalizedVisibleSessionIds = snapshot.visibleSessionIds.map(
    (sessionId) => sessionIdByLegacyId.get(sessionId) ?? sessionId,
  );
  const visibleSessionIds = getNormalizedVisibleIds(
    awakeSessions,
    visibleCount,
    focusedSessionId,
    normalizedVisibleSessionIds,
  );
  const paneLayoutSessionIds = sessions.map((session) => session.sessionId);
  const snapshotPaneLayoutSessionIds = getPaneLayoutSessionIds(snapshot.paneLayout).map(
    (sessionId) => sessionIdByLegacyId.get(sessionId) ?? sessionId,
  );
  const normalizedPaneLayoutSessionIds = [
    ...normalizedVisibleSessionIds,
    ...snapshotPaneLayoutSessionIds,
  ].filter(
    (sessionId, index, sessionIds) =>
      paneLayoutSessionIds.includes(sessionId) && sessionIds.indexOf(sessionId) === index,
  );
  /**
   * CDXC:PaneTabs 2026-05-11-01:31
   * Sleeping a session must park its native surface without deleting its
   * persisted pane/tab position. Normalize paneLayout against the visible set
   * plus the sessions already encoded in the stored pane tree, while
   * visibleSessionIds still tracks the currently awake subset. Interactive
   * wake can intentionally rewrite paneLayout without the normalizer adding
   * every background session back as a pane.
   */
  const paneLayout = normalizePaneLayout(
    snapshot.paneLayout,
    paneLayoutSessionIds,
    normalizedPaneLayoutSessionIds,
    focusedSessionId,
  );

  return {
    focusedSessionId,
    fullscreenRestoreVisibleCount:
      visibleCount === 1 && snapshot.fullscreenRestoreVisibleCount !== undefined
        ? clampSupportedVisibleCount(snapshot.fullscreenRestoreVisibleCount)
        : undefined,
    ...(paneLayout ? { paneLayout } : {}),
    sessions,
    viewMode: snapshot.viewMode ?? "grid",
    visibleCount,
    visibleSessionIds,
  };
}

function wakeSessionsIntoFocusedPaneTabGroup(
  snapshot: SessionGroupRecord["snapshot"],
  sessionIds: readonly string[],
): SessionGroupRecord["snapshot"] {
  const wakeSessionIdSet = new Set(sessionIds.map((sessionId) => sessionId.trim()));
  const sessions = snapshot.sessions.map((session) =>
    wakeSessionIdSet.has(session.sessionId)
      ? { ...session, isPoppedOut: undefined, isSleeping: false }
      : session,
  );
  const restoredSessionIds = snapshot.sessions
    .filter((session) => wakeSessionIdSet.has(session.sessionId) && session.isSleeping === true)
    .map((session) => session.sessionId);
  if (restoredSessionIds.length === 0) {
    return normalizeGroupSnapshot({ ...snapshot, sessions });
  }

  let currentFocusedSessionId = snapshot.focusedSessionId;
  let currentPaneLayout = snapshot.paneLayout;
  let currentVisibleSessionIds = snapshot.visibleSessionIds;
  let currentVisibleCount = snapshot.visibleCount;

  for (const restoredSessionId of restoredSessionIds) {
    const nextVisibleSessionIds = getNextVisibleIdsForRestoredSleepingSession(
      getAwakeSessions(sessions),
      currentVisibleCount,
      restoredSessionId,
      currentVisibleSessionIds,
      currentFocusedSessionId,
    );
    const nextPaneLayout = getNextPaneLayoutForRestoredSleepingSession(
      currentPaneLayout,
      currentVisibleSessionIds,
      nextVisibleSessionIds,
      restoredSessionId,
      currentFocusedSessionId,
    );

    currentFocusedSessionId = restoredSessionId;
    currentPaneLayout = nextPaneLayout;
    currentVisibleSessionIds = nextVisibleSessionIds;
    currentVisibleCount = clampSupportedVisibleCount(Math.max(1, nextVisibleSessionIds.length));
  }

  /**
   * CDXC:SessionSleep 2026-05-18-15:47:
   * Any wake path can run while Code/Git owns the visible workarea. Reattach
   * restored sessions to the focused pane tab group before marking them active
   * so the later Agents-mode native layout sync does not append each awake but
   * paneLayout-missing session as a separate split pane.
   */
  return normalizeGroupSnapshot({
    ...snapshot,
    focusedSessionId: currentFocusedSessionId,
    ...(currentPaneLayout ? { paneLayout: currentPaneLayout } : {}),
    sessions,
    visibleCount: currentVisibleCount,
    visibleSessionIds: currentVisibleSessionIds,
  });
}

function getVisibleSessionPlacementResult(
  sessions: readonly SessionRecord[],
  currentVisibleCount: VisibleSessionCount,
  currentVisibleSessionIds: readonly string[],
  nextSessionId: string,
  placement: VisibleSessionPlacement,
): { replacedSessionId?: string; visibleCount: VisibleSessionCount; visibleSessionIds: string[] } {
  /**
   * CDXC:NativeSplits 2026-05-10-18:30
   * Native split buttons and Cmd+D/Cmd+Shift+D create a real new workspace
   * session and surface it in a deterministic pane slot. Placement is computed
   * before focus changes so title-bar button clicks cannot accidentally replace
   * the previously focused pane instead of the pane the user clicked.
   */
  const stableVisibleSessionIds = getPlacementStableVisibleIds(
    sessions,
    currentVisibleSessionIds,
  );
  switch (placement.kind) {
    case "appendFullWidth":
      return insertVisibleSessionAfterTarget(stableVisibleSessionIds, nextSessionId, undefined);
    case "appendToTabGroup":
      return insertVisibleSessionAfterTarget(
        stableVisibleSessionIds,
        nextSessionId,
        placement.targetSessionId,
      );
    case "insertAfter":
      return insertVisibleSessionAfterTarget(
        stableVisibleSessionIds,
        nextSessionId,
        placement.targetSessionId,
      );
    case "replace":
      return replaceVisibleSessionTarget(
        currentVisibleCount,
        stableVisibleSessionIds,
        nextSessionId,
        placement.targetSessionId,
      );
    case "replaceNonFocused":
      return replaceNonFocusedVisibleSession(
        currentVisibleCount,
        sessions,
        stableVisibleSessionIds,
        nextSessionId,
        placement.preserveSessionId,
      );
  }
}

function getNextPaneLayoutForCreatedSession(
  currentLayout: SessionPaneLayoutNode | undefined,
  currentVisibleSessionIds: readonly string[],
  nextVisibleSessionIds: readonly string[],
  nextSessionId: string,
  currentFocusedSessionId: string | undefined,
  placement: VisibleSessionPlacement | undefined,
  replacedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  /**
   * CDXC:NativeSplits 2026-05-10-18:30
   * Session creation owns pane-layout mutation. Split commands insert the new
   * leaf into the targeted split tree, while replacement commands swap the leaf
   * that was intentionally replaced. This keeps native geometry persistent
   * across app restart instead of rebuilding from visible-session counts.
   *
   * CDXC:PaneTabs 2026-05-11-18:13
   * Seed creation from the persisted paneLayout inventory, not only the awake
   * visible ids. Sleeping/offscreen tab members are still part of the user's
   * tab group and must survive split, fork, restore, and debug creation.
   */
  const currentPaneLayoutSessionIds = getPaneLayoutSessionIds(currentLayout);
  const seededLayoutSessionIds = dedupeVisibleSessionIds([
    ...currentVisibleSessionIds,
    ...currentPaneLayoutSessionIds,
  ]);
  const normalizedCurrentLayout = normalizePaneLayout(
    currentLayout,
    seededLayoutSessionIds,
    seededLayoutSessionIds,
  );
  const seededLayout =
    normalizedCurrentLayout ??
    (placement?.kind === "insertAfter" || placement?.kind === "appendFullWidth"
      ? createPaneLayoutFromVisibleIds(currentVisibleSessionIds)
      : createInitialPaneLayoutForCreatedSession(
          currentVisibleSessionIds,
          currentFocusedSessionId ?? currentVisibleSessionIds.at(-1) ?? nextSessionId,
        ));
  if (!seededLayout) {
    return createInitialPaneLayoutForCreatedSession(nextVisibleSessionIds, nextSessionId);
  }

  if (!placement) {
    /**
     * CDXC:PaneTabs 2026-05-11-18:13:
     * Fork, restore, debug, and legacy creation paths may not pass an explicit
     * placement. They must still preserve the existing split/tab tree instead
     * of rebuilding paneLayout from visibleSessionIds, which flattens every tab
     * group into single-tab split panes.
     *
     * CDXC:SplitIntent 2026-05-19-08:29:
     * Missing placement is not split intent. Default creation paths add the new
     * session to the focused pane's tab group; only explicit split placements
     * such as insertAfter or appendFullWidth may create a new split leaf.
     */
    const targetSessionId =
      currentFocusedSessionId && currentVisibleSessionIds.includes(currentFocusedSessionId)
        ? currentFocusedSessionId
        : currentVisibleSessionIds[0];
    const preservedLayoutSessionIds = dedupeVisibleSessionIds([
      ...nextVisibleSessionIds,
      ...currentPaneLayoutSessionIds,
    ]).filter((sessionId) => sessionId !== nextSessionId);
    const preservedLayout =
      normalizePaneLayout(
        currentLayout,
        preservedLayoutSessionIds,
        preservedLayoutSessionIds,
        nextSessionId,
      ) ?? seededLayout;
    const nextPaneLayoutSessionIds = dedupeVisibleSessionIds([
      ...nextVisibleSessionIds,
      ...getPaneLayoutSessionIds(preservedLayout),
      nextSessionId,
    ]);
    const tabbedLayout = targetSessionId
      ? addSessionToPaneTabGroup(preservedLayout, targetSessionId, nextSessionId)
      : undefined;
    return normalizePaneLayout(
      tabbedLayout ?? createInitialPaneLayoutForCreatedSession(nextVisibleSessionIds, nextSessionId),
      nextPaneLayoutSessionIds,
      nextPaneLayoutSessionIds,
      nextSessionId,
    );
  }

  if (placement.kind === "insertAfter") {
    const targetSessionId = placement.targetSessionId ?? currentVisibleSessionIds.at(-1);
    const splitDirection = placement.splitDirection ?? "horizontal";
    const insertedLayout = targetSessionId
      ? insertSessionIntoPaneLayout(seededLayout, targetSessionId, nextSessionId, splitDirection)
      : undefined;
    return normalizeCreatedPaneLayout(
      insertedLayout ?? appendSessionToPaneLayout(seededLayout, nextSessionId, splitDirection),
      nextVisibleSessionIds,
      nextSessionId,
    );
  }

  if (placement.kind === "appendFullWidth") {
    /**
     * CDXC:WorkspacePanes 2026-05-11-03:20
     * The Settings-row secondary terminal button creates a normal terminal in
     * the active project, but it must span the full workarea width. Wrap the
     * existing pane tree in an 85/15 vertical split and append the new leaf as
     * a 15%-height bottom row instead of splitting the currently focused pane.
     */
    return normalizeCreatedPaneLayout(
      appendFullWidthSessionToPaneLayout(seededLayout, nextSessionId),
      nextVisibleSessionIds,
      nextSessionId,
    );
  }

  if (placement.kind === "appendToTabGroup") {
    /**
     * CDXC:PaneTabs 2026-05-11-16:16
     * Title-bar New Terminal and Open Browser create a new tab in the clicked
     * pane. Do not replace the target session: keeping the old and new sessions
     * in one paneLayout tabs node prevents native layout sync from appending the
     * still-awake old session as a separate split pane.
     *
     * CDXC:PaneTabs 2026-05-11-17:04
     * Tab groups and visible panes no longer use a fixed pane cap. Keep the
     * full tab inventory in paneLayout and visibleSessionIds so native sync
     * never has to recover a trimmed awake session as a separate split.
     */
    const tabbedLayout = addSessionToPaneTabGroup(
      seededLayout,
      placement.targetSessionId,
      nextSessionId,
      placement.position,
    );
    return normalizeCreatedPaneLayout(
      tabbedLayout ?? createPaneLayoutFromVisibleIds(nextVisibleSessionIds),
      nextVisibleSessionIds,
      nextSessionId,
    );
  }

  if (placement.kind === "replace" || placement.kind === "replaceNonFocused") {
    const targetSessionId =
      placement.kind === "replace" ? placement.targetSessionId : replacedSessionId;
    const replacedLayout = targetSessionId
      ? replaceSessionInPaneLayout(seededLayout, targetSessionId, nextSessionId)
      : undefined;
    return normalizeCreatedPaneLayout(
      replacedLayout ?? createPaneLayoutFromVisibleIds(nextVisibleSessionIds),
      nextVisibleSessionIds,
      nextSessionId,
    );
  }
}

function normalizeCreatedPaneLayout(
  layout: SessionPaneLayoutNode | undefined,
  nextVisibleSessionIds: readonly string[],
  nextSessionId: string,
): SessionPaneLayoutNode | undefined {
  /**
   * CDXC:PaneTabs 2026-05-11-18:48
   * Every explicit pane-creation placement must normalize against the newly
   * visible ids plus the full paneLayout inventory. Otherwise split, full-row,
   * tab, and replace actions can prune sleeping/offscreen tab members and make
   * native sync rebuild the user's grouped tabs as separate panes.
   */
  const paneLayoutSessionIds = dedupeVisibleSessionIds([
    ...nextVisibleSessionIds,
    ...getPaneLayoutSessionIds(layout),
  ]);
  return normalizePaneLayout(
    layout,
    paneLayoutSessionIds,
    paneLayoutSessionIds,
    nextSessionId,
  );
}

function createInitialPaneLayoutForCreatedSession(
  nextVisibleSessionIds: readonly string[],
  nextSessionId: string,
): SessionPaneLayoutNode | undefined {
  const sessionIds = dedupeVisibleSessionIds(nextVisibleSessionIds);
  if (sessionIds.length === 0) {
    return undefined;
  }
  if (sessionIds.length === 1) {
    return { kind: "leaf", sessionId: sessionIds[0]! };
  }
  const activeSessionId = sessionIds.includes(nextSessionId) ? nextSessionId : sessionIds.at(-1);
  return { activeSessionId, kind: "tabs", sessionIds };
}

function createPaneLayoutFromVisibleIds(
  visibleSessionIds: readonly string[],
): SessionPaneLayoutNode | undefined {
  const leafs = dedupeVisibleSessionIds(visibleSessionIds).map((sessionId) => ({
    kind: "leaf" as const,
    sessionId,
  }));
  if (leafs.length === 0) {
    return undefined;
  }
  if (leafs.length === 1) {
    return leafs[0];
  }
  return { children: leafs, direction: "horizontal", kind: "split" };
}

function insertSessionIntoPaneLayout(
  layout: SessionPaneLayoutNode,
  targetSessionId: string,
  nextSessionId: string,
  direction: SessionPaneSplitDirection,
): SessionPaneLayoutNode | undefined {
  const result = insertSessionIntoPaneLayoutNode(layout, targetSessionId, nextSessionId, direction);
  return result.didInsert ? result.node : undefined;
}

function insertSessionIntoPaneLayoutNode(
  node: SessionPaneLayoutNode,
  targetSessionId: string,
  nextSessionId: string,
  direction: SessionPaneSplitDirection,
): { didInsert: boolean; node: SessionPaneLayoutNode } {
  if (node.kind === "leaf") {
    if (node.sessionId !== targetSessionId) {
      return { didInsert: false, node };
    }
    return {
      didInsert: true,
      node: {
        children: [node, { kind: "leaf", sessionId: nextSessionId }],
        direction,
        kind: "split",
      },
    };
  }

  if (node.kind === "tabs") {
    if (!node.sessionIds.includes(targetSessionId)) {
      return { didInsert: false, node };
    }
    return {
      didInsert: true,
      node: {
        children: [node, { kind: "leaf", sessionId: nextSessionId }],
        direction,
        kind: "split",
      },
    };
  }

  const children: SessionPaneLayoutNode[] = [];
  let didInsert = false;
  for (const child of node.children) {
    if (!didInsert && paneLayoutContainsSession(child, targetSessionId)) {
      if (node.direction === direction) {
        children.push(child, { kind: "leaf", sessionId: nextSessionId });
        didInsert = true;
      } else {
        const result = insertSessionIntoPaneLayoutNode(
          child,
          targetSessionId,
          nextSessionId,
          direction,
        );
        children.push(result.node);
        didInsert = result.didInsert;
      }
    } else {
      children.push(child);
    }
  }
  return {
    didInsert,
    node: flattenPaneLayoutSplit({ ...node, children }),
  };
}

function appendSessionToPaneLayout(
  layout: SessionPaneLayoutNode,
  nextSessionId: string,
  direction: SessionPaneSplitDirection,
): SessionPaneLayoutNode {
  if (layout.kind === "split" && layout.direction === direction) {
    return flattenPaneLayoutSplit({
      ...layout,
      children: [...layout.children, { kind: "leaf", sessionId: nextSessionId }],
    });
  }
  return {
    children: [layout, { kind: "leaf", sessionId: nextSessionId }],
    direction,
    kind: "split",
  };
}

function appendFullWidthSessionToPaneLayout(
  layout: SessionPaneLayoutNode,
  nextSessionId: string,
): SessionPaneLayoutNode {
  return {
    children: [layout, { kind: "leaf", sessionId: nextSessionId }],
    direction: "vertical",
    kind: "split",
    ratio: 0.85,
  };
}

function replaceSessionInPaneLayout(
  layout: SessionPaneLayoutNode,
  targetSessionId: string,
  nextSessionId: string,
): SessionPaneLayoutNode | undefined {
  const result = replaceSessionInPaneLayoutNode(layout, targetSessionId, nextSessionId);
  return result.didReplace ? result.node : undefined;
}

function replaceSessionInPaneLayoutNode(
  node: SessionPaneLayoutNode,
  targetSessionId: string,
  nextSessionId: string,
): { didReplace: boolean; node: SessionPaneLayoutNode } {
  if (node.kind === "leaf") {
    return node.sessionId === targetSessionId
      ? { didReplace: true, node: { kind: "leaf", sessionId: nextSessionId } }
      : { didReplace: false, node };
  }
  if (node.kind === "tabs") {
    if (!node.sessionIds.includes(targetSessionId)) {
      return { didReplace: false, node };
    }
    const sessionIds = node.sessionIds.map((sessionId) =>
      sessionId === targetSessionId ? nextSessionId : sessionId,
    );
    return {
      didReplace: true,
      node: {
        ...node,
        activeSessionId:
          node.activeSessionId === targetSessionId ? nextSessionId : node.activeSessionId,
        sessionIds,
      },
    };
  }
  let didReplace = false;
  const children = node.children.map((child) => {
    if (didReplace) {
      return child;
    }
    const result = replaceSessionInPaneLayoutNode(child, targetSessionId, nextSessionId);
    didReplace = result.didReplace;
    return result.node;
  });
  return {
    didReplace,
    node: flattenPaneLayoutSplit({ ...node, children }),
  };
}

function setActiveSessionInPaneLayout(
  layout: SessionPaneLayoutNode | undefined,
  sessionId: string,
): SessionPaneLayoutNode | undefined {
  if (!layout) {
    return undefined;
  }
  if (layout.kind === "tabs") {
    return layout.sessionIds.includes(sessionId)
      ? { ...layout, activeSessionId: sessionId }
      : layout;
  }
  if (layout.kind === "split") {
    return {
      ...layout,
      children: layout.children.map((child) => setActiveSessionInPaneLayout(child, sessionId) ?? child),
    };
  }
  return layout;
}

function removeSessionFromPaneLayout(
  layout: SessionPaneLayoutNode,
  sessionId: string,
  replacementSessionId?: string,
): SessionPaneLayoutNode | undefined {
  if (layout.kind === "leaf") {
    return layout.sessionId === sessionId ? undefined : layout;
  }
  if (layout.kind === "tabs") {
    const sessionIds = layout.sessionIds.filter((id) => id !== sessionId);
    if (sessionIds.length === 0) {
      return undefined;
    }
    if (sessionIds.length === 1) {
      return { kind: "leaf", sessionId: sessionIds[0]! };
    }
    const fallbackActiveSessionId =
      replacementSessionId && sessionIds.includes(replacementSessionId)
        ? replacementSessionId
        : sessionIds[0];
    return {
      ...layout,
      activeSessionId:
        layout.activeSessionId && sessionIds.includes(layout.activeSessionId)
          ? layout.activeSessionId
          : fallbackActiveSessionId,
      sessionIds,
    };
  }
  const children = layout.children
    .map((child) => removeSessionFromPaneLayout(child, sessionId, replacementSessionId))
    .filter((child): child is SessionPaneLayoutNode => child !== undefined);
  if (children.length === 0) {
    return undefined;
  }
  if (children.length === 1) {
    return children[0];
  }
  return flattenPaneLayoutSplit({ ...layout, children });
}

function getClosingPaneTabReplacementSessionId(
  layout: SessionPaneLayoutNode | undefined,
  sessionId: string,
): string | undefined {
  if (!layout) {
    return undefined;
  }
  if (layout.kind === "leaf") {
    return undefined;
  }
  if (layout.kind === "tabs") {
    return getAdjacentPaneTabSessionId(layout.sessionIds, sessionId);
  }
  for (const child of layout.children) {
    const replacementSessionId = getClosingPaneTabReplacementSessionId(child, sessionId);
    if (replacementSessionId) {
      return replacementSessionId;
    }
  }
  return undefined;
}

function getAdjacentPaneTabSessionId(
  sessionIds: readonly string[],
  sessionId: string,
): string | undefined {
  const index = sessionIds.indexOf(sessionId);
  if (index < 0 || sessionIds.length <= 1) {
    return undefined;
  }
  /*
   * CDXC:PaneTabs 2026-06-06-04:32:
   * Closing a selected tab follows browser-style selection: prefer the tab immediately to the right, then use the left sibling only when the closed tab was rightmost.
   */
  return sessionIds[index + 1] ?? sessionIds[index - 1];
}

function isActivePaneLayoutSession(
  layout: SessionPaneLayoutNode | undefined,
  sessionId: string,
): boolean {
  if (!layout) {
    return false;
  }
  if (layout.kind === "leaf") {
    return layout.sessionId === sessionId;
  }
  if (layout.kind === "tabs") {
    return (layout.activeSessionId ?? layout.sessionIds[0]) === sessionId;
  }
  return layout.children.some((child) => isActivePaneLayoutSession(child, sessionId));
}

function getVisibleSessionIdsAfterSessionClose(
  visibleSessionIds: readonly string[],
  sessionId: string,
  replacementSessionId: string | undefined,
): string[] {
  const nextVisibleSessionIds: string[] = [];
  for (const visibleSessionId of visibleSessionIds) {
    if (visibleSessionId === sessionId) {
      if (replacementSessionId && !nextVisibleSessionIds.includes(replacementSessionId)) {
        nextVisibleSessionIds.push(replacementSessionId);
      }
      continue;
    }
    if (visibleSessionId === replacementSessionId && visibleSessionIds.includes(sessionId)) {
      continue;
    }
    nextVisibleSessionIds.push(visibleSessionId);
  }
  return nextVisibleSessionIds;
}

function countPaneLayoutOwnerSlots(layout: SessionPaneLayoutNode): number {
  if (layout.kind === "split") {
    return layout.children.reduce((count, child) => count + countPaneLayoutOwnerSlots(child), 0);
  }
  return 1;
}

function addSessionToPaneTabGroup(
  layout: SessionPaneLayoutNode,
  targetSessionId: string,
  sourceSessionId: string,
  position: SessionPaneTabInsertPosition = "append",
): SessionPaneLayoutNode | undefined {
  const result = addSessionToPaneTabGroupNode(layout, targetSessionId, sourceSessionId, position);
  return result.didAdd ? result.node : undefined;
}

function addSessionToPaneTabGroupNode(
  node: SessionPaneLayoutNode,
  targetSessionId: string,
  sourceSessionId: string,
  position: SessionPaneTabInsertPosition,
): { didAdd: boolean; node: SessionPaneLayoutNode } {
  if (node.kind === "leaf") {
    return node.sessionId === targetSessionId
      ? {
          didAdd: true,
          node: {
            activeSessionId: sourceSessionId,
            kind: "tabs",
            sessionIds: [targetSessionId, sourceSessionId],
          },
        }
      : { didAdd: false, node };
  }
  if (node.kind === "tabs") {
    if (!node.sessionIds.includes(targetSessionId)) {
      return { didAdd: false, node };
    }
    const nextSessionIds =
      position === "append"
        ? [...node.sessionIds, sourceSessionId]
        : (() => {
            const sessionIdsWithoutSource = node.sessionIds.filter(
              (sessionId) => sessionId !== sourceSessionId,
            );
            const targetIndex = sessionIdsWithoutSource.indexOf(targetSessionId);
            const insertIndex = position === "before" ? targetIndex : targetIndex + 1;
            sessionIdsWithoutSource.splice(insertIndex, 0, sourceSessionId);
            return sessionIdsWithoutSource;
          })();
    /**
     * CDXC:PaneTabs 2026-06-06-04:36:
     * Cmd+T and Cmd+N create tabs in the focused pane immediately next to the focused tab, not at the far right of the tab strip. Preserve the old append behavior unless callers explicitly request before/after placement.
     */
    return {
      didAdd: true,
      node: {
        ...node,
        activeSessionId: sourceSessionId,
        sessionIds: dedupeVisibleSessionIds(nextSessionIds),
      },
    };
  }
  let didAdd = false;
  const children = node.children.map((child) => {
    if (didAdd) {
      return child;
    }
    const result = addSessionToPaneTabGroupNode(
      child,
      targetSessionId,
      sourceSessionId,
      position,
    );
    didAdd = result.didAdd;
    return result.node;
  });
  return {
    didAdd,
    node: flattenPaneLayoutSplit({ ...node, children }),
  };
}

function insertExistingSessionBesidePane(
  layout: SessionPaneLayoutNode,
  targetSessionId: string,
  sourceSessionId: string,
  direction: SessionPaneSplitDirection,
  placeAfterTarget: boolean,
): SessionPaneLayoutNode | undefined {
  const result = insertExistingSessionBesidePaneNode(
    layout,
    targetSessionId,
    sourceSessionId,
    direction,
    placeAfterTarget,
  );
  return result.didInsert ? result.node : undefined;
}

function insertExistingSessionBesidePaneNode(
  node: SessionPaneLayoutNode,
  targetSessionId: string,
  sourceSessionId: string,
  direction: SessionPaneSplitDirection,
  placeAfterTarget: boolean,
): { didInsert: boolean; node: SessionPaneLayoutNode } {
  const sourceLeaf: SessionPaneLayoutNode = { kind: "leaf", sessionId: sourceSessionId };
  if (node.kind === "leaf" || node.kind === "tabs") {
    const containsTarget = paneLayoutContainsSession(node, targetSessionId);
    if (!containsTarget) {
      return { didInsert: false, node };
    }
    return {
      didInsert: true,
      node: {
        children: placeAfterTarget ? [node, sourceLeaf] : [sourceLeaf, node],
        direction,
        kind: "split",
      },
    };
  }
  const children: SessionPaneLayoutNode[] = [];
  let didInsert = false;
  for (const child of node.children) {
    if (!didInsert && paneLayoutContainsSession(child, targetSessionId)) {
      if (node.direction === direction) {
        if (placeAfterTarget) {
          children.push(child, sourceLeaf);
        } else {
          children.push(sourceLeaf, child);
        }
        didInsert = true;
      } else {
        const result = insertExistingSessionBesidePaneNode(
          child,
          targetSessionId,
          sourceSessionId,
          direction,
          placeAfterTarget,
        );
        children.push(result.node);
        didInsert = result.didInsert;
      }
    } else {
      children.push(child);
    }
  }
  return {
    didInsert,
    node: flattenPaneLayoutSplit({ ...node, children }),
  };
}

function getSamePaneSplitAnchorSessionId(
  layout: SessionPaneLayoutNode,
  sourceSessionId: string,
  placeAfterTarget: boolean,
): string | undefined {
  if (layout.kind === "leaf") {
    return undefined;
  }
  if (layout.kind === "tabs") {
    if (!layout.sessionIds.includes(sourceSessionId) || layout.sessionIds.length <= 1) {
      return undefined;
    }
    const siblingSessionIds = layout.sessionIds.filter((sessionId) => sessionId !== sourceSessionId);
    /**
     * CDXC:PaneTabs 2026-05-12-11:08
     * Same-pane side drops split the dragged tab beside the remaining tab group.
     * Use the first remaining tab for left/top drops and the last remaining tab
     * for right/bottom drops so visible order mirrors the requested edge.
     */
    return placeAfterTarget ? siblingSessionIds[siblingSessionIds.length - 1] : siblingSessionIds[0];
  }
  for (const child of layout.children) {
    const anchorSessionId = getSamePaneSplitAnchorSessionId(
      child,
      sourceSessionId,
      placeAfterTarget,
    );
    if (anchorSessionId) {
      return anchorSessionId;
    }
  }
  return undefined;
}

function reorderSessionInPaneTabGroupNode(
  node: SessionPaneLayoutNode,
  sourceSessionId: string,
  targetSessionId: string,
  position: SessionPaneTabReorderPosition,
): { didReorder: boolean; node: SessionPaneLayoutNode } {
  if (node.kind === "leaf") {
    return { didReorder: false, node };
  }
  if (node.kind === "tabs") {
    if (!node.sessionIds.includes(sourceSessionId) || !node.sessionIds.includes(targetSessionId)) {
      return { didReorder: false, node };
    }
    const sessionIdsWithoutSource = node.sessionIds.filter(
      (sessionId) => sessionId !== sourceSessionId,
    );
    const targetIndex = sessionIdsWithoutSource.indexOf(targetSessionId);
    if (targetIndex < 0) {
      return { didReorder: false, node };
    }
    const insertIndex = position === "before" ? targetIndex : targetIndex + 1;
    const nextSessionIds = [...sessionIdsWithoutSource];
    nextSessionIds.splice(insertIndex, 0, sourceSessionId);
    if (nextSessionIds.join("\u0000") === node.sessionIds.join("\u0000")) {
      return { didReorder: false, node };
    }
    return {
      didReorder: true,
      node: {
        ...node,
        sessionIds: nextSessionIds,
      },
    };
  }
  let didReorder = false;
  const children = node.children.map((child) => {
    if (didReorder) {
      return child;
    }
    const result = reorderSessionInPaneTabGroupNode(
      child,
      sourceSessionId,
      targetSessionId,
      position,
    );
    didReorder = result.didReorder;
    return result.node;
  });
  return {
    didReorder,
    node: didReorder ? flattenPaneLayoutSplit({ ...node, children }) : node,
  };
}

function moveVisibleSessionNearTarget(
  visibleSessionIds: readonly string[],
  sourceSessionId: string,
  targetSessionId: string,
  placeAfterTarget: boolean,
): string[] {
  const nextVisibleSessionIds = visibleSessionIds.filter((sessionId) => sessionId !== sourceSessionId);
  const targetIndex = nextVisibleSessionIds.indexOf(targetSessionId);
  if (targetIndex < 0) {
    return [...visibleSessionIds];
  }
  nextVisibleSessionIds.splice(targetIndex + (placeAfterTarget ? 1 : 0), 0, sourceSessionId);
  return nextVisibleSessionIds;
}

function getPaneSessionIds(snapshot: SessionGroupRecord["snapshot"]): string[] {
  /**
   * CDXC:PaneTabs 2026-05-11-14:07
   * Native pane tabs render every session, not just the legacy visibleSessionIds
   * slice. Pane drop mutations must use the same inventory or a tab appended by
   * the all-session rule can be dragged in Swift but ignored by the sidebar as
   * "not visible".
   */
  const sessionIdSet = new Set(snapshot.sessions.map((session) => session.sessionId));
  const paneSessionIds: string[] = [];
  for (const sessionId of [
    ...snapshot.visibleSessionIds,
    ...snapshot.sessions.map((session) => session.sessionId),
  ]) {
    if (!sessionIdSet.has(sessionId) || paneSessionIds.includes(sessionId)) {
      continue;
    }
    paneSessionIds.push(sessionId);
  }
  return paneSessionIds;
}

export function ensureAllSessionsInFocusedPaneTabGroupInSimpleWorkspace(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  options: EnsureVirtualPaneTabsOptions = {},
): WorkspaceMutationResult {
  const normalizedSnapshot = normalizeSimpleGroupedSessionWorkspaceSnapshot(snapshot);
  const group = getGroupById(normalizedSnapshot, groupId);
  if (!group) {
    return { changed: false, snapshot: normalizedSnapshot };
  }
  if (
    group.snapshot.visibleCount === 1 &&
    group.snapshot.fullscreenRestoreVisibleCount !== undefined
  ) {
    /*
     * CDXC:SessionFocusMode 2026-06-02-18:45:
     * Publish-time virtual tab materialization must not rewrite paneLayout while Focus mode is active.
     * Focus mode intentionally renders only the focused tab group, so treating visibleSessionIds as the complete rendered owner set would prune the hidden split branches and make Exit focus restore only a tab group instead of the original split.
     */
    return {
      changed: !areSnapshotsEqual(snapshot, normalizedSnapshot),
      snapshot: normalizedSnapshot,
    };
  }
  const nextGroupSnapshot = materializeAllSessionsInFocusedPaneTabGroup(group.snapshot, {
    preserveSplitTopology: options.intent === "passiveSync",
  });
  const nextSnapshot = updateGroup(normalizedSnapshot, groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: nextGroupSnapshot,
  }));
  return {
    changed: !areSnapshotsEqual(normalizedSnapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

function materializeAllSessionsInFocusedPaneTabGroup(
  snapshot: SessionGroupRecord["snapshot"],
  options: MaterializeVirtualPaneTabsOptions = {},
): SessionGroupRecord["snapshot"] {
  if (snapshot.visibleCount === 1 && snapshot.fullscreenRestoreVisibleCount !== undefined) {
    /*
     * CDXC:SessionFocusMode 2026-06-04-20:37:
     * Focus mode intentionally hides other split branches without deleting them from paneLayout.
     * Keep private virtual-tab materialization as a no-op during Focus too, so callers cannot bypass the public publish-time guard and flatten hidden panes into the focused tab group before Exit focus restores.
     */
    return normalizeGroupSnapshot(snapshot);
  }
  const sessionIds = dedupeVisibleSessionIds(snapshot.sessions.map((session) => session.sessionId));
  const awakeSessionIds = dedupeVisibleSessionIds(
    snapshot.sessions
      .filter((session) => session.isSleeping !== true)
      .map((session) => session.sessionId),
  );
  if (sessionIds.length === 0) {
    return normalizeGroupSnapshot(snapshot);
  }
  const nextPaneLayout = normalizeSessionsIntoFocusedPaneTabGroup(
    snapshot.paneLayout,
    sessionIds,
    awakeSessionIds,
    snapshot.visibleSessionIds,
    snapshot.focusedSessionId,
    options,
  );
  if (!nextPaneLayout) {
    return normalizeGroupSnapshot(snapshot);
  }
  return normalizeGroupSnapshot({
    ...snapshot,
    /**
     * CDXC:PaneTabs 2026-05-29-09:04:
     * macOS native tabs must represent every sidebar session in the active
     * group, even when the provider session is missing or the native pane is
     * unmounted. Persist virtual tab membership in the focused pane tab group so
     * tab clicks, context menus, drag/drop, and restart restore all use one
     * paneLayout source of truth instead of a display-only synthesized tab list.
     *
     * CDXC:PaneTabs 2026-05-29-09:26:
     * App restart can leave sessions in sleeping-only paneLayout branches that
     * Swift correctly prunes because no native pane owns those branches. Treat
     * runtime materialization as an idempotent layout normalization: preserve
     * rendered split pane owners and their existing tab siblings, but relocate
     * sessions from non-rendered branches into the focused tab group so the
     * sidebar and tab strip expose the same session inventory.
     */
    paneLayout: nextPaneLayout,
  });
}

function normalizeSessionsIntoFocusedPaneTabGroup(
  layout: SessionPaneLayoutNode | undefined,
  sessionIds: readonly string[],
  awakeSessionIds: readonly string[],
  visibleSessionIds: readonly string[],
  focusedSessionId: string | undefined,
  options: MaterializeVirtualPaneTabsOptions = {},
): SessionPaneLayoutNode | undefined {
  const allowedSessionIds = dedupeVisibleSessionIds(sessionIds);
  const allowedSessionIdSet = new Set(allowedSessionIds);
  const awakeSessionIdSet = new Set(
    awakeSessionIds.filter((sessionId) => allowedSessionIdSet.has(sessionId)),
  );
  /*
   * CDXC:PaneTabs 2026-06-12-06:35:
   * Opening or closing tabs can leave legacy visibleSessionIds missing a split's selected owner while paneLayout still has the stable pane owner.
   * Treat awake active owners from the existing paneLayout as rendered owners before virtual-tab materialization, so live split panes are not pruned and their tab inventories cannot be appended into another pane.
   */
  const activePaneOwnerSessionIds = layout
    ? getRenderedPaneOwnerSessionIds(layout, awakeSessionIdSet)
    : [];
  const visiblePaneOwnerSessionIds = dedupeVisibleSessionIds([
    ...activePaneOwnerSessionIds,
    ...visibleSessionIds.filter((sessionId) => allowedSessionIdSet.has(sessionId)),
  ]);
  const seedSessionIds = dedupeVisibleSessionIds([
    ...visiblePaneOwnerSessionIds,
    ...(focusedSessionId && allowedSessionIdSet.has(focusedSessionId) ? [focusedSessionId] : []),
    ...allowedSessionIds,
  ]);
  const renderedLayout =
    layout && visiblePaneOwnerSessionIds.length > 0
      ? retainRenderedPaneLayoutSessions(
          layout,
          allowedSessionIdSet,
          new Set(visiblePaneOwnerSessionIds),
          focusedSessionId,
        )
      : undefined;
  const missingVisibleSessionIds = visiblePaneOwnerSessionIds.filter(
    (sessionId) => !paneLayoutContainsSession(renderedLayout, sessionId),
  );
  const missingVisibleLayout =
    missingVisibleSessionIds.length > 0
      ? createInitialPaneLayoutForCreatedSession(
          missingVisibleSessionIds,
          focusedSessionId && missingVisibleSessionIds.includes(focusedSessionId)
            ? focusedSessionId
            : missingVisibleSessionIds[0] ?? "",
        )
      : undefined;
  const baseLayout =
    renderedLayout && missingVisibleLayout
      ? flattenPaneLayoutSplit({
          children: [renderedLayout, missingVisibleLayout],
          direction: "horizontal",
          kind: "split",
        })
      : renderedLayout ??
        missingVisibleLayout ??
        createInitialPaneLayoutForCreatedSession(
          seedSessionIds.length > 0 ? seedSessionIds : allowedSessionIds,
          focusedSessionId && allowedSessionIdSet.has(focusedSessionId)
            ? focusedSessionId
            : seedSessionIds[0] ?? allowedSessionIds[0] ?? "",
        );
  if (!baseLayout) {
    return undefined;
  }
  const baseLayoutSessionIds = new Set(getPaneLayoutSessionIds(baseLayout));
  const backgroundSessionIds = allowedSessionIds.filter(
    (sessionId) => !baseLayoutSessionIds.has(sessionId),
  );
  const materializedLayout = (() => {
    if (backgroundSessionIds.length === 0) {
      return baseLayout;
    }
    const targetSessionId = resolveFocusedPaneTabGroupTargetSessionId(
      baseLayout,
      focusedSessionId,
      visiblePaneOwnerSessionIds,
      allowedSessionIds,
    );
    if (targetSessionId) {
      const appendedToFocusedGroup = appendSessionsToPaneTabGroupPreservingActive(
        baseLayout,
        targetSessionId,
        backgroundSessionIds,
      );
      if (appendedToFocusedGroup) {
        return appendedToFocusedGroup;
      }
    }
    return appendSessionsToFirstPaneTabGroupPreservingActive(baseLayout, backgroundSessionIds).node;
  })();
  if (!options.preserveSplitTopology || !layout) {
    return materializedLayout;
  }

  const topologyPreservingLayout = createSplitTopologyPreservingVirtualTabLayout(
    layout,
    allowedSessionIds,
    visiblePaneOwnerSessionIds,
    focusedSessionId,
  );
  if (
    topologyPreservingLayout &&
    doesVirtualTabMaterializationReduceSplitTopology(
      topologyPreservingLayout,
      materializedLayout,
    )
  ) {
    /*
     * CDXC:PaneTabs 2026-06-12-09:18:
     * Publish-time virtual tab materialization is passive synchronization, not
     * pane mutation. If stale visible/awake state would reduce an existing
     * split tree, keep the prior pane owner slots and append only missing
     * virtual tab ids into the focused tab group; explicit close, split, drag,
     * Focus, and Merge All Tabs actions own intentional topology changes.
     */
    return topologyPreservingLayout;
  }
  return materializedLayout;
}

function createSplitTopologyPreservingVirtualTabLayout(
  layout: SessionPaneLayoutNode,
  allowedSessionIds: readonly string[],
  visiblePaneOwnerSessionIds: readonly string[],
  focusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  const allowedSessionIdSet = new Set(allowedSessionIds);
  const preservedLayout = preserveExistingPaneLayoutTopology(
    layout,
    allowedSessionIdSet,
    focusedSessionId,
  );
  if (!preservedLayout) {
    return undefined;
  }
  const preservedSessionIdSet = new Set(getPaneLayoutSessionIds(preservedLayout));
  const missingSessionIds = allowedSessionIds.filter(
    (sessionId) => !preservedSessionIdSet.has(sessionId),
  );
  if (missingSessionIds.length === 0) {
    return preservedLayout;
  }
  const targetSessionId = resolveFocusedPaneTabGroupTargetSessionId(
    preservedLayout,
    focusedSessionId,
    visiblePaneOwnerSessionIds,
    allowedSessionIds,
  );
  if (targetSessionId) {
    const appendedLayout = appendSessionsToPaneTabGroupPreservingActive(
      preservedLayout,
      targetSessionId,
      missingSessionIds,
    );
    if (appendedLayout) {
      return appendedLayout;
    }
  }
  return appendSessionsToFirstPaneTabGroupPreservingActive(preservedLayout, missingSessionIds).node;
}

function preserveExistingPaneLayoutTopology(
  node: SessionPaneLayoutNode,
  allowedSessionIdSet: ReadonlySet<string>,
  focusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  if (node.kind === "leaf") {
    return allowedSessionIdSet.has(node.sessionId) ? node : undefined;
  }
  if (node.kind === "tabs") {
    const sessionIds = dedupeVisibleSessionIds(node.sessionIds).filter((sessionId) =>
      allowedSessionIdSet.has(sessionId),
    );
    if (sessionIds.length === 0) {
      return undefined;
    }
    const activeSessionId =
      (node.activeSessionId && sessionIds.includes(node.activeSessionId)
        ? node.activeSessionId
        : undefined) ??
      (focusedSessionId && sessionIds.includes(focusedSessionId) ? focusedSessionId : undefined) ??
      sessionIds[0];
    return sessionIds.length === 1
      ? { kind: "leaf", sessionId: sessionIds[0]! }
      : { activeSessionId, kind: "tabs", sessionIds };
  }
  const children = node.children
    .map((child) =>
      preserveExistingPaneLayoutTopology(child, allowedSessionIdSet, focusedSessionId),
    )
    .filter((child): child is SessionPaneLayoutNode => child !== undefined);
  if (children.length === 0) {
    return undefined;
  }
  if (children.length === 1) {
    return children[0];
  }
  return flattenPaneLayoutSplit({ ...node, children });
}

function doesVirtualTabMaterializationReduceSplitTopology(
  previousLayout: SessionPaneLayoutNode,
  nextLayout: SessionPaneLayoutNode,
): boolean {
  const previousOwnerSlotCount = countPaneLayoutOwnerSlots(previousLayout);
  const nextOwnerSlotCount = countPaneLayoutOwnerSlots(nextLayout);
  if (previousOwnerSlotCount > 1 && nextOwnerSlotCount < previousOwnerSlotCount) {
    return true;
  }
  const previousSplitCount = countPaneLayoutSplitNodes(previousLayout);
  const nextSplitCount = countPaneLayoutSplitNodes(nextLayout);
  return previousSplitCount > 0 && nextSplitCount < previousSplitCount;
}

function countPaneLayoutSplitNodes(layout: SessionPaneLayoutNode): number {
  if (layout.kind !== "split") {
    return 0;
  }
  return 1 + layout.children.reduce(
    (count, child) => count + countPaneLayoutSplitNodes(child),
    0,
  );
}

function retainRenderedPaneLayoutSessions(
  node: SessionPaneLayoutNode,
  allowedSessionIdSet: ReadonlySet<string>,
  visiblePaneOwnerSessionIdSet: ReadonlySet<string>,
  focusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  if (node.kind === "leaf") {
    return allowedSessionIdSet.has(node.sessionId) && visiblePaneOwnerSessionIdSet.has(node.sessionId)
      ? node
      : undefined;
  }
  if (node.kind === "tabs") {
    const sessionIds = dedupeVisibleSessionIds(node.sessionIds).filter((sessionId) =>
      allowedSessionIdSet.has(sessionId),
    );
    const hasVisiblePaneOwner = sessionIds.some((sessionId) =>
      visiblePaneOwnerSessionIdSet.has(sessionId),
    );
    if (sessionIds.length === 0 || !hasVisiblePaneOwner) {
      return undefined;
    }
    const activeSessionId =
      (node.activeSessionId && visiblePaneOwnerSessionIdSet.has(node.activeSessionId)
        ? node.activeSessionId
        : undefined) ??
      (focusedSessionId && visiblePaneOwnerSessionIdSet.has(focusedSessionId)
        ? focusedSessionId
        : undefined) ??
      sessionIds.find((sessionId) => visiblePaneOwnerSessionIdSet.has(sessionId)) ??
      sessionIds[0];
    return sessionIds.length === 1
      ? { kind: "leaf", sessionId: sessionIds[0]! }
      : { activeSessionId, kind: "tabs", sessionIds };
  }
  /**
   * CDXC:PaneTabs 2026-05-29-09:26:
   * Preserve complete tab groups that already have a visible native pane owner,
   * including their sleeping/unmounted tab siblings. Only sessions trapped in
   * branches with no rendered owner are relocated to the focused tab group.
   */
  const children = node.children
    .map((child) =>
      retainRenderedPaneLayoutSessions(
        child,
        allowedSessionIdSet,
        visiblePaneOwnerSessionIdSet,
        focusedSessionId,
      ),
    )
    .filter((child): child is SessionPaneLayoutNode => child !== undefined);
  if (children.length === 0) {
    return undefined;
  }
  if (children.length === 1) {
    return children[0];
  }
  return flattenPaneLayoutSplit({ ...node, children });
}

function resolveFocusedPaneTabGroupTargetSessionId(
  layout: SessionPaneLayoutNode,
  focusedSessionId: string | undefined,
  visibleSessionIds: readonly string[],
  allSessionIds: readonly string[],
): string | undefined {
  const layoutSessionIds = new Set(getPaneLayoutSessionIds(layout));
  for (const sessionId of [
    ...(focusedSessionId ? [focusedSessionId] : []),
    ...visibleSessionIds,
    ...allSessionIds,
  ]) {
    if (layoutSessionIds.has(sessionId)) {
      return sessionId;
    }
  }
  return undefined;
}

function appendSessionsToPaneTabGroupPreservingActive(
  layout: SessionPaneLayoutNode,
  targetSessionId: string,
  sessionIdsToAppend: readonly string[],
): SessionPaneLayoutNode | undefined {
  const result = appendSessionsToPaneTabGroupPreservingActiveNode(
    layout,
    targetSessionId,
    sessionIdsToAppend,
  );
  return result.didAppend ? result.node : undefined;
}

function appendSessionsToPaneTabGroupPreservingActiveNode(
  node: SessionPaneLayoutNode,
  targetSessionId: string,
  sessionIdsToAppend: readonly string[],
): { didAppend: boolean; node: SessionPaneLayoutNode } {
  const appendIds = sessionIdsToAppend.filter((sessionId) => sessionId !== targetSessionId);
  if (appendIds.length === 0) {
    return { didAppend: false, node };
  }
  if (node.kind === "leaf") {
    return node.sessionId === targetSessionId
      ? {
          didAppend: true,
          node: {
            activeSessionId: targetSessionId,
            kind: "tabs",
            sessionIds: dedupeVisibleSessionIds([targetSessionId, ...appendIds]),
          },
        }
      : { didAppend: false, node };
  }
  if (node.kind === "tabs") {
    if (!node.sessionIds.includes(targetSessionId)) {
      return { didAppend: false, node };
    }
    const sessionIds = dedupeVisibleSessionIds([...node.sessionIds, ...appendIds]);
    return {
      didAppend: true,
      node: {
        ...node,
        activeSessionId:
          node.activeSessionId && sessionIds.includes(node.activeSessionId)
            ? node.activeSessionId
            : targetSessionId,
        sessionIds,
      },
    };
  }
  let didAppend = false;
  const children = node.children.map((child) => {
    if (didAppend) {
      return child;
    }
    const result = appendSessionsToPaneTabGroupPreservingActiveNode(
      child,
      targetSessionId,
      appendIds,
    );
    didAppend = result.didAppend;
    return result.node;
  });
  return {
    didAppend,
    node: didAppend ? flattenPaneLayoutSplit({ ...node, children }) : node,
  };
}

function appendSessionsToFirstPaneTabGroupPreservingActive(
  node: SessionPaneLayoutNode,
  sessionIdsToAppend: readonly string[],
): { didAppend: boolean; node: SessionPaneLayoutNode } {
  const appendIds = dedupeVisibleSessionIds(sessionIdsToAppend);
  if (appendIds.length === 0) {
    return { didAppend: false, node };
  }
  if (node.kind === "leaf") {
    return {
      didAppend: true,
      node: {
        activeSessionId: node.sessionId,
        kind: "tabs",
        sessionIds: dedupeVisibleSessionIds([node.sessionId, ...appendIds]),
      },
    };
  }
  if (node.kind === "tabs") {
    const sessionIds = dedupeVisibleSessionIds([...node.sessionIds, ...appendIds]);
    return {
      didAppend: true,
      node: {
        ...node,
        activeSessionId:
          node.activeSessionId && sessionIds.includes(node.activeSessionId)
            ? node.activeSessionId
            : sessionIds[0],
        sessionIds,
      },
    };
  }
  let didAppend = false;
  const children = node.children.map((child) => {
    if (didAppend) {
      return child;
    }
    const result = appendSessionsToFirstPaneTabGroupPreservingActive(child, appendIds);
    didAppend = result.didAppend;
    return result.node;
  });
  return {
    didAppend,
    node: didAppend ? flattenPaneLayoutSplit({ ...node, children }) : node,
  };
}

function normalizePaneLayout(
  layout: SessionPaneLayoutNode | undefined,
  allowedSessionIds: readonly string[],
  visibleSessionIds: readonly string[],
  focusedSessionId?: string,
): SessionPaneLayoutNode | undefined {
  const allowedSessionIdSet = new Set(allowedSessionIds);
  const visibleSessionIdSet = new Set(visibleSessionIds);
  const normalized = layout
    ? normalizePaneLayoutNode(layout, allowedSessionIdSet, visibleSessionIdSet, focusedSessionId)
    : undefined;
  const missingVisibleSessionIds = visibleSessionIds.filter(
    (sessionId) => !paneLayoutContainsSession(normalized, sessionId),
  );
  if (missingVisibleSessionIds.length === 0) {
    return normalized;
  }
  if (!normalized) {
    /**
     * CDXC:SplitIntent 2026-05-19-08:29:
     * Legacy snapshots without paneLayout have no explicit split intent. Backfill
     * their visible sessions as one tab group so normalization cannot invent new
     * split panes before a real split command supplies an explicit split tree.
     */
    return createInitialPaneLayoutForCreatedSession(
      missingVisibleSessionIds,
      focusedSessionId ?? missingVisibleSessionIds.at(-1) ?? "",
    );
  }
  const missingLayout = createInitialPaneLayoutForCreatedSession(
    missingVisibleSessionIds,
    focusedSessionId ?? missingVisibleSessionIds.at(-1) ?? "",
  );
  if (!missingLayout) {
    return normalized;
  }
  return flattenPaneLayoutSplit({
    children: [normalized, missingLayout],
    direction: "horizontal",
    kind: "split",
  });
}

function normalizePaneLayoutNode(
  node: SessionPaneLayoutNode,
  allowedSessionIdSet: ReadonlySet<string>,
  visibleSessionIdSet: ReadonlySet<string>,
  focusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  if (node.kind === "leaf") {
    return allowedSessionIdSet.has(node.sessionId) && visibleSessionIdSet.has(node.sessionId)
      ? node
      : undefined;
  }
  if (node.kind === "tabs") {
    const sessionIds = dedupeVisibleSessionIds(node.sessionIds).filter(
      (sessionId) => allowedSessionIdSet.has(sessionId) && visibleSessionIdSet.has(sessionId),
    );
    if (sessionIds.length === 0) {
      return undefined;
    }
    const activeSessionId =
      (node.activeSessionId && sessionIds.includes(node.activeSessionId)
        ? node.activeSessionId
        : undefined) ??
      (focusedSessionId && sessionIds.includes(focusedSessionId) ? focusedSessionId : undefined) ??
      sessionIds[0];
    return sessionIds.length === 1
      ? { kind: "leaf", sessionId: sessionIds[0]! }
      : { activeSessionId, kind: "tabs", sessionIds };
  }
  const children = node.children
    .map((child) =>
      normalizePaneLayoutNode(child, allowedSessionIdSet, visibleSessionIdSet, focusedSessionId),
    )
    .filter((child): child is SessionPaneLayoutNode => child !== undefined);
  if (children.length === 0) {
    return undefined;
  }
  if (children.length === 1) {
    return children[0];
  }
  return flattenPaneLayoutSplit({ ...node, children });
}

function flattenPaneLayoutSplit(
  node: Extract<SessionPaneLayoutNode, { kind: "split" }>,
): SessionPaneLayoutNode {
  /**
   * CDXC:WorkspacePanes 2026-05-11-03:20
   * Ratio-bearing splits encode intentional pane sizing. Do not flatten their
   * same-direction children, because an 85/15 bottom terminal row must remain a
   * two-child split even when the existing workarea is already vertically split.
   */
  if (node.ratio !== undefined) {
    return node.children.length === 1 ? node.children[0]! : node;
  }
  const children = node.children.flatMap((child) =>
    child.kind === "split" && child.direction === node.direction ? child.children : [child],
  );
  return children.length === 1 ? children[0]! : { ...node, children };
}

function rotatePaneLayoutNodeClockwise(node: SessionPaneLayoutNode): SessionPaneLayoutNode {
  if (node.kind !== "split") {
    return node;
  }
  const rotatedChildren = node.children.map(rotatePaneLayoutNodeClockwise);
  const shouldReverseChildren = node.direction === "vertical";
  const children = shouldReverseChildren ? rotatedChildren.toReversed() : rotatedChildren;
  const ratio =
    node.ratio === undefined
      ? undefined
      : shouldReverseChildren
        ? Math.max(0, Math.min(1, 1 - node.ratio))
        : node.ratio;
  return flattenPaneLayoutSplit({
    children,
    direction: node.direction === "horizontal" ? "vertical" : "horizontal",
    kind: "split",
    ...(ratio === undefined ? {} : { ratio }),
  });
}

function paneLayoutContainsSession(
  node: SessionPaneLayoutNode | undefined,
  sessionId: string,
): boolean {
  if (!node) {
    return false;
  }
  switch (node.kind) {
    case "leaf":
      return node.sessionId === sessionId;
    case "tabs":
      return node.sessionIds.includes(sessionId);
    case "split":
      return node.children.some((child) => paneLayoutContainsSession(child, sessionId));
  }
}

function findPaneTabGroupSessionIds(
  node: SessionPaneLayoutNode | undefined,
  sessionId: string,
): string[] | undefined {
  if (!node) {
    return undefined;
  }
  if (node.kind === "leaf") {
    return node.sessionId === sessionId ? [sessionId] : undefined;
  }
  if (node.kind === "tabs") {
    return node.sessionIds.includes(sessionId) ? node.sessionIds : undefined;
  }
  for (const child of node.children) {
    const tabSessionIds = findPaneTabGroupSessionIds(child, sessionId);
    if (tabSessionIds) {
      return tabSessionIds;
    }
  }
  return undefined;
}

function restoreFocusModeForExternalSession(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): GroupedSessionWorkspaceSnapshot {
  const activeGroup = getActiveGroup(snapshot);
  if (
    !activeGroup ||
    activeGroup.snapshot.visibleCount !== 1 ||
    activeGroup.snapshot.fullscreenRestoreVisibleCount === undefined
  ) {
    return snapshot;
  }
  const focusedSessionId = activeGroup.snapshot.focusedSessionId;
  if (!focusedSessionId) {
    return snapshot;
  }
  const focusedTabSessionIds =
    findPaneTabGroupSessionIds(activeGroup.snapshot.paneLayout, focusedSessionId) ?? [
      focusedSessionId,
    ];
  if (focusedTabSessionIds.includes(sessionId)) {
    return snapshot;
  }
  const restoreVisibleCount = activeGroup.snapshot.fullscreenRestoreVisibleCount;
  if (restoreVisibleCount === undefined) {
    return snapshot;
  }

  return updateGroup(snapshot, activeGroup.groupId, (group) => ({
    ...group,
    snapshot: normalizeGroupSnapshot({
      ...group.snapshot,
      /**
       * CDXC:SessionFocusMode 2026-05-23-09:28:
       * Selecting a session outside the focused tab group exits focus mode
       * before selecting that session. This restores the prior split count and
       * then lets normal paneLayout focus choose the target tab group.
      */
      fullscreenRestoreVisibleCount: undefined,
      visibleCount: restoreVisibleCount,
    }),
  }));
}

function getPaneLayoutSessionIds(node: SessionPaneLayoutNode | undefined): string[] {
  if (!node) {
    return [];
  }
  switch (node.kind) {
    case "leaf":
      return [node.sessionId];
    case "tabs":
      return node.sessionIds;
    case "split":
      return node.children.flatMap((child) => getPaneLayoutSessionIds(child));
  }
}

export function hasMultiplePaneOwners(snapshot: SessionGridSnapshot): boolean {
  const awakeSessionIds = new Set(
    snapshot.sessions
      .filter((session) => session.isSleeping !== true)
      .map((session) => session.sessionId),
  );
  const paneOwnerSessionIds = getRenderedPaneOwnerSessionIds(
    snapshot.paneLayout,
    awakeSessionIds,
  );
  if (paneOwnerSessionIds.length > 0) {
    return paneOwnerSessionIds.length > 1;
  }
  return snapshot.visibleSessionIds.filter((sessionId) => awakeSessionIds.has(sessionId)).length > 1;
}

function getRenderedPaneOwnerSessionIds(
  node: SessionPaneLayoutNode | undefined,
  awakeSessionIds: ReadonlySet<string>,
): string[] {
  if (!node) {
    return [];
  }
  switch (node.kind) {
    case "leaf":
      return awakeSessionIds.has(node.sessionId) ? [node.sessionId] : [];
    case "tabs": {
      const activeSessionId =
        node.activeSessionId && awakeSessionIds.has(node.activeSessionId)
          ? node.activeSessionId
          : node.sessionIds.find((sessionId) => awakeSessionIds.has(sessionId));
      return activeSessionId ? [activeSessionId] : [];
    }
    case "split":
      return node.children.flatMap((child) =>
        getRenderedPaneOwnerSessionIds(child, awakeSessionIds),
      );
  }
}

function getPlacementStableVisibleIds(
  sessions: readonly SessionRecord[],
  currentVisibleSessionIds: readonly string[],
): string[] {
  const sessionIdSet = new Set(sessions.map((session) => session.sessionId));
  const visibleSessionIds: string[] = [];
  for (const sessionId of currentVisibleSessionIds) {
    if (!sessionIdSet.has(sessionId) || visibleSessionIds.includes(sessionId)) {
      continue;
    }
    visibleSessionIds.push(sessionId);
  }
  return visibleSessionIds;
}

function insertVisibleSessionAfterTarget(
  currentVisibleSessionIds: readonly string[],
  nextSessionId: string,
  targetSessionId: string | undefined,
): { replacedSessionId?: string; visibleCount: VisibleSessionCount; visibleSessionIds: string[] } {
  const nextVisibleSessionIds = currentVisibleSessionIds.filter(
    (sessionId) => sessionId !== nextSessionId,
  );
  const targetIndex = targetSessionId
    ? nextVisibleSessionIds.indexOf(targetSessionId)
    : nextVisibleSessionIds.length - 1;
  nextVisibleSessionIds.splice(Math.max(0, targetIndex + 1), 0, nextSessionId);
  const visibleCount = clampSupportedVisibleCount(
    Math.max(1, nextVisibleSessionIds.length),
  );
  return {
    visibleCount,
    visibleSessionIds: nextVisibleSessionIds.slice(0, visibleCount),
  };
}

function replaceVisibleSessionTarget(
  currentVisibleCount: VisibleSessionCount,
  currentVisibleSessionIds: readonly string[],
  nextSessionId: string,
  targetSessionId: string,
): { replacedSessionId?: string; visibleCount: VisibleSessionCount; visibleSessionIds: string[] } {
  const nextVisibleSessionIds = [...currentVisibleSessionIds];
  const targetIndex = nextVisibleSessionIds.indexOf(targetSessionId);
  let replacedSessionId: string | undefined;
  if (targetIndex >= 0) {
    replacedSessionId = nextVisibleSessionIds[targetIndex];
    nextVisibleSessionIds[targetIndex] = nextSessionId;
  } else if (nextVisibleSessionIds.length > 0) {
    replacedSessionId = nextVisibleSessionIds[nextVisibleSessionIds.length - 1];
    nextVisibleSessionIds[nextVisibleSessionIds.length - 1] = nextSessionId;
  } else {
    nextVisibleSessionIds.push(nextSessionId);
  }
  return {
    replacedSessionId,
    visibleCount: currentVisibleCount,
    visibleSessionIds: dedupeVisibleSessionIds(nextVisibleSessionIds),
  };
}

function replaceNonFocusedVisibleSession(
  currentVisibleCount: VisibleSessionCount,
  sessions: readonly SessionRecord[],
  currentVisibleSessionIds: readonly string[],
  nextSessionId: string,
  preserveSessionId: string | undefined,
): { replacedSessionId?: string; visibleCount: VisibleSessionCount; visibleSessionIds: string[] } {
  const nextVisibleSessionIds = currentVisibleSessionIds.filter(
    (sessionId) => sessionId !== nextSessionId,
  );
  const visibleCapacity = Math.min(currentVisibleCount, sessions.length);
  if (nextVisibleSessionIds.length < visibleCapacity) {
    nextVisibleSessionIds.push(nextSessionId);
    return {
      visibleCount: currentVisibleCount,
      visibleSessionIds: nextVisibleSessionIds,
    };
  }

  const replacementIndex = pickReplacementVisibleSessionIndex(
    nextVisibleSessionIds,
    preserveSessionId,
  );
  const replacedSessionId =
    replacementIndex >= 0 ? nextVisibleSessionIds[replacementIndex] : undefined;
  if (replacementIndex >= 0) {
    nextVisibleSessionIds[replacementIndex] = nextSessionId;
  } else {
    nextVisibleSessionIds.push(nextSessionId);
  }
  return {
    replacedSessionId,
    visibleCount: currentVisibleCount,
    visibleSessionIds: dedupeVisibleSessionIds(nextVisibleSessionIds).slice(0, currentVisibleCount),
  };
}

function pickReplacementVisibleSessionIndex(
  visibleSessionIds: readonly string[],
  preserveSessionId: string | undefined,
): number {
  const nonPreservedIndex = visibleSessionIds.findIndex(
    (sessionId) => sessionId !== preserveSessionId,
  );
  if (nonPreservedIndex >= 0) {
    return nonPreservedIndex;
  }
  return visibleSessionIds.length > 0 ? visibleSessionIds.length - 1 : -1;
}

function dedupeVisibleSessionIds(visibleSessionIds: readonly string[]): string[] {
  const nextVisibleSessionIds: string[] = [];
  for (const sessionId of visibleSessionIds) {
    if (!nextVisibleSessionIds.includes(sessionId)) {
      nextVisibleSessionIds.push(sessionId);
    }
  }
  return nextVisibleSessionIds;
}

function getNormalizedVisibleIds(
  sessions: readonly SessionRecord[],
  visibleCount: VisibleSessionCount,
  focusedSessionId: string | undefined,
  currentVisibleSessionIds: readonly string[],
): string[] {
  if (!focusedSessionId || sessions.length === 0) {
    return [];
  }

  if (visibleCount === 1) {
    return [focusedSessionId];
  }

  const sessionIdSet = new Set(sessions.map((session) => session.sessionId));
  const visibleSessionIds: string[] = [];
  for (const sessionId of currentVisibleSessionIds) {
    if (!sessionIdSet.has(sessionId) || visibleSessionIds.includes(sessionId)) {
      continue;
    }

    visibleSessionIds.push(sessionId);
  }

  if (!visibleSessionIds.includes(focusedSessionId)) {
    visibleSessionIds.push(focusedSessionId);
  }

  for (const session of sessions) {
    if (visibleSessionIds.length >= visibleCount) {
      break;
    }
    if (!visibleSessionIds.includes(session.sessionId)) {
      visibleSessionIds.push(session.sessionId);
    }
  }

  if (visibleSessionIds.length <= visibleCount) {
    return visibleSessionIds;
  }

  const passiveVisibleIds = visibleSessionIds.filter((sessionId) => sessionId !== focusedSessionId);
  return passiveVisibleIds.slice(0, Math.max(0, visibleCount - 1)).concat(focusedSessionId);
}

function getNextVisibleIdsForFocusedSession(
  sessions: readonly SessionRecord[],
  visibleCount: VisibleSessionCount,
  nextFocusedSessionId: string,
  currentVisibleSessionIds: readonly string[],
  currentFocusedSessionId: string | undefined,
): string[] {
  if (visibleCount === 1) {
    return [nextFocusedSessionId];
  }

  const stableVisibleSessionIds = getStableVisibleIds(
    sessions,
    visibleCount,
    currentVisibleSessionIds,
  );

  if (stableVisibleSessionIds.includes(nextFocusedSessionId)) {
    return stableVisibleSessionIds;
  }

  if (stableVisibleSessionIds.length < Math.min(visibleCount, sessions.length)) {
    return getStableVisibleIds(sessions, visibleCount, [
      ...stableVisibleSessionIds,
      nextFocusedSessionId,
    ]);
  }

  if (currentFocusedSessionId) {
    const focusedVisibleIndex = stableVisibleSessionIds.indexOf(currentFocusedSessionId);
    if (focusedVisibleIndex >= 0) {
      const nextVisibleSessionIds = [...stableVisibleSessionIds];
      nextVisibleSessionIds[focusedVisibleIndex] = nextFocusedSessionId;
      return getStableVisibleIds(sessions, visibleCount, nextVisibleSessionIds);
    }
  }

  if (stableVisibleSessionIds.length === 0) {
    return [nextFocusedSessionId];
  }

  return getStableVisibleIds(
    sessions,
    visibleCount,
    stableVisibleSessionIds.map((sessionId, index, visibleSessionIds) =>
      index === visibleSessionIds.length - 1 ? nextFocusedSessionId : sessionId,
    ),
  );
}

function getNextVisibleIdsForRestoredSleepingSession(
  sessions: readonly SessionRecord[],
  visibleCount: VisibleSessionCount,
  nextFocusedSessionId: string,
  currentVisibleSessionIds: readonly string[],
  currentFocusedSessionId: string | undefined,
): string[] {
  /**
   * CDXC:SessionSleep 2026-05-11-02:20
   * Startup normalization preserves sleeping sessions in paneLayout so an app
   * restart can recover the last surfaced split/tab shape. A normal sidebar
   * click is different: waking a sleeping card should put it in the pane the
   * user is looking at, not resurrect an old hidden placement.
   *
   * CDXC:SessionSleep 2026-05-15-19:26:
   * Waking a sleeping sidebar session while split panes are open must add the
   * restored terminal as the active tab in the pane that already owns the
   * current active tab. Do not replace that pane or append a separate split,
   * because command/workspace pane layouts should keep their existing split
   * count when a parked terminal wakes.
   */
  const stableVisibleSessionIds = getStableVisibleIds(
    sessions,
    visibleCount,
    currentVisibleSessionIds,
  );
  const targetSessionId =
    currentFocusedSessionId && stableVisibleSessionIds.includes(currentFocusedSessionId)
      ? currentFocusedSessionId
      : stableVisibleSessionIds[0];
  return insertVisibleSessionAfterTarget(
    stableVisibleSessionIds,
    nextFocusedSessionId,
    targetSessionId,
  ).visibleSessionIds;
}

function getNextPaneLayoutForFocusedSession(
  currentLayout: SessionPaneLayoutNode | undefined,
  currentVisibleSessionIds: readonly string[],
  nextVisibleSessionIds: readonly string[],
  nextFocusedSessionId: string,
  currentFocusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  if (
    currentVisibleSessionIds.includes(nextFocusedSessionId) ||
    paneLayoutContainsSession(currentLayout, nextFocusedSessionId)
  ) {
    /*
     * CDXC:PaneFocus 2026-06-12-13:13:
     * Native terminalFocused can arrive with stale visibleSessionIds after a
     * pane close, while paneLayout still contains the clicked pane/tab owner.
     * Select existing paneLayout members in place instead of replacing the
     * focused pane, otherwise clicking real split panes can merge them into the
     * focused tab group one owner at a time.
     */
    return setActiveSessionInPaneLayout(currentLayout, nextFocusedSessionId);
  }
  /**
   * CDXC:PaneFocus 2026-05-11-16:45
   * Generic focus for hidden sessions should reuse the currently focused pane
   * slot. visibleSessionIds already replaces the focused session id; the
   * paneLayout must mirror that replacement so native sync does not append the
   * clicked session as a new split pane.
   *
   * CDXC:SidebarSessionFocus 2026-05-29-09:47:
   * Sidebar card clicks use focusSidebarSessionInSimpleWorkspace first, so
   * existing paneLayout tabs are selected in place. This replacement path stays
   * available for focus commands that intentionally retarget a hidden session
   * into the focused pane or for sessions with no existing paneLayout tab.
   */
  return replaceFocusedSessionInPaneLayout(
    currentLayout,
    currentVisibleSessionIds,
    nextVisibleSessionIds,
    nextFocusedSessionId,
    currentFocusedSessionId,
  );
}

function getNextPaneLayoutForRestoredSleepingSession(
  currentLayout: SessionPaneLayoutNode | undefined,
  currentVisibleSessionIds: readonly string[],
  nextVisibleSessionIds: readonly string[],
  nextFocusedSessionId: string,
  currentFocusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  const targetSessionId =
    currentFocusedSessionId && currentVisibleSessionIds.includes(currentFocusedSessionId)
      ? currentFocusedSessionId
      : currentVisibleSessionIds[0];
  const currentPaneLayoutSessionIds = getPaneLayoutSessionIds(currentLayout);
  const seededLayoutSessionIds = dedupeVisibleSessionIds([
    ...currentVisibleSessionIds,
    ...currentPaneLayoutSessionIds,
  ]);
  const seededLayout =
    normalizePaneLayout(
      currentLayout,
      seededLayoutSessionIds,
      seededLayoutSessionIds,
      currentFocusedSessionId,
    ) ?? createPaneLayoutFromVisibleIds(seededLayoutSessionIds);
  const layoutWithoutRestoredSession = seededLayout
    ? removeSessionFromPaneLayout(seededLayout, nextFocusedSessionId)
    : undefined;
  const tabbedLayout =
    targetSessionId && layoutWithoutRestoredSession
      ? addSessionToPaneTabGroup(
          layoutWithoutRestoredSession,
          targetSessionId,
          nextFocusedSessionId,
        )
      : undefined;
  const nextPaneLayoutSessionIds = dedupeVisibleSessionIds([
    ...nextVisibleSessionIds,
    ...getPaneLayoutSessionIds(tabbedLayout),
  ]);

  return normalizePaneLayout(
    tabbedLayout ?? createPaneLayoutFromVisibleIds(nextVisibleSessionIds),
    nextPaneLayoutSessionIds,
    nextPaneLayoutSessionIds,
    nextFocusedSessionId,
  );
}

function replaceFocusedSessionInPaneLayout(
  currentLayout: SessionPaneLayoutNode | undefined,
  currentVisibleSessionIds: readonly string[],
  nextVisibleSessionIds: readonly string[],
  nextFocusedSessionId: string,
  currentFocusedSessionId: string | undefined,
): SessionPaneLayoutNode | undefined {
  const targetSessionId =
    currentFocusedSessionId && currentVisibleSessionIds.includes(currentFocusedSessionId)
      ? currentFocusedSessionId
      : currentVisibleSessionIds[0];
  const currentPaneLayoutSessionIds = getPaneLayoutSessionIds(currentLayout);
  const seededLayoutSessionIds = dedupeVisibleSessionIds([
    ...currentVisibleSessionIds,
    ...currentPaneLayoutSessionIds,
  ]);
  /**
   * CDXC:PaneFocus 2026-05-11-18:48
   * Hidden-session clicks and sleeping-session wake replace the focused pane,
   * but they must not discard other members of that pane's tab group. Seed and
   * finalize replacement with paneLayout ids as well as visible ids so sidebar
   * focus actions cannot flatten grouped tabs.
   */
  const seededLayout =
    normalizePaneLayout(
      currentLayout,
      seededLayoutSessionIds,
      seededLayoutSessionIds,
      currentFocusedSessionId,
    ) ?? createPaneLayoutFromVisibleIds(seededLayoutSessionIds);
  const layoutWithoutRestoredSession = seededLayout
    ? removeSessionFromPaneLayout(seededLayout, nextFocusedSessionId)
    : undefined;
  const replacedLayout =
    targetSessionId && layoutWithoutRestoredSession
      ? replaceSessionInPaneLayout(
          layoutWithoutRestoredSession,
          targetSessionId,
          nextFocusedSessionId,
        )
      : undefined;
  const nextPaneLayoutSessionIds = dedupeVisibleSessionIds([
    ...nextVisibleSessionIds,
    ...getPaneLayoutSessionIds(replacedLayout),
  ]);

  return normalizePaneLayout(
    replacedLayout ?? createPaneLayoutFromVisibleIds(nextVisibleSessionIds),
    nextPaneLayoutSessionIds,
    nextPaneLayoutSessionIds,
    nextFocusedSessionId,
  );
}

function getStableVisibleIds(
  sessions: readonly SessionRecord[],
  visibleCount: VisibleSessionCount,
  desiredVisibleSessionIds: readonly string[],
): string[] {
  const sessionIdSet = new Set(sessions.map((session) => session.sessionId));
  const visibleSessionIds: string[] = [];

  for (const sessionId of desiredVisibleSessionIds) {
    if (!sessionIdSet.has(sessionId) || visibleSessionIds.includes(sessionId)) {
      continue;
    }

    visibleSessionIds.push(sessionId);
    if (visibleSessionIds.length >= visibleCount) {
      return visibleSessionIds;
    }
  }

  for (const session of sessions) {
    if (visibleSessionIds.includes(session.sessionId)) {
      continue;
    }

    visibleSessionIds.push(session.sessionId);
    if (visibleSessionIds.length >= visibleCount) {
      break;
    }
  }

  return visibleSessionIds;
}

function getFallbackActiveGroupId(
  snapshot: GroupedSessionWorkspaceSnapshot,
  emptiedGroupId: string,
): string {
  const emptiedGroupIndex = snapshot.groups.findIndex((group) => group.groupId === emptiedGroupId);
  if (emptiedGroupIndex < 0) {
    return snapshot.activeGroupId;
  }

  const previousNonEmptyGroup = snapshot.groups
    .slice(0, emptiedGroupIndex)
    .reverse()
    .find((group) => getActiveSessionCount(group) > 0);
  if (previousNonEmptyGroup) {
    return previousNonEmptyGroup.groupId;
  }

  const nextNonEmptyGroup = snapshot.groups
    .slice(emptiedGroupIndex + 1)
    .find((group) => getActiveSessionCount(group) > 0);
  return nextNonEmptyGroup?.groupId ?? emptiedGroupId;
}

function getAwakeSessions(sessions: readonly SessionRecord[]): SessionRecord[] {
  return sessions.filter((session) => !session.isSleeping);
}

function getActiveSessionCount(group: SessionGroupRecord | undefined): number {
  if (!group) {
    return 0;
  }

  return getAwakeSessions(group.snapshot.sessions).length;
}

function resolveSessionReference(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
): { group: SessionGroupRecord; session: SessionRecord } | undefined {
  for (const group of snapshot.groups) {
    const exactSession = group.snapshot.sessions.find((session) => session.sessionId === sessionId);
    if (exactSession) {
      return { group, session: exactSession };
    }
  }

  /**
   * CDXC:SessionIdentity 2026-05-10-18:30
   * Drag payloads can contain a canonical display-derived session reference
   * after normalization even when a legacy record still carries its old opaque
   * sessionId. Resolve that reference through the normalized displayId so
   * group-drag actions operate on the canonical session instead of failing to
   * find the record.
   */
  const displayId = displayIdFromSessionReference(sessionId);
  if (!displayId) {
    return undefined;
  }
  for (const group of snapshot.groups) {
    const displaySession = group.snapshot.sessions.find((session) => session.displayId === displayId);
    if (displaySession) {
      return { group, session: displaySession };
    }
  }
}

function displayIdFromSessionReference(sessionId: string): string | undefined {
  const match = /^session-(\d+)$/.exec(sessionId);
  if (!match) {
    return undefined;
  }
  const displayNumber = Number.parseInt(match[1]!, 10) - 1;
  if (!Number.isFinite(displayNumber) || displayNumber < 0) {
    return undefined;
  }
  return formatSessionDisplayId(displayNumber);
}

function updateGroup(
  snapshot: GroupedSessionWorkspaceSnapshot,
  groupId: string,
  update: (group: SessionGroupRecord) => SessionGroupRecord,
): GroupedSessionWorkspaceSnapshot {
  return normalizeSimpleGroupedSessionWorkspaceSnapshot({
    ...snapshot,
    groups: snapshot.groups.map((group) => (group.groupId === groupId ? update(group) : group)),
  });
}

function updateSession(
  snapshot: GroupedSessionWorkspaceSnapshot,
  sessionId: string,
  update: (session: SessionRecord) => SessionRecord,
): WorkspaceMutationResult {
  const group = getGroupForSession(snapshot, sessionId);
  if (!group) {
    return { changed: false, snapshot };
  }

  const nextSnapshot = updateGroup(snapshot, group.groupId, (targetGroup) => ({
    ...targetGroup,
    snapshot: normalizeGroupSnapshot({
      ...targetGroup.snapshot,
      sessions: targetGroup.snapshot.sessions.map((session) =>
        session.sessionId === sessionId ? update(session) : session,
      ),
    }),
  }));

  return {
    changed: !areSnapshotsEqual(snapshot, nextSnapshot),
    snapshot: nextSnapshot,
  };
}

function normalizeTerminalSessionLastActivityAt(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  if (!normalized || Number.isNaN(Date.parse(normalized))) {
    return undefined;
  }
  return normalized;
}

function normalizeSessionLifecycleTimestamp(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  if (!normalized || Number.isNaN(Date.parse(normalized))) {
    return undefined;
  }
  return normalized;
}

function createEmptyGroup(groupId: string, title: string): SessionGroupRecord {
  return {
    groupId,
    snapshot: createDefaultSessionGridSnapshot(),
    title,
  };
}

function clampSupportedVisibleCount(value: number | undefined): VisibleSessionCount {
  return clampVisibleSessionCount(value ?? 1);
}

function getNextGroupNumber(groups: readonly SessionGroupRecord[]): number {
  let nextGroupNumber = 2;
  for (const group of groups) {
    const match = /^group-(\d+)$/.exec(group.groupId);
    if (!match) {
      continue;
    }

    const parsedNumber = Number.parseInt(match[1]!, 10);
    if (Number.isInteger(parsedNumber)) {
      nextGroupNumber = Math.max(nextGroupNumber, parsedNumber + 1);
    }
  }
  return nextGroupNumber;
}

function getNextSessionNumber(groups: readonly SessionGroupRecord[]): number {
  let nextSessionNumber = 1;
  for (const session of groups.flatMap((group) => group.snapshot.sessions)) {
    nextSessionNumber = Math.max(
      nextSessionNumber,
      (getSessionNumberFromSessionId(session.sessionId) ?? 0) + 1,
    );
  }
  return nextSessionNumber;
}

function getWorkspaceSessionIds(snapshot: GroupedSessionWorkspaceSnapshot): string[] {
  return snapshot.groups.flatMap((group) =>
    group.snapshot.sessions.map((session) => session.sessionId),
  );
}

function haveSameStringSet(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) {
    return false;
  }

  const rightSet = new Set(right);
  return left.every((value) => rightSet.has(value));
}

function areSnapshotsEqual(
  left: GroupedSessionWorkspaceSnapshot,
  right: GroupedSessionWorkspaceSnapshot,
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

/*
CDXC:SessionFocusMode 2026-06-05-22:26:
Focus-mode mutations compare both whole workspaces and individual group snapshots.
Keep group-level equality typed separately so release typecheck catches accidental
cross-scope comparisons instead of weakening the workspace snapshot contract.
*/
function areGroupSnapshotsEqual(left: SessionGridSnapshot, right: SessionGridSnapshot): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
