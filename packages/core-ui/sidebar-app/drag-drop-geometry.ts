import type { SidebarActiveSessionsSortMode } from '../../shared/session-grid-contract';
import { moveProjectsWithWorktrees, type ProjectWorktreeOrderItem } from '../../shared/project-worktree-order';
import type { SidebarProjectCollectionsState } from '../project-collections';
import { SIDEBAR_REORDER_DISTANCE_PX } from '../sidebar-reorder-activation';
import {
  canonicalizeSidebarSessionDropTarget,
  getClientPoint,
  getSidebarDropData,
  getSidebarGroupDropTargetFromEvent,
  getSidebarSessionDropTarget,
  getSidebarSessionDropTargetAtPoint,
  getSidebarSessionDropTargetFromEvent,
  moveGroupIdsByDropTarget,
  moveSessionIdsByDropTarget,
  type SidebarGroupDropTarget,
  type SidebarSessionDropTarget,
} from '../sidebar-dnd';
import { readRenderedSidebarSessionSlots } from '../sidebar-visible-session-slots';
import { findSessionGroupId, haveSameSessionOrder } from './session-ordering';
import type { SessionIdsByGroup, SidebarGroupsById, SidebarSessionsById } from './types';

export type SidebarPointerDownSessionTarget = {
  groupId: string;
  point: {
    x: number;
    y: number;
  };
  sessionId: string;
};

export type SidebarSessionPointerDragState = {
  didMove: boolean;
  startPoint?: {
    x: number;
    y: number;
  };
};
export type SidebarProjectGroupOrderItem = ProjectWorktreeOrderItem & {
  orderId: string;
};
export type SidebarProjectGroupLookup = Record<
  string,
  | {
      projectContext?: {
        path?: string;
        editor: {
          projectId: string;
        };
        worktree?: {
          parentProjectId: string;
        };
      };
      remoteMachineContext?: {
        machineId: string;
        projectId?: string;
      };
    }
  | undefined
>;
export function summarizeSidebarWakeScrollOrderState({
  activeSessionsSortMode,
  displayedWorkspaceGroupIds,
  displayedWorkspaceSessionIdsByGroup,
  focusedSessionId,
  groupsById,
  revision,
  sessionsById,
}: {
  activeSessionsSortMode: SidebarActiveSessionsSortMode;
  displayedWorkspaceGroupIds: readonly string[];
  displayedWorkspaceSessionIdsByGroup: SessionIdsByGroup;
  focusedSessionId: string;
  groupsById: SidebarGroupsById;
  revision: number;
  sessionsById: SidebarSessionsById;
}): Record<string, unknown> {
  const groupId = findSessionGroupId(displayedWorkspaceSessionIdsByGroup, focusedSessionId);
  const groupSessionIds = groupId ? (displayedWorkspaceSessionIdsByGroup[groupId] ?? []) : [];
  const groupIndex = groupId ? displayedWorkspaceGroupIds.indexOf(groupId) : -1;
  const targetIndexInGroup = groupSessionIds.indexOf(focusedSessionId);
  const group = groupId ? groupsById[groupId] : undefined;
  const session = sessionsById[focusedSessionId];
  return {
    activeSessionsSortMode,
    displayedGroupCount: displayedWorkspaceGroupIds.length,
    firstSessionIdInGroup: groupSessionIds[0],
    focusedSessionId,
    groupId,
    groupIndex,
    groupIsChatCollection: group?.isChatCollection === true,
    groupIsProject: Boolean(group?.projectContext),
    groupIsRemote: Boolean(group?.remoteMachineContext),
    groupSessionCount: groupSessionIds.length,
    lastSessionIdInGroup: groupSessionIds.at(-1),
    revision,
    sessionActivity: session?.activity,
    sessionIsFocused: session?.isFocused,
    sessionIsLive: session?.isLive,
    sessionIsPinned: session?.isPinned,
    sessionIsSleeping: session?.isSleeping,
    sessionIsVisible: session?.isVisible,
    sessionKind: session?.sessionKind ?? session?.kind,
    sessionLastInteractionAt: session?.lastInteractionAt,
    sessionLifecycleState: session?.lifecycleState,
    sessionNativePaneState: session?.nativePaneState,
    sessionProviderSessionState: session?.providerSessionState,
    targetIndexInGroup,
    targetWindowSessionIds: createSidebarWakeScrollSessionIdWindow(groupSessionIds, targetIndexInGroup),
  };
}

export function summarizeSidebarWakeScrollRenderedSlots(
  root: ParentNode,
  focusedSessionId: string
): Record<string, unknown> {
  const slots = readRenderedSidebarSessionSlots(root);
  const renderedSessionIds = slots.map((slot) => slot.sessionId);
  const renderedIndex = renderedSessionIds.indexOf(focusedSessionId);
  return {
    renderedAwakeSlotCount: slots.filter((slot) => !slot.isSleeping).length,
    renderedFirstSessionId: renderedSessionIds[0],
    renderedIndex,
    renderedLastSessionId: renderedSessionIds.at(-1),
    renderedSleepingSlotCount: slots.filter((slot) => slot.isSleeping).length,
    renderedSlotCount: slots.length,
    renderedWindowSessionIds: createSidebarWakeScrollSessionIdWindow(renderedSessionIds, renderedIndex),
  };
}

export function summarizeSidebarWakeScrollGeometry(
  focusedSessionElement: HTMLElement,
  scrollViewport: HTMLElement
): Record<string, unknown> {
  const rowBounds = focusedSessionElement.getBoundingClientRect();
  const viewportBounds = scrollViewport.getBoundingClientRect();
  return {
    clientHeight: roundSidebarWakeScrollMetric(scrollViewport.clientHeight),
    isAboveViewport: rowBounds.top < viewportBounds.top,
    isBelowViewport: rowBounds.bottom > viewportBounds.bottom,
    isOutsideViewport: rowBounds.top < viewportBounds.top || rowBounds.bottom > viewportBounds.bottom,
    rowBottomRelativeToViewport: roundSidebarWakeScrollMetric(rowBounds.bottom - viewportBounds.top),
    rowHeight: roundSidebarWakeScrollMetric(rowBounds.height),
    rowTopRelativeToViewport: roundSidebarWakeScrollMetric(rowBounds.top - viewportBounds.top),
    scrollHeight: roundSidebarWakeScrollMetric(scrollViewport.scrollHeight),
    scrollTop: roundSidebarWakeScrollMetric(scrollViewport.scrollTop),
    viewportHeight: roundSidebarWakeScrollMetric(viewportBounds.height),
  };
}

export function createSidebarWakeScrollSessionIdWindow(
  sessionIds: readonly string[],
  targetIndex: number,
  radius = 3
): string[] {
  if (targetIndex < 0) {
    return [];
  }
  return sessionIds.slice(Math.max(0, targetIndex - radius), Math.min(sessionIds.length, targetIndex + radius + 1));
}

export function roundSidebarWakeScrollMetric(value: number): number {
  return Math.round(value * 100) / 100;
}

export function movePinnedSessionIdsByDropTarget(
  previousPinnedSessionIds: readonly string[],
  sourceSessionId: string,
  target: SidebarSessionDropTarget
): string[] {
  if (target.kind !== 'session') {
    return [...previousPinnedSessionIds];
  }

  return (
    moveSessionIdsByDropTarget(
      {
        [target.groupId]: [...previousPinnedSessionIds],
      },
      sourceSessionId,
      target
    )[target.groupId] ?? [...previousPinnedSessionIds]
  );
}

export function createPinnedSessionDropTargetLogKey(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }>,
  target: SidebarSessionDropTarget | undefined
): string {
  if (!target) {
    return `${sourceData.groupId}:${sourceData.sessionId}:none`;
  }

  if (target.kind === 'group') {
    return `${sourceData.groupId}:${sourceData.sessionId}:${target.groupId}:group:${target.position}`;
  }

  return `${sourceData.groupId}:${sourceData.sessionId}:${target.groupId}:${target.sessionId}:${target.position}`;
}

export function createPinnedSessionReorderDebugState(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }>,
  currentSessionIdsByGroup: SessionIdsByGroup,
  effectiveSessionIdsByGroup: SessionIdsByGroup,
  authoritativeSessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<string, { isPinned?: boolean; sessionId?: string } | undefined>
): Record<string, unknown> {
  const currentSessionIds = currentSessionIdsByGroup[sourceData.groupId] ?? [];
  const effectiveSessionIds = effectiveSessionIdsByGroup[sourceData.groupId] ?? [];
  const authoritativeSessionIds = authoritativeSessionIdsByGroup[sourceData.groupId] ?? [];
  const currentPinnedSessionIds = currentSessionIds.filter((sessionId) => sessionsById[sessionId]?.isPinned === true);
  const effectivePinnedSessionIds = effectiveSessionIds.filter(
    (sessionId) => sessionsById[sessionId]?.isPinned === true
  );

  return {
    authoritativeSessionIds,
    currentPinnedSessionIds,
    currentSessionIds,
    effectivePinnedSessionIds,
    effectiveSessionIds,
    pinnedCount: currentPinnedSessionIds.length,
    sourceCurrentIndex: currentSessionIds.indexOf(sourceData.sessionId),
    sourceCurrentPinnedIndex: currentPinnedSessionIds.indexOf(sourceData.sessionId),
    sourceEffectiveIndex: effectiveSessionIds.indexOf(sourceData.sessionId),
    sourceEffectivePinnedIndex: effectivePinnedSessionIds.indexOf(sourceData.sessionId),
    sourceIsPinned: sessionsById[sourceData.sessionId]?.isPinned === true,
  };
}

export function summarizePointerEventForPinnedReorder(event: PointerEvent): Record<string, unknown> {
  return {
    button: event.button,
    buttons: event.buttons,
    clientX: event.clientX,
    clientY: event.clientY,
    isPrimary: event.isPrimary,
    pointerType: event.pointerType,
  };
}

export function createPinnedSessionDomDebugState(groupId: string, sessionId: string): Record<string, unknown> {
  const groupElement = getSidebarGroupElementById(groupId);
  const sessionElement = getTargetSessionElement(sessionId, undefined);
  const frameElement = sessionElement?.closest<HTMLElement>('.session-frame');

  return {
    group: {
      collapsed: groupElement?.dataset.collapsed,
      dragging: groupElement?.dataset.dragging,
      found: Boolean(groupElement),
      rect: summarizeElementRectForPinnedReorder(groupElement),
    },
    session: {
      dragging: sessionElement?.dataset.dragging,
      found: Boolean(sessionElement),
      frameFound: Boolean(frameElement),
      pinned: sessionElement?.dataset.pinned,
      rect: summarizeElementRectForPinnedReorder(sessionElement),
      visible: sessionElement?.dataset.visible,
    },
  };
}

export function createPinnedSessionDropResolutionDebugState(
  nativeEvent: Event | undefined,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }>,
  sessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<string, { isPinned?: boolean } | undefined>
): Record<string, unknown> {
  const point = getClientPoint(nativeEvent);
  const groupElement = getSidebarGroupElementById(sourceData.groupId);
  const groupBounds = groupElement?.getBoundingClientRect();
  const groupSessionIds = sessionIdsByGroup[sourceData.groupId] ?? [];
  const pinnedSessionIds = groupSessionIds.filter((sessionId) => sessionsById[sessionId]?.isPinned === true);
  const targetMetrics = pinnedSessionIds
    .filter((sessionId) => sessionId !== sourceData.sessionId)
    .map((sessionId) => {
      const element = getTargetSessionElement(sessionId, point);
      const bounds = element?.getBoundingClientRect();
      return {
        elementFound: Boolean(element),
        height: bounds?.height,
        midpointY: bounds ? bounds.top + bounds.height / 2 : undefined,
        pinnedIndex: pinnedSessionIds.indexOf(sessionId),
        pointBeforeMidpoint: bounds && point ? point.y <= bounds.top + bounds.height / 2 : undefined,
        top: bounds?.top,
      };
    });
  const renderedPinnedBounds = targetMetrics
    .filter((metric) => metric.elementFound && metric.top !== undefined && metric.height !== undefined)
    .reduce<{ bottom: number; top: number } | undefined>((bounds, metric) => {
      const top = metric.top!;
      const bottom = top + metric.height!;
      return bounds ? { bottom: Math.max(bounds.bottom, bottom), top: Math.min(bounds.top, top) } : { bottom, top };
    }, undefined);
  const pointInsideGroup =
    point !== undefined &&
    (groupBounds !== undefined
      ? point.y >= groupBounds.top && point.y <= groupBounds.bottom
      : renderedPinnedBounds !== undefined &&
        point.y >= renderedPinnedBounds.top &&
        point.y <= renderedPinnedBounds.bottom);

  return {
    groupElementFound: Boolean(groupElement),
    groupRect: summarizeElementRectForPinnedReorder(groupElement),
    groupSessionCount: groupSessionIds.length,
    hasPoint: Boolean(point),
    pinnedCount: pinnedSessionIds.length,
    point,
    pointInsideGroup,
    sourceInPinnedSet: pinnedSessionIds.includes(sourceData.sessionId),
    sourcePinnedIndex: pinnedSessionIds.indexOf(sourceData.sessionId),
    targetMetricCount: targetMetrics.filter((metric) => metric.elementFound).length,
    targetMetrics,
  };
}

export function summarizeElementRectForPinnedReorder(
  element: Element | null | undefined
): Record<string, number> | undefined {
  if (!element) {
    return undefined;
  }

  const bounds = element.getBoundingClientRect();
  return {
    bottom: bounds.bottom,
    height: bounds.height,
    top: bounds.top,
  };
}

export function findCreatedGroupId(
  previousGroups: readonly string[],
  nextGroups: readonly string[]
): string | undefined {
  const previousGroupIds = new Set(previousGroups);
  return nextGroups.find((groupId) => !previousGroupIds.has(groupId));
}

export function resolveSessionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  sessionIdsByGroup: SessionIdsByGroup,
  targetData: ReturnType<typeof getSidebarDropData>,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }> | undefined
) {
  const point = getClientPoint(nativeEvent);
  /*
   * CDXC:Sidebar 2026-06-19-11:12:
   * Prefer current pointer hit testing over dnd-kit's reported target so the
   * insertion line follows the hovered row midpoint continuously, including
   * the exact center of a session row.
   */
  const candidates = [
    point ? getSidebarSessionDropTargetAtPoint(document, point.x, point.y) : undefined,
    getSidebarSessionDropTargetFromEvent(nativeEvent),
    getSidebarSessionDropTargetFromDropData(targetData, point),
    getSidebarSessionDropTarget(targetData),
  ];

  for (const rawCandidate of candidates) {
    if (!rawCandidate) {
      continue;
    }

    const candidate = canonicalizeSidebarSessionDropTarget(rawCandidate);
    const groupSessionIds = sessionIdsByGroup[candidate.groupId];
    if (!groupSessionIds) {
      continue;
    }

    if (candidate.kind === 'session' && !groupSessionIds.includes(candidate.sessionId)) {
      continue;
    }

    /*
     * CDXC:Sidebar 2026-07-02-13:05:
     * When releasing here would keep the session exactly where it started,
     * suppress the insertion line entirely instead of falling through to a
     * different candidate, so no line is shown for a no-op drop.
     */
    if (
      isSourceSessionDropTarget(candidate, sourceData) ||
      (sourceData && isNoOpSessionDropTarget(sessionIdsByGroup, sourceData.sessionId, candidate))
    ) {
      return null;
    }

    return candidate;
  }

  return null;
}

export function isNoOpSessionDropTarget(
  sessionIdsByGroup: SessionIdsByGroup,
  sessionId: string,
  target: SidebarSessionDropTarget
): boolean {
  const nextSessionIdsByGroup = moveSessionIdsByDropTarget(sessionIdsByGroup, sessionId, target);
  if (nextSessionIdsByGroup === sessionIdsByGroup) {
    return true;
  }

  return Object.entries(nextSessionIdsByGroup).every(([groupId, nextSessionIds]) =>
    haveSameSessionOrder(sessionIdsByGroup[groupId] ?? [], nextSessionIds)
  );
}

export function resolvePinnedSessionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }>,
  sessionIdsByGroup: SessionIdsByGroup,
  sessionsById: Record<string, { isPinned?: boolean } | undefined>
): SidebarSessionDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return undefined;
  }

  const groupElement = getSidebarGroupElementById(sourceData.groupId);
  const groupBounds = groupElement?.getBoundingClientRect();
  const groupSessionIds = sessionIdsByGroup[sourceData.groupId] ?? [];
  const pinnedSessionIds = groupSessionIds.filter((sessionId) => sessionsById[sessionId]?.isPinned === true);
  if (pinnedSessionIds.length < 2 || !pinnedSessionIds.includes(sourceData.sessionId)) {
    return undefined;
  }

  const targetSessionMetrics = pinnedSessionIds
    .filter((sessionId) => sessionId !== sourceData.sessionId)
    .flatMap((sessionId) => {
      const element = getTargetSessionElement(sessionId, point);
      return element
        ? [
            {
              bounds: element.getBoundingClientRect(),
              sessionId,
            },
          ]
        : [];
    });
  if (targetSessionMetrics.length === 0) {
    return undefined;
  }
  const renderedPinnedTop = Math.min(...targetSessionMetrics.map((target) => target.bounds.top));
  const renderedPinnedBottom = Math.max(...targetSessionMetrics.map((target) => target.bounds.bottom));
  if (
    groupBounds
      ? point.y < groupBounds.top || point.y > groupBounds.bottom
      : point.y < renderedPinnedTop || point.y > renderedPinnedBottom
  ) {
    return undefined;
  }

  /*
   * CDXC:Sessions 2026-05-28-14:29:
   * Pinned session drag feedback should be a stable insertion line within the
   * pinned partition. Base the active slot on pinned row midpoints only, not on
   * whichever full-project or unpinned-row droppable dnd-kit reports while the
   * pointer crosses row gaps.
   *
   * CDXC:Sidebar 2026-06-19-11:12:
   * The exact midpoint belongs to the lower half so a session row always shows
   * an insertion line: center/down is after, center/up is before.
   */
  const resolvedTarget = ((): SidebarSessionDropTarget => {
    for (const target of targetSessionMetrics) {
      if (point.y < target.bounds.top + target.bounds.height / 2) {
        return {
          groupId: sourceData.groupId,
          kind: 'session',
          position: 'before',
          sessionId: target.sessionId,
        };
      }
    }

    const lastTarget = targetSessionMetrics[targetSessionMetrics.length - 1];
    return {
      groupId: sourceData.groupId,
      kind: 'session',
      position: 'after',
      sessionId: lastTarget.sessionId,
    };
  })();

  /*
   * CDXC:Sidebar 2026-07-02-13:05:
   * Pinned reorder also hides its insertion feedback when releasing would keep
   * the pinned row in its current slot.
   */
  return isNoOpSessionDropTarget(sessionIdsByGroup, sourceData.sessionId, resolvedTarget) ? undefined : resolvedTarget;
}

export type SidebarRemoteMachineDropTarget = {
  position: 'before' | 'after';
  remoteMachineId: string;
};

export type SidebarProjectCollectionDropTarget = {
  collectionId: string;
  position: 'before' | 'after';
};

export function resolveRemoteMachineDropTargetFromPoint(
  nativeEvent: Event | undefined,
  remoteMachineIds: readonly string[],
  sourceRemoteMachineId: string,
  targetData: ReturnType<typeof getSidebarDropData>
): SidebarRemoteMachineDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  const candidate = point
    ? getRemoteMachineBoundaryTargetAtY(remoteMachineIds, point.y)
    : targetData?.kind === 'remote-machine' && remoteMachineIds.includes(targetData.remoteMachineId)
      ? { remoteMachineId: targetData.remoteMachineId, position: 'before' as const }
      : undefined;
  if (!candidate) {
    return undefined;
  }
  return moveRemoteMachineIdToDropTarget(remoteMachineIds, sourceRemoteMachineId, candidate) ? candidate : undefined;
}

export function getRemoteMachineBoundaryTargetAtY(
  remoteMachineIds: readonly string[],
  y: number
): SidebarRemoteMachineDropTarget | undefined {
  const headerMidpoints = remoteMachineIds.flatMap((remoteMachineId) => {
    const section = document.querySelector<HTMLElement>(
      `[data-sidebar-remote-machine-id="${CSS.escape(remoteMachineId)}"]`
    );
    const header = section?.querySelector<HTMLElement>('.reference-sidebar-section-row');
    if (!header) {
      return [];
    }
    const bounds = header.getBoundingClientRect();
    return bounds.height > 0 ? [{ midpoint: bounds.top + bounds.height / 2, remoteMachineId }] : [];
  });
  if (headerMidpoints.length === 0) {
    return undefined;
  }
  for (const header of headerMidpoints) {
    if (y < header.midpoint) {
      return { remoteMachineId: header.remoteMachineId, position: 'before' };
    }
  }
  return {
    remoteMachineId: headerMidpoints[headerMidpoints.length - 1].remoteMachineId,
    position: 'after',
  };
}

export function moveRemoteMachineIdToDropTarget(
  remoteMachineIds: readonly string[],
  sourceRemoteMachineId: string,
  target: SidebarRemoteMachineDropTarget
): string[] | undefined {
  const withoutSource = remoteMachineIds.filter((remoteMachineId) => remoteMachineId !== sourceRemoteMachineId);
  if (withoutSource.length === remoteMachineIds.length) {
    return undefined;
  }
  const anchorIndex = withoutSource.indexOf(target.remoteMachineId);
  if (target.remoteMachineId === sourceRemoteMachineId || anchorIndex < 0) {
    return undefined;
  }
  const insertionIndex = target.position === 'before' ? anchorIndex : anchorIndex + 1;
  const next = [...withoutSource];
  next.splice(insertionIndex, 0, sourceRemoteMachineId);
  return next.every((remoteMachineId, index) => remoteMachineId === remoteMachineIds[index]) ? undefined : next;
}

export function areSameRemoteMachineDropTarget(
  left: SidebarRemoteMachineDropTarget | undefined,
  right: SidebarRemoteMachineDropTarget | undefined
): boolean {
  return left?.remoteMachineId === right?.remoteMachineId && left?.position === right?.position;
}

export const LOCAL_PROJECT_LIST_SCOPE_ID = 'local';

export function createRemoteProjectListScopeId(remoteMachineId: string): string {
  return `remote:${remoteMachineId}`;
}

/*
 * CDXC:Projects 2026-07-23:
 * A collection-only Projects or Remote Machine section has no ungrouped row
 * whose collection id can resolve to undefined. Give each section a real
 * normal-flow end zone, then resolve it only for a grouped project from that
 * same local/remote scope. The zone owns the line below the final collection;
 * project rows and collection panels keep their existing independent drag
 * boundaries.
 */
export function resolveProjectUngroupDropScopeFromPoint(
  nativeEvent: Event | undefined,
  sourceGroupId: string,
  groupsById: SidebarProjectGroupLookup
): string | undefined {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return undefined;
  }
  const remoteMachineId = groupsById[sourceGroupId]?.remoteMachineContext?.machineId;
  const scopeId = remoteMachineId ? createRemoteProjectListScopeId(remoteMachineId) : LOCAL_PROJECT_LIST_SCOPE_ID;
  const element = document.querySelector<HTMLElement>(
    `[data-sidebar-project-ungroup-drop-zone="${CSS.escape(scopeId)}"]`
  );
  if (!element) {
    return undefined;
  }
  const bounds = element.getBoundingClientRect();
  return bounds.height > 0 &&
    point.x >= bounds.left &&
    point.x <= bounds.right &&
    point.y >= bounds.top &&
    point.y <= bounds.bottom
    ? scopeId
    : undefined;
}

export function moveProjectGroupFamilyToEnd(
  groupIds: readonly string[],
  sourceGroupId: string,
  groupsById: SidebarProjectGroupLookup
): string[] {
  const sourceProjectId = groupsById[sourceGroupId]?.projectContext?.editor.projectId;
  if (!sourceProjectId) {
    return [...groupIds];
  }
  const familyProjectIds = new Set(getProjectCollectionFamilyProjectIds(sourceProjectId, groupIds, groupsById));
  const isFamilyGroup = (groupId: string) => {
    const projectId = groupsById[groupId]?.projectContext?.editor.projectId;
    return Boolean(projectId && familyProjectIds.has(projectId));
  };
  return [...groupIds.filter((groupId) => !isFamilyGroup(groupId)), ...groupIds.filter(isFamilyGroup)];
}

/*
 * CDXC:Projects 2026-07-21:
 * Collection drags use feedback "none", so dnd-kit's rect-overlap collision
 * never reports a target (the source shape never leaves its slot). Resolve the
 * insertion boundary from the pointer against the local collection panels'
 * midpoints, exactly like resolveGroupDropTargetFromPoint does for project
 * rows. Remote sections render the same collections with the same ids, so the
 * lookup skips any panel inside a remote machine section.
 */
export function getLocalProjectCollectionElement(collectionId: string): HTMLElement | undefined {
  const elements = document.querySelectorAll<HTMLElement>(
    `section.project-collection[data-sidebar-project-collection-id="${CSS.escape(collectionId)}"]`
  );
  for (const element of elements) {
    if (!element.closest('.reference-remote-machine-section')) {
      return element;
    }
  }
  return undefined;
}

export function resolveProjectCollectionDropTargetFromPoint(
  nativeEvent: Event | undefined,
  collectionIds: readonly string[],
  sourceCollectionId: string,
  targetData: ReturnType<typeof getSidebarDropData>
): SidebarProjectCollectionDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  const candidate = point
    ? getProjectCollectionBoundaryTargetAtY(collectionIds, point.y)
    : targetData?.kind === 'project-collection' && collectionIds.includes(targetData.collectionId)
      ? { collectionId: targetData.collectionId, position: 'before' as const }
      : undefined;
  if (!candidate) {
    return undefined;
  }
  return moveCollectionIdToDropTarget(collectionIds, sourceCollectionId, candidate) ? candidate : undefined;
}

export function getProjectCollectionBoundaryTargetAtY(
  collectionIds: readonly string[],
  y: number
): SidebarProjectCollectionDropTarget | undefined {
  const midpoints = collectionIds.flatMap((collectionId) => {
    const element = getLocalProjectCollectionElement(collectionId);
    if (!element) {
      return [];
    }
    const bounds = element.getBoundingClientRect();
    return bounds.height > 0 ? [{ collectionId, midpoint: bounds.top + bounds.height / 2 }] : [];
  });
  if (midpoints.length === 0) {
    return undefined;
  }
  for (const entry of midpoints) {
    if (y < entry.midpoint) {
      return { collectionId: entry.collectionId, position: 'before' };
    }
  }
  return {
    collectionId: midpoints[midpoints.length - 1].collectionId,
    position: 'after',
  };
}

/*
 * Returns the reordered id list, or undefined when the drop is a no-op (the
 * boundary sits directly around the dragged collection's own slot).
 */
export function moveCollectionIdToDropTarget(
  collectionIds: readonly string[],
  sourceCollectionId: string,
  target: SidebarProjectCollectionDropTarget
): string[] | undefined {
  const withoutSource = collectionIds.filter((collectionId) => collectionId !== sourceCollectionId);
  if (withoutSource.length === collectionIds.length) {
    return undefined;
  }
  const anchorIndex = withoutSource.indexOf(target.collectionId);
  const insertionIndex =
    target.collectionId === sourceCollectionId
      ? undefined
      : anchorIndex < 0
        ? undefined
        : target.position === 'before'
          ? anchorIndex
          : anchorIndex + 1;
  if (insertionIndex === undefined) {
    return undefined;
  }
  const next = [...withoutSource];
  next.splice(insertionIndex, 0, sourceCollectionId);
  return next.every((collectionId, index) => collectionId === collectionIds[index]) ? undefined : next;
}

export function resolveGroupDropTargetFromPoint(
  nativeEvent: Event | undefined,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
  targetData: ReturnType<typeof getSidebarDropData>,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'group' }> | undefined,
  /*
   * CDXC:Projects 2026-07-30:
   * How "this drop would change nothing" is decided. V1's default answer runs the
   * physical project-with-worktrees move; grouped V2 passes its own, because its
   * ids are LOGICAL rows and the two moves can disagree about which boundaries
   * are no-ops. Letting the caller supply the predicate keeps the drop line and
   * the committed reorder answering the same question.
   */
  isNoOpTarget?: (target: SidebarGroupDropTarget) => boolean
): SidebarGroupDropTarget | undefined {
  const point = getClientPoint(nativeEvent);
  /*
   * CDXC:Projects 2026-07-02-13:05:
   * The insertion line was dancing because dnd-kit's rect-overlap target could
   * disagree with the pointer position, and because the same boundary could be
   * reported as "after A" or "before B", which draw in different spots. While
   * the pointer is known, resolve one canonical boundary from the visible
   * header midpoints ("before" the first group whose header midpoint is below
   * the pointer, "after" only past the last group) and suppress the line for
   * no-op drops instead of falling through to another candidate.
   */
  const candidates = point
    ? [getSidebarGroupBoundaryTargetAtY(groupIds, point.y)]
    : [getSidebarGroupDropTargetFromDropData(targetData, point), getSidebarGroupDropTargetFromEvent(nativeEvent)];

  for (const candidate of candidates) {
    if (!candidate) {
      continue;
    }

    if (!groupIds.includes(candidate.groupId)) {
      continue;
    }

    if (candidate.groupId === sourceData?.groupId) {
      return undefined;
    }

    if (
      sourceData &&
      (isNoOpTarget
        ? isNoOpTarget(candidate)
        : isNoOpGroupDropTarget(groupIds, sourceData.groupId, candidate, groupsById))
    ) {
      return undefined;
    }

    return candidate;
  }

  return undefined;
}

export function getSidebarGroupBoundaryTargetAtY(
  groupIds: readonly string[],
  y: number
): SidebarGroupDropTarget | undefined {
  const groupHeaderMidpoints = groupIds.flatMap((groupId) => {
    const groupElement = getSidebarGroupElementById(groupId);
    if (!groupElement) {
      return [];
    }

    const bounds = getSidebarGroupDropBoundsElement(groupElement).getBoundingClientRect();
    return bounds.height > 0 ? [{ groupId, midpoint: bounds.top + bounds.height / 2 }] : [];
  });
  if (groupHeaderMidpoints.length === 0) {
    return undefined;
  }

  for (const header of groupHeaderMidpoints) {
    if (y < header.midpoint) {
      return { groupId: header.groupId, position: 'before' };
    }
  }

  return {
    groupId: groupHeaderMidpoints[groupHeaderMidpoints.length - 1].groupId,
    position: 'after',
  };
}

export function areSameGroupDropTarget(
  left: SidebarGroupDropTarget | undefined,
  right: SidebarGroupDropTarget | undefined
): boolean {
  return left?.groupId === right?.groupId && left?.position === right?.position;
}

export function areSameSessionDropTarget(
  left: SidebarSessionDropTarget | undefined,
  right: SidebarSessionDropTarget | undefined
): boolean {
  if (!left || !right || left.kind !== right.kind || left.groupId !== right.groupId) {
    return left === right;
  }

  if (left.kind === 'session' && right.kind === 'session') {
    return left.sessionId === right.sessionId && left.position === right.position;
  }

  return left.position === right.position;
}

export function isSourceSessionDropTarget(
  candidate: SidebarSessionDropTarget,
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }> | undefined
): boolean {
  return Boolean(
    sourceData &&
    candidate.kind === 'session' &&
    candidate.groupId === sourceData.groupId &&
    candidate.sessionId === sourceData.sessionId
  );
}

export function getSidebarSessionDropTargetFromDropData(
  targetData: ReturnType<typeof getSidebarDropData>,
  point: ReturnType<typeof getClientPoint>
): SidebarSessionDropTarget | undefined {
  if (targetData?.kind === 'session') {
    const sessionElement = getTargetSessionElement(targetData.sessionId, point);
    if (!sessionElement) {
      return undefined;
    }

    const bounds = sessionElement.getBoundingClientRect();
    const relativeY = point?.y ?? bounds.top + bounds.height / 2;
    /*
     * CDXC:Sidebar 2026-06-19-11:12:
     * Dnd-kit may report a broad target while the pointer is around a row
     * midpoint. Resolve the explicit target with the same center/down-after
     * rule as point-based row hit testing so the line stays visible.
     */
    const position: 'after' | 'before' = relativeY >= bounds.top + bounds.height / 2 ? 'after' : 'before';
    return {
      groupId: targetData.groupId,
      kind: 'session',
      position,
      sessionId: targetData.sessionId,
    };
  }

  if (targetData?.kind === 'group') {
    const groupElement = document.querySelector<HTMLElement>(`[data-sidebar-group-id="${targetData.groupId}"]`);
    if (!groupElement) {
      return undefined;
    }

    const bounds = groupElement.getBoundingClientRect();
    const relativeY = point?.y ?? bounds.top;
    const position: 'end' | 'start' = relativeY > bounds.top + bounds.height / 2 ? 'end' : 'start';
    return {
      groupId: targetData.groupId,
      kind: 'group',
      position,
    };
  }

  return undefined;
}

export function getSidebarGroupDropTargetFromDropData(
  targetData: ReturnType<typeof getSidebarDropData>,
  point: ReturnType<typeof getClientPoint>
): SidebarGroupDropTarget | undefined {
  if (targetData?.kind !== 'group') {
    return undefined;
  }

  const groupElement = getTargetGroupElement(targetData.groupId, point);
  if (!groupElement) {
    return undefined;
  }

  /*
   * CDXC:Projects 2026-05-22-22:18:
   * Dnd-kit target data can point at an expanded project container. Use the
   * same header-row bounds as point-based hit testing so the drop line does not
   * jump between above and below while the pointer moves through session rows.
   */
  const boundsElement = getSidebarGroupDropBoundsElement(groupElement);
  const bounds = boundsElement.getBoundingClientRect();
  const relativeY = point?.y ?? bounds.top + bounds.height / 2;
  return {
    groupId: targetData.groupId,
    position: relativeY > bounds.top + bounds.height / 2 ? 'after' : 'before',
  };
}

export function isNoOpGroupDropTarget(
  groupIds: readonly string[],
  sourceGroupId: string,
  target: SidebarGroupDropTarget,
  groupsById: SidebarProjectGroupLookup
): boolean {
  /*
   * CDXC:Projects 2026-05-22-22:18:
   * Do not show an insertion line for adjacent before/after targets that would
   * leave the project order unchanged on drop. The preview should only mark
   * committed position changes.
   *
   * CDXC:Worktrees 2026-05-25-12:38:
   * Worktree projects cannot be dropped outside their main-project family, and
   * a main-project drag is computed as a family move so its worktrees stay
   * directly underneath it in the same order.
   */
  return haveSameSessionOrder(groupIds, moveGroupIdsByProjectDropTarget(groupIds, sourceGroupId, target, groupsById));
}

export function moveGroupIdsByProjectDropTarget(
  groupIds: readonly string[],
  sourceGroupId: string,
  target: SidebarGroupDropTarget,
  groupsById: SidebarProjectGroupLookup
): string[] {
  const projectGroupItems = createProjectGroupOrderItems(groupIds, groupsById);
  if (projectGroupItems.length !== groupIds.length) {
    return moveGroupIdsByDropTarget(groupIds, sourceGroupId, target);
  }

  return moveProjectsWithWorktrees(projectGroupItems, sourceGroupId, {
    orderId: target.groupId,
    position: target.position,
  }).map((project) => project.orderId);
}

export function createProjectGroupOrderItems(
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup
): SidebarProjectGroupOrderItem[] {
  return groupIds.flatMap((groupId) => {
    const projectContext = groupsById[groupId]?.projectContext;
    if (!projectContext) {
      return [];
    }

    return [
      {
        orderId: groupId,
        projectId: projectContext.editor.projectId,
        worktree: projectContext.worktree ? { parentProjectId: projectContext.worktree.parentProjectId } : undefined,
      },
    ];
  });
}

export function getProjectCollectionFamilyProjectIds(
  projectId: string,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup
): string[] {
  const requestedProjectContext = groupIds
    .map((groupId) => groupsById[groupId]?.projectContext)
    .find((projectContext) => projectContext?.editor.projectId === projectId);
  const familyParentProjectId = requestedProjectContext?.worktree?.parentProjectId ?? projectId;
  const projectIds = groupIds.flatMap((groupId) => {
    const projectContext = groupsById[groupId]?.projectContext;
    const candidateProjectId = projectContext?.editor.projectId;
    if (
      candidateProjectId === familyParentProjectId ||
      projectContext?.worktree?.parentProjectId === familyParentProjectId
    ) {
      return candidateProjectId ? [candidateProjectId] : [];
    }
    return [];
  });
  return projectIds.length > 0 ? [...new Set(projectIds)] : [projectId];
}

export function createProjectCollectionIdByProjectId(
  state: SidebarProjectCollectionsState,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup,
  resolveProjectId: (groupId: string) => string | undefined
): Map<string, string> {
  const result = new Map<string, string>();
  for (const collection of state.collections) {
    for (const projectId of collection.projectIds) {
      result.set(projectId, collection.collectionId);
    }
  }
  for (const groupId of groupIds) {
    const projectId = resolveProjectId(groupId);
    const parentProjectId = groupsById[groupId]?.projectContext?.worktree?.parentProjectId;
    const inheritedCollectionId = parentProjectId ? result.get(parentProjectId) : undefined;
    if (projectId && inheritedCollectionId) {
      result.set(projectId, inheritedCollectionId);
    }
  }
  return result;
}

export function getRemoteProjectCollectionFamilyProjectIds(
  scopedProjectId: string,
  groupIds: readonly string[],
  groupsById: SidebarProjectGroupLookup
): string[] {
  const requestedGroup = groupIds
    .map((groupId) => groupsById[groupId])
    .find((group) => group?.projectContext?.editor.projectId === scopedProjectId);
  const rawProjectId = requestedGroup?.remoteMachineContext?.projectId;
  if (!rawProjectId) {
    return [];
  }
  const familyParentProjectId = requestedGroup?.projectContext?.worktree?.parentProjectId ?? rawProjectId;
  const projectIds = groupIds.flatMap((groupId) => {
    const group = groupsById[groupId];
    const candidateProjectId = group?.remoteMachineContext?.projectId;
    if (
      candidateProjectId === familyParentProjectId ||
      group?.projectContext?.worktree?.parentProjectId === familyParentProjectId
    ) {
      return candidateProjectId ? [candidateProjectId] : [];
    }
    return [];
  });
  return projectIds.length > 0 ? [...new Set(projectIds)] : [rawProjectId];
}

export function getSidebarGroupDropBoundsElement(groupElement: HTMLElement): HTMLElement {
  return groupElement.querySelector<HTMLElement>('.group-head') ?? groupElement;
}

export function getTargetSessionElement(
  sessionId: string,
  point: ReturnType<typeof getClientPoint>
): HTMLElement | undefined {
  const selector = `[data-sidebar-session-id="${sessionId}"]`;
  if (point) {
    for (const element of document.elementsFromPoint(point.x, point.y)) {
      const sessionElement = element.closest<HTMLElement>(selector);
      if (sessionElement && sessionElement.dataset.dragging !== 'true') {
        return sessionElement;
      }
    }
  }

  return Array.from(document.querySelectorAll<HTMLElement>(selector)).find(
    (sessionElement) => sessionElement.dataset.dragging !== 'true'
  );
}

export function getSidebarGroupElementById(groupId: string): HTMLElement | undefined {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-sidebar-group-id]')).find(
    (groupElement) => groupElement.dataset.sidebarGroupId === groupId
  );
}

export function getTargetGroupElement(
  groupId: string,
  point: ReturnType<typeof getClientPoint>
): HTMLElement | undefined {
  const selector = `[data-sidebar-group-id="${groupId}"]`;
  if (point) {
    for (const element of document.elementsFromPoint(point.x, point.y)) {
      const groupElement = element.closest<HTMLElement>(selector);
      if (groupElement && groupElement.dataset.dragging !== 'true') {
        return groupElement;
      }
    }
  }

  return Array.from(document.querySelectorAll<HTMLElement>(selector)).find(
    (groupElement) => groupElement.dataset.dragging !== 'true'
  );
}

export function getDragNativeEvent(value: unknown): Event | undefined {
  return isObjectRecord(value) && value.nativeEvent instanceof Event ? value.nativeEvent : undefined;
}

export function updateGroupDragPreviewFromEvent<Preview extends { pointerOffsetY: number; top: number }>(
  setGroupDragPreview: (updater: (previous: Preview | undefined) => Preview | undefined) => void,
  nativeEvent: Event | undefined
): void {
  const point = getClientPoint(nativeEvent);
  if (!point) {
    return;
  }

  setGroupDragPreview((previous) =>
    previous
      ? {
          ...previous,
          top: point.y - previous.pointerOffsetY,
        }
      : previous
  );
}

export function getProjectGroupDragHeaderMetrics(
  groupId: string,
  point: { x: number; y: number }
): { left: number; pointerOffsetY: number; top: number; width: number } | undefined {
  const groupElement = Array.from(document.querySelectorAll<HTMLElement>('[data-sidebar-group-id]')).find(
    (candidate) => candidate.dataset.sidebarGroupId === groupId && candidate.dataset.dragging !== 'true'
  );
  const headerElement = groupElement?.querySelector<HTMLElement>('.group-head');
  const headerRect = headerElement?.getBoundingClientRect();
  if (!headerRect) {
    return undefined;
  }

  return {
    left: headerRect.left,
    pointerOffsetY: point.y - headerRect.top,
    top: headerRect.top,
    width: headerRect.width,
  };
}

export function getProjectCollectionDragMetrics(
  source: unknown,
  collectionId: string
): { left: number; top: number; width: number } | undefined {
  /*
   * CDXC:Projects 2026-07-22:
   * The same collection can render once locally and once per remote machine
   * section, so prefer the dnd-kit source element (the grabbed section) over a
   * document query that could match another instance. The section rect is used
   * instead of the header rect so the ghost's own 1px panel border lands
   * exactly on the grabbed panel's border.
   */
  const sourceElement = isObjectRecord(source) && source.element instanceof HTMLElement ? source.element : undefined;
  const sectionElement =
    sourceElement?.dataset.sidebarProjectCollectionId === collectionId
      ? sourceElement
      : Array.from(document.querySelectorAll<HTMLElement>('[data-sidebar-project-collection-id]')).find(
          (candidate) => candidate.dataset.sidebarProjectCollectionId === collectionId
        );
  const sectionRect = sectionElement?.getBoundingClientRect();
  if (!sectionRect) {
    return undefined;
  }

  return {
    left: sectionRect.left,
    top: sectionRect.top,
    width: sectionRect.width,
  };
}

export function getRemoteMachineDragHeaderMetrics(
  remoteMachineId: string,
  point: { x: number; y: number }
): { left: number; pointerOffsetY: number; top: number; width: number } | undefined {
  const section = document.querySelector<HTMLElement>(
    `[data-sidebar-remote-machine-id="${CSS.escape(remoteMachineId)}"]`
  );
  const header = section?.querySelector<HTMLElement>('.reference-sidebar-section-row');
  const bounds = header?.getBoundingClientRect();
  if (!bounds) {
    return undefined;
  }
  return {
    left: bounds.left,
    pointerOffsetY: point.y - bounds.top,
    top: bounds.top,
    width: bounds.width,
  };
}

export function createSessionPointerDragState(
  sourceData: Extract<ReturnType<typeof getSidebarDropData>, { kind: 'session' }>,
  pointerDownSessionTarget: SidebarPointerDownSessionTarget | undefined,
  nativeEvent: Event | undefined
): SidebarSessionPointerDragState {
  const startPoint =
    pointerDownSessionTarget &&
    pointerDownSessionTarget.groupId === sourceData.groupId &&
    pointerDownSessionTarget.sessionId === sourceData.sessionId
      ? pointerDownSessionTarget.point
      : undefined;

  return {
    didMove: hasPointerDragMovedPastThreshold(startPoint, getClientPoint(nativeEvent)),
    startPoint,
  };
}

export function updateSessionPointerDragState(
  pointerDragState: SidebarSessionPointerDragState | undefined,
  nativeEvent: Event | undefined
): void {
  if (!pointerDragState || pointerDragState.didMove) {
    return;
  }

  pointerDragState.didMove = hasPointerDragMovedPastThreshold(pointerDragState.startPoint, getClientPoint(nativeEvent));
}

export function hasPointerDragMovedPastThreshold(
  startPoint: { x: number; y: number } | undefined,
  currentPoint: { x: number; y: number } | undefined
): boolean {
  if (!startPoint || !currentPoint) {
    return false;
  }

  return Math.hypot(currentPoint.x - startPoint.x, currentPoint.y - startPoint.y) >= SIDEBAR_REORDER_DISTANCE_PX;
}

export function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
